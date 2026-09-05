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
/// before substantial reasoning. Prefers moderate or strong evidence, with
/// at most two explicitly unconfirmed lessons when stronger evidence is absent.
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
            token_budget: super::MAX_TOKEN_BUDGET,
            ..RecallOptions::default()
        };
        let mut result = self.recall(project, options, context)?;
        result
            .experiences
            .retain(|item| item.situation.is_some() && item.lesson.is_some());
        // Confirmed lessons are worth unsolicited context; unconfirmed ones
        // are not, except to bootstrap: when nothing stronger exists, show at
        // most two tagged as unconfirmed so the agent can confirm or refute
        // them and evidence can start accumulating.
        let has_confirmed = result
            .experiences
            .iter()
            .any(|item| item.strength != EvidenceStrength::Limited);
        if has_confirmed {
            result
                .experiences
                .retain(|item| item.strength != EvidenceStrength::Limited);
        } else {
            result.experiences.retain(|item| item.reference.is_some());
            result.experiences.truncate(UNCONFIRMED_BOOTSTRAP_ITEMS);
        }
        Ok(render_brief(
            &result,
            token_budget.clamp(32, super::MAX_TOKEN_BUDGET),
        ))
    }
}

/// Unconfirmed lessons shown by preflight when a project has nothing stronger.
pub const UNCONFIRMED_BOOTSTRAP_ITEMS: usize = 2;

const HEADER: &str =
    "Relevant prior practice from Regurgitate (historical evidence, not current truth):";

/// Renders ranked items as numbered lines, trimming the lowest-ranked until
/// the approximate token budget is met. Returns an empty brief rather than
/// a header with no items.
pub fn render_brief(result: &RecallResult, token_budget: usize) -> ExperienceBrief {
    // Context-free aggregates are diagnostic data, not actionable reminders.
    let items: Vec<_> = result
        .experiences
        .iter()
        .filter(|item| item.situation.is_some() && item.lesson.is_some())
        .collect();
    let mut count = items.len();
    loop {
        if count == 0 {
            return ExperienceBrief::empty();
        }
        let mut text = String::from(HEADER);
        for (index, item) in items.iter().take(count).enumerate() {
            let strength = match item.strength {
                EvidenceStrength::Limited => "unconfirmed",
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
                .as_deref()
                .expect("contextual items have lessons");
            text.push_str(&format!(
                "\n{}. [{strength} / {}{}] {body} ({} succeeded, {} failed{guidance}{})",
                index + 1,
                item.scope,
                if item.challenged { ", challenged" } else { "" },
                item.successes,
                item.failures,
                item.reference
                    .as_deref()
                    .map(|reference| format!("; ref {reference}"))
                    .unwrap_or_default(),
            ));
            if let Some(situation) = &item.situation {
                text.push_str(&format!("\n   When: {situation}"));
            }
            if let Some(caveat) = &item.caveat {
                text.push_str(&format!("\n   Caveat: {caveat}"));
            }
            if let Some(reason) = &item.failure_reason {
                text.push_str(&format!("\n   Failure: {reason}"));
            }
        }
        if items
            .iter()
            .take(count)
            .any(|item| item.reference.is_some())
        {
            text.push_str(
                "\nIf applied: `regurgitate experience confirm --match <ref> --outcome success|failure`.",
            );
        }
        let omitted = result.omitted + items.len() - count;
        if omitted > 0 {
            text.push_str(&format!(
                "\n({omitted} more relevant lesson{} omitted for budget.)",
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

#[cfg(test)]
mod tests {
    use crate::{
        core::{MemoryScope, TaskKind},
        query::{ExperienceBriefItem, HookSummary},
    };

    use super::*;

    fn item(index: usize) -> ExperienceBriefItem {
        ExperienceBriefItem {
            reference: Some(format!("{index:012x}")),
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
    fn compact_brief_keeps_caveats_counterevidence_and_receipts_whole() {
        let mut lesson = item(0);
        lesson.reference = Some(format!("r1_{}", "a".repeat(48)));
        lesson.guidance = Some(PracticeGuidance::Avoid);
        lesson.challenged = true;
        lesson.failure_reason = Some(crate::core::FailureReason::VerificationFailed);
        let result = RecallResult {
            status: crate::query::RecallStatus::Matches,
            experiences: vec![lesson.clone()],
            omitted: 0,
            hook_summary: HookSummary {
                sampled_executions: 100,
                reported_successes: 100,
                ..HookSummary::default()
            },
            approximate_tokens: 0,
        };
        let brief = render_brief(&result, 240);
        assert_eq!(brief.items, 1);
        assert!(brief.text.len() < serde_json::to_string(&result).unwrap().len());
        assert!(brief.text.contains(lesson.situation.as_deref().unwrap()));
        assert!(brief.text.contains(lesson.caveat.as_deref().unwrap()));
        assert!(brief.text.contains(lesson.reference.as_deref().unwrap()));
        assert!(brief.text.contains("challenged"));
        assert!(brief.text.contains("avoid"));
        assert!(brief.text.contains("1 failed"));
        assert!(brief.text.contains("verification-failed"));
        assert!(!brief.text.contains("posterior"));
        assert!(!brief.text.contains("100"));
        // Budget pressure drops the whole note; it must not strip the warning.
        assert_eq!(render_brief(&result, 32), ExperienceBrief::empty());
        for budget in 32..=240 {
            let bounded = render_brief(&result, budget);
            assert!(bounded.approximate_tokens <= budget);
            if bounded.items > 0 {
                assert!(bounded.text.contains(lesson.caveat.as_deref().unwrap()));
                assert!(bounded.text.contains(lesson.reference.as_deref().unwrap()));
            }
        }
    }

    #[test]
    fn renders_numbered_lines_within_budget_and_stays_silent_when_empty() {
        let result = RecallResult {
            status: crate::query::RecallStatus::Matches,
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
        // Legacy observations have neither receipts nor independent cohort
        // provenance, so repetition alone cannot earn unsolicited injection.
        assert_eq!(brief, ExperienceBrief::empty());
    }

    #[test]
    fn context_free_legacy_items_do_not_consume_agent_attention() {
        let mut legacy = item(0);
        legacy.lesson = None;
        legacy.situation = None;
        legacy.caveat = None;
        legacy.legacy = true;
        let result = RecallResult {
            status: crate::query::RecallStatus::Matches,
            experiences: vec![legacy],
            omitted: 0,
            hook_summary: HookSummary::default(),
            approximate_tokens: 0,
        };
        let brief = render_brief(&result, 300);
        assert_eq!(brief, ExperienceBrief::empty());
    }
}
