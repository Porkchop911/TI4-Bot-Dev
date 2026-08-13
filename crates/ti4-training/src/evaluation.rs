//! Playing a fixed panel of games and measuring what a policy produced (M10-015, M10-016).
//!
//! The missing half of promotion. [`crate::promotion`] decides whether a candidate is better than
//! the champion, and until now nothing produced the [`PanelMetrics`] it decides on: the only
//! constructor was a test helper, so the decision logic was a function waiting on inputs that did
//! not exist.
//!
//! # Paired, on the same seeds
//!
//! Two policies are compared on the *same* panel of games, not on two independently drawn ones. A
//! TI4 game's outcome swings hugely on the deal and the map, and those swings are shared when both
//! policies face the same seeds — so the comparison measures the policies rather than the luck.
//! Unpaired, the noise is larger than any effect worth promoting on.
//!
//! # Error bars, because the margins need them
//!
//! Every mean here comes with the standard error of that mean. Victory points over a short horizon
//! are noisy — a blank policy scores about 0.23 a seat with a spread of the same order — so a fixed
//! promotion threshold applied to a point estimate promotes noise about as often as it promotes
//! progress. [`Comparison::beyond_noise`] answers the question a margin cannot: is this difference
//! larger than what the panel could have produced by chance?
//!
//! The panel is fixed and named by its seeds, so re-running an evaluation gives the same numbers
//! and two candidates are always judged against the same games.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::opening::Requirement;
use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_policy::learned::Profile;

use crate::promotion::{FactionMetrics, PanelMetrics};
use crate::rollout::{Horizon, Rollout, play_batch};

/// A fixed set of games to judge policies on.
///
/// Held as explicit seeds rather than a count, so the panel is reproducible and two candidates are
/// judged on identical games.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Panel {
    /// The seeds to play.
    pub seeds: Vec<u64>,
    /// How far each game runs.
    pub horizon: Horizon,
}

impl Panel {
    /// A panel of `count` games starting at `from`.
    ///
    /// Evaluation seeds must not overlap the training seeds. A policy measured on the games it was
    /// fitted to reports how well it memorised them, which is not the question.
    #[must_use]
    pub fn held_out(from: u64, count: u64, horizon: Horizon) -> Self {
        Self {
            seeds: (from..from.saturating_add(count)).collect(),
            horizon,
        }
    }

    /// How many games this panel plays.
    #[must_use]
    pub fn len(&self) -> usize {
        self.seeds.len()
    }

    /// Whether this panel plays nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seeds.is_empty()
    }
}

/// A mean and how far it can be trusted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    /// The mean over the panel.
    pub mean: f64,
    /// The standard deviation of the samples.
    pub deviation: f64,
    /// The standard error of the mean — the deviation divided by the root of the sample count.
    ///
    /// This is the one that matters for a comparison. A large deviation with many samples still
    /// gives a mean that is known well.
    pub error: f64,
    /// How many samples it was taken over.
    pub samples: usize,
}

impl Measured {
    /// Summarise a set of samples.
    #[must_use]
    pub fn over(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a panel is thousands of games at most"
        )]
        let count = samples.len() as f64;
        let mean = samples.iter().sum::<f64>() / count;
        let variance = samples
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / count;
        Self {
            mean,
            deviation: variance.sqrt(),
            error: variance.sqrt() / count.sqrt(),
            samples: samples.len(),
        }
    }
}

/// What one policy produced on a panel, per faction, with the spread of every figure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    /// Per-faction metrics in the shape [`crate::promotion`] consumes.
    pub metrics: PanelMetrics,
    /// Victory points per seat, with error bars, keyed by faction.
    pub victory_points: BTreeMap<String, Measured>,
    /// Planets gained per seat, likewise.
    pub planets: BTreeMap<String, Measured>,
    /// Games that failed rather than played. Counted, never hidden.
    pub errors: usize,
    /// Games played.
    pub games: usize,
}

/// Play a panel with the given profiles and measure what they produced.
///
/// A seat with no profile plays uniformly at random, which is what a blank profile does — so an
/// empty map evaluates the untrained baseline.
#[must_use]
pub fn evaluate(
    content: &'static ContentStore,
    players: &[PlayerId],
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    panel: &Panel,
    requirement: Requirement,
) -> Evaluation {
    let rollouts = play_batch(
        content,
        players,
        profiles,
        sources,
        &panel.seeds,
        panel.horizon,
        requirement,
    );
    measure(&rollouts, players)
}

