//! Behavioral distribution suite for the authored bot (M08-021).
//!
//! The authored bot is the comparison baseline every cross-time VP measurement depends on
//! (SD-1, `plans/M08_AUTHORED_BOTS.md`). Determinism pins catch *run-to-run* drift; this suite
//! catches *version-to-version* behavioral drift: it plays a fixed seed set twice, asserts
//! per-seed identity before any comparison, and checks ten behavioral metrics against bounds
//! recorded from the current baseline (v3 — see [`baseline_bounds`]).
//!
//! Protocol (v1): six seats `p1`..`p6` on [`crate::run::Table::seated`]'s stable roster, POK
//! scope, [`Seats::Scored`] (one authored bot per seat), [`Horizon::default()`] — 50 rounds /
//! 2M steps; v1 games end by objective exhaustion in about nine rounds.
//!
//! Metrics: VP pace (mean victory points per round), completion (clean endings), score spread
//! (per-game standard deviation of the six final scores — within-game dispersion, invariant to
//! which seat scored what), faction differentiation (standard deviation of the six per-faction
//! mean VPs across the seed set — the spec's "spread across the seated factions"), and an action
//! mix: the share of each tactical event label in a game's event stream, averaged over seeds.
//! The labels are signatures of the bot's tactical choices; their census is recorded in
//! `plans/evidence/M08-021.md`.
//!
//! Bounds: 95% bootstrap confidence intervals (2000 resamples per metric, deterministic
//! splitmix64) over the current baseline's per-seed values. A degenerate bound (`lo == hi`) is
//! legal only when the baseline's per-seed values were constant — for `completion` in v1 and v2
//! that is the point: every game must end cleanly, so any error or horizon cutoff fails the
//! gate.
//! **Re-baseline discipline:** the bounds in [`baseline_bounds`] may change only through a
//! versioned process — record old and new values side by side in `plans/evidence/M08-021.md`,
//! state the semantic cause (which package changed what), and get review approval. An
//! out-of-bounds result on an unchanged tree is a failure to diagnose, never a reason to
//! re-baseline silently.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;

use crate::result::{Batch, Ending, GameResult};
use crate::run::{Horizon, Seats, run_with};

/// The fixed seed set (v1). Committed, not regenerated: a batch's numbers are comparable across
/// versions only if the games being compared are the same games. Consecutive integers in an
/// unused range — no other property is claimed for them.
pub const SEEDS: [u64; 30] = [
    812_001, 812_002, 812_003, 812_004, 812_005, 812_006, 812_007, 812_008, 812_009, 812_010,
    812_011, 812_012, 812_013, 812_014, 812_015, 812_016, 812_017, 812_018, 812_019, 812_020,
    812_021, 812_022, 812_023, 812_024, 812_025, 812_026, 812_027, 812_028, 812_029, 812_030,
];

/// Action-mix labels: event signatures of the bot's tactical choices (v1 census in evidence).
const ACTION_LABELS: [&str; 6] = [
    "SYSTEM_ACTIVATED",
    "SHIP_MOVED",
    "PRODUCTION_RESOLVED",
    "INVASION_RESOLVED",
    "SPACE_COMBAT_RESOLVED",
    "TACTICAL_ACTION_BEGAN",
];

/// The seats, in the stable order every v1 report uses.
fn players() -> Vec<PlayerId> {
    ["p1", "p2", "p3", "p4", "p5", "p6"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect()
}

/// Play the fixed seed set with one authored bot per seat.
#[must_use]
pub fn play_batch(content: &'static ContentStore) -> Batch {
    run_with(
        content,
        &players(),
        SEEDS,
        Horizon::default(),
        Seats::Scored,
    )
}

/// One game reduced to this suite's per-seed metric values.
#[derive(Debug)]
pub struct SeedMetrics {
    /// Mean final VPs across the seats divided by rounds played (0 when nothing was played).
    pub vp_pace: f64,
    /// 1.0 for a clean ending (no error, not horizon-cut), else 0.0.
    pub completed: f64,
    /// Within-game dispersion: population standard deviation of the six final scores. Invariant
    /// to which seat scored what — for across-faction differentiation see
    /// [`faction_differentiation`].
    pub score_spread: f64,
    /// Each action label's share of the game's event stream.
    pub label_shares: BTreeMap<&'static str, f64>,
}

/// Counts in this suite are tiny (seats ≤ 6, seeds = 30, event counts far below 2⁵³), so the
/// cast is exact; the helper keeps that justification in one place.
#[allow(
    clippy::cast_precision_loss,
    reason = "see doc: every input is far below 2^53"
)]
fn count_as_f64(count: usize) -> f64 {
    count as f64
}

