use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    application::{ProjectLocator, ScopeKey, workspace_locator},
    core::{
        ArtifactKind, EXPERIENCE_SCHEMA_VERSION, Ecosystem, ErrorClass, EvidenceEntry,
        EvidenceKind, ExperienceCapsule, ExperienceIdentity, FailureReason, HistoryEvent,
        HostClass, MemoryLifecycle, MemoryScope, Operation, Outcome, Phase, Procedure,
        SemanticOutcome, TaskKind, ToolFamily,
    },
};

use super::{RankingPolicy, policy::Posterior, task::TaskIntent};

pub const DEFAULT_RECALL_LIMIT: usize = 10;
pub const MAX_RECALL_LIMIT: usize = 20;
pub const DEFAULT_TOKEN_BUDGET: usize = 300;
pub const MAX_TOKEN_BUDGET: usize = 1_000;
pub const DEFAULT_PREFLIGHT_TOKEN_BUDGET: usize = 220;
const MIN_TOKEN_BUDGET: usize = 32;
const MAX_CANDIDATE_EVENTS_PER_KIND: usize = 1_000;
/// Bounded decrypt window per scope bucket.
pub const MAX_CANDIDATES_PER_SCOPE: usize = 300;

pub trait ProjectLookup {
    fn find_project(&self, locator: &ProjectLocator) -> Result<Option<Uuid>>;
}

pub trait ProjectEventSource {
    fn recent_project_events(
        &self,
        project_id: Uuid,
        evidence_kind: EvidenceKind,
        limit: usize,
    ) -> Result<Vec<HistoryEvent>>;
}

/// Read-only candidate access for one scope bucket.
pub trait ExperienceSource {
    fn scoped_experiences(&self, scope: ScopeKey, limit: usize) -> Result<Vec<ExperienceCapsule>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallOptions {
    pub operation: Option<Operation>,
    pub failures_only: bool,
    pub limit: usize,
    pub token_budget: usize,
}

impl Default for RecallOptions {
    fn default() -> Self {
        Self {
            operation: None,
            failures_only: false,
            limit: DEFAULT_RECALL_LIMIT,
            token_budget: DEFAULT_TOKEN_BUDGET,
        }
    }
}

impl RecallOptions {
    pub fn validate(&self) -> Result<()> {
        if self.limit == 0 || self.limit > MAX_RECALL_LIMIT {
            bail!("recall limit must be between 1 and {MAX_RECALL_LIMIT}");
        }
        if self.token_budget < MIN_TOKEN_BUDGET || self.token_budget > MAX_TOKEN_BUDGET {
            bail!("recall token budget must be between {MIN_TOKEN_BUDGET} and {MAX_TOKEN_BUDGET}");
        }
        Ok(())
    }
}

/// The current task, normalized ephemerally. Explicit controlled metadata
/// outranks keyword inference from `query`; the query text is never stored.
#[derive(Debug, Clone, Copy, Default)]
pub struct EphemeralTaskContext<'a> {
    pub query: Option<&'a str>,
    pub task: Option<TaskKind>,
    pub phase: Option<Phase>,
    pub artifact: Option<ArtifactKind>,
    pub ecosystem: Option<Ecosystem>,
    pub tool_family: Option<ToolFamily>,
}

impl<'a> EphemeralTaskContext<'a> {
    pub fn from_query(query: Option<&'a str>) -> Self {
        Self {
            query,
            ..Self::default()
        }
    }

    /// Whether the query text maps onto at least one controlled task.
    pub fn has_task_hints(&self) -> bool {
        self.query
            .is_some_and(|query| TaskIntent::classify(query).has_task_hints())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecallResult {
    pub experiences: Vec<ExperienceBriefItem>,
    pub hook_summary: HookSummary,
    pub approximate_tokens: usize,
}

impl RecallResult {
    pub fn empty() -> Self {
        budgeted_result(Vec::new(), HookSummary::default(), DEFAULT_TOKEN_BUDGET)
    }
}

/// A bounded diagnostic sample of provider-reported tool execution outcomes.
/// These counts never participate in procedural guidance.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HookSummary {
    pub sampled_executions: usize,
    pub reported_successes: usize,
    pub reported_failures: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    Limited,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeGuidance {
    Prefer,
    Avoid,
    Mixed,
}

/// One ranked lesson. Numbers are rounded so the serialized brief stays
/// small; identifiers and timestamps are deliberately absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExperienceBriefItem {
    pub scope: MemoryScope,
    pub task: TaskKind,
    pub procedure: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub situation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lesson: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<PracticeGuidance>,
    pub strength: EvidenceStrength,
    pub posterior: f64,
    pub interval: [f64; 2],
    pub effective_evidence: f64,
    pub successes: usize,
    pub failures: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub challenged: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub legacy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<FailureReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_error: Option<ErrorClass>,
}

pub struct RecallService<'a, H> {
    history: &'a H,
    policy: RankingPolicy,
}