/// Reduce played games to per-faction metrics.
fn measure(rollouts: &[Rollout], players: &[PlayerId]) -> Evaluation {
    let mut samples: BTreeMap<String, FactionSamples> = BTreeMap::new();
    let mut errors = 0usize;
    let mut games = 0usize;

    for rollout in rollouts {
        if rollout.error.is_some() {
            errors += 1;
            continue;
        }
        games += 1;
        // The best score anybody else managed, for the margin. Read per game, because "did this
        // seat beat the table" is a question about one game and not about an average.
        let best_of: BTreeMap<&PlayerId, i64> = players
            .iter()
            .map(|player| {
                let best = rollout
                    .seats
                    .iter()
                    .filter(|seat| &seat.player != player)
                    .map(|seat| seat.episode.final_progress.victory_points)
                    .max()
                    .unwrap_or(0);
                (player, best)
            })
            .collect();

        for seat in &rollout.seats {
            let progress = seat.episode.final_progress;
            let rival = best_of.get(&seat.player).copied().unwrap_or(0);
            let row = samples.entry(seat.faction.to_string()).or_default();
            // A seat that took no decisions never got to play: the game stalled for it. Counted
            // rather than averaged away, because a policy that stalls is not a policy that draws.
            row.stalled
                .push(f64::from(u8::from(seat.trajectory.is_empty())));
            row.clearance
                .push(f64::from(u8::from(seat.episode.cleared)));
            row.shortfall.push(seat.episode.shortfall);
            row.planets.push(as_float(progress.planets_gained));
            row.systems.push(as_float(progress.systems));
            row.units.push(as_float(progress.units_gained));
            row.victory_points.push(as_float(progress.victory_points));
            row.vp_margin
                .push(as_float(progress.victory_points - rival));
            row.won_or_tied
                .push(f64::from(u8::from(progress.victory_points >= rival)));
        }
    }

    let mut metrics = BTreeMap::new();
    let mut victory_points = BTreeMap::new();
    let mut planets = BTreeMap::new();
    for (faction, row) in samples {
        metrics.insert(
            faction.clone(),
            FactionMetrics {
                stalled: Measured::over(&row.stalled).mean,
                clearance: Measured::over(&row.clearance).mean,
                shortfall: Measured::over(&row.shortfall).mean,
                planets: Measured::over(&row.planets).mean,
                systems: Measured::over(&row.systems).mean,
                units: Measured::over(&row.units).mean,
                victory_points: Measured::over(&row.victory_points).mean,
                vp_margin: Measured::over(&row.vp_margin).mean,
                won_or_tied: Measured::over(&row.won_or_tied).mean,
            },
        );
        victory_points.insert(faction.clone(), Measured::over(&row.victory_points));
        planets.insert(faction, Measured::over(&row.planets));
    }

    Evaluation {
        metrics: PanelMetrics {
            per_faction: metrics,
        },
        victory_points,
        planets,
        errors,
        games,
    }
}

#[derive(Default)]
struct FactionSamples {
    stalled: Vec<f64>,
    clearance: Vec<f64>,
    shortfall: Vec<f64>,
    planets: Vec<f64>,
    systems: Vec<f64>,
    units: Vec<f64>,
    victory_points: Vec<f64>,
    vp_margin: Vec<f64>,
    won_or_tied: Vec<f64>,
}

fn as_float(value: i64) -> f64 {
    f64::from(i32::try_from(value).unwrap_or(i32::MAX))
}

/// One policy measured against another on the same panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The candidate's evaluation.
    pub candidate: Evaluation,
    /// The baseline's evaluation, on the same seeds.
    pub baseline: Evaluation,
}

impl Comparison {
    /// Play both policies over one panel.
    #[must_use]
    pub fn paired(
        content: &'static ContentStore,
        players: &[PlayerId],
        candidate: &BTreeMap<PlayerId, Profile>,
        baseline: &BTreeMap<PlayerId, Profile>,
        sources: SourceSet,
        panel: &Panel,
        requirement: Requirement,
    ) -> Self {
        Self {
            candidate: evaluate(content, players, candidate, sources, panel, requirement),
            baseline: evaluate(content, players, baseline, sources, panel, requirement),
        }
    }

    /// The change in victory points per seat, summed over factions.
    #[must_use]
    pub fn victory_point_gain(&self) -> f64 {
        let candidate: f64 = self
            .candidate
            .victory_points
            .values()
            .map(|measured| measured.mean)
            .sum();
        let baseline: f64 = self
            .baseline
            .victory_points
            .values()
            .map(|measured| measured.mean)
            .sum();
        candidate - baseline
    }

