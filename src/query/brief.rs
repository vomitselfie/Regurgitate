//! Plain-text experience brief for host preflight injection.

use anyhow::Result;
use serde::Serialize;

use crate::application::ProjectLocator;

use super::{
    EphemeralTaskContext, EvidenceStrength, ExperienceSource, MAX_RECALL_LIMIT, PracticeGuidance,
    ProjectEventSource, ProjectLookup, RecallOptions, RecallResult, RecallService,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExperienceBrief {
    pub text: String,
    pub items: usize,
    /// Eligible lessons that did not fit the budget.
    pub omitted: usize,
    pub approximate_tokens: usize,
}

impl ExperienceBrief {
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            items: 0,
            omitted: 0,
            approximate_tokens: 0,
        }
    }
}

/// Host-neutral preflight port. Adapters call it at the earliest safe point
/// where a task context is known; the result is small enough to inject
/// before substantial reasoning. Unlike `recall`, it only carries lessons
/// with moderate or strong evidence (`n_eff >= min_effective_evidence`):
/// unsolicited context must earn its place, and one unconfirmed capsule
/// must not be pushed into every session.
pub trait RecallBroker {
    fn brief(
        &self,
        project: &ProjectLocator,
        context: EphemeralTaskContext<'_>,
        token_budget: usize,
    ) -> Result<ExperienceBrief>;
}

impl<H> RecallBroker for RecallService<'_, H>
where
    H: ProjectLookup + ProjectEventSource + ExperienceSource,
{
    fn brief(
        &self,
        project: &ProjectLocator,
        context: EphemeralTaskContext<'_>,
        token_budget: usize,
    ) -> Result<ExperienceBrief> {
        // Automatic injection must stay quiet unless the task is recognizable:
        // a prompt that maps to no controlled task gets no brief at all.
        if context.task.is_none() && !context.has_task_hints() {
            return Ok(ExperienceBrief::empty());
        }
        let options = RecallOptions {
            limit: MAX_RECALL_LIMIT.min(8),
            token_budget: token_budget.clamp(32, super::MAX_TOKEN_BUDGET),
            ..RecallOptions::default()
        };
        let mut result = self.recall(project, options, context)?;
        // Weak hints are for an agent that asked; they are not worth
        // unsolicited context.
        result
            .experiences
            .retain(|item| item.strength != EvidenceStrength::Limited);
        Ok(render_brief(&result, token_budget))
    }
}

const HEADER: &str =
    "Relevant prior practice from Regurgitate (historical evidence, not current truth):";

/// Renders ranked items as numbered lines, trimming the lowest-ranked until
/// the approximate token budget is met. Returns an empty brief rather than
/// a header with no items.
pub fn render_brief(result: &RecallResult, token_budget: usize) -> ExperienceBrief {
    let mut count = result.experiences.len();
    loop {
        if count == 0 {
            return ExperienceBrief::empty();
        }
        let mut text = String::from(HEADER);
        for (index, item) in result.experiences.iter().take(count).enumerate() {
            let strength = match item.strength {
                EvidenceStrength::Limited => "limited",
                EvidenceStrength::Moderate => "moderate",
                EvidenceStrength::Strong => "strong",
            };
            let guidance = match item.guidance {
                Some(PracticeGuidance::Prefer) => "; prefer",
                Some(PracticeGuidance::Avoid) => "; avoid",
                Some(PracticeGuidance::Mixed) => "; mixed",
                None => "",
            };
            let body = item
                .lesson
                .clone()
                .unwrap_or_else(|| format!("{} for {}", item.procedure, item.task_label()));
            text.push_str(&format!(
                "\n{}. [{strength} / {}{}] {body} (posterior≈{:.2}; n_eff={:.1}; {}✓ {}✗{guidance})",
                index + 1,
                item.scope,
                if item.challenged { ", challenged" } else { "" },
                item.posterior,
                item.effective_evidence,
                item.successes,
                item.failures,
            ));
            if let Some(situation) = &item.situation {
                text.push_str(&format!("\n   When: {situation}"));
            }
            if let Some(caveat) = &item.caveat {
                text.push_str(&format!("\n   Caveat: {caveat}"));
            }
            if item.lesson.is_none() && item.legacy {
                text.push_str("\n   (legacy aggregate without context)");
            }
        }
        let omitted = result.experiences.len() - count;
        if omitted > 0 {
            text.push_str(&format!(
                "\n({omitted} more relevant lesson{} omitted for budget; `regurgitate recall` lists them)",
                if omitted == 1 { "" } else { "s" }
            ));
        }
        let approximate_tokens = (text.len() + 1).div_ceil(4);
        if approximate_tokens <= token_budget {
            return ExperienceBrief {
                text,
                items: count,
                omitted,
                approximate_tokens,
            };
        }
        count -= 1;
    }
}

