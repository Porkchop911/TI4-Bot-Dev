//! The fixed benchmark protocol (M00-012), and the statistics it requires.
//!
//! Implements the protocol recorded in `plans/evidence/M00-012*.md` rather than inventing one, so
//! a Rust measurement and a Python measurement are comparable by construction:
//!
//! - **30 timed samples** per implementation, none discarded. An outlier is data about the system,
//!   and dropping it is how a benchmark reports the machine somebody wished they had.
//! - **Interleaved in a fixed order**: for pair `i`, Python first when `i` is even and Rust first
//!   when it is odd, both on seed `manifest + i`. Running one implementation to completion and then
//!   the other measures thermal drift as if it were a language difference.
//! - **Monotonic elapsed nanoseconds**, never wall-clock.
//! - **Variance thresholds fixed in advance**, so a noisy result is rejected rather than explained.
//!
//! # The semantic gate is the honest part
//!
//! A speed comparison is only meaningful if both sides did comparable work. These two engines are
//! not at parity — differential replay puts them at about 5% of decisions agreeing — so "identical
//! trajectories" is not available as a gate and pretending otherwise would be the lie that makes
//! the whole measurement worthless.
//!
//! What is available is *the same workload shape*: the same seats, seeds, horizon and generation
//! count, from blank profiles, completing without error. That is what [`SemanticGate`] checks.
//!
//! The sample also records **how many decisions were actually taken**, and that number needs
//! reading carefully. Measured on one seed over four rounds, this engine raises 188 decisions
//! where the oracle raises about 358 — but the games are the same length, and the policy records
//! every one of the 188, so neither "shorter games" nor "answered blind" explains it.
//!
//! Two things do. Swapping only the policy, blank learned for the authored bot on the same engine
//! and seed, moves 188 to 245 and makes loading and committing appear at all: a uniformly-random
//! policy declines its way past them. The remainder is engine surface — production, payment,
//! scoring, ability, development and agenda raise nothing here, and trade raises 18 against 66.
//!
//! So time per generation compares equal games doing unequal work, and time per decision compares
//! unequal mixes of work by policies that do not behave alike. Report both, claim neither as *the*
//! speedup, and expect a clean figure only at parity.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The protocol's fixed sample count.
pub const SAMPLES: usize = 30;

/// The protocol's fixed warmup count.
pub const WARMUP: usize = 10;

/// Whether a sample counts as comparable work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticGate {
    /// The workload ran the shape it was asked for.
    Pass,
    /// It did not, and no timing from it may be compared.
    Fail,
}

/// One timed run of a workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Which pair this belongs to.
    pub pair: usize,
    /// The seed this sample ran.
    pub seed: u64,
    /// Monotonic elapsed nanoseconds.
    pub nanos: u128,
    /// Games played.
    pub games: usize,
    /// Decisions taken across every seat.
    ///
    /// The normaliser. Two engines at different content coverage play games of different lengths,
    /// and time per generation would credit the less complete one for being less complete.
    pub decisions: usize,
    /// Whether this sample did the work it was asked to.
    pub gate: SemanticGate,
}

/// Summary statistics over a set of samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statistics {
    /// How many samples.
    pub samples: usize,
    /// Fastest.
    pub min_nanos: u128,
    /// Slowest.
    pub max_nanos: u128,
    /// Arithmetic mean.
    pub mean_nanos: f64,
    /// Median, also reported as p50.
    pub median_nanos: f64,
    /// Sample standard deviation.
    pub stdev_nanos: f64,
    /// 95th percentile.
    pub p95_nanos: f64,
    /// 99th percentile.
    pub p99_nanos: f64,
    /// Mean nanoseconds per decision taken — the figure to compare across engines.
    pub nanos_per_decision: f64,
    /// Total decisions across the samples, so the normaliser can be checked.
    pub decisions: usize,
}

impl Statistics {
    /// Summarise samples. Nothing is discarded.
    ///
    /// # Panics
    /// Never; an empty set returns zeroes, which the variance gate then rejects.
    #[must_use]
    pub fn over(samples: &[Sample]) -> Self {
        if samples.is_empty() {
            return Self {
                samples: 0,
                min_nanos: 0,
                max_nanos: 0,
                mean_nanos: 0.0,
                median_nanos: 0.0,
                stdev_nanos: 0.0,
                p95_nanos: 0.0,
                p99_nanos: 0.0,
                nanos_per_decision: 0.0,
                decisions: 0,
            };
        }
        let mut times: Vec<f64> = samples.iter().map(|s| as_float(s.nanos)).collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        #[expect(
            clippy::cast_precision_loss,
            reason = "the protocol fixes this at 30 samples"
        )]
        let count = times.len() as f64;
        let mean = times.iter().sum::<f64>() / count;
        let variance = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / count;
        let decisions: usize = samples.iter().map(|s| s.decisions).sum();

        Self {
            samples: samples.len(),
            min_nanos: samples.iter().map(|s| s.nanos).min().unwrap_or(0),
            max_nanos: samples.iter().map(|s| s.nanos).max().unwrap_or(0),
            mean_nanos: mean,
            median_nanos: percentile(&times, 0.50),
            stdev_nanos: variance.sqrt(),
            p95_nanos: percentile(&times, 0.95),
            p99_nanos: percentile(&times, 0.99),
            nanos_per_decision: if decisions == 0 {
                0.0
            } else {
                samples.iter().map(|s| as_float(s.nanos)).sum::<f64>() / as_float(decisions as u128)
            },
            decisions,
        }
    }

    /// Whether this set is stable enough to support a comparison.
    ///
    /// The training-throughput thresholds from M00-012e: spread at most a tenth of the mean, and
    /// the p95 no more than a fifth above the median. Fixed in advance so a noisy run is rejected
    /// rather than narrated.
    #[must_use]
    pub fn within_training_thresholds(&self) -> bool {
        if self.samples == 0 || self.mean_nanos <= 0.0 || self.median_nanos <= 0.0 {
            return false;
        }
        self.stdev_nanos / self.mean_nanos <= 0.10
            && (self.p95_nanos - self.median_nanos) / self.median_nanos <= 0.20
    }
}