/// Reduce one result to per-seed metric values.
#[must_use]
pub fn per_seed(result: &GameResult) -> SeedMetrics {
    let vps: Vec<f64> = result
        .victory_points
        .values()
        .map(|vp| f64::from(*vp))
        .collect();
    let mean_vp = if vps.is_empty() {
        0.0
    } else {
        vps.iter().sum::<f64>() / count_as_f64(vps.len())
    };
    let vp_pace = if result.rounds > 0 {
        mean_vp / f64::from(result.rounds)
    } else {
        0.0
    };

    let completed = if result.error.is_none()
        && matches!(
            result.ended_because,
            Ending::VictoryPoints | Ending::ObjectivesExhausted
        ) {
        1.0
    } else {
        0.0
    };

    // Within-game dispersion (population standard deviation of the six final scores).
    let score_spread = if vps.is_empty() {
        0.0
    } else {
        let variance = vps
            .iter()
            .map(|vp| (vp - mean_vp) * (vp - mean_vp))
            .sum::<f64>()
            / count_as_f64(vps.len());
        variance.sqrt()
    };

    let total_events: usize = result.events.values().sum();
    let label_shares = ACTION_LABELS
        .iter()
        .map(|label| {
            let count = result.events.get(*label).copied().unwrap_or(0);
            (
                *label,
                if total_events > 0 {
                    count_as_f64(count) / count_as_f64(total_events)
                } else {
                    0.0
                },
            )
        })
        .collect();

    SeedMetrics {
        vp_pace,
        completed,
        score_spread,
        label_shares,
    }
}

/// The nine batch metrics: each per-seed value averaged over the seed set.
#[must_use]
pub fn batch_metrics(batch: &Batch) -> BTreeMap<String, f64> {
    let seeds: Vec<SeedMetrics> = batch.results.iter().map(per_seed).collect();
    let n = count_as_f64(seeds.len());

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "vp_pace".to_owned(),
        seeds.iter().map(|seed| seed.vp_pace).sum::<f64>() / n,
    );
    metrics.insert(
        "completion".to_owned(),
        seeds.iter().map(|seed| seed.completed).sum::<f64>() / n,
    );
    metrics.insert(
        "score_spread".to_owned(),
        seeds.iter().map(|seed| seed.score_spread).sum::<f64>() / n,
    );
    // Across-faction differentiation: the spec's quantity — SD of the six per-faction mean VPs.
    metrics.insert(
        "faction_differentiation".to_owned(),
        faction_differentiation(&per_seed_seat_vps(batch)),
    );
    for label in ACTION_LABELS {
        metrics.insert(
            format!("share_{label}"),
            seeds
                .iter()
                .map(|seed| seed.label_shares[label])
                .sum::<f64>()
                / n,
        );
    }
    metrics
}

/// One splitmix64 draw in `[0, 1)`. Deterministic given the state — no thread or wall-clock
/// input anywhere in this suite. The top 53 bits fit exactly in an `f64` mantissa, so the cast
/// loses nothing.
#[allow(
    clippy::cast_precision_loss,
    reason = "top 53 bits are exact in an f64 mantissa"
)]
fn splitmix64(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let z = z ^ (z >> 31);
    // Top 53 bits → [0, 1).
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// 95% bootstrap confidence interval over `values`: `draws` resamples with replacement of the
/// mean, deterministic under `seed`. The interval is a property of the seed set's spread — it
/// absorbs per-game noise so that only a real behavioral shift moves the batch metric out.
///
/// # Panics
/// If `values` is empty (nothing to resample).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "resampled counts are far below 2^53"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "draws lie in [0, 1), so the resample index is bounded by n"
)]
pub fn bootstrap_ci(values: &[f64], draws: u32, seed: u64) -> (f64, f64) {
    assert!(!values.is_empty(), "no values to resample");
    let n = values.len();
    let mut state = seed;
    let mut stats = Vec::with_capacity(draws as usize);
    for _ in 0..draws {
        let mut sum = 0.0;
        for _ in 0..n {
            let index = (splitmix64(&mut state) * count_as_f64(n)) as usize % n;
            sum += values[index];
        }
        stats.push(sum / count_as_f64(n));
    }
    percentile_interval(&mut stats, draws)
}

