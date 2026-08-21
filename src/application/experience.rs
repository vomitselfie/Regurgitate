use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::core::{
    ApplicabilityTags, Caveat, EXPERIENCE_SCHEMA_VERSION, Ecosystem, EnvironmentFingerprint,
    EvidenceEntry, ExperienceCapsule, FailureReason, HostClass, Lesson, MemoryLifecycle,
    MemoryScope, Procedure, SemanticOutcome, Situation, TaskKind, jaccard,
};

use super::{ProjectLocator, ProjectResolver};

/// Bounded candidate window loaded per scope when looking for an equivalent
/// capsule before insertion.
pub const MAX_DEDUP_CANDIDATES: usize = 200;
/// Token-set similarity at or above which two situations describe the same
/// condition.
pub const EQUIVALENT_SITUATION_SIMILARITY: f64 = 0.6;
/// Lesson similarity below which an otherwise equivalent capsule is treated as
/// a contradicting lesson rather than a paraphrase.
pub const CONTRADICTING_LESSON_SIMILARITY: f64 = 0.35;
/// Evidence entries a challenged capsule needs before it is active again.
pub const CHALLENGE_RESOLUTION_EVIDENCE: usize = 3;

/// Which retrieval bucket a capsule lives in. Storage derives an HMAC token
/// from this value; the plaintext never reaches the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKey {
    Project(Uuid),
    Workspace(Uuid),
    Ecosystem(Ecosystem),
    Machine,
    Global,
}

impl ScopeKey {
    pub fn for_capsule(capsule: &ExperienceCapsule) -> Result<Self> {
        Ok(match capsule.scope {
            MemoryScope::Project => Self::Project(
                capsule
                    .scope_id
                    .context("project-scoped capsule has no project identity")?,
            ),
            MemoryScope::Workspace => Self::Workspace(
                capsule
                    .scope_id
                    .context("workspace-scoped capsule has no workspace identity")?,
            ),
            MemoryScope::Ecosystem => Self::Ecosystem(
                capsule
                    .applicability
                    .ecosystem
                    .context("ecosystem-scoped capsule has no ecosystem tag")?,
            ),
            MemoryScope::Machine => Self::Machine,
            MemoryScope::Global => Self::Global,
        })
    }

    pub fn scope(self) -> MemoryScope {
        match self {
            Self::Project(_) => MemoryScope::Project,
            Self::Workspace(_) => MemoryScope::Workspace,
            Self::Ecosystem(_) => MemoryScope::Ecosystem,
            Self::Machine => MemoryScope::Machine,
            Self::Global => MemoryScope::Global,
        }
    }

    /// Stable bytes hashed into the storage lookup token.
    pub fn identity_bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.scope() as u8];
        match self {
            Self::Project(id) | Self::Workspace(id) => bytes.extend_from_slice(id.as_bytes()),
            Self::Ecosystem(ecosystem) => bytes.extend_from_slice(ecosystem.label().as_bytes()),
            Self::Machine | Self::Global => {}
        }
        bytes
    }
}

/// Persistence port for experience capsules.
pub trait ExperienceStore {
    /// Encrypts and inserts; `false` when the id already exists.
    fn append_experience(&self, capsule: &ExperienceCapsule) -> Result<bool>;
    /// Re-encrypts an existing capsule in place; `false` when it is missing.
    fn replace_experience(&self, capsule: &ExperienceCapsule) -> Result<bool>;
    /// Most recently confirmed capsules in one scope bucket.
    fn scoped_experiences(&self, scope: ScopeKey, limit: usize) -> Result<Vec<ExperienceCapsule>>;
    /// Capsules recorded from one project, for maintenance and selectors.
    fn project_experiences(&self, project_id: Uuid, limit: usize)
    -> Result<Vec<ExperienceCapsule>>;
}

