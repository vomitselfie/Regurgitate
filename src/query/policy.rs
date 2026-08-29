//! Every ranking weight, half-life, and threshold lives here so they can be
//! tuned from benchmark data in one place. The coefficients are starting
//! points, not truths; what matters is that contextual applicability ranks
//! before raw popularity and that posterior uncertainty stays visible.

use crate::core::{MemoryLifecycle, MemoryScope};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankingPolicy {
    /// Relevance prior per scope.
    pub scope_weight_project: f64,
    pub scope_weight_workspace: f64,
    pub scope_weight_ecosystem: f64,
    pub scope_weight_machine: f64,
    pub scope_weight_global: f64,
    /// Evidence half-life in days per scope.
    pub half_life_days_project: f64,
    pub half_life_days_workspace: f64,
    pub half_life_days_ecosystem: f64,
    pub half_life_days_machine: f64,
    pub half_life_days_global: f64,
    /// Applicability term weights; must sum to one.
    pub applicability_task: f64,
    pub applicability_artifact: f64,
    pub applicability_ecosystem: f64,
    pub applicability_phase: f64,
    pub applicability_environment: f64,
    /// Lifecycle multipliers for normal recall.
    pub lifecycle_active: f64,
    pub lifecycle_challenged: f64,
    /// Uniform Beta prior.
    pub prior_alpha: f64,
    pub prior_beta: f64,
    /// Two-sided credible mass used for the guidance interval.
    pub credible_mass: f64,
    pub prefer_threshold: f64,
    pub avoid_threshold: f64,
    /// Kish effective sample size required before a strong label.
    pub min_effective_evidence: f64,
    /// Effective evidence at which guidance is reported as strong.
    pub strong_effective_evidence: f64,
    /// Final ranking weights; must sum to one.
    pub rank_applicability: f64,
    pub rank_guidance: f64,
    pub rank_confidence: f64,
    pub rank_recency: f64,
    pub rank_scope: f64,
    /// Small additive bonus for capsules that carry situation/lesson text so
    /// they outrank equally relevant legacy aggregates.
    pub rank_context_bonus: f64,
    /// Active project-scope candidates below which retrieval expands outward.
    pub sparse_local_evidence: usize,
    /// Minimum applicability for a capsule from a broader scope to surface.
    pub broader_scope_min_applicability: f64,
    /// Minimum applicability for any capsule to surface at all.
    pub min_applicability: f64,
}

impl Default for RankingPolicy {
    fn default() -> Self {
        Self {
            scope_weight_project: 1.00,
            scope_weight_workspace: 0.85,
            scope_weight_ecosystem: 0.65,
            scope_weight_machine: 0.55,
            scope_weight_global: 0.35,
            half_life_days_project: 120.0,
            half_life_days_workspace: 120.0,
            half_life_days_ecosystem: 180.0,
            half_life_days_machine: 60.0,
            half_life_days_global: 180.0,
            applicability_task: 0.35,
            applicability_artifact: 0.25,
            applicability_ecosystem: 0.20,
            applicability_phase: 0.10,
            applicability_environment: 0.10,
            lifecycle_active: 1.0,
            lifecycle_challenged: 0.35,
            prior_alpha: 1.0,
            prior_beta: 1.0,
            credible_mass: 0.80,
            prefer_threshold: 0.65,
            avoid_threshold: 0.35,
            min_effective_evidence: 2.5,
            strong_effective_evidence: 6.0,
            rank_applicability: 0.45,
            rank_guidance: 0.25,
            rank_confidence: 0.15,
            rank_recency: 0.10,
            rank_scope: 0.05,
            rank_context_bonus: 0.02,
            sparse_local_evidence: 3,
            broader_scope_min_applicability: 0.6,
            min_applicability: 0.2,
        }
    }
}

impl RankingPolicy {
    pub fn scope_weight(&self, scope: MemoryScope) -> f64 {
        match scope {
            MemoryScope::Project => self.scope_weight_project,
            MemoryScope::Workspace => self.scope_weight_workspace,
            MemoryScope::Ecosystem => self.scope_weight_ecosystem,
            MemoryScope::Machine => self.scope_weight_machine,
            MemoryScope::Global => self.scope_weight_global,
        }
    }

    pub fn half_life_days(&self, scope: MemoryScope) -> f64 {
        match scope {
            MemoryScope::Project => self.half_life_days_project,
            MemoryScope::Workspace => self.half_life_days_workspace,
            MemoryScope::Ecosystem => self.half_life_days_ecosystem,
            MemoryScope::Machine => self.half_life_days_machine,
            MemoryScope::Global => self.half_life_days_global,
        }
    }