/// Central 95% percentile interval over `stats` produced by `draws` resamples: the 2.5th and
/// 97.5th order statistics (symmetric indices — 50 of 2000 strictly below, 50 above).
fn percentile_interval(stats: &mut [f64], draws: u32) -> (f64, f64) {
    stats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let low_index = (draws * 25) / 1000;
    let high_index = draws - 1 - (draws * 25) / 1000;
    (stats[low_index as usize], stats[high_index as usize])
}

/// Per-seed VP vector in seat order (`p1`..`p6`). `Table::seated` is seed-independent, so each
/// seat holds the same faction in every game — per-seat means are per-faction means.
#[must_use]
pub fn per_seed_seat_vps(batch: &Batch) -> Vec<[f64; 6]> {
    let players = players();
    batch
        .results
        .iter()
        .map(|result| {
            let mut vps = [0.0; 6];
            for (seat, player) in vps.iter_mut().zip(&players) {
                *seat = f64::from(
                    result
                        .victory_points
                        .get(&player.to_string())
                        .copied()
                        .unwrap_or(0),
                );
            }
            vps
        })
        .collect()
}

/// Across-faction differentiation (spec deliverable 2): population standard deviation of the six
/// per-faction mean VPs over `vps`. It moves when the *spread* of faction strengths changes — a
/// weak faction becoming competitive, or a strong one falling back. A consistent relabeling of
/// which seat holds which strength permutes the six means and leaves this value (and
/// [`SeedMetrics::score_spread`]) untouched; both metrics are blind to that permutation by
/// construction.
#[must_use]
pub fn faction_differentiation(vps: &[[f64; 6]]) -> f64 {
    if vps.is_empty() {
        return 0.0;
    }
    let n = count_as_f64(vps.len());
    let mut means = [0.0; 6];
    for seat in 0..6 {
        means[seat] = vps.iter().map(|row| row[seat]).sum::<f64>() / n;
    }
    population_sd(&means)
}

/// Population standard deviation of six values.
fn population_sd(values: &[f64; 6]) -> f64 {
    let center = values.iter().sum::<f64>() / count_as_f64(6);
    let variance = values
        .iter()
        .map(|value| (value - center) * (value - center))
        .sum::<f64>()
        / count_as_f64(6);
    variance.sqrt()
}

/// 95% CI for [`faction_differentiation`] under protocol v1: resample seeds with replacement,
/// recompute the statistic on the resampled rows. The resampling unit is the seed — the
/// statistic has no per-seed scalar form, so it cannot go through [`bootstrap_ci`].
///
/// # Panics
/// If `vps` is empty (no games to resample).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "resampled counts are far below 2^53"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "draws lie in [0, 1), so the resample index is bounded by n"
)]
pub fn faction_differentiation_ci(vps: &[[f64; 6]]) -> (f64, f64) {
    assert!(!vps.is_empty(), "no games to resample");
    let n = vps.len();
    let mut state = BOOTSTRAP_SEED;
    let mut stats = Vec::with_capacity(BOOTSTRAP_DRAWS as usize);
    for _ in 0..BOOTSTRAP_DRAWS {
        let mut row_sums = [0.0; 6];
        for _ in 0..n {
            let index = (splitmix64(&mut state) * count_as_f64(n)) as usize % n;
            for seat in 0..6 {
                row_sums[seat] += vps[index][seat];
            }
        }
        let mut means = [0.0; 6];
        for seat in 0..6 {
            means[seat] = row_sums[seat] / count_as_f64(n);
        }
        stats.push(population_sd(&means));
    }
    percentile_interval(&mut stats, BOOTSTRAP_DRAWS)
}