/// Resolves the workspace (parent directory) identity for a project.
pub fn workspace_locator(project: &ProjectLocator) -> Option<ProjectLocator> {
    let parent: &Path = project.as_path().parent()?;
    if parent.as_os_str().is_empty() || parent.parent().is_none() {
        // The filesystem root is not a workspace.
        return None;
    }
    Some(ProjectLocator::new(parent.to_path_buf()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceStatus {
    Recorded,
    Confirmed,
    Challenged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExperienceReport {
    pub status: ExperienceStatus,
    pub lifecycle: MemoryLifecycle,
    pub evidence: usize,
}

/// Everything the agent or CLI may supply when recording one experience.
pub struct ExperienceInput {
    pub scope: MemoryScope,
    pub task: TaskKind,
    pub situation: Option<Situation>,
    pub lesson: Option<Lesson>,
    pub caveat: Option<Caveat>,
    pub procedure: Procedure,
    pub outcome: SemanticOutcome,
    pub failure_reason: Option<FailureReason>,
    pub applicability: ApplicabilityTags,
    pub environment: EnvironmentFingerprint,
}

impl ExperienceInput {
    fn validate(&self) -> Result<()> {
        self.procedure.validate()?;
        if self.situation.is_some() && self.lesson.is_none() {
            bail!("a situation requires a lesson");
        }
        if self.failure_reason.is_some() && self.outcome != SemanticOutcome::Failure {
            bail!("a failure reason requires a failure outcome");
        }
        if self.scope == MemoryScope::Ecosystem && self.applicability.ecosystem.is_none() {
            bail!("ecosystem scope requires --ecosystem");
        }
        Ok(())
    }
}

/// Records, confirms, and transitions experience capsules.
pub struct ExperienceService<H> {
    history: H,
}

impl<H> ExperienceService<H>
where
    H: ExperienceStore + ProjectResolver,
{
    pub fn new(history: H) -> Self {
        Self { history }
    }

    /// Records one capsule, or confirms an equivalent active capsule instead
    /// of accumulating a paraphrase duplicate.
    pub fn record(
        &self,
        project: ProjectLocator,
        input: ExperienceInput,
    ) -> Result<ExperienceReport> {
        self.record_at(project, input, Utc::now())
    }

    pub fn record_at(
        &self,
        project: ProjectLocator,
        input: ExperienceInput,
        now: DateTime<Utc>,
    ) -> Result<ExperienceReport> {
        input.validate()?;
        let project_id = self
            .history
            .resolve_project(&project)
            .context("could not resolve the encrypted project identity")?;
        let scope_id = match input.scope {
            MemoryScope::Project => Some(project_id),
            MemoryScope::Workspace => {
                let workspace = workspace_locator(&project)
                    .context("this project directory has no usable workspace parent")?;
                Some(self.history.resolve_project(&workspace)?)
            }
            _ => None,
        };
        let entry = EvidenceEntry {
            at: now,
            outcome: input.outcome,
            failure_reason: input.failure_reason,
        };
        let mut environment = input.environment;
        if environment.host_class.is_none() {
            environment.host_class = HostClass::current();
        }
        let mut candidate = ExperienceCapsule {
            id: Uuid::new_v4(),
            project_id,
            scope: input.scope,
            scope_id,
            task: input.task,
            situation: input.situation,
            lesson: input.lesson,
            caveat: input.caveat,
            procedure: input.procedure,
            applicability: input.applicability,
            environment,
            lifecycle: MemoryLifecycle::Active,
            evidence: vec![entry],
            created_at: now,
            last_confirmed_at: now,
            schema_version: EXPERIENCE_SCHEMA_VERSION,
        };
        candidate.validate()?;

        let scope = ScopeKey::for_capsule(&candidate)?;
        let existing = self
            .history
            .scoped_experiences(scope, MAX_DEDUP_CANDIDATES)?;
        match find_equivalent(&candidate, &existing) {
            Some(Equivalence::Same(mut matched)) => {
                matched.confirm(entry);
                if matched.caveat.is_none() {
                    matched.caveat = candidate.caveat.take();
                }
                if matched.lifecycle == MemoryLifecycle::Challenged
                    && matched.evidence.len() >= CHALLENGE_RESOLUTION_EVIDENCE
                {
                    matched.lifecycle = MemoryLifecycle::Active;
                }
                matched.validate()?;
                if !self.history.replace_experience(&matched)? {
                    bail!("the equivalent capsule disappeared during confirmation");
                }
                Ok(ExperienceReport {
                    status: ExperienceStatus::Confirmed,
                    lifecycle: matched.lifecycle,
                    evidence: matched.evidence.len(),
                })
            }
            Some(Equivalence::Contradiction(mut matched)) => {
                matched.lifecycle = MemoryLifecycle::Challenged;
                candidate.lifecycle = MemoryLifecycle::Challenged;
                self.history.replace_experience(&matched)?;
                self.history.append_experience(&candidate)?;
                Ok(ExperienceReport {
                    status: ExperienceStatus::Challenged,
                    lifecycle: candidate.lifecycle,
                    evidence: 1,
                })
            }
            None => {
                if !self.history.append_experience(&candidate)? {
                    bail!("capsule identity collision");
                }
                Ok(ExperienceReport {
                    status: ExperienceStatus::Recorded,
                    lifecycle: candidate.lifecycle,
                    evidence: 1,
                })
            }
        }
    }

    /// Marks one capsule with a new lifecycle. Selectors are opaque local id
    /// prefixes that only match capsules recorded from the given project.
    pub fn transition(
        &self,
        project: &ProjectLocator,
        selector: &str,
        lifecycle: MemoryLifecycle,
    ) -> Result<TransitionReport> {
        let project_id = self.history.resolve_project(project)?;
        let mut capsule = self.select(project_id, selector)?;
        let previous = capsule.lifecycle;
        capsule.lifecycle = lifecycle;
        capsule.validate()?;
        self.history.replace_experience(&capsule)?;
        Ok(TransitionReport {
            previous,
            lifecycle,
        })
    }

    /// Marks `old` superseded by `new`, which becomes active.
    pub fn supersede(
        &self,
        project: &ProjectLocator,
        old_selector: &str,
        new_selector: &str,
    ) -> Result<TransitionReport> {
        let project_id = self.history.resolve_project(project)?;
        let mut old = self.select(project_id, old_selector)?;
        let mut new = self.select(project_id, new_selector)?;
        if old.id == new.id {
            bail!("a capsule cannot supersede itself");
        }
        let previous = old.lifecycle;
        old.lifecycle = MemoryLifecycle::Superseded;
        new.lifecycle = MemoryLifecycle::Active;
        self.history.replace_experience(&old)?;
        self.history.replace_experience(&new)?;
        Ok(TransitionReport {
            previous,
            lifecycle: MemoryLifecycle::Superseded,
        })
    }

    /// Aggregate maintenance listing. Contains no lesson text.
    pub fn list(&self, project: &ProjectLocator, limit: usize) -> Result<Vec<ExperienceSummary>> {
        let project_id = self.history.resolve_project(project)?;
        Ok(self
            .history
            .project_experiences(project_id, limit)?
            .iter()
            .map(ExperienceSummary::from)
            .collect())
    }

    fn select(&self, project_id: Uuid, selector: &str) -> Result<ExperienceCapsule> {
        let selector = selector.trim().to_ascii_lowercase().replace('-', "");
        if selector.len() < 8
            || !selector
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            bail!("a capsule selector is at least eight hexadecimal characters");
        }
        let mut matches: Vec<ExperienceCapsule> = self
            .history
            .project_experiences(project_id, usize::MAX)?
            .into_iter()
            .filter(|capsule| capsule.id.simple().to_string().starts_with(&selector))
            .collect();
        match matches.len() {
            0 => bail!("no capsule recorded from this project matches the selector"),
            1 => Ok(matches.remove(0)),
            _ => bail!("the selector is ambiguous; supply more characters"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TransitionReport {
    pub previous: MemoryLifecycle,
    pub lifecycle: MemoryLifecycle,
}

/// Human maintenance projection: status and shape, never the lesson.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperienceSummary {
    pub selector: String,
    pub scope: MemoryScope,
    pub task: TaskKind,
    pub procedure: String,
    pub lifecycle: MemoryLifecycle,
    pub has_text: bool,
    pub successes: usize,
    pub failures: usize,
    pub created_at: DateTime<Utc>,
    pub last_confirmed_at: DateTime<Utc>,
}

impl From<&ExperienceCapsule> for ExperienceSummary {
    fn from(capsule: &ExperienceCapsule) -> Self {
        Self {
            selector: capsule.id.simple().to_string()[..12].to_owned(),
            scope: capsule.scope,
            task: capsule.task,
            procedure: capsule.procedure.summary(),
            lifecycle: capsule.lifecycle,
            has_text: capsule.has_text(),
            successes: capsule.successes(),
            failures: capsule.failures(),
            created_at: capsule.created_at,
            last_confirmed_at: capsule.last_confirmed_at,
        }
    }
}

enum Equivalence {
    Same(ExperienceCapsule),
    Contradiction(ExperienceCapsule),
}

/// Finds an existing capsule with the same controlled identity whose
/// situation matches. Similarity is computed ephemerally over decrypted
/// candidates in memory; no derived vectors are retained.
fn find_equivalent(
    candidate: &ExperienceCapsule,
    existing: &[ExperienceCapsule],
) -> Option<Equivalence> {
    let identity = candidate.identity();
    let candidate_situation = token_set(candidate.situation.as_ref().map(|text| text.as_str()));
    let candidate_lesson = token_set(candidate.lesson.as_ref().map(|text| text.as_str()));
    let mut best: Option<(f64, &ExperienceCapsule)> = None;
    for capsule in existing {
        if capsule.identity() != identity
            || matches!(
                capsule.lifecycle,
                MemoryLifecycle::Superseded | MemoryLifecycle::Obsolete
            )
        {
            continue;
        }
        let similarity = match (candidate.has_text(), capsule.has_text()) {
            // Two text-free (legacy-style) capsules with one identity are the
            // same aggregate.
            (false, false) => 1.0,
            (true, true) => jaccard(
                &candidate_situation,
                &token_set(capsule.situation.as_ref().map(|text| text.as_str())),
            ),
            _ => 0.0,
        };
        if similarity >= EQUIVALENT_SITUATION_SIMILARITY
            && best.is_none_or(|(score, _)| similarity > score)
        {
            best = Some((similarity, capsule));
        }
    }
    let (_, matched) = best?;
    if !candidate.has_text() {
        return Some(Equivalence::Same(matched.clone()));
    }
    let lesson_similarity = jaccard(
        &candidate_lesson,
        &token_set(matched.lesson.as_ref().map(|text| text.as_str())),
    );
    if lesson_similarity < CONTRADICTING_LESSON_SIMILARITY
        && matched.lifecycle == MemoryLifecycle::Active
    {
        Some(Equivalence::Contradiction(matched.clone()))
    } else {
        Some(Equivalence::Same(matched.clone()))
    }
}

fn token_set(text: Option<&str>) -> std::collections::BTreeSet<String> {
    text.map(|text| crate::core::tokenize(text).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use chrono::TimeZone;

    use crate::core::{MutationMode, VerificationMode};

    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        capsules: RefCell<Vec<ExperienceCapsule>>,
    }

    impl ExperienceStore for Rc<MemoryStore> {
        fn append_experience(&self, capsule: &ExperienceCapsule) -> Result<bool> {
            let mut capsules = self.capsules.borrow_mut();
            if capsules.iter().any(|existing| existing.id == capsule.id) {
                return Ok(false);
            }
            capsules.push(capsule.clone());
            Ok(true)
        }

        fn replace_experience(&self, capsule: &ExperienceCapsule) -> Result<bool> {
            let mut capsules = self.capsules.borrow_mut();
            match capsules
                .iter_mut()
                .find(|existing| existing.id == capsule.id)
            {
                Some(slot) => {
                    *slot = capsule.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        fn scoped_experiences(
            &self,
            scope: ScopeKey,
            limit: usize,
        ) -> Result<Vec<ExperienceCapsule>> {
            Ok(self
                .capsules
                .borrow()
                .iter()
                .filter(|capsule| ScopeKey::for_capsule(capsule).unwrap() == scope)
                .take(limit)
                .cloned()
                .collect())
        }

        fn project_experiences(
            &self,
            project_id: Uuid,
            limit: usize,
        ) -> Result<Vec<ExperienceCapsule>> {
            Ok(self
                .capsules
                .borrow()
                .iter()
                .filter(|capsule| capsule.project_id == project_id)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    impl ProjectResolver for Rc<MemoryStore> {
        fn resolve_project(&self, locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                locator.as_path().to_string_lossy().as_bytes(),
            ))
        }
    }

    fn input(situation: &str, lesson: &str, outcome: SemanticOutcome) -> ExperienceInput {
        ExperienceInput {
            scope: MemoryScope::Project,
            task: TaskKind::Debugging,
            situation: Some(Situation::new(situation).unwrap()),
            lesson: Some(Lesson::new(lesson).unwrap()),
            caveat: None,
            procedure: Procedure {
                mutation: Some(MutationMode::IncrementalNativeRegeneration),
                verification: Some(VerificationMode::Native),
                ..Procedure::default()
            },
            outcome,
            failure_reason: None,
            applicability: ApplicabilityTags::default(),
            environment: EnvironmentFingerprint::default(),
        }
    }

    fn project() -> ProjectLocator {
        ProjectLocator::new(PathBuf::from("/private/workspace/SECRET_PROJECT"))
    }

    #[test]
    fn equivalent_capsules_confirm_instead_of_duplicating() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let now = Utc.timestamp_millis_opt(1_776_254_400_000).unwrap();

        let first = service
            .record_at(
                project(),
                input(
                    "Generated native artifact where parser acceptance is weaker than native checks.",
                    "Change one placement class at a time, then run native verification.",
                    SemanticOutcome::Success,
                ),
                now,
            )
            .unwrap();
        let second = service
            .record_at(
                project(),
                input(
                    "Generated native artifact; parser acceptance is weaker than the native check.",
                    "Change a single placement class at a time and run native verification after.",
                    SemanticOutcome::Failure,
                ),
                now + chrono::Duration::hours(1),
            )
            .unwrap();

        assert_eq!(first.status, ExperienceStatus::Recorded);
        assert_eq!(second.status, ExperienceStatus::Confirmed);
        assert_eq!(second.evidence, 2);
        let capsules = store.capsules.borrow();
        assert_eq!(capsules.len(), 1);
        assert_eq!(capsules[0].successes(), 1);
        assert_eq!(capsules[0].failures(), 1);
        assert_eq!(
            capsules[0].last_confirmed_at,
            now + chrono::Duration::hours(1)
        );
        assert!(capsules[0].environment.host_class.is_some());
    }

    #[test]
    fn contradicting_lessons_challenge_both_capsules() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let now = Utc::now();
        service
            .record_at(
                project(),
                input(
                    "Generated native artifact where parser acceptance is weaker than native checks.",
                    "Change one placement class at a time, then run native verification.",
                    SemanticOutcome::Success,
                ),
                now,
            )
            .unwrap();
        let report = service
            .record_at(
                project(),
                input(
                    "Generated native artifact where parser acceptance is weaker than native checks.",
                    "Regenerate the whole board in bulk; the incremental approach wastes effort.",
                    SemanticOutcome::Success,
                ),
                now,
            )
            .unwrap();

        assert_eq!(report.status, ExperienceStatus::Challenged);
        let capsules = store.capsules.borrow();
        assert_eq!(capsules.len(), 2);
        assert!(
            capsules
                .iter()
                .all(|capsule| capsule.lifecycle == MemoryLifecycle::Challenged)
        );
    }

    #[test]
    fn different_situations_are_separate_capsules() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let now = Utc::now();
        for situation in [
            "Generated native artifact where parser acceptance is weaker than native checks.",
            "Hand-written configuration file edited under version control with review.",
        ] {
            service
                .record_at(
                    project(),
                    input(
                        situation,
                        "Change one placement class at a time, then run native verification.",
                        SemanticOutcome::Success,
                    ),
                    now,
                )
                .unwrap();
        }
        assert_eq!(store.capsules.borrow().len(), 2);
    }

    #[test]
    fn text_free_capsules_aggregate_by_identity() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let mut shorthand = input("a b c", "d e f", SemanticOutcome::Success);
        shorthand.situation = None;
        shorthand.lesson = None;
        service.record(project(), shorthand).unwrap();
        let mut shorthand = input("a b c", "d e f", SemanticOutcome::Failure);
        shorthand.situation = None;
        shorthand.lesson = None;
        let report = service.record(project(), shorthand).unwrap();
        assert_eq!(report.status, ExperienceStatus::Confirmed);
        assert_eq!(store.capsules.borrow().len(), 1);
    }

    #[test]
    fn transitions_use_opaque_selectors_scoped_to_the_project() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        service
            .record(
                project(),
                input(
                    "Generated native artifact where parser acceptance is weaker than native checks.",
                    "Change one placement class at a time, then run native verification.",
                    SemanticOutcome::Success,
                ),
            )
            .unwrap();
        let listing = service.list(&project(), 10).unwrap();
        assert_eq!(listing.len(), 1);
        let encoded = serde_json::to_string(&listing).unwrap();
        assert!(!encoded.contains("placement"));
        assert!(!encoded.contains("SECRET_PROJECT"));

        let report = service
            .transition(&project(), &listing[0].selector, MemoryLifecycle::Obsolete)
            .unwrap();
        assert_eq!(report.previous, MemoryLifecycle::Active);
        assert_eq!(
            store.capsules.borrow()[0].lifecycle,
            MemoryLifecycle::Obsolete
        );

        let other = ProjectLocator::new(PathBuf::from("/private/workspace/OTHER"));
        assert!(
            service
                .transition(&other, &listing[0].selector, MemoryLifecycle::Active)
                .is_err()
        );
        assert!(
            service
                .transition(&project(), "zz", MemoryLifecycle::Active)
                .is_err()
        );
    }

    #[test]
    fn workspace_scope_anchors_to_the_parent_directory() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let mut workspace = input(
            "Generated native artifact where parser acceptance is weaker than native checks.",
            "Change one placement class at a time, then run native verification.",
            SemanticOutcome::Success,
        );
        workspace.scope = MemoryScope::Workspace;
        service.record(project(), workspace).unwrap();
        let capsule = &store.capsules.borrow()[0];
        let expected = store
            .resolve_project(&ProjectLocator::new(PathBuf::from("/private/workspace")))
            .unwrap();
        assert_eq!(capsule.scope_id, Some(expected));
        assert_ne!(capsule.scope_id, Some(capsule.project_id));
    }

    #[test]
    fn rejects_inconsistent_inputs_before_storage() {
        let store = Rc::new(MemoryStore::default());
        let service = ExperienceService::new(Rc::clone(&store));
        let mut bad = input("a b c", "d e f", SemanticOutcome::Success);
        bad.failure_reason = Some(FailureReason::TooSlow);
        assert!(service.record(project(), bad).is_err());
        let mut bad = input("a b c", "d e f", SemanticOutcome::Success);
        bad.scope = MemoryScope::Ecosystem;
        assert!(service.record(project(), bad).is_err());
        assert!(store.capsules.borrow().is_empty());
    }
}