impl<'a, H> RecallService<'a, H>
where
    H: ProjectLookup + ProjectEventSource + ExperienceSource,
{
    pub fn new(history: &'a H) -> Self {
        Self {
            history,
            policy: RankingPolicy::default(),
        }
    }

    pub fn with_policy(mut self, policy: RankingPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn recall(
        &self,
        locator: &ProjectLocator,
        options: RecallOptions,
        context: EphemeralTaskContext<'_>,
    ) -> Result<RecallResult> {
        self.recall_at(locator, options, context, Utc::now())
    }

    pub fn recall_at(
        &self,
        locator: &ProjectLocator,
        options: RecallOptions,
        context: EphemeralTaskContext<'_>,
        now: DateTime<Utc>,
    ) -> Result<RecallResult> {
        options.validate()?;
        let Some(project_id) = self.history.find_project(locator)? else {
            return Ok(budgeted_result(
                Vec::new(),
                HookSummary::default(),
                options.token_budget,
            ));
        };
        let hook_events = self.history.recent_project_events(
            project_id,
            EvidenceKind::HookExecution,
            MAX_CANDIDATE_EVENTS_PER_KIND,
        )?;
        let hook_summary = summarize_hooks(&hook_events, options);

        let intent = context.query.map(TaskIntent::classify).unwrap_or_default();
        let matcher = ContextMatcher::new(&context, &intent);

        // Stage 1: bounded project-scope window, including v2 legacy rows.
        let mut candidates = self
            .history
            .scoped_experiences(ScopeKey::Project(project_id), MAX_CANDIDATES_PER_SCOPE)?;
        let practice_events = self.history.recent_project_events(
            project_id,
            EvidenceKind::LearnedPractice,
            MAX_CANDIDATE_EVENTS_PER_KIND,
        )?;
        let (legacy, legacy_errors) = materialize_legacy(project_id, &practice_events);
        candidates.extend(legacy);

        let local_active = candidates
            .iter()
            .filter(|capsule| {
                capsule.lifecycle == MemoryLifecycle::Active
                    && matcher.applicability(&self.policy, capsule).is_some()
            })
            .count();
        if local_active < self.policy.sparse_local_evidence {
            if let Some(workspace) = workspace_locator(locator)
                && let Some(workspace_id) = self.history.find_project(&workspace)?
            {
                candidates.extend(self.history.scoped_experiences(
                    ScopeKey::Workspace(workspace_id),
                    MAX_CANDIDATES_PER_SCOPE,
                )?);
            }
            for ecosystem in matcher.ecosystems() {
                candidates.extend(self.history.scoped_experiences(
                    ScopeKey::Ecosystem(ecosystem),
                    MAX_CANDIDATES_PER_SCOPE,
                )?);
            }
            candidates.extend(
                self.history
                    .scoped_experiences(ScopeKey::Machine, MAX_CANDIDATES_PER_SCOPE)?,
            );
            candidates.extend(
                self.history
                    .scoped_experiences(ScopeKey::Global, MAX_CANDIDATES_PER_SCOPE)?,
            );
        }

        // Stage 2: contextual reranking over the decrypted window.
        let experiences = rank(
            &self.policy,
            &matcher,
            options,
            candidates,
            &legacy_errors,
            now,
        );
        Ok(budgeted_result(
            experiences,
            hook_summary,
            options.token_budget,
        ))
    }
}

fn summarize_hooks(hook_events: &[HistoryEvent], options: RecallOptions) -> HookSummary {
    let mut summary = HookSummary::default();
    for event in hook_events {
        if options
            .operation
            .is_some_and(|wanted| event.operation != wanted)
        {
            continue;
        }
        summary.sampled_executions += 1;
        match event.outcome {
            Outcome::Success => summary.reported_successes += 1,
            Outcome::Failure => summary.reported_failures += 1,
            Outcome::Unknown => summary.unknown += 1,
        }
    }
    summary
}

/// v2 `LearnedPractice` rows become one text-free legacy capsule per
/// task/strategy pair. Their lower specificity is explicit in the output.
fn materialize_legacy(
    project_id: Uuid,
    events: &[HistoryEvent],
) -> (Vec<ExperienceCapsule>, LegacyErrors) {
    let mut legacy_errors = LegacyErrors::new();
    #[derive(PartialEq, Eq, Hash)]
    struct Key(TaskKind, crate::core::Strategy);
    let mut groups: HashMap<Key, (Vec<EvidenceEntry>, BTreeMap<ErrorClass, usize>)> =
        HashMap::new();
    for event in events {
        let (Some(task), Some(strategy)) = (event.task, event.strategy) else {
            continue;
        };
        let outcome = match event.outcome {
            Outcome::Success => SemanticOutcome::Success,
            Outcome::Failure => SemanticOutcome::Failure,
            Outcome::Unknown => continue,
        };
        let entry = groups.entry(Key(task, strategy)).or_default();
        entry.0.push(EvidenceEntry {
            at: event.timestamp,
            outcome,
            failure_reason: None,
        });
        if let Some(error) = event.error_class {
            *entry.1.entry(error).or_default() += 1;
        }
    }
    let mut legacy: Vec<ExperienceCapsule> = groups
        .into_iter()
        .map(|(Key(task, strategy), (mut evidence, errors))| {
            evidence.sort_by_key(|entry| entry.at);
            let created_at = evidence.first().map(|entry| entry.at).unwrap_or_default();
            let last_confirmed_at = evidence.last().map(|entry| entry.at).unwrap_or_default();
            let mut capsule = ExperienceCapsule {
                // Deterministic, never persisted.
                id: Uuid::new_v5(
                    &project_id,
                    format!("legacy:{task:?}:{strategy:?}").as_bytes(),
                ),
                project_id,
                scope: MemoryScope::Project,
                scope_id: Some(project_id),
                task,
                situation: None,
                lesson: None,
                caveat: None,
                procedure: Procedure::from_strategy(strategy),
                applicability: Default::default(),
                environment: Default::default(),
                lifecycle: MemoryLifecycle::Active,
                evidence,
                created_at,
                last_confirmed_at,
                schema_version: EXPERIENCE_SCHEMA_VERSION,
            };
            capsule.evidence.truncate(crate::core::MAX_EVIDENCE_ENTRIES);
            legacy_errors.insert(capsule.id, errors);
            capsule
        })
        .collect();
    legacy.sort_by_key(|capsule| capsule.id);
    (legacy, legacy_errors)
}

/// Provider error classes for legacy aggregates, keyed by the transient
/// legacy capsule id. Lives only for the duration of one recall.
type LegacyErrors = HashMap<Uuid, BTreeMap<ErrorClass, usize>>;

struct ContextMatcher<'c> {
    context: &'c EphemeralTaskContext<'c>,
    intent: &'c TaskIntent,
    host: Option<HostClass>,
}