/// Recompute the recorded bound for `name` from `batch` under protocol v1 — `None` if `name`
/// names no gated metric. The gate's integrity check uses this to pin both key sets and values:
/// a bounds entry with no metric behind it fails the gate, not just narrows it.
#[must_use]
pub fn recompute_bound(batch: &Batch, name: &str) -> Option<(f64, f64)> {
    if name == "faction_differentiation" {
        return Some(faction_differentiation_ci(&per_seed_seat_vps(batch)));
    }
    per_seed_values(batch)
        .get(name)
        .map(|values| bootstrap_ci(values, BOOTSTRAP_DRAWS, BOOTSTRAP_SEED))
}

/// Bootstrap protocol v1: `draws` resamples per metric under this fixed seed. Changing either
/// constant is a re-baseline event (see module docs) — the gate test's integrity check ties the
/// recorded constants to exactly these parameters.
pub const BOOTSTRAP_DRAWS: u32 = 2000;
pub const BOOTSTRAP_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// The current baseline bounds (v2): metric name → (lo, hi). Recorded at full double
/// precision under protocol v1 — raw values and the v1→v2 old/new comparison in
/// `plans/evidence/M08-021.md`. Changing these requires the re-baseline discipline stated at
/// the top of this module.
#[must_use]
pub fn baseline_bounds() -> BTreeMap<String, (f64, f64)> {
    // v3 — recorded 2026-08-23 on branch wp/m08-019-reopened-frontier-review after the
    // Tier-C correction round completed F-M08-019-1: C1 canonicalized invasion landing-option
    // order (landable_planets from the system record's `planets` array) and C2 threaded the
    // active content/sources through annexable. The single versioned rederivation required by
    // the verdict's disposition — v2 was interim because the missing invasion fix would change
    // option ordering again. Option reordering changes bot sampling on tied scores; one seed
    // flipped its ending (ObjectivesExhausted → VictoryPoints) and every metric moved modestly.
    // v1 (base `45fe569`) and v2 values are preserved side by side in plans/evidence/M08-021.md.
    let mut bounds = BTreeMap::new();
    bounds.insert(
        "vp_pace".to_owned(),
        (0.383_333_333_333_333_5, 0.448_765_432_098_765_46),
    );
    // Degenerate on purpose: all thirty v1, v2 and v3 games ended cleanly, so the bound is the
    // strict invariant "every game ends cleanly", not a statistical interval.
    bounds.insert("completion".to_owned(), (1.0, 1.0));
    bounds.insert(
        "score_spread".to_owned(),
        (1.608_870_963_060_335_5, 1.922_138_631_083_791),
    );
    // V3: the spec's across-faction quantity — recorded from the same baseline run.
    bounds.insert(
        "faction_differentiation".to_owned(),
        (0.306_362_570_643_605_3, 0.763_378_617_450_244_5),
    );
    bounds.insert(
        "share_INVASION_RESOLVED".to_owned(),
        (0.027_887_127_382_587_116, 0.029_379_724_541_312_928),
    );
    bounds.insert(
        "share_PRODUCTION_RESOLVED".to_owned(),
        (0.047_428_583_456_618_63, 0.048_660_660_227_172_75),
    );
    bounds.insert(
        "share_SHIP_MOVED".to_owned(),
        (0.067_420_702_892_928_33, 0.072_385_601_009_851_66),
    );
    bounds.insert(
        "share_SPACE_COMBAT_RESOLVED".to_owned(),
        (0.008_142_061_219_023_835, 0.009_275_008_762_437_54),
    );
    bounds.insert(
        "share_SYSTEM_ACTIVATED".to_owned(),
        (0.093_517_207_031_071_19, 0.095_809_575_282_834_01),
    );
    bounds.insert(
        "share_TACTICAL_ACTION_BEGAN".to_owned(),
        (0.046_066_967_306_267_2, 0.047_168_934_633_599_81),
    );
    bounds
}