    pub fn lifecycle_weight(&self, lifecycle: MemoryLifecycle) -> f64 {
        match lifecycle {
            MemoryLifecycle::Active => self.lifecycle_active,
            MemoryLifecycle::Challenged => self.lifecycle_challenged,
            MemoryLifecycle::Superseded | MemoryLifecycle::Obsolete => 0.0,
        }
    }

    /// `exp(-ln 2 · age / H)`; one at age zero, one half after one half-life.
    pub fn age_weight(&self, scope: MemoryScope, age_days: f64) -> f64 {
        let age_days = age_days.max(0.0);
        (-(std::f64::consts::LN_2) * age_days / self.half_life_days(scope)).exp()
    }
}

/// Weighted Beta-Binomial summary for one cluster of evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Posterior {
    pub mean: f64,
    pub lower: f64,
    pub upper: f64,
    pub effective_evidence: f64,
    pub weighted_successes: f64,
    pub weighted_failures: f64,
}

impl Posterior {
    /// `weights` pairs each evidence weight with whether it was a success.
    pub fn from_weighted(policy: &RankingPolicy, weights: &[(f64, bool)]) -> Self {
        Self::from_weighted_with_effective_evidence(policy, weights, None)
    }

    pub(crate) fn from_weighted_with_effective_evidence(
        policy: &RankingPolicy,
        weights: &[(f64, bool)],
        effective_evidence: Option<f64>,
    ) -> Self {
        let mut successes = 0.0;
        let mut failures = 0.0;
        let mut sum = 0.0;
        let mut sum_squares = 0.0;
        for &(weight, success) in weights {
            if weight <= 0.0 {
                continue;
            }
            if success {
                successes += weight;
            } else {
                failures += weight;
            }
            sum += weight;
            sum_squares += weight * weight;
        }
        let alpha = policy.prior_alpha + successes;
        let beta = policy.prior_beta + failures;
        let tail = (1.0 - policy.credible_mass) / 2.0;
        Self {
            mean: alpha / (alpha + beta),
            lower: beta_quantile(tail, alpha, beta),
            upper: beta_quantile(1.0 - tail, alpha, beta),
            effective_evidence: effective_evidence.unwrap_or_else(|| {
                if sum_squares > 0.0 {
                    sum * sum / sum_squares
                } else {
                    0.0
                }
            }),
            weighted_successes: successes,
            weighted_failures: failures,
        }
    }
}

/// Quantile of Beta(a, b) by bisection on the regularized incomplete beta
/// function. Accurate to roughly 1e-9, which is far below anything the
/// ranking can distinguish.
pub fn beta_quantile(probability: f64, a: f64, b: f64) -> f64 {
    if probability <= 0.0 {
        return 0.0;
    }
    if probability >= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for _ in 0..100 {
        let mid = 0.5 * (low + high);
        if regularized_incomplete_beta(mid, a, b) < probability {
            low = mid;
        } else {
            high = mid;
        }
        if high - low < 1e-12 {
            break;
        }
    }
    0.5 * (low + high)
}

/// `I_x(a, b)` via the Lentz continued fraction (Numerical Recipes, betacf).
pub fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let log_front = ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln();
    let front = log_front.exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * continued_fraction(x, a, b) / a
    } else {
        1.0 - front * continued_fraction(1.0 - x, b, a) / b
    }
}

fn continued_fraction(x: f64, a: f64, b: f64) -> f64 {
    const MAX_ITERATIONS: usize = 300;
    const EPSILON: f64 = 1e-14;
    const TINY: f64 = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITERATIONS {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < EPSILON {
            break;
        }
    }
    h
}