impl<'c> ContextMatcher<'c> {
    fn new(context: &'c EphemeralTaskContext<'c>, intent: &'c TaskIntent) -> Self {
        Self {
            context,
            intent,
            host: HostClass::current(),
        }
    }

    fn ecosystems(&self) -> Vec<Ecosystem> {
        if let Some(ecosystem) = self.context.ecosystem {
            return vec![ecosystem];
        }
        self.intent.ecosystems().iter().copied().collect()
    }

    fn task_filter_active(&self) -> bool {
        self.context.task.is_some() || self.intent.has_task_hints()
    }

    /// Returns `None` when the capsule is outside the current task region.
    fn applicability(&self, policy: &RankingPolicy, capsule: &ExperienceCapsule) -> Option<f64> {
        let task = match self.context.task {
            Some(task) => f64::from(u8::from(task == capsule.task)),
            None if self.intent.has_task_hints() => {
                f64::from(u8::from(self.intent.matches_task(capsule.task)))
            }
            None => 1.0,
        };
        if self.task_filter_active() && task == 0.0 {
            return None;
        }
        let artifact = match_term(
            self.context.artifact,
            self.intent.artifacts().iter().copied(),
            capsule.applicability.artifact_kind,
        );
        let ecosystem = match_term(
            self.context.ecosystem,
            self.intent.ecosystems().iter().copied(),
            capsule.applicability.ecosystem,
        );
        let tool = match_term(
            self.context.tool_family,
            self.intent.tool_families().iter().copied(),
            capsule
                .applicability
                .tool_family
                .or(capsule.environment.tool_family),
        );
        let phase = match_term(
            self.context.phase,
            self.intent.phases().iter().copied(),
            capsule.applicability.phase,
        );
        let environment = match (capsule.environment.host_class, self.host) {
            (Some(recorded), Some(current)) if recorded == current => 1.0,
            (Some(_), Some(_)) => 0.25,
            _ => 0.75,
        };
        let score = policy.applicability_task * task
            + policy.applicability_artifact * artifact
            + policy.applicability_ecosystem * (0.5 * ecosystem + 0.5 * tool)
            + policy.applicability_phase * phase
            + policy.applicability_environment * environment;
        if score < policy.min_applicability {
            return None;
        }
        if capsule.scope != MemoryScope::Project && score < policy.broader_scope_min_applicability {
            return None;
        }
        Some(score)
    }
}

/// 1 on agreement, 0 on disagreement. A context that says nothing about a
/// dimension cannot penalize it (1); a capsule that carries no tag is less
/// specific than one that matches (0.5).
fn match_term<T: Copy + PartialEq>(
    explicit: Option<T>,
    inferred: impl Iterator<Item = T>,
    recorded: Option<T>,
) -> f64 {
    let mut hints: Vec<T> = explicit.into_iter().collect();
    if hints.is_empty() {
        hints.extend(inferred);
    }
    if hints.is_empty() {
        return 1.0;
    }
    match recorded {
        None => 0.5,
        Some(recorded) => f64::from(u8::from(hints.contains(&recorded))),
    }
}

struct Cluster {
    representative: ExperienceCapsule,
    representative_rank: (u8, f64, DateTime<Utc>),
    weights: Vec<(f64, bool)>,
    successes: usize,
    failures: usize,
    best_applicability: f64,
    best_scope_weight: f64,
    latest: DateTime<Utc>,
    challenged: bool,
    legacy: bool,
    failure_reasons: BTreeMap<FailureReason, usize>,
    errors: BTreeMap<ErrorClass, usize>,
}

