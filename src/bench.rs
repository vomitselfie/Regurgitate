//! Paired cold/warm benchmark summary. Input is one JSON object per line;
//! see `benchmarks/README.md` for the protocol and field definitions.

use std::io::BufRead;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedRun {
    pub task: String,
    #[serde(default)]
    pub memory_relevant: bool,
    pub cold: RunMetrics,
    pub warm: RunMetrics,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunMetrics {
    pub total_tokens: u64,
    #[serde(default)]
    pub recall_tokens: u64,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub failed_actions: u64,
    #[serde(default)]
    pub seconds: f64,
    /// Final correctness in [0, 1].
    pub correctness: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchReport {
    pub runs: usize,
    pub memory_relevant_runs: usize,
    pub median_token_savings: f64,
    pub median_token_savings_relevant: f64,
    pub median_token_roi: f64,
    pub mean_retry_reduction: f64,
    pub mean_correctness_delta: f64,
    pub mean_recall_overhead_irrelevant: f64,
    pub gate: ReleaseGate,
}

/// The v0.8 release gate from the direction brief.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseGate {
    pub no_correctness_regression: bool,
    pub positive_median_savings_on_relevant_tasks: bool,
    pub fewer_failed_actions: bool,
    pub near_zero_overhead_on_irrelevant_tasks: bool,
    pub passed: bool,
}

/// Irrelevant tasks may cost at most this many recall tokens on average.
const IRRELEVANT_OVERHEAD_TOKENS: f64 = 40.0;

pub fn parse_runs<R: BufRead>(reader: R) -> Result<Vec<PairedRun>> {
    let mut runs = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.context("could not read benchmark runs")?;
        if line.trim().is_empty() {
            continue;
        }
        let run: PairedRun = serde_json::from_str(&line)
            .with_context(|| format!("invalid benchmark run on line {}", index + 1))?;
        for metrics in [run.cold, run.warm] {
            if !(0.0..=1.0).contains(&metrics.correctness) {
                bail!("correctness on line {} must be within [0, 1]", index + 1);
            }
        }
        runs.push(run);
    }
    Ok(runs)
}

pub fn summarize(runs: &[PairedRun]) -> Result<BenchReport> {
    if runs.is_empty() {
        bail!("no benchmark runs supplied");
    }
    let savings: Vec<f64> = runs.iter().map(token_savings).collect();
    let relevant: Vec<&PairedRun> = runs.iter().filter(|run| run.memory_relevant).collect();
    let irrelevant: Vec<&PairedRun> = runs.iter().filter(|run| !run.memory_relevant).collect();
    let relevant_savings: Vec<f64> = relevant.iter().map(|run| token_savings(run)).collect();
    let roi: Vec<f64> = runs.iter().map(token_roi).collect();
    let retry_reduction = mean(runs.iter().map(|run| {
        (run.cold.failed_actions as f64 - run.warm.failed_actions as f64)
            / (run.cold.failed_actions as f64).max(1.0)
    }));
    let correctness_delta = mean(
        runs.iter()
            .map(|run| run.warm.correctness - run.cold.correctness),
    );
    let overhead = if irrelevant.is_empty() {
        0.0
    } else {
        mean(irrelevant.iter().map(|run| run.warm.recall_tokens as f64))
    };
    let median_relevant = if relevant_savings.is_empty() {
        0.0
    } else {
        median(&relevant_savings)
    };
    let gate = ReleaseGate {
        no_correctness_regression: correctness_delta >= -1e-9,
        positive_median_savings_on_relevant_tasks: !relevant.is_empty() && median_relevant > 0.0,
        fewer_failed_actions: retry_reduction >= 0.0,
        near_zero_overhead_on_irrelevant_tasks: overhead <= IRRELEVANT_OVERHEAD_TOKENS,
        passed: false,
    };
    let passed = gate.no_correctness_regression
        && gate.positive_median_savings_on_relevant_tasks
        && gate.fewer_failed_actions
        && gate.near_zero_overhead_on_irrelevant_tasks;
    Ok(BenchReport {
        runs: runs.len(),
        memory_relevant_runs: relevant.len(),
        median_token_savings: median(&savings),
        median_token_savings_relevant: median_relevant,
        median_token_roi: median(&roi),
        mean_retry_reduction: retry_reduction,
        mean_correctness_delta: correctness_delta,
        mean_recall_overhead_irrelevant: overhead,
        gate: ReleaseGate { passed, ..gate },
    })
}

/// `T_total,cold − T_total,warm − T_recall`
fn token_savings(run: &PairedRun) -> f64 {
    run.cold.total_tokens as f64 - run.warm.total_tokens as f64 - run.warm.recall_tokens as f64
}

/// Net tokens avoided per recall token.
fn token_roi(run: &PairedRun) -> f64 {
    token_savings(run) / (run.warm.recall_tokens as f64).max(1.0)
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0usize), |(sum, count), value| {
        (sum + value, count + 1)
    });
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNS: &str = r#"
{"task":"trap-a","memory_relevant":true,"cold":{"total_tokens":9000,"tool_calls":30,"failed_actions":3,"seconds":120,"correctness":1.0},"warm":{"total_tokens":6000,"recall_tokens":200,"tool_calls":18,"failed_actions":1,"seconds":80,"correctness":1.0}}
{"task":"trap-b","memory_relevant":true,"cold":{"total_tokens":5000,"failed_actions":2,"correctness":0.5},"warm":{"total_tokens":4200,"recall_tokens":150,"failed_actions":1,"correctness":1.0}}
{"task":"unrelated","cold":{"total_tokens":3000,"correctness":1.0},"warm":{"total_tokens":3010,"recall_tokens":10,"correctness":1.0}}
"#;

    #[test]
    fn computes_gate_metrics_from_paired_runs() {
        let runs = parse_runs(RUNS.as_bytes()).unwrap();
        let report = summarize(&runs).unwrap();
        assert_eq!(report.runs, 3);
        assert_eq!(report.memory_relevant_runs, 2);
        assert_eq!(report.median_token_savings_relevant, (2800.0 + 650.0) / 2.0);
        assert!(report.median_token_roi > 1.0);
        assert!(report.mean_retry_reduction > 0.0);
        assert!(report.mean_correctness_delta > 0.0);
        assert_eq!(report.mean_recall_overhead_irrelevant, 10.0);
        assert!(report.gate.passed);
    }

    #[test]
    fn regressions_fail_the_gate() {
        let mut runs = parse_runs(RUNS.as_bytes()).unwrap();
        runs[0].warm.correctness = 0.0;
        let report = summarize(&runs).unwrap();
        assert!(!report.gate.no_correctness_regression);
        assert!(!report.gate.passed);
        assert!(summarize(&[]).is_err());
        assert!(parse_runs(r#"{"task":"x","cold":{"total_tokens":1,"correctness":2},"warm":{"total_tokens":1,"correctness":1}}"#.as_bytes()).is_err());
    }
}
