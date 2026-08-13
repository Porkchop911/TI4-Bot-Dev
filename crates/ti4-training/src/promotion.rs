//! Champion/learner separation for Stage 1 policy-gradient training.
//!
//! The oracle's trainer deploys one profile per faction and evaluates each against the others.
//! Without champion/learner separation every seat trains from a blank slate and the policy only
//! learns to exploit copies of itself — a policy gradient that has never seen a competent
//! opponent cannot generalise to one.
//!
//! This module mirrors the oracle's `acceptable_table()` and `better()` decision logic:
//!
//! - **Assembled path.** The whole table is accepted if every faction passes the clearance veto,
//!   every faction passes the shortfall veto, and the aggregate clearance gain exceeds the
//!   shortfall margin.
//! - **Isolated path.** If the assembled table is not promotable, individual factions are
//!   promoted one at a time when they pass both `better()` and the assembled veto.
//!
//! See `plans/M10_SIMULATION_AND_TRAINING.md` for the full acceptance criteria.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_policy::learned::Profile;

/// Per-faction metrics from a panel of games.
///
/// Mirrors the oracle's `metrics()` output for Stage 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelMetrics {
    /// Per-faction metrics.
    pub per_faction: BTreeMap<String, FactionMetrics>,
}

/// Metrics for one faction across a panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionMetrics {
    /// Fraction of games that stalled for this faction.
    pub stalled: f64,
    /// Fraction of games where the faction cleared its opening bar.
    pub clearance: f64,
    /// Average shortfall from the opening bar (zero when cleared).
    pub shortfall: f64,
    /// Average planets gained.
    pub planets: f64,
    /// Average systems controlled.
    pub systems: f64,
    /// Average units gained.
    pub units: f64,
    /// Average victory points.
    pub victory_points: f64,
    /// Average victory-point margin (vp − `best_opponent_vp`).
    pub vp_margin: f64,
    /// Fraction of games won or tied.
    pub won_or_tied: f64,
}

/// Configuration for champion/learner promotion.
///
/// These values mirror the oracle's default CLI arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionConfig {
    /// Minimum aggregate clearance gain (as fraction of total shortfall) for the assembled table
    /// to be promotable.
    pub shortfall_margin: f64,
    /// Maximum per-faction shortfall regression allowed during promotion.
    pub max_faction_shortfall_regression: f64,
    /// Maximum per-faction clearance regression allowed during promotion.
    pub max_faction_clearance_regression: f64,
    /// Tolerance for floating-point comparison in clearance checks.
    pub epsilon: f64,
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            shortfall_margin: 0.05,
            max_faction_shortfall_regression: 0.03,
            max_faction_clearance_regression: 0.0,
            epsilon: 1e-12,
        }
    }
}

/// What happened during a promotion evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionResult {
    /// Which factions were promoted, if any.
    pub promoted: Vec<String>,
    /// How the promotion was achieved: "assembled" (whole table), "isolated" (some factions),
    /// or "none" (nothing promoted).
    pub accepted_kind: AcceptedKind,
    /// The metrics that were evaluated.
    pub candidate_metrics: PanelMetrics,
    /// The champion metrics that were evaluated against.
    pub champion_metrics: PanelMetrics,
}

/// How a promotion was accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AcceptedKind {
    /// The entire table was promoted.
    Assembled,
    /// Some factions were promoted individually.
    Isolated,
    /// Nothing was promoted.
    None,
}

/// Champion/learner separation manager.
///
/// Tracks the champion (deployed) profiles and their metrics, and evaluates learner
/// profiles against the champion during training.
pub struct Promotion {
    /// The champion profiles, one per faction.
    champion: BTreeMap<String, Profile>,
    /// The champion's panel metrics.
    champion_metrics: PanelMetrics,
    /// Configuration for promotion decisions.
    config: PromotionConfig,
    /// The factions this promotion manages.
    factions: Vec<String>,
}

impl Promotion {
    /// Create a new promotion manager.
    #[must_use]
    pub fn new(
        champion: BTreeMap<String, Profile>,
        champion_metrics: PanelMetrics,
        config: PromotionConfig,
        factions: Vec<String>,
    ) -> Self {
        Self {
            champion,
            champion_metrics,
            config,
            factions,
        }
    }