fn rank(
    policy: &RankingPolicy,
    matcher: &ContextMatcher<'_>,
    options: RecallOptions,
    candidates: Vec<ExperienceCapsule>,
    legacy_errors: &LegacyErrors,
    now: DateTime<Utc>,
) -> Vec<ExperienceBriefItem> {
    let mut clusters: BTreeMap<ExperienceIdentity, Cluster> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for capsule in candidates {
        if !seen.insert(capsule.id) {
            continue;
        }
        if matches!(
            capsule.lifecycle,
            MemoryLifecycle::Superseded | MemoryLifecycle::Obsolete
        ) {
            continue;
        }
        if options
            .operation
            .is_some_and(|wanted| capsule.procedure.classification().1 != wanted)
        {
            continue;
        }
        if options.failures_only && capsule.failures() == 0 {
            continue;
        }
        let Some(applicability) = matcher.applicability(policy, &capsule) else {
            continue;
        };
        let scope_weight = policy.scope_weight(capsule.scope);
        let lifecycle_weight = policy.lifecycle_weight(capsule.lifecycle);
        let weights: Vec<(f64, bool)> = capsule
            .evidence
            .iter()
            .map(|entry| {
                let age_days = (now - entry.at).num_seconds() as f64 / 86_400.0;
                (
                    scope_weight
                        * policy.age_weight(capsule.scope, age_days)
                        * applicability
                        * lifecycle_weight,
                    entry.outcome == SemanticOutcome::Success,
                )
            })
            .collect();
        let legacy = legacy_errors.contains_key(&capsule.id);
        let representative_rank = (
            u8::from(capsule.has_text()),
            scope_weight * applicability,
            capsule.last_confirmed_at,
        );
        let identity = capsule.identity();
        let cluster = clusters.entry(identity).or_insert_with(|| Cluster {
            representative: capsule.clone(),
            representative_rank,
            weights: Vec::new(),
            successes: 0,
            failures: 0,
            best_applicability: 0.0,
            best_scope_weight: 0.0,
            latest: capsule.last_confirmed_at,
            challenged: false,
            legacy: true,
            failure_reasons: BTreeMap::new(),
            errors: BTreeMap::new(),
        });
        if representative_rank > cluster.representative_rank {
            cluster.representative = capsule.clone();
            cluster.representative_rank = representative_rank;
        }
        cluster.weights.extend(weights);
        cluster.successes += capsule.successes();
        cluster.failures += capsule.failures();
        cluster.best_applicability = cluster.best_applicability.max(applicability);
        cluster.best_scope_weight = cluster.best_scope_weight.max(scope_weight);
        cluster.latest = cluster.latest.max(capsule.last_confirmed_at);
        cluster.challenged |= capsule.lifecycle == MemoryLifecycle::Challenged;
        cluster.legacy &= legacy;
        for entry in &capsule.evidence {
            if let Some(reason) = entry.failure_reason {
                *cluster.failure_reasons.entry(reason).or_default() += 1;
            }
        }
        if let Some(errors) = legacy_errors.get(&capsule.id) {
            for (error, count) in errors {
                *cluster.errors.entry(*error).or_default() += count;
            }
        }
    }

    let mut ranked: Vec<(
        f64,
        u8,
        DateTime<Utc>,
        ExperienceIdentity,
        ExperienceBriefItem,
    )> = clusters
        .into_iter()
        .map(|(identity, cluster)| {
            let posterior = Posterior::from_weighted(policy, &cluster.weights);
            let guidance = guidance(policy, &posterior);
            let strength = strength(policy, &posterior);
            let guidance_strength = match guidance {
                Some(PracticeGuidance::Prefer | PracticeGuidance::Avoid) => {
                    ((posterior.mean - 0.5).abs() * 2.0).clamp(0.0, 1.0)
                }
                Some(PracticeGuidance::Mixed) => 0.25,
                None => 0.0,
            };
            let confidence = (1.0 - (posterior.upper - posterior.lower)).clamp(0.0, 1.0);
            let age_days = (now - cluster.latest).num_seconds() as f64 / 86_400.0;
            let recency = policy.age_weight(cluster.representative.scope, age_days);
            let score = policy.rank_applicability * cluster.best_applicability
                + policy.rank_guidance * guidance_strength
                + policy.rank_confidence * confidence
                + policy.rank_recency * recency
                + policy.rank_scope * cluster.best_scope_weight
                + if cluster.representative.has_text() {
                    policy.rank_context_bonus
                } else {
                    0.0
                };
            let representative = cluster.representative;
            let item = ExperienceBriefItem {
                scope: representative.scope,
                task: representative.task,
                procedure: representative.procedure.summary(),
                situation: representative
                    .situation
                    .map(|text| text.as_str().to_owned()),
                lesson: representative.lesson.map(|text| text.as_str().to_owned()),
                caveat: representative.caveat.map(|text| text.as_str().to_owned()),
                guidance,
                strength,
                posterior: round(posterior.mean, 100.0),
                interval: [round(posterior.lower, 100.0), round(posterior.upper, 100.0)],
                effective_evidence: round(posterior.effective_evidence, 10.0),
                successes: cluster.successes,
                failures: cluster.failures,
                challenged: cluster.challenged,
                legacy: cluster.legacy,
                failure_reason: most_common(&cluster.failure_reasons),
                common_error: most_common(&cluster.errors),
            };
            (
                score,
                u8::from(item.lesson.is_some()),
                cluster.latest,
                identity,
                item,
            )
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    ranked
        .into_iter()
        .take(options.limit)
        .map(|(_, _, _, _, item)| item)
        .collect()
}

fn most_common<T: Copy + Ord>(counts: &BTreeMap<T, usize>) -> Option<T> {
    counts
        .iter()
        .max_by_key(|(key, count)| (**count, std::cmp::Reverse(**key)))
        .map(|(key, _)| *key)
}

fn guidance(policy: &RankingPolicy, posterior: &Posterior) -> Option<PracticeGuidance> {
    if posterior.effective_evidence < policy.min_effective_evidence {
        return None;
    }
    if posterior.lower >= policy.prefer_threshold {
        Some(PracticeGuidance::Prefer)
    } else if posterior.upper <= policy.avoid_threshold {
        Some(PracticeGuidance::Avoid)
    } else if posterior.mean > policy.avoid_threshold && posterior.mean < policy.prefer_threshold {
        Some(PracticeGuidance::Mixed)
    } else {
        // Leaning one way but the credible interval has not cleared the
        // threshold; the posterior stays visible without a label.
        None
    }
}

fn strength(policy: &RankingPolicy, posterior: &Posterior) -> EvidenceStrength {
    if posterior.effective_evidence < policy.min_effective_evidence {
        EvidenceStrength::Limited
    } else if posterior.effective_evidence < policy.strong_effective_evidence {
        EvidenceStrength::Moderate
    } else {
        EvidenceStrength::Strong
    }
}

fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

fn budgeted_result(
    mut experiences: Vec<ExperienceBriefItem>,
    hook_summary: HookSummary,
    token_budget: usize,
) -> RecallResult {
    loop {
        let mut result = RecallResult {
            experiences,
            hook_summary,
            approximate_tokens: 0,
        };
        loop {
            let estimate = estimate_tokens(&result);
            if estimate == result.approximate_tokens {
                break;
            }
            result.approximate_tokens = estimate;
        }
        if result.approximate_tokens <= token_budget || result.experiences.is_empty() {
            return result;
        }
        experiences = result.experiences;
        experiences.pop();
    }
}

pub(super) fn estimate_tokens(value: &impl Serialize) -> usize {
    let bytes = serde_json::to_vec_pretty(value)
        .expect("fixed-schema recall output must remain serializable")
        .len()
        + 1; // CLI newline
    bytes.div_ceil(4)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, path::PathBuf};

    use chrono::TimeZone;

    use crate::core::{
        AgentKind, ApplicabilityTags, CURRENT_SCHEMA_VERSION, Capability, EnvironmentFingerprint,
        Lesson, MutationMode, Situation, Strategy, VerificationMode,
    };

    use super::*;

    struct MemoryHistory {
        project_id: Option<Uuid>,
        events: Vec<HistoryEvent>,
        capsules: Vec<ExperienceCapsule>,
        requested_limit: Cell<usize>,
    }

    impl MemoryHistory {
        fn new(project_id: Option<Uuid>) -> Self {
            Self {
                project_id,
                events: Vec::new(),
                capsules: Vec::new(),
                requested_limit: Cell::new(0),
            }
        }
    }

    impl ProjectLookup for MemoryHistory {
        fn find_project(&self, locator: &ProjectLocator) -> Result<Option<Uuid>> {
            if locator.as_path().ends_with("project") {
                Ok(self.project_id)
            } else {
                Ok(None)
            }
        }
    }

    impl ProjectEventSource for MemoryHistory {
        fn recent_project_events(
            &self,
            _project_id: Uuid,
            evidence_kind: EvidenceKind,
            limit: usize,
        ) -> Result<Vec<HistoryEvent>> {
            self.requested_limit.set(limit);
            Ok(self
                .events
                .iter()
                .filter(|event| event.evidence_kind == evidence_kind)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    impl ExperienceSource for MemoryHistory {
        fn scoped_experiences(
            &self,
            scope: ScopeKey,
            limit: usize,
        ) -> Result<Vec<ExperienceCapsule>> {
            Ok(self
                .capsules
                .iter()
                .filter(|capsule| ScopeKey::for_capsule(capsule).unwrap() == scope)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    const NOW_MS: i64 = 1_776_254_400_000;

    fn now() -> DateTime<Utc> {
        Utc.timestamp_millis_opt(NOW_MS).unwrap()
    }

    fn project() -> ProjectLocator {
        ProjectLocator::new(PathBuf::from("/private/project"))
    }

    fn event(id: u128, operation: Operation, outcome: Outcome) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc
                .timestamp_millis_opt(NOW_MS - 3_600_000 + id as i64)
                .unwrap(),
            session_id: None,
            project_id: Some(Uuid::from_u128(7)),
            agent: None,
            evidence_kind: EvidenceKind::LearnedPractice,
            task: Some(TaskKind::Testing),
            capability: Capability::Test,
            operation,
            strategy: Some(Strategy::TargetedVerification),
            outcome,
            duration_ms: None,
            error_class: None,
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    fn capsule(id: u128, scope: MemoryScope, outcomes: &[SemanticOutcome]) -> ExperienceCapsule {
        let at = now() - chrono::Duration::days(1);
        ExperienceCapsule {
            id: Uuid::from_u128(id),
            project_id: Uuid::from_u128(7),
            scope,
            scope_id: (scope == MemoryScope::Project).then(|| Uuid::from_u128(7)),
            task: TaskKind::Debugging,
            situation: Some(
                Situation::new("Generated native artifact where parser acceptance is weak.")
                    .unwrap(),
            ),
            lesson: Some(
                Lesson::new("Change one placement class at a time, then verify natively.").unwrap(),
            ),
            caveat: None,
            procedure: Procedure {
                mutation: Some(MutationMode::IncrementalNativeRegeneration),
                verification: Some(VerificationMode::Native),
                ..Procedure::default()
            },
            applicability: ApplicabilityTags {
                artifact_kind: Some(ArtifactKind::NativeCad),
                phase: Some(Phase::Verify),
                ecosystem: Some(Ecosystem::Kicad),
                ..ApplicabilityTags::default()
            },
            environment: EnvironmentFingerprint::default(),
            lifecycle: MemoryLifecycle::Active,
            evidence: outcomes
                .iter()
                .enumerate()
                .map(|(index, outcome)| EvidenceEntry {
                    at: at + chrono::Duration::minutes(index as i64),
                    outcome: *outcome,
                    failure_reason: None,
                })
                .collect(),
            created_at: at,
            last_confirmed_at: at + chrono::Duration::minutes(outcomes.len() as i64),
            schema_version: EXPERIENCE_SCHEMA_VERSION,
        }
    }

    fn recall(
        history: &MemoryHistory,
        options: RecallOptions,
        context: EphemeralTaskContext<'_>,
    ) -> RecallResult {
        RecallService::new(history)
            .recall_at(&project(), options, context, now())
            .unwrap()
    }

    #[test]
    fn legacy_practice_is_aggregated_without_identifiers() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        history.events = vec![
            event(1, Operation::Command, Outcome::Success),
            event(2, Operation::Command, Outcome::Failure),
            event(3, Operation::ApplyPatch, Outcome::Success),
        ];
        let result = recall(
            &history,
            RecallOptions {
                operation: Some(Operation::Command),
                limit: 1,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::default(),
        );

        assert_eq!(history.requested_limit.get(), MAX_CANDIDATE_EVENTS_PER_KIND);
        assert_eq!(result.experiences.len(), 1);
        let item = &result.experiences[0];
        assert!(item.legacy);
        assert_eq!(item.successes, 2);
        assert_eq!(item.failures, 1);
        assert_eq!(item.guidance, Some(PracticeGuidance::Mixed));
        assert_eq!(item.strength, EvidenceStrength::Moderate);
        assert!(item.lesson.is_none());
        let encoded = serde_json::to_string(&result).unwrap();
        for forbidden in [
            "/private/project",
            &Uuid::from_u128(7).to_string(),
            "\"id\"",
        ] {
            assert!(!encoded.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn hook_activity_is_separate_from_verified_practice() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        for id in 1..=20 {
            let mut unknown = event(id, Operation::Command, Outcome::Unknown);
            unknown.evidence_kind = EvidenceKind::HookExecution;
            unknown.task = None;
            unknown.strategy = None;
            unknown.session_id = Some("PRIVATE_SESSION".to_owned());
            unknown.agent = Some(AgentKind::Codex);
            history.events.push(unknown);
        }
        for id in 21..=22 {
            let mut verified = event(id, Operation::Command, Outcome::Success);
            verified.strategy = Some(Strategy::TargetedVerification);
            history.events.push(verified);
        }

        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::from_query(Some("test verification")),
        );

        assert_eq!(result.experiences.len(), 1);
        assert_eq!(result.experiences[0].procedure, "targeted-verification");
        assert_eq!(result.experiences[0].successes, 2);
        // Two successes are no longer a recommendation.
        assert_eq!(result.experiences[0].guidance, None);
        assert!(result.experiences[0].interval[0] < 0.65);
        assert_eq!(result.hook_summary.sampled_executions, 20);
        assert_eq!(result.hook_summary.unknown, 20);
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("PRIVATE_SESSION")
        );
    }

    #[test]
    fn two_successes_cannot_become_strong_guidance_but_eight_can() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        history.capsules.push(capsule(
            1,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 2],
        ));
        let weak = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(weak.experiences[0].guidance, None);
        assert_eq!(weak.experiences[0].strength, EvidenceStrength::Limited);

        history.capsules[0] = capsule(1, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        let strong = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(
            strong.experiences[0].guidance,
            Some(PracticeGuidance::Prefer)
        );
        assert_eq!(strong.experiences[0].strength, EvidenceStrength::Strong);
        assert!(strong.experiences[0].effective_evidence >= 6.0);

        history.capsules[0] = capsule(1, MemoryScope::Project, &[SemanticOutcome::Failure; 8]);
        let avoid = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(avoid.experiences[0].guidance, Some(PracticeGuidance::Avoid));
    }

    #[test]
    fn stale_evidence_decays_and_recent_contradiction_wins() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut stale = capsule(1, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        for entry in &mut stale.evidence {
            entry.at -= chrono::Duration::days(720);
        }
        stale.created_at -= chrono::Duration::days(720);
        stale.last_confirmed_at -= chrono::Duration::days(720);
        for offset in 0..3 {
            stale.confirm(EvidenceEntry {
                at: now() - chrono::Duration::hours(offset),
                outcome: SemanticOutcome::Failure,
                failure_reason: Some(FailureReason::VersionMismatch),
            });
        }
        history.capsules.push(stale);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        let item = &result.experiences[0];
        assert_eq!(item.successes, 8);
        assert_eq!(item.failures, 3);
        assert!(item.posterior < 0.5, "posterior {}", item.posterior);
        assert_ne!(item.guidance, Some(PracticeGuidance::Prefer));
        assert_eq!(item.failure_reason, Some(FailureReason::VersionMismatch));
    }

    #[test]
    fn explicit_metadata_outranks_keyword_inference() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        history.capsules.push(capsule(
            1,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 3],
        ));
        let mut testing = capsule(2, MemoryScope::Project, &[SemanticOutcome::Success; 3]);
        testing.task = TaskKind::Testing;
        history.capsules.push(testing);

        let roomy = RecallOptions {
            token_budget: MAX_TOKEN_BUDGET,
            ..RecallOptions::default()
        };
        let inferred = recall(
            &history,
            roomy,
            EphemeralTaskContext::from_query(Some("debug the flaky test")),
        );
        assert_eq!(inferred.experiences.len(), 2);

        let explicit = recall(
            &history,
            roomy,
            EphemeralTaskContext {
                query: Some("debug the flaky test"),
                task: Some(TaskKind::Testing),
                ..EphemeralTaskContext::default()
            },
        );
        assert_eq!(explicit.experiences.len(), 1);
        assert_eq!(explicit.experiences[0].task, TaskKind::Testing);
    }

    #[test]
    fn paraphrased_queries_retrieve_the_same_capsule_set() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        history.capsules.push(capsule(
            1,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 3],
        ));
        let first = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::from_query(Some("debugging generated kicad pcb drc")),
        );
        let second = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::from_query(Some(
                "fix the broken schematic footprint verification bug",
            )),
        );
        assert_eq!(first.experiences.len(), 1);
        assert_eq!(second.experiences.len(), 1);
        assert_eq!(first.experiences[0].lesson, second.experiences[0].lesson);
        assert_eq!(
            first.experiences[0].procedure,
            second.experiences[0].procedure
        );
        let unrelated = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::from_query(Some("write release notes documentation")),
        );
        assert!(unrelated.experiences.is_empty());
    }

    #[test]
    fn project_scope_beats_global_at_equal_applicability() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut global = capsule(1, MemoryScope::Global, &[SemanticOutcome::Success; 4]);
        global.scope_id = None;
        global.lesson =
            Some(Lesson::new("Regenerate everything in bulk and verify once at the end.").unwrap());
        global.procedure = Procedure {
            mutation: Some(MutationMode::BulkChange),
            ..Procedure::default()
        };
        history.capsules.push(global);
        history.capsules.push(capsule(
            2,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 4],
        ));
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(result.experiences.len(), 2);
        assert_eq!(result.experiences[0].scope, MemoryScope::Project);
        assert_eq!(result.experiences[1].scope, MemoryScope::Global);
    }

    #[test]
    fn broader_scopes_are_consulted_only_when_local_evidence_is_sparse() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut global = capsule(1, MemoryScope::Global, &[SemanticOutcome::Success; 4]);
        global.scope_id = None;
        history.capsules.push(global);
        for id in 2..=5 {
            let mut local = capsule(id, MemoryScope::Project, &[SemanticOutcome::Success; 2]);
            local.procedure.steps = vec![ProcedureStep::Inspect; (id - 1) as usize];
            history.capsules.push(local);
        }
        let result = recall(
            &history,
            RecallOptions {
                limit: MAX_RECALL_LIMIT,
                token_budget: MAX_TOKEN_BUDGET,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::default(),
        );
        assert!(
            result
                .experiences
                .iter()
                .all(|item| item.scope == MemoryScope::Project)
        );
    }

    use crate::core::ProcedureStep;

    #[test]
    fn other_projects_private_capsules_never_surface() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut foreign = capsule(1, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        foreign.project_id = Uuid::from_u128(8);
        foreign.scope_id = Some(Uuid::from_u128(8));
        foreign.lesson =
            Some(Lesson::new("FOREIGN LESSON must never leak across projects.").unwrap());
        history.capsules.push(foreign);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert!(result.experiences.is_empty());
        assert!(!serde_json::to_string(&result).unwrap().contains("FOREIGN"));
    }

    #[test]
    fn superseded_and_obsolete_capsules_are_excluded_and_challenged_are_flagged() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut superseded = capsule(1, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        superseded.lifecycle = MemoryLifecycle::Superseded;
        let mut obsolete = capsule(2, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        obsolete.lifecycle = MemoryLifecycle::Obsolete;
        obsolete.procedure.steps = vec![ProcedureStep::Rollback];
        let mut challenged = capsule(3, MemoryScope::Project, &[SemanticOutcome::Success; 8]);
        challenged.lifecycle = MemoryLifecycle::Challenged;
        challenged.procedure.steps = vec![ProcedureStep::Inspect];
        history.capsules.extend([superseded, obsolete, challenged]);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(result.experiences.len(), 1);
        assert!(result.experiences[0].challenged);
        assert_ne!(
            result.experiences[0].guidance,
            Some(PracticeGuidance::Prefer)
        );
    }

    #[test]
    fn equivalent_capsules_cluster_before_aggregation() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        history.capsules.push(capsule(
            1,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 3],
        ));
        let mut twin = capsule(2, MemoryScope::Project, &[SemanticOutcome::Success; 3]);
        twin.lesson = Some(
            Lesson::new("Change a single placement class at a time and verify natively.").unwrap(),
        );
        history.capsules.push(twin);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(result.experiences.len(), 1);
        assert_eq!(result.experiences[0].successes, 6);
        assert_eq!(
            result.experiences[0].guidance,
            Some(PracticeGuidance::Prefer)
        );
    }

    #[test]
    fn rejects_enumeration_sized_limits_before_querying_storage() {
        let history = MemoryHistory::new(Some(Uuid::nil()));
        let error = RecallService::new(&history)
            .recall(
                &project(),
                RecallOptions {
                    limit: MAX_RECALL_LIMIT + 1,
                    ..RecallOptions::default()
                },
                EphemeralTaskContext::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("recall limit"));
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn unknown_project_returns_no_observations() {
        let history = MemoryHistory::new(None);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert!(result.experiences.is_empty());
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn hard_limit_and_token_budget_survive_many_distinct_groups() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        for id in 0..30_u128 {
            let mut item = capsule(id + 1, MemoryScope::Project, &[SemanticOutcome::Success; 3]);
            item.procedure.steps =
                vec![ProcedureStep::ALL[(id % 11) as usize]; 1 + (id / 11) as usize];
            history.capsules.push(item);
        }
        let result = recall(
            &history,
            RecallOptions {
                limit: MAX_RECALL_LIMIT,
                token_budget: MAX_TOKEN_BUDGET,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::default(),
        );
        assert!(result.experiences.len() <= MAX_RECALL_LIMIT);
        assert!(result.approximate_tokens <= MAX_TOKEN_BUDGET);
        let tight = recall(
            &history,
            RecallOptions {
                token_budget: 120,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::from_query(Some("debug SUPER_SECRET_TASK_TOKEN")),
        );
        assert!(tight.approximate_tokens <= 120);
        assert!(
            !serde_json::to_string(&tight)
                .unwrap()
                .contains("SUPER_SECRET_TASK_TOKEN")
        );
    }

    #[test]
    fn rejects_oversized_token_budgets_before_querying_storage() {
        let history = MemoryHistory::new(Some(Uuid::nil()));
        let error = RecallService::new(&history)
            .recall(
                &project(),
                RecallOptions {
                    token_budget: MAX_TOKEN_BUDGET + 1,
                    ..RecallOptions::default()
                },
                EphemeralTaskContext::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("token budget"));
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn failures_filter_selects_failed_practice_not_tool_executions() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        let mut hook = event(1, Operation::ApplyPatch, Outcome::Success);
        hook.evidence_kind = EvidenceKind::HookExecution;
        hook.task = None;
        hook.strategy = Some(Strategy::DirectTextMutation);
        let mut failed_practice = event(2, Operation::ApplyPatch, Outcome::Failure);
        failed_practice.task = Some(TaskKind::Documentation);
        failed_practice.strategy = Some(Strategy::DirectTextMutation);
        history.events = vec![hook, failed_practice];
        history.capsules.push(capsule(
            3,
            MemoryScope::Project,
            &[SemanticOutcome::Success; 3],
        ));

        let result = recall(
            &history,
            RecallOptions {
                failures_only: true,
                ..RecallOptions::default()
            },
            EphemeralTaskContext::default(),
        );

        assert_eq!(result.experiences.len(), 1);
        assert_eq!(result.experiences[0].failures, 1);
        assert_eq!(result.experiences[0].task, TaskKind::Documentation);
        assert_eq!(result.experiences[0].common_error, None);
        assert_eq!(result.hook_summary.reported_successes, 1);
    }

    #[test]
    fn context_bearing_capsules_rank_above_equally_relevant_legacy_aggregates() {
        let mut history = MemoryHistory::new(Some(Uuid::from_u128(7)));
        for id in 1..=4 {
            let mut legacy = event(id, Operation::ApplyPatch, Outcome::Success);
            legacy.task = Some(TaskKind::Debugging);
            legacy.strategy = Some(Strategy::IncrementalNativeRegeneration);
            history.events.push(legacy);
        }
        let mut modern = capsule(9, MemoryScope::Project, &[SemanticOutcome::Success; 4]);
        modern.applicability = ApplicabilityTags::default();
        modern.procedure = Procedure {
            mutation: Some(MutationMode::StructuredPatch),
            ..Procedure::default()
        };
        history.capsules.push(modern);
        let result = recall(
            &history,
            RecallOptions::default(),
            EphemeralTaskContext::default(),
        );
        assert_eq!(result.experiences.len(), 2);
        assert!(!result.experiences[0].legacy);
        assert!(result.experiences[1].legacy);
    }
}