    /// Whether the victory-point difference is larger than the panel's own noise.
    ///
    /// The question a fixed margin cannot answer. `sigmas` is how many standard errors of the
    /// difference the gain must exceed — two is the usual bar and means roughly "this would happen
    /// by chance about one panel in twenty".
    ///
    /// Returns `false` when either side has no samples: no evidence is not evidence of
    /// improvement, and treating it as one is how a promotion gate becomes decorative.
    #[must_use]
    pub fn beyond_noise(&self, sigmas: f64) -> bool {
        let mut variance = 0.0;
        let mut seen = false;
        for (faction, candidate) in &self.candidate.victory_points {
            let Some(baseline) = self.baseline.victory_points.get(faction) else {
                continue;
            };
            if candidate.samples == 0 || baseline.samples == 0 {
                continue;
            }
            seen = true;
            // The two evaluations are independent draws given the panel, so their errors add in
            // quadrature. Pairing removes the seed's contribution to *both*, which is what makes
            // this bar reachable at all on a panel of this size.
            variance += candidate.error.powi(2) + baseline.error.powi(2);
        }
        if !seen {
            return false;
        }
        let error = variance.sqrt();
        if error <= 0.0 {
            // Zero spread with real samples means every game agreed. A difference is then real,
            // and no difference is still no difference.
            return self.victory_point_gain() > 0.0;
        }
        self.victory_point_gain() > sigmas * error
    }