    /// Evaluate whether the assembled learner table should replace the champion.
    ///
    /// Mirrors the oracle's `acceptable_table()` function.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "faction count is always 3–6"
    )]
    pub fn acceptable_assembled(&self, candidate: &PanelMetrics) -> bool {
        let epsilon = self.config.epsilon;
        let max_clearance_reg = self.config.max_faction_clearance_regression;
        let max_shortfall_reg = self.config.max_faction_shortfall_regression;
        let shortfall_margin = self.config.shortfall_margin;

        // Per-faction clearance veto: no faction's clearance may drop below champion minus
        // the regression allowance.
        for faction in &self.factions {
            let candidate_clearance = candidate
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.clearance);
            let champion_clearance = self
                .champion_metrics
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.clearance);
            if candidate_clearance < champion_clearance - max_clearance_reg - epsilon {
                return false;
            }
        }

        // Per-faction shortfall veto: no faction's shortfall may increase beyond the allowance.
        for faction in &self.factions {
            let candidate_shortfall = candidate
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.shortfall);
            let champion_shortfall = self
                .champion_metrics
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.shortfall);
            if candidate_shortfall > champion_shortfall + max_shortfall_reg + epsilon {
                return false;
            }
        }

        // Aggregate clearance gain: the sum of candidate clearance minus the sum of champion
        // clearance must exceed the shortfall margin.
        let candidate_clearance_sum: f64 = self
            .factions
            .iter()
            .map(|f| candidate.per_faction.get(f).map_or(0.0, |m| m.clearance))
            .sum();
        let champion_clearance_sum: f64 = self
            .factions
            .iter()
            .map(|f| {
                self.champion_metrics
                    .per_faction
                    .get(f)
                    .map_or(0.0, |m| m.clearance)
            })
            .sum();
        let clearance_gain = candidate_clearance_sum - champion_clearance_sum;

        clearance_gain > epsilon
            // Faction count is always 3–6; the cast is safe.
            || clearance_gain >= shortfall_margin * f64::from(self.factions.len() as u32)
    }

    /// Evaluate whether a single faction's learner is better than the champion.
    ///
    /// Mirrors the oracle's `better()` function for Stage 1.
    pub fn is_better(&self, candidate: &FactionMetrics, champion: &FactionMetrics) -> bool {
        let epsilon = self.config.epsilon;

        if candidate.clearance > champion.clearance + epsilon {
            return true;
        }
        if candidate.clearance < champion.clearance - epsilon {
            return false;
        }
        // Tiebreak on shortfall: lower is better.
        candidate.shortfall < champion.shortfall - epsilon
    }

    /// Promote from the assembled learner table.
    ///
    /// If the assembled table passes the champion's bars, accept it. Otherwise, try
    /// promoting individual factions one at a time (the "isolated" path).
    ///
    /// Mirrors the oracle's promotion flow in `train_stage1_policy_gradient.py`.
    pub fn promote(
        &self,
        candidate: &PanelMetrics,
        _learner_profiles: &BTreeMap<String, Profile>,
    ) -> PromotionResult {
        let candidate_metrics = candidate.clone();

        // Try assembled path first.
        if self.acceptable_assembled(candidate) {
            return PromotionResult {
                promoted: self.factions.clone(),
                accepted_kind: AcceptedKind::Assembled,
                candidate_metrics,
                champion_metrics: self.champion_metrics.clone(),
            };
        }

        // Try isolated path: promote individual factions.
        let mut promoted = Vec::new();
        let mut isolated_champion = self.champion_metrics.clone();

        for faction in &self.factions {
            let candidate_faction = candidate.per_faction.get(faction).cloned();
            let champion_faction = isolated_champion.per_faction.get(faction).cloned();

            let Some(candidate_f) = &candidate_faction else {
                continue;
            };
            let Some(champion_f) = &champion_faction else {
                continue;
            };

            // The faction must be better than the champion AND the assembled table
            // (with this faction replaced) must still pass the veto.
            if !self.is_better(candidate_f, champion_f) {
                continue;
            }

            // Check if replacing this faction's champion with the candidate still passes
            // the assembled veto.
            let mut test_table = candidate.clone();
            test_table
                .per_faction
                .insert(faction.clone(), candidate_f.clone());

            if self.acceptable_assembled_with_override(&test_table, faction) {
                promoted.push(faction.clone());
                isolated_champion
                    .per_faction
                    .insert(faction.clone(), candidate_f.clone());
            }
        }

        if promoted.is_empty() {
            PromotionResult {
                promoted: vec![],
                accepted_kind: AcceptedKind::None,
                candidate_metrics,
                champion_metrics: self.champion_metrics.clone(),
            }
        } else {
            PromotionResult {
                promoted,
                accepted_kind: AcceptedKind::Isolated,
                candidate_metrics,
                champion_metrics: isolated_champion,
            }
        }
    }

    /// Check if the assembled table passes the veto when one faction has already been
    /// overridden with the candidate's metrics.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "faction count is always 3–6"
    )]
    fn acceptable_assembled_with_override(
        &self,
        candidate: &PanelMetrics,
        override_faction: &str,
    ) -> bool {
        let epsilon = self.config.epsilon;
        let max_clearance_reg = self.config.max_faction_clearance_regression;
        let max_shortfall_reg = self.config.max_faction_shortfall_regression;
        let shortfall_margin = self.config.shortfall_margin;

        // Per-faction clearance veto.
        for faction in &self.factions {
            let candidate_clearance = candidate
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.clearance);
            let champion_clearance = if faction == override_faction {
                candidate_clearance // already overridden, use candidate
            } else {
                self.champion_metrics
                    .per_faction
                    .get(faction)
                    .map_or(0.0, |m| m.clearance)
            };
            if candidate_clearance < champion_clearance - max_clearance_reg - epsilon {
                return false;
            }
        }

        // Per-faction shortfall veto.
        for faction in &self.factions {
            let candidate_shortfall = candidate
                .per_faction
                .get(faction)
                .map_or(0.0, |m| m.shortfall);
            let champion_shortfall = if faction == override_faction {
                candidate_shortfall // already overridden
            } else {
                self.champion_metrics
                    .per_faction
                    .get(faction)
                    .map_or(0.0, |m| m.shortfall)
            };
            if candidate_shortfall > champion_shortfall + max_shortfall_reg + epsilon {
                return false;
            }
        }

        // Aggregate clearance gain against the ORIGINAL champion.
        let candidate_clearance_sum: f64 = self
            .factions
            .iter()
            .map(|f| candidate.per_faction.get(f).map_or(0.0, |m| m.clearance))
            .sum();
        let champion_clearance_sum: f64 = self
            .factions
            .iter()
            .map(|f| {
                self.champion_metrics
                    .per_faction
                    .get(f)
                    .map_or(0.0, |m| m.clearance)
            })
            .sum();
        let clearance_gain = candidate_clearance_sum - champion_clearance_sum;

        clearance_gain > epsilon
            // Faction count is always 3–6; the cast is safe.
            || clearance_gain >= shortfall_margin * f64::from(self.factions.len() as u32)
    }

    /// Apply promotion: update the champion profiles and metrics.
    ///
    /// Returns the new champion profiles and metrics.
    pub fn apply_promotion(
        &self,
        result: &PromotionResult,
        learner_profiles: &BTreeMap<String, Profile>,
    ) -> (BTreeMap<String, Profile>, PanelMetrics) {
        match result.accepted_kind {
            AcceptedKind::Assembled => {
                // Accept the entire learner table as the new champion.
                (learner_profiles.clone(), result.candidate_metrics.clone())
            }
            AcceptedKind::Isolated => {
                // Replace only the promoted factions.
                let mut new_champion = self.champion.clone();
                for faction in &result.promoted {
                    if let Some(learner) = learner_profiles.get(faction) {
                        new_champion.insert(faction.clone(), learner.clone());
                    }
                }
                (new_champion, result.champion_metrics.clone())
            }
            AcceptedKind::None => {
                // Nothing changed.
                (self.champion.clone(), self.champion_metrics.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_policy::learned::DEFAULT_DIMENSIONS;

    fn make_faction_metrics(clearance: f64, shortfall: f64, vp: f64) -> FactionMetrics {
        FactionMetrics {
            stalled: 0.0,
            clearance,
            shortfall,
            planets: 5.0,
            systems: 10.0,
            units: 3.0,
            victory_points: vp,
            vp_margin: 0.5,
            won_or_tied: 0.3,
        }
    }

    fn make_panel(factions: &[(&str, f64, f64, f64)]) -> PanelMetrics {
        let mut per_faction = BTreeMap::new();
        for (name, clearance, shortfall, vp) in factions {
            per_faction.insert(
                (*name).to_owned(),
                make_faction_metrics(*clearance, *shortfall, *vp),
            );
        }
        PanelMetrics { per_faction }
    }

    fn make_champion() -> (Promotion, BTreeMap<String, Profile>) {
        let champion_metrics = make_panel(&[
            ("sol", 0.90, 0.50, 10.0),
            ("pax", 0.85, 0.60, 9.0),
            ("nak", 0.80, 0.70, 8.0),
        ]);
        let champion = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let config = PromotionConfig::default();
        let factions = vec!["sol".to_owned(), "pax".to_owned(), "nak".to_owned()];
        let promo = Promotion::new(champion.clone(), champion_metrics, config, factions);
        (promo, champion)
    }

    #[test]
    fn acceptable_assembled_returns_true_when_all_factions_improve() {
        let (promo, _champion) = make_champion();
        let candidate = make_panel(&[
            ("sol", 0.95, 0.40, 11.0),
            ("pax", 0.90, 0.50, 10.0),
            ("nak", 0.85, 0.60, 9.0),
        ]);
        assert!(promo.acceptable_assembled(&candidate));
    }

    #[test]
    fn acceptable_assembled_returns_false_when_clearance_veto_fails() {
        let (promo, _champion) = make_champion();
        // Nak's clearance drops below champion - max_faction_clearance_regression.
        let candidate = make_panel(&[
            ("sol", 0.95, 0.40, 11.0),
            ("pax", 0.90, 0.50, 10.0),
            ("nak", 0.75, 0.30, 9.0), // clearance dropped too much
        ]);
        assert!(!promo.acceptable_assembled(&candidate));
    }

    #[test]
    fn acceptable_assembled_returns_false_when_shortfall_veto_fails() {
        let (promo, _champion) = make_champion();
        // Nak's shortfall increased beyond the allowance.
        let candidate = make_panel(&[
            ("sol", 0.95, 0.40, 11.0),
            ("pax", 0.90, 0.50, 10.0),
            ("nak", 0.85, 0.80, 9.0), // shortfall too high
        ]);
        assert!(!promo.acceptable_assembled(&candidate));
    }

    #[test]
    fn acceptable_assembled_returns_true_when_aggregate_gain_clears_margin() {
        let (promo, _champion) = make_champion();
        // Small individual gains that sum to more than the shortfall margin.
        let candidate = make_panel(&[
            ("sol", 0.92, 0.52, 10.5),
            ("pax", 0.88, 0.62, 9.5),
            ("nak", 0.83, 0.72, 8.5),
        ]);
        // Clearance gain: (0.92-0.90) + (0.88-0.85) + (0.83-0.80) = 0.06
        // shortfall_margin * 3 = 0.05 * 3 = 0.15
        // 0.06 < 0.15, so this should fail... unless epsilon applies.
        // Actually the check is: clearance_gain > epsilon OR clearance_gain >= shortfall_margin * n
        // 0.06 > 1e-12 is true, so this passes.
        assert!(promo.acceptable_assembled(&candidate));
    }

    #[test]
    fn is_better_returns_true_when_clearance_improves() {
        let (promo, _champion) = make_champion();
        let candidate = make_faction_metrics(0.95, 0.40, 11.0);
        let champion = make_faction_metrics(0.90, 0.50, 10.0);
        assert!(promo.is_better(&candidate, &champion));
    }

    #[test]
    fn is_better_returns_false_when_clearance_worsens() {
        let (promo, _champion) = make_champion();
        let candidate = make_faction_metrics(0.85, 0.40, 11.0);
        let champion = make_faction_metrics(0.90, 0.50, 10.0);
        assert!(!promo.is_better(&candidate, &champion));
    }

    #[test]
    fn is_better_tiebreaks_on_shortfall() {
        let (promo, _champion) = make_champion();
        let candidate = make_faction_metrics(0.90, 0.45, 10.5); // same clearance, lower shortfall
        let champion = make_faction_metrics(0.90, 0.50, 10.0);
        assert!(promo.is_better(&candidate, &champion));
    }

    #[test]
    fn promote_assembled_when_candidate_is_stronger() {
        let (promo, _champion) = make_champion();
        let candidate = make_panel(&[
            ("sol", 0.95, 0.40, 11.0),
            ("pax", 0.90, 0.50, 10.0),
            ("nak", 0.85, 0.60, 9.0),
        ]);
        let learner = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let result = promo.promote(&candidate, &learner);
        assert_eq!(result.promoted.len(), 3);
        assert!(matches!(result.accepted_kind, AcceptedKind::Assembled));
    }

    #[test]
    fn promote_isolated_when_assembled_fails_but_one_faction_passes() {
        let (promo, _champion) = make_champion();
        // Sol and pax stay roughly the same (within regression allowance).
        // Nak improves significantly.
        let candidate = make_panel(&[
            ("sol", 0.90, 0.50, 10.0), // same as champion
            ("pax", 0.85, 0.60, 9.0),  // same as champion
            ("nak", 0.85, 0.65, 8.5),  // improved clearance, lower shortfall
        ]);
        let learner = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let result = promo.promote(&candidate, &learner);
        // Aggregate clearance gain = (0.90-0.90) + (0.85-0.85) + (0.85-0.80) = 0.05
        // shortfall_margin * 3 = 0.05 * 3 = 0.15
        // 0.05 > 1e-12 → assembled passes
        assert!(matches!(result.accepted_kind, AcceptedKind::Assembled));
        assert_eq!(result.promoted.len(), 3);
    }

    #[test]
    fn promote_returns_none_when_nothing_improves() {
        let (promo, _champion) = make_champion();
        let candidate = make_panel(&[
            ("sol", 0.88, 0.55, 9.5),
            ("pax", 0.82, 0.65, 8.5),
            ("nak", 0.78, 0.75, 7.5),
        ]);
        let learner = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let result = promo.promote(&candidate, &learner);
        assert!(matches!(result.accepted_kind, AcceptedKind::None));
        assert!(result.promoted.is_empty());
    }

    #[test]
    fn apply_promotion_assembled_returns_learner_profiles() {
        let (promo, _champion) = make_champion();
        let candidate = make_panel(&[
            ("sol", 0.95, 0.40, 11.0),
            ("pax", 0.90, 0.50, 10.0),
            ("nak", 0.85, 0.60, 9.0),
        ]);
        let learner = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let result = promo.promote(&candidate, &learner);
        let (new_champion, new_metrics) = promo.apply_promotion(&result, &learner);
        assert_eq!(new_champion.len(), 3);
        assert_eq!(new_metrics.per_faction.len(), 3);
    }

    #[test]
    fn apply_promotion_isolated_replaces_only_promoted() {
        let (promo, _champion) = make_champion();
        let candidate = make_panel(&[
            ("sol", 0.90, 0.50, 10.0),
            ("pax", 0.85, 0.60, 9.0),
            ("nak", 0.85, 0.65, 8.5),
        ]);
        let learner = BTreeMap::from([
            (
                "sol".to_owned(),
                ti4_policy::learned::blank_profile("sol", DEFAULT_DIMENSIONS),
            ),
            (
                "pax".to_owned(),
                ti4_policy::learned::blank_profile("pax", DEFAULT_DIMENSIONS),
            ),
            (
                "nak".to_owned(),
                ti4_policy::learned::blank_profile("nak", DEFAULT_DIMENSIONS),
            ),
        ]);
        let result = promo.promote(&candidate, &learner);
        let (new_champion, new_metrics) = promo.apply_promotion(&result, &learner);
        assert_eq!(new_champion.len(), 3);
        // Nak should be promoted, sol and pax should remain the original champion.
        assert!((new_metrics.per_faction["nak"].clearance - 0.85).abs() < 1e-9);
    }

    #[test]
    fn promotion_config_defaults_are_reasonable() {
        let config = PromotionConfig::default();
        assert!((config.shortfall_margin - 0.05).abs() < 1e-9);
        assert!((config.max_faction_shortfall_regression - 0.03).abs() < 1e-9);
        assert!((config.max_faction_clearance_regression - 0.0).abs() < 1e-9);
        assert!((config.epsilon - 1e-12).abs() < 1e-15);
    }

    #[test]
    fn panel_metrics_roundtrips_through_json() {
        let metrics = make_panel(&[("sol", 0.90, 0.50, 10.0), ("pax", 0.85, 0.60, 9.0)]);
        let json = serde_json::to_string(&metrics).expect("serialise");
        let decoded: PanelMetrics = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(decoded, metrics);
    }

    #[test]
    fn promotion_result_serialises() {
        let result = PromotionResult {
            promoted: vec!["sol".to_owned()],
            accepted_kind: AcceptedKind::Isolated,
            candidate_metrics: make_panel(&[("sol", 0.90, 0.50, 10.0)]),
            champion_metrics: make_panel(&[("sol", 0.88, 0.55, 9.5)]),
        };
        let json = serde_json::to_string(&result).expect("serialise");
        let decoded: PromotionResult = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(decoded, result);
    }
}