impl super::ExperienceBriefItem {
    fn task_label(&self) -> String {
        format!("{:?}", self.task)
            .chars()
            .enumerate()
            .flat_map(|(index, character)| {
                if character.is_uppercase() && index > 0 {
                    vec!['-', character.to_ascii_lowercase()]
                } else {
                    vec![character.to_ascii_lowercase()]
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        core::{MemoryScope, TaskKind},
        query::{ExperienceBriefItem, HookSummary},
    };

    use super::*;

    fn item(index: usize) -> ExperienceBriefItem {
        ExperienceBriefItem {
            scope: MemoryScope::Project,
            task: TaskKind::FeatureImplementation,
            procedure: "structured-patch + targeted-verification".into(),
            situation: Some(format!(
                "Situation number {index} with enough words present."
            )),
            lesson: Some(format!(
                "Lesson number {index} about verifying after each patch."
            )),
            caveat: (index == 0).then(|| "Parser acceptance alone was insufficient.".into()),
            guidance: Some(PracticeGuidance::Prefer),
            strength: EvidenceStrength::Strong,
            posterior: 0.86,
            interval: [0.7, 0.95],
            effective_evidence: 5.2,
            successes: 6,
            failures: 1,
            challenged: false,
            legacy: false,
            failure_reason: None,
            common_error: None,
        }
    }

    #[test]
    fn renders_numbered_lines_within_budget_and_stays_silent_when_empty() {
        let result = RecallResult {
            experiences: (0..5).map(item).collect(),
            omitted: 0,
            hook_summary: HookSummary::default(),
            approximate_tokens: 0,
        };
        let brief = render_brief(&result, 120);
        assert!(brief.items < 5);
        assert!(brief.items >= 1);
        assert_eq!(brief.items + brief.omitted, 5);
        assert!(brief.text.contains("omitted for budget"));
        assert!(brief.approximate_tokens <= 120);
        assert!(brief.text.starts_with(HEADER));
        assert!(brief.text.contains("1. [strong / project] Lesson number 0"));
        assert!(brief.text.contains("Caveat: Parser acceptance"));
        assert!(brief.text.contains("prefer"));

        let empty = RecallResult::empty();
        assert_eq!(render_brief(&empty, 300), ExperienceBrief::empty());
    }

    #[test]
    fn broker_stays_silent_without_a_recognizable_task_or_solid_evidence() {
        use std::path::PathBuf;

        use anyhow::Result;
        use uuid::Uuid;

        use crate::{
            application::ScopeKey,
            core::{EvidenceKind, ExperienceCapsule, HistoryEvent, Outcome, Strategy},
            query::{ProjectEventSource, ProjectLookup, RecallService},
        };

        struct History(Vec<HistoryEvent>);
        impl ProjectLookup for History {
            fn find_project(&self, _: &ProjectLocator) -> Result<Option<Uuid>> {
                Ok(Some(Uuid::from_u128(7)))
            }
        }
        impl ProjectEventSource for History {
            fn recent_project_events(
                &self,
                _: Uuid,
                kind: EvidenceKind,
                _: usize,
            ) -> Result<Vec<HistoryEvent>> {
                Ok(self
                    .0
                    .iter()
                    .filter(|event| event.evidence_kind == kind)
                    .cloned()
                    .collect())
            }
        }
        impl ExperienceSource for History {
            fn scoped_experiences(&self, _: ScopeKey, _: usize) -> Result<Vec<ExperienceCapsule>> {
                Ok(Vec::new())
            }
        }
        let event = |id: u128| HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: chrono::Utc::now(),
            session_id: None,
            project_id: Some(Uuid::from_u128(7)),
            agent: None,
            evidence_kind: EvidenceKind::LearnedPractice,
            task: Some(TaskKind::Testing),
            capability: crate::core::Capability::Test,
            operation: crate::core::Operation::Command,
            strategy: Some(Strategy::TargetedVerification),
            outcome: Outcome::Success,
            duration_ms: None,
            error_class: None,
            schema_version: crate::core::CURRENT_SCHEMA_VERSION,
        };
        let project = ProjectLocator::new(PathBuf::from("/private/project"));

        let weak = History(vec![event(1)]);
        let service = RecallService::new(&weak);
        assert_eq!(
            service
                .brief(
                    &project,
                    EphemeralTaskContext::from_query(Some("run the tests")),
                    220
                )
                .unwrap(),
            ExperienceBrief::empty()
        );

        let strong = History((1..=8).map(event).collect());
        let service = RecallService::new(&strong);
        assert_eq!(
            service
                .brief(
                    &project,
                    EphemeralTaskContext::from_query(Some("write the changelog")),
                    220
                )
                .unwrap(),
            ExperienceBrief::empty()
        );
        let brief = service
            .brief(
                &project,
                EphemeralTaskContext::from_query(Some("run the tests")),
                220,
            )
            .unwrap();
        assert_eq!(brief.items, 1);
        assert!(brief.text.contains("targeted-verification for testing"));
    }

    #[test]
    fn legacy_items_describe_the_procedure_and_task() {
        let mut legacy = item(0);
        legacy.lesson = None;
        legacy.situation = None;
        legacy.caveat = None;
        legacy.legacy = true;
        let result = RecallResult {
            experiences: vec![legacy],
            omitted: 0,
            hook_summary: HookSummary::default(),
            approximate_tokens: 0,
        };
        let brief = render_brief(&result, 300);
        assert!(
            brief
                .text
                .contains("structured-patch + targeted-verification for feature-implementation")
        );
        assert!(brief.text.contains("legacy aggregate"));
    }
}