/// Lanczos approximation (g = 7, n = 9), accurate to ~1e-15 for x > 0.
fn ln_gamma(x: f64) -> f64 {
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = COEFFICIENTS[0];
    let t = x + 7.5;
    for (index, coefficient) in COEFFICIENTS.iter().enumerate().skip(1) {
        sum += coefficient / (x + index as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64, tolerance: f64) -> bool {
        (left - right).abs() <= tolerance
    }

    #[test]
    fn weights_sum_to_one() {
        let policy = RankingPolicy::default();
        let applicability = policy.applicability_task
            + policy.applicability_artifact
            + policy.applicability_ecosystem
            + policy.applicability_phase
            + policy.applicability_environment;
        let rank = policy.rank_applicability
            + policy.rank_guidance
            + policy.rank_confidence
            + policy.rank_recency
            + policy.rank_scope;
        assert!(close(applicability, 1.0, 1e-9));
        assert!(close(rank, 1.0, 1e-9));
    }

    #[test]
    fn age_weight_halves_per_half_life() {
        let policy = RankingPolicy::default();
        assert!(close(
            policy.age_weight(MemoryScope::Project, 0.0),
            1.0,
            1e-12
        ));
        assert!(close(
            policy.age_weight(MemoryScope::Project, 120.0),
            0.5,
            1e-9
        ));
        assert!(close(
            policy.age_weight(MemoryScope::Machine, 120.0),
            0.25,
            1e-9
        ));
        assert!(close(
            policy.age_weight(MemoryScope::Global, -5.0),
            1.0,
            1e-12
        ));
    }

    #[test]
    fn incomplete_beta_matches_known_values() {
        // Uniform: I_x(1,1) = x.
        assert!(close(
            regularized_incomplete_beta(0.3, 1.0, 1.0),
            0.3,
            1e-12
        ));
        // I_x(2,1) = x^2.
        assert!(close(
            regularized_incomplete_beta(0.5, 2.0, 1.0),
            0.25,
            1e-12
        ));
        // I_x(1,3) = 1 - (1-x)^3.
        assert!(close(
            regularized_incomplete_beta(0.5, 1.0, 3.0),
            0.875,
            1e-12
        ));
        // Symmetric: I_0.5(3,3) = 0.5.
        assert!(close(
            regularized_incomplete_beta(0.5, 3.0, 3.0),
            0.5,
            1e-12
        ));
        // I_0.2(2.5, 4.5) ≈ 0.196393 (numerical integration).
        assert!(close(
            regularized_incomplete_beta(0.2, 2.5, 4.5),
            0.196_393,
            1e-5
        ));
    }

    #[test]
    fn quantiles_invert_the_cdf() {
        for (a, b) in [(1.0, 1.0), (3.0, 1.0), (1.5, 4.0), (8.0, 2.0), (2.2, 2.2)] {
            for probability in [0.05, 0.1, 0.5, 0.9, 0.95] {
                let x = beta_quantile(probability, a, b);
                assert!(close(
                    regularized_incomplete_beta(x, a, b),
                    probability,
                    1e-8
                ));
            }
        }
        assert!(close(beta_quantile(0.5, 3.0, 1.0), 0.5_f64.cbrt(), 1e-9));
    }

    #[test]
    fn two_successes_leave_a_wide_interval_and_little_evidence() {
        let policy = RankingPolicy::default();
        let posterior = Posterior::from_weighted(&policy, &[(1.0, true), (1.0, true)]);
        assert!(close(posterior.mean, 0.75, 1e-12));
        assert!(close(posterior.effective_evidence, 2.0, 1e-12));
        // Beta(3,1): 10% quantile is 0.1^(1/3) ≈ 0.464.
        assert!(posterior.lower < policy.prefer_threshold);
        assert!(posterior.effective_evidence < policy.min_effective_evidence);
    }

    #[test]
    fn many_tiny_weights_do_not_masquerade_as_evidence() {
        let policy = RankingPolicy::default();
        let stale: Vec<(f64, bool)> = (0..100).map(|_| (0.02, true)).collect();
        let posterior = Posterior::from_weighted(&policy, &stale);
        // Kish n_eff of 100 equal weights is 100, but the posterior itself
        // only saw two pseudo-observations.
        assert!(close(posterior.weighted_successes, 2.0, 1e-9));
        assert!(posterior.lower < policy.prefer_threshold);

        let mut mixed = vec![(1.0, true)];
        mixed.extend((0..50).map(|_| (0.01, true)));
        let posterior = Posterior::from_weighted(&policy, &mixed);
        // One real observation plus a long tail of stale ones stays below the
        // strong-guidance gate.
        assert!(posterior.effective_evidence < policy.min_effective_evidence);
        assert!(posterior.lower < policy.prefer_threshold);
    }

    #[test]
    fn strong_consistent_evidence_clears_the_prefer_gate() {
        let policy = RankingPolicy::default();
        let evidence: Vec<(f64, bool)> = (0..8).map(|_| (1.0, true)).collect();
        let posterior = Posterior::from_weighted(&policy, &evidence);
        assert!(posterior.lower >= policy.prefer_threshold);
        assert!(posterior.effective_evidence >= policy.min_effective_evidence);
        let failures: Vec<(f64, bool)> = (0..8).map(|_| (1.0, false)).collect();
        let posterior = Posterior::from_weighted(&policy, &failures);
        assert!(posterior.upper <= policy.avoid_threshold);
    }
}