/// The per-seed values behind each batch metric — what the bootstrap resamples.
#[must_use]
pub fn per_seed_values(batch: &Batch) -> BTreeMap<String, Vec<f64>> {
    let seeds: Vec<SeedMetrics> = batch.results.iter().map(per_seed).collect();
    let mut values = BTreeMap::new();
    values.insert(
        "vp_pace".to_owned(),
        seeds.iter().map(|seed| seed.vp_pace).collect(),
    );
    values.insert(
        "completion".to_owned(),
        seeds.iter().map(|seed| seed.completed).collect(),
    );
    values.insert(
        "score_spread".to_owned(),
        seeds.iter().map(|seed| seed.score_spread).collect(),
    );
    for label in ACTION_LABELS {
        values.insert(
            format!("share_{label}"),
            seeds.iter().map(|seed| seed.label_shares[label]).collect(),
        );
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The suite's gate: two runs from the same seed set must be per-seed identical (the
    /// determinism precondition — a flaky bound could otherwise hide an engine nondeterminism
    /// regression), and every batch metric must sit inside its recorded v1 bounds.
    #[test]
    fn the_suite_reproduces_and_stays_within_the_recorded_bounds() {
        let content = ContentStore::embedded();

        // Determinism precondition, asserted before any comparison.
        let first = play_batch(content);
        let second = play_batch(content);
        assert_eq!(first.results.len(), SEEDS.len());
        for (a, b) in first.results.iter().zip(&second.results) {
            assert_eq!(a.seed, b.seed, "same seed order");
            assert_eq!(a.victory_points, b.victory_points, "seed {}", a.seed);
            assert_eq!(a.rounds, b.rounds, "seed {}", a.seed);
            assert_eq!(a.decisions, b.decisions, "seed {}", a.seed);
            assert_eq!(a.events, b.events, "seed {}", a.seed);
            assert_eq!(a.ended_because, b.ended_because, "seed {}", a.seed);
            assert_eq!(
                a.error.is_none(),
                b.error.is_none(),
                "seed {}: error status must match",
                a.seed
            );
        }

        // The behavioral gate: current tree inside the recorded bounds.
        let metrics = batch_metrics(&first);
        let bounds = baseline_bounds();

        // Key sets must match exactly (review V4): a metric computed but never compared, or a
        // bound with no metric behind it, would silently narrow the gate while every test stays
        // green.
        assert_eq!(metrics.len(), bounds.len());
        for name in metrics.keys() {
            assert!(
                bounds.contains_key(name),
                "metric {name} has no recorded bound"
            );
        }

        for (name, value) in &metrics {
            let (lo, hi) = bounds[name];
            assert!(
                *value >= lo && *value <= hi,
                "metric {name} = {value:.6} is outside the recorded bounds [{lo:.6}, {hi:.6}] — \
                 diagnose before re-baselining (see module docs)"
            );
        }

        // Protocol integrity: the recorded constants must be exactly what the documented
        // bootstrap protocol produces from this tree's baseline data. A transcription error in
        // either the values or the parameters would fail here — the bounds check above alone
        // could not catch one, because every v1 metric sits well inside its interval.
        for (name, (lo, hi)) in &bounds {
            let recomputed = recompute_bound(&first, name)
                .unwrap_or_else(|| panic!("recorded bound for {name} has no metric behind it"));
            assert_eq!(
                (*lo, *hi),
                recomputed,
                "recorded bound for {name} does not match the protocol recomputation: \
                 recorded ({lo:?}, {hi:?}), recomputed ({recomputed:?})"
            );
        }
    }

    /// Bounds must be finite and ordered. Degeneracy (lo == hi) is legal only when the baseline
    /// data were constant — `completion` in v1 is such a case, where it encodes a strict
    /// invariant rather than an interval; anything else degenerate would make the gate either
    /// vacuous or always-failing and is a re-baseline process error.
    #[test]
    fn the_recorded_bounds_are_finite_and_ordered() {
        for (name, (lo, hi)) in baseline_bounds() {
            assert!(
                lo.is_finite() && hi.is_finite(),
                "bound for {name} must be finite"
            );
            assert!(lo <= hi, "bound for {name} is inverted: [{lo:.6}, {hi:.6}]");
        }
    }

    /// The bootstrap must be deterministic and sane on known input.
    #[test]
    fn the_bootstrap_is_deterministic_and_ordered() {
        let values = [0.4, 0.5, 0.3, 0.6, 0.45];
        let once = bootstrap_ci(&values, 2000, 7);
        let twice = bootstrap_ci(&values, 2000, 7);
        assert_eq!(once, twice, "same input and seed give the same interval");
        assert!(once.0 <= once.1, "lo must not exceed hi");
        // Every resampled mean lies within the data's own range, so the percentile bounds
        // must too — an interval outside it means the resampling is broken.
        let data_min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let data_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            once.0 >= data_min && once.1 <= data_max + 1e-9,
            "the interval must sit inside the data's range of means: {once:?}"
        );
    }

    /// Review W1 guard (M08-021 close-out): `faction_differentiation` moves when the *spread* of
    /// faction strengths changes and is invariant under consistent relabeling — a permutation of
    /// which seat holds which strength permutes the six per-faction means without changing their
    /// standard deviation. The CI inherits both properties: constant rows resample to a single
    /// statistic (the degenerate interval equals the point estimate), and relabeling leaves it
    /// bit-identical because the resample index stream is unchanged and every intermediate value
    /// in this fixture is exactly representable (small integers — exactness by construction, so
    /// strict comparison is the right assertion).
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "exact by construction: small-integer fixture keeps every intermediate value representable"
    )]
    fn faction_differentiation_moves_on_spread_not_relabeling() {
        // Three games; seat 5 is clearly the weakest.
        let base: [[f64; 6]; 3] = [
            [5.0, 4.0, 4.0, 4.0, 3.0, 2.0],
            [5.0, 4.0, 4.0, 4.0, 3.0, 2.0],
            [5.0, 4.0, 4.0, 4.0, 3.0, 2.0],
        ];
        let value = faction_differentiation(&base);

        // Consistent relabeling: swap seats 0 and 1 in every game.
        let mut relabeled = base;
        for row in &mut relabeled {
            row.swap(0, 1);
        }
        assert_eq!(
            faction_differentiation(&relabeled),
            value,
            "a consistent permutation of seat strengths must leave the metric untouched"
        );

        // A change in the spread: strengthen the weakest faction by +1.3.
        let mut narrowed = base;
        for row in &mut narrowed {
            row[5] += 1.3;
        }
        assert_ne!(
            faction_differentiation(&narrowed),
            value,
            "a change in the spread of faction strengths must move the metric"
        );

        // The CI inherits both properties.
        let ci = faction_differentiation_ci(&base);
        assert_eq!(
            ci,
            (value, value),
            "constant rows give the degenerate CI at the point"
        );
        assert_eq!(
            faction_differentiation_ci(&relabeled),
            ci,
            "the resample index stream is permutation-invariant"
        );
    }

    /// A failed game contributes zero pace and no completion — not a panic. The zeros are
    /// exact by construction: the zero path assigns literal `0.0` without arithmetic, so
    /// strict comparison is the right assertion.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "exact zeros by construction — the zero path assigns literals"
    )]
    fn a_failed_game_reduces_to_zeros_not_a_panic() {
        let players = players();
        let result = GameResult {
            seed: 1,
            finished: false,
            winner: None,
            rounds: 0,
            victory_points: players
                .iter()
                .map(|player| (player.to_string(), 0))
                .collect(),
            events: BTreeMap::new(),
            decisions: 0,
            seconds: 0.0,
            ended_because: Ending::Error,
            error: Some("setup".to_owned()),
        };
        let seed = per_seed(&result);
        assert_eq!(seed.vp_pace, 0.0);
        assert_eq!(seed.completed, 0.0);
        assert_eq!(seed.score_spread, 0.0);
        for share in seed.label_shares.values() {
            assert_eq!(*share, 0.0);
        }
    }
}