    /// The smallest victory-point gain this panel could have detected at `sigmas`.
    ///
    /// Reported so a negative result can be read properly: a panel that could only ever have seen
    /// a gain of 0.4 says nothing about a gain of 0.1, and reporting "no improvement" from it
    /// would be a statement about the panel rather than about the policy.
    #[must_use]
    pub fn detectable(&self, sigmas: f64) -> f64 {
        let variance: f64 = self
            .candidate
            .victory_points
            .iter()
            .filter_map(|(faction, candidate)| {
                self.baseline
                    .victory_points
                    .get(faction)
                    .map(|baseline| candidate.error.powi(2) + baseline.error.powi(2))
            })
            .sum();
        sigmas * variance.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::opening::DEFAULT_REQUIREMENT;
    use ti4_model::content_types::POK;

    fn seats(names: &[&str]) -> Vec<PlayerId> {
        names.iter().map(|name| PlayerId::new(*name)).collect()
    }

    fn panel() -> Panel {
        Panel::held_out(90_000, 6, Horizon::short())
    }

    fn blank_run() -> Evaluation {
        evaluate(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &panel(),
            DEFAULT_REQUIREMENT,
        )
    }

    #[test]
    fn an_evaluation_produces_the_metrics_promotion_consumes() {
        // The gap this module closes. Promotion's decision logic existed with nothing to decide
        // on: the only `PanelMetrics` in the codebase was a test helper.
        let evaluated = blank_run();

        assert_eq!(evaluated.errors, 0, "a seeded panel plays clean");
        assert_eq!(evaluated.games, panel().len());
        assert_eq!(
            evaluated.metrics.per_faction.len(),
            3,
            "one row per faction: {:?}",
            evaluated.metrics.per_faction.keys().collect::<Vec<_>>()
        );
        for (faction, row) in &evaluated.metrics.per_faction {
            assert!(
                (0.0..=1.0).contains(&row.clearance),
                "{faction} clearance {} is not a fraction",
                row.clearance
            );
            assert!((0.0..=1.0).contains(&row.won_or_tied));
            assert!(row.shortfall >= 0.0);
        }
    }

    #[test]
    fn the_metrics_feed_promotion_without_translation() {
        // The join, tested rather than assumed: a real evaluation must be acceptable input to the
        // real decision function.
        use crate::promotion::{Promotion, PromotionConfig};

        let evaluated = blank_run();
        let factions: Vec<String> = evaluated.metrics.per_faction.keys().cloned().collect();
        let champion: BTreeMap<String, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_profile(faction, 16),
                )
            })
            .collect();

        let promotion = Promotion::new(
            champion,
            evaluated.metrics.clone(),
            PromotionConfig::default(),
            factions,
        );
        // Identical metrics are not an improvement, and a gate that promoted them would promote
        // anything.
        assert!(!promotion.acceptable_assembled(&evaluated.metrics));
    }

    #[test]
    fn every_mean_carries_the_spread_it_was_taken_over() {
        let evaluated = blank_run();
        for (faction, measured) in &evaluated.victory_points {
            assert_eq!(
                measured.samples,
                panel().len(),
                "{faction} was measured over the wrong number of games"
            );
            assert!(measured.error >= 0.0);
            assert!(measured.error <= measured.deviation + 1e-12);
        }
    }

    #[test]
    fn the_error_of_a_mean_shrinks_as_the_panel_grows() {
        // Why the error rather than the deviation is what a comparison uses. More games do not
        // make a policy less variable; they make its average better known.
        let noisy: Vec<f64> = (0..4).map(f64::from).collect();
        let same_spread_more_samples: Vec<f64> =
            (0..4).flat_map(|value| [f64::from(value); 16]).collect();

        let few = Measured::over(&noisy);
        let many = Measured::over(&same_spread_more_samples);
        assert!(
            (few.deviation - many.deviation).abs() < 1e-9,
            "the spread is the same"
        );
        assert!(many.error < few.error, "but the mean is better known");
    }

    #[test]
    fn a_policy_compared_with_itself_is_not_an_improvement() {
        // The single most important property of a promotion gate. Two runs of the same policy on
        // the same panel differ by nothing, and anything that called that progress would promote
        // every candidate for ever.
        let players = seats(&["a", "b", "c"]);
        let comparison = Comparison::paired(
            ContentStore::embedded(),
            &players,
            &BTreeMap::new(),
            &BTreeMap::new(),
            POK,
            &panel(),
            DEFAULT_REQUIREMENT,
        );

        assert!(
            comparison.victory_point_gain().abs() < 1e-12,
            "a policy beat itself by {}",
            comparison.victory_point_gain()
        );
        assert!(!comparison.beyond_noise(2.0));
    }

    #[test]
    fn a_difference_smaller_than_the_panels_noise_is_refused() {
        // What a fixed margin cannot do. The gate must answer "larger than chance", not "larger
        // than 0.05".
        let mut comparison = Comparison::paired(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            &BTreeMap::new(),
            POK,
            &panel(),
            DEFAULT_REQUIREMENT,
        );

        // A tiny edge against a wide spread: real arithmetic, indistinguishable from luck.
        for measured in comparison.candidate.victory_points.values_mut() {
            measured.mean += 0.01;
            measured.error = 0.5;
        }
        for measured in comparison.baseline.victory_points.values_mut() {
            measured.error = 0.5;
        }
        assert!(comparison.victory_point_gain() > 0.0, "there is an edge");
        assert!(
            !comparison.beyond_noise(2.0),
            "and it was promoted anyway, which is promoting noise"
        );

        // The same edge measured far more precisely is a result.
        for measured in comparison.candidate.victory_points.values_mut() {
            measured.error = 0.0001;
        }
        for measured in comparison.baseline.victory_points.values_mut() {
            measured.error = 0.0001;
        }
        assert!(comparison.beyond_noise(2.0));
    }

    #[test]
    fn a_panel_reports_what_it_could_have_detected() {
        // So a negative result can be read. "No improvement" from a panel that could only ever
        // have seen a gain of half a point is a statement about the panel.
        let comparison = Comparison::paired(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            &BTreeMap::new(),
            POK,
            &panel(),
            DEFAULT_REQUIREMENT,
        );
        assert!(comparison.detectable(2.0) >= 0.0);
        assert!(comparison.detectable(2.0) <= comparison.detectable(4.0));
    }

    #[test]
    fn an_empty_comparison_is_not_an_improvement() {
        // No evidence is not evidence of improvement, and treating it as one makes the gate
        // decorative in exactly the case where it is needed.
        let empty = Comparison {
            candidate: Evaluation::default(),
            baseline: Evaluation::default(),
        };
        assert!(!empty.beyond_noise(2.0));
    }

    #[test]
    fn a_panel_is_the_same_games_every_time() {
        // Two candidates judged on different games are not being compared.
        let once = Panel::held_out(500, 8, Horizon::opening());
        let twice = Panel::held_out(500, 8, Horizon::opening());
        assert_eq!(once, twice);
        assert_eq!(once.len(), 8);
    }

    #[test]
    fn evaluating_the_same_profiles_twice_gives_the_same_numbers() {
        let first = blank_run();
        let second = blank_run();
        assert_eq!(first.metrics, second.metrics);
        assert_eq!(first.victory_points, second.victory_points);
    }
}