/// Nanoseconds as a float.
///
/// Exact below 2^53, which is about 104 days — comfortably past any benchmark that is going to be
/// waited for. An earlier version divided by a thousand first to dodge a lint and turned every
/// sub-microsecond sample into zero, which made a spike invisible and the mean nonsense.
#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond counts are far below 2^53; 2^53ns is 104 days"
)]
fn as_float(value: u128) -> f64 {
    u64::try_from(value).unwrap_or(u64::MAX) as f64
}

/// The value at `fraction` through a sorted list, by nearest rank.
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the protocol fixes this at 30 samples"
    )]
    let count = sorted.len() as f64;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rank is bounded by the sample count"
    )]
    let rank = ((fraction * count).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Where a benchmark ran, so two reports can be checked for comparability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    /// Operating system description.
    pub os: String,
    /// Processor description.
    pub cpu: String,
    /// Logical processors visible to the process.
    pub logical_processors: usize,
    /// The affinity policy in force. The runner never changes it.
    pub affinity: String,
}

impl Host {
    /// Read the host, changing nothing about it.
    #[must_use]
    pub fn observed() -> Self {
        Self {
            os: std::env::consts::OS.to_owned(),
            cpu: std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".to_owned()),
            logical_processors: std::thread::available_parallelism()
                .map_or(1, std::num::NonZero::get),
            affinity: "inherited; unchanged by the runner".to_owned(),
        }
    }
}

/// One implementation's benchmark report, in the M00-012d schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Schema version, fixed by the protocol.
    pub schema_version: String,
    /// Which benchmark this is.
    pub benchmark_id: String,
    /// `python` or `rust`.
    pub implementation: String,
    /// The oracle commit, when one applies.
    pub oracle_commit: Option<String>,
    /// The Rust commit, when one applies.
    pub rust_commit: Option<String>,
    /// Where it ran.
    pub host: Host,
    /// What it ran.
    pub workload: Workload,
    /// Every timed sample, undiscarded.
    pub samples: Vec<Sample>,
    /// The summary.
    pub statistics: Statistics,
    /// Whether the variance thresholds held.
    pub stable: bool,
}

/// What a benchmark ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workload {
    /// The fixture or plan this measured.
    pub fixture_id: String,
    /// The manifest seed the pairs were derived from.
    pub seed: u64,
    /// Workers used.
    pub workers: usize,
    /// Whether every sample did the work it was asked for.
    pub semantic_gate: SemanticGate,
    /// Seats at the table.
    pub seats: usize,
    /// Games per sample.
    pub games: usize,
}

impl Report {
    /// Assemble a report from samples.
    #[must_use]
    pub fn assemble(
        benchmark_id: &str,
        implementation: &str,
        workload: Workload,
        samples: Vec<Sample>,
    ) -> Self {
        let statistics = Statistics::over(&samples);
        // A single failed sample invalidates the run, per the protocol: a benchmark that quietly
        // averaged over a failure would report the speed of doing less.
        let gate = if samples.iter().all(|s| s.gate == SemanticGate::Pass) && !samples.is_empty() {
            SemanticGate::Pass
        } else {
            SemanticGate::Fail
        };
        Self {
            schema_version: "1.0.0".to_owned(),
            benchmark_id: benchmark_id.to_owned(),
            implementation: implementation.to_owned(),
            oracle_commit: None,
            rust_commit: None,
            host: Host::observed(),
            workload: Workload {
                semantic_gate: gate,
                ..workload
            },
            stable: statistics.within_training_thresholds() && gate == SemanticGate::Pass,
            statistics,
            samples,
        }
    }
}

