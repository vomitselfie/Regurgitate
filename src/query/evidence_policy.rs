//! Evidence quality and independence policy, separate from task relevance.

use std::collections::HashMap;

use crate::core::{
    AgentKind, EnvironmentFingerprint, EvidenceAttestation, EvidenceEntry, EvidenceSource,
    EvidenceVerification, SemanticOutcome,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct WeightedObservation {
    pub base_weight: f64,
    pub evidence: EvidenceEntry,
}

pub(crate) struct EvidenceSummary {
    pub weighted_outcomes: Vec<(f64, bool)>,
    pub effective_evidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CohortKey {
    Explicit([u8; 32]),
    Inferred {
        utc_day: i64,
        source: EvidenceSource,
        agent: Option<AgentKind>,
        environment: EnvironmentFingerprint,
    },
}

/// Collapses correlated observations to at most the strongest observation in
/// their cohort. Raw success/failure counts remain available separately; this
/// policy controls only posterior mass and effective evidence.
pub(crate) fn summarize(observations: &[WeightedObservation]) -> EvidenceSummary {
    let mut groups: HashMap<CohortKey, Vec<(f64, bool)>> = HashMap::new();
    for observation in observations {
        let evidence = observation.evidence;
        let key = evidence
            .cohort
            .map(CohortKey::Explicit)
            .unwrap_or_else(|| CohortKey::Inferred {
                utc_day: evidence.at.timestamp().div_euclid(86_400),
                source: evidence.source,
                agent: evidence.agent,
                environment: evidence.environment,
            });
        let weight = observation.base_weight.max(0.0) * quality_weight(evidence);
        groups
            .entry(key)
            .or_default()
            .push((weight, evidence.outcome == SemanticOutcome::Success));
    }

    let mut weighted_outcomes = Vec::with_capacity(observations.len());
    let mut group_weights = Vec::with_capacity(groups.len());
    for group in groups.into_values() {
        let sum: f64 = group.iter().map(|(weight, _)| *weight).sum();
        let cap = group
            .iter()
            .map(|(weight, _)| *weight)
            .fold(0.0_f64, f64::max);
        if sum <= 0.0 || cap <= 0.0 {
            continue;
        }
        let scale = (cap / sum).min(1.0);
        weighted_outcomes.extend(
            group
                .into_iter()
                .map(|(weight, success)| (weight * scale, success)),
        );
        group_weights.push(cap);
    }

    let sum: f64 = group_weights.iter().sum();
    let squares: f64 = group_weights.iter().map(|weight| weight * weight).sum();
    let kish = if squares > 0.0 {
        sum * sum / squares
    } else {
        0.0
    };
    EvidenceSummary {
        weighted_outcomes,
        // Absolute evidence mass matters as well as diversity: many tiny,
        // stale, incompatible, or self-reported observations cannot receive a
        // strong label solely because their relative weights are equal.
        effective_evidence: kish.min(sum),
    }
}

fn quality_weight(evidence: EvidenceEntry) -> f64 {
    let attestation = match evidence.attestation {
        EvidenceAttestation::SelfReported => 0.75,
        EvidenceAttestation::HostAttested => 1.0,
        EvidenceAttestation::HumanAttested => 1.1,
    };
    let verification = match evidence.verification {
        EvidenceVerification::None => 1.0,
        EvidenceVerification::Targeted => 1.05,
        EvidenceVerification::Full | EvidenceVerification::Native => 1.1,
    };
    attestation * verification
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::core::{EnvironmentFingerprint, SemanticOutcome};

    use super::*;

    fn observation(day: u32, cohort: Option<[u8; 32]>) -> WeightedObservation {
        let mut evidence = EvidenceEntry::agent_reported(
            Utc.with_ymd_and_hms(2026, 8, day, 12, 0, 0).unwrap(),
            SemanticOutcome::Success,
            None,
            EnvironmentFingerprint::default(),
        );
        evidence.cohort = cohort;
        WeightedObservation {
            base_weight: 1.0,
            evidence,
        }
    }

    #[test]
    fn correlated_observations_contribute_one_effective_trial() {
        let observations = (0..10)
            .map(|_| observation(20, Some([7; 32])))
            .collect::<Vec<_>>();
        let summary = summarize(&observations);
        assert!((summary.effective_evidence - 0.75).abs() < 1e-9);
        assert!(
            (summary
                .weighted_outcomes
                .iter()
                .map(|(weight, _)| weight)
                .sum::<f64>()
                - 0.75)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn explicit_cohorts_and_separate_days_are_independent() {
        let explicit = (0..8)
            .map(|index| observation(20, Some([index; 32])))
            .collect::<Vec<_>>();
        assert!((summarize(&explicit).effective_evidence - 6.0).abs() < 1e-9);

        let inferred = vec![
            observation(20, None),
            observation(20, None),
            observation(21, None),
        ];
        assert!((summarize(&inferred).effective_evidence - 1.5).abs() < 1e-9);
    }
}