/// Time one closure, recording what it did as well as how long it took.
pub fn sample<F>(pair: usize, seed: u64, mut workload: F) -> Sample
where
    F: FnMut(u64) -> Option<(usize, usize)>,
{
    let started = std::time::Instant::now();
    let outcome = workload(seed);
    let elapsed: Duration = started.elapsed();
    match outcome {
        Some((games, decisions)) => Sample {
            pair,
            seed,
            nanos: elapsed.as_nanos(),
            games,
            decisions,
            gate: SemanticGate::Pass,
        },
        None => Sample {
            pair,
            seed,
            nanos: elapsed.as_nanos(),
            games: 0,
            decisions: 0,
            gate: SemanticGate::Fail,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples_of(times: &[u128]) -> Vec<Sample> {
        times
            .iter()
            .enumerate()
            .map(|(pair, nanos)| Sample {
                pair,
                seed: pair as u64,
                nanos: *nanos,
                games: 4,
                decisions: 100,
                gate: SemanticGate::Pass,
            })
            .collect()
    }

    #[test]
    fn nothing_is_discarded_from_a_sample_set() {
        // The protocol says so, and the reason is that an outlier is data about the machine. A
        // benchmark that drops them reports the computer somebody wished they had.
        let with_a_spike = samples_of(&[100, 100, 100, 100, 900]);
        let statistics = Statistics::over(&with_a_spike);

        assert_eq!(statistics.samples, 5);
        assert_eq!(statistics.max_nanos, 900);
        assert!(
            statistics.mean_nanos > 100.0,
            "the spike must move the mean: {}",
            statistics.mean_nanos
        );
    }

    #[test]
    fn a_noisy_run_is_rejected_rather_than_explained() {
        // Thresholds fixed in advance by M00-012e. A run that fails them cannot support a
        // comparison, however much somebody wants the number.
        let steady = Statistics::over(&samples_of(&[1_000; 30]));
        assert!(steady.within_training_thresholds());

        let mut times = vec![1_000u128; 29];
        times.push(20_000);
        let noisy = Statistics::over(&samples_of(&times));
        assert!(
            !noisy.within_training_thresholds(),
            "stdev/mean {:.3}",
            noisy.stdev_nanos / noisy.mean_nanos
        );
    }

    #[test]
    fn one_failed_sample_invalidates_the_whole_report() {
        // Averaging over a failure reports the speed of doing less work.
        let mut samples = samples_of(&[1_000; 30]);
        samples[7].gate = SemanticGate::Fail;

        let report = Report::assemble(
            "training_generation",
            "rust",
            Workload {
                fixture_id: "smoke".to_owned(),
                seed: 0,
                workers: 1,
                semantic_gate: SemanticGate::Pass,
                seats: 3,
                games: 4,
            },
            samples,
        );
        assert_eq!(report.workload.semantic_gate, SemanticGate::Fail);
        assert!(!report.stable, "a failed gate cannot be stable");
    }

    #[test]
    fn time_per_decision_is_reported_beside_time_per_run() {
        // The honest normaliser. An engine implementing fewer cards plays shorter games, and
        // time-per-generation would credit it for being less complete.
        let quick_but_short: Vec<Sample> = samples_of(&[1_000; 4])
            .into_iter()
            .map(|s| Sample { decisions: 10, ..s })
            .collect();
        let slower_but_full: Vec<Sample> = samples_of(&[2_000; 4])
            .into_iter()
            .map(|s| Sample {
                decisions: 100,
                ..s
            })
            .collect();

        let short = Statistics::over(&quick_but_short);
        let full = Statistics::over(&slower_but_full);
        assert!(short.mean_nanos < full.mean_nanos, "it looks faster");
        assert!(
            short.nanos_per_decision > full.nanos_per_decision,
            "and per unit of work it is not: {} against {}",
            short.nanos_per_decision,
            full.nanos_per_decision
        );
    }

    #[test]
    fn percentiles_come_from_the_sorted_samples() {
        let statistics = Statistics::over(&samples_of(&[5, 1, 3, 2, 4]));
        assert!((statistics.median_nanos - 3.0).abs() < f64::EPSILON);
        assert!((statistics.p95_nanos - 5.0).abs() < f64::EPSILON);
        assert_eq!(statistics.min_nanos, 1);
    }

    #[test]
    fn an_empty_set_is_not_stable() {
        // No samples is not a fast result.
        let nothing = Statistics::over(&[]);
        assert!(!nothing.within_training_thresholds());
    }

    #[test]
    fn a_failing_workload_is_timed_but_gated_out() {
        let failed = sample(0, 7, |_| None);
        assert_eq!(failed.gate, SemanticGate::Fail);
        assert_eq!(failed.decisions, 0);

        let worked = sample(1, 8, |_| Some((4, 120)));
        assert_eq!(worked.gate, SemanticGate::Pass);
        assert_eq!(worked.decisions, 120);
    }

    #[test]
    fn a_report_carries_what_makes_two_of_them_comparable() {
        // Host, workload, seed and worker count. Two reports from different shapes are not a
        // comparison however similar their numbers look.
        let report = Report::assemble(
            "training_generation",
            "rust",
            Workload {
                fixture_id: "smoke".to_owned(),
                seed: 0,
                workers: 1,
                semantic_gate: SemanticGate::Pass,
                seats: 3,
                games: 4,
            },
            samples_of(&[1_000; 30]),
        );
        assert_eq!(report.schema_version, "1.0.0");
        assert_eq!(report.workload.seats, 3);
        assert!(report.host.logical_processors >= 1);
        assert!(report.stable);
    }
}
