//! Faction hooks for strategy cards, initiative and unit forms (`strategy.rs`, `strategy_cards.rs`, `supply.rs`, `production.rs`, `fleet.rs`): initiative order, strategy-card swaps, capture, mobile docks, dual-form units.
//!
//! Owned by one wave-B package at a time (`plans/BASE_FACTIONS_PLAN_2026-10-02.md`). Each
//! field is optional and called for every module; dispatch functions live here beside the
//! fields, iterate [`super::MODULES`] in order, and are what the shared engine calls. The rules
//! on `super::Hooks` (atomicity, ordering, ownership) apply. Tests install hooks with a
//! `#[cfg(test)] with_test_hooks(hooks, || ..)` that restores on panic, as in `hooks_combat.rs`.
//!
//! BF-00d-strategy adds three hooks; most of the package is plain state and API instead
//! (`plans/evidence/BF-00d-strategy.md`).
//!
//! | Hook | Card text it exists for | Call point |
//! |---|---|---|
//! | [`StrategyHooks::strategy_phase_ended`] | Naalu Telepathic, Gift of Prescience | `phase::advance_phase`, leaving the strategy phase, before initiative is read |
//! | [`StrategyHooks::secondary_waivers`] | Winnu Acquiescence | `strategy.rs`, the follower's secondary choice |
//! | [`StrategyHooks::secondary_waived`] | Winnu Acquiescence | `strategy.rs`, when that option is taken |

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;

use super::MODULES;

/// A way to resolve a strategy card's secondary without spending a command token.
///
/// Offered beside the ordinary follow option as `follow|waived|<id>` (`STRATEGY_KIND`), so the
/// follower chooses whether to use it. Taking it spends nothing; the module's
/// [`StrategyHooks::secondary_waived`] pays whatever it costs (Acquiescence returns to its owner).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondaryWaiver {
    /// Stable id, unique within the module (`"acq"`).
    pub id: String,
    /// What the option says ("resolve the secondary using Acquiescence").
    pub label: String,
    /// What the label names by id, for `details.phrases` of the question (display only).
    pub worded: Option<crate::phrases::Worded>,
}

/// A selected cost for a prerequisite-ignoring research waiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchWaiverPayment {
    /// Stable id for this exact payment target. It is revalidated when the waiver resolves.
    pub id: String,
    /// Player-facing description of the payment target.
    pub label: String,
}

/// A way to research a technology without its prerequisites, paid for by the module (BF-F3).
///
/// The resolver offers the waiver, then one of [`Self::payments`]. The selected payment id is
/// passed back to [`StrategyHooks::research_waiver_paid`], which must revalidate and atomically
/// apply its cost before the technology is granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchWaiver {
    /// Stable id, unique within the module (`"yin_infantry"`).
    pub id: String,
    /// What the option says ("return 1 infantry to ignore its prerequisites").
    pub label: String,
    /// The exact legal costs the researching player may choose now.
    pub payments: Vec<ResearchWaiverPayment>,
}

/// Hooks for this area. Every field is optional; a module sets only what it needs.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::type_complexity,
    reason = "plain fn-pointer table; aliases would only move the signatures elsewhere"
)]
pub struct StrategyHooks {
    /// The strategy phase is over and the action phase is about to begin: write
    /// [`GameState::initiative_overrides`] here (through
    /// `crate::strategy::set_initiative_override`).
    ///
    /// Naalu Telepathic: "At the end of the strategy phase: Place the Naalu '0' token on your
    /// strategy card; you are first in initiative order." Gift of Prescience: "At the end of the
    /// Strategy Phase: Place this card faceup in your play area and place the Naalu '0' token on
    /// your strategy card, you are the first in initiative order. The Naalu player cannot use
    /// their Telepathic faction ability during this game round. Return this card to the Naalu
    /// player at the end of the status phase."
    ///
    /// Called once per module, in [`MODULES`] order. Overrides are cleared when the *next* round
    /// begins, so one a promissory-note window set earlier this round is still there. The hook
    /// has no table, so it decides without asking: a module whose card is a *choice* (Gift is the
    /// holder's) records the choice earlier, in a window, and this reads it. A later module sees
    /// what an earlier one wrote and may refuse to act on it (Telepathic when the token is
    /// already spoken for). `phase::advance_phase` takes no content store; use
    /// `ContentStore::embedded()`.
    pub strategy_phase_ended: Option<fn(&mut GameState)>,
    /// Ways `follower` may resolve the secondary of strategy card `card` (the printed name:
    /// `"Technology"`) without spending a token, while `primary` resolves its strategic action.
    ///
    /// Winnu Acquiescence: "When the Winnu player resolves a strategic action: You do not have to
    /// spend or place a command token to resolve the secondary ability of that strategy card.
    /// Then, return this card to the Winnu player." Pure and read-only: it is also asked when
    /// deciding whether the follower is offered the secondary at all (a follower with no
    /// strategy token but a waiver still is). Only asked for a secondary that costs a token.
    pub secondary_waivers:
        Option<fn(&GameState, &ContentStore, &PlayerId, &PlayerId, &str) -> Vec<SecondaryWaiver>>,
    /// `follower` took waiver `waiver` (a [`SecondaryWaiver::id`]) of this module: pay its cost.
    /// Called once, before the card's secondary is performed, after the choice is validated; no
    /// token has been spent.
    pub secondary_waived: Option<fn(&mut GameState, &ContentStore, &PlayerId, &PlayerId, &str)>,
    /// Whether `player` may score objectives without controlling every planet in their home
    /// system (LRR 61.16 lifted). Any module's `true` lifts it.
    ///
    /// Saar Nomadic: "You can score objectives even if you do not control the planets in your
    /// home system."
    pub scores_without_home: Option<fn(&GameState, &PlayerId) -> bool>,
    /// Extra technology prerequisite holdings for `player` as `(colour, count)` with `colour` one of
    /// a track name (`"BIOTIC"`, `"CYBERNETIC"`, `"PROPULSION"`, `"WARFARE"`) or its colour (`"green"`,
    /// `"yellow"`, `"blue"`, `"red"`), any case; an unknown colour is ignored. Added to the
    /// holdings `technology::prerequisites_met` reads, so they pay for a prerequisite exactly like an
    /// owned technology (and take part in synergies). Yin commander ("this card satisfies a green
    /// technology prerequisite"). Sum over modules.
    pub extra_prerequisite_colours:
        Option<fn(&GameState, &ContentStore, &PlayerId) -> Vec<(String, usize)>>,
    /// A way for `player` to research `tech` ignoring every prerequisite, at a cost the module pays in
    /// [`Self::research_waiver_paid`]. `None` when not available (affordability is the module's to
    /// check). The offer names every legal payment target. Yin commander ("return 1 of your
    /// infantry to reinforcements to ignore its prerequisites" when the tech is owned by another
    /// player). Pure and read-only.
    pub research_waiver_offer:
        Option<fn(&GameState, &ContentStore, &PlayerId, &TechnologyId) -> Option<ResearchWaiver>>,
    /// `player` chose `payment` while researching `tech` using this module's waiver. Revalidate and
    /// pay that exact cost, returning whether it was applied. Called before the technology is
    /// granted; a `false` result leaves the whole research transition unchanged.
    pub research_waiver_paid:
        Option<fn(&mut GameState, &ContentStore, &PlayerId, &TechnologyId, &str) -> bool>,
    /// The unit id `player` uses for `base_type` (`"cruiser"`) instead of `chosen_id`, the one the
    /// upgrade lookup picked. Any module's `Some` wins (first in [`MODULES`] order). Mentak
    /// Corsair's acquisition (`mentak_cruiser3` for a cruiser once the breakthrough is held).
    /// Consulted where production offers units (`production::buildable_for`) and where a
    /// researched upgrade replaces units on the board (`technology::apply_unit_upgrades`).
    pub unit_form_override: Option<
        fn(&GameState, &ContentStore, SourceSet, &PlayerId, &str, &str) -> Option<UnitTypeId>,
    >,
}

impl StrategyHooks {
    /// No hooks.
    pub const NONE: Self = Self {
        strategy_phase_ended: None,
        secondary_waivers: None,
        secondary_waived: None,
        scores_without_home: None,
        extra_prerequisite_colours: None,
        research_waiver_offer: None,
        research_waiver_paid: None,
        unit_form_override: None,
    };
}

// -- dispatch ------------------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// A hook table a test installs beside the (empty) module hooks, so the call sites can be
    /// exercised before any faction registers one. Per test thread.
    static TEST_HOOKS: std::cell::Cell<Option<StrategyHooks>> =
        const { std::cell::Cell::new(None) };
}

/// Run `run` with `hooks` installed beside the module hooks on this thread, restoring the
/// previous table afterwards even if `run` panics.
#[cfg(test)]
pub(crate) fn with_test_hooks<T>(hooks: StrategyHooks, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<StrategyHooks>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_HOOKS.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(TEST_HOOKS.with(|cell| cell.replace(Some(hooks))));
    run()
}

#[cfg(test)]
fn test_table() -> Option<StrategyHooks> {
    TEST_HOOKS.with(std::cell::Cell::get)
}

#[cfg(not(test))]
#[allow(clippy::unnecessary_wraps, reason = "same shape as the test build")]
const fn test_table() -> Option<StrategyHooks> {
    None
}

/// Every module's strategy hooks in [`MODULES`] order.
fn tables() -> impl Iterator<Item = StrategyHooks> {
    MODULES
        .iter()
        .map(|module| module.hooks.strategy)
        .chain(test_table())
}

/// Whether any module lets `player` score without their home system.
pub(crate) fn scores_without_home(state: &GameState, player: &PlayerId) -> bool {
    tables()
        .filter_map(|table| table.scores_without_home)
        .any(|lifted| lifted(state, player))
}

/// Every module's extra prerequisite holdings for `player`, summed per colour.
pub(crate) fn extra_prerequisite_colours(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<(String, usize)> {
    tables()
        .filter_map(|table| table.extra_prerequisite_colours)
        .flat_map(|hook| hook(state, content, player))
        .collect()
}

/// Every module's prerequisite-ignoring way to research `tech`, indexed by its position among the
/// waivers actually offered to `player` for this technology — not among the modules that carry the
/// hook. Registering another faction therefore never renumbers an existing waiver. The same index
/// selects the module that must pay in [`research_waiver_paid`], which re-evaluates the offers on
/// the same state.
pub(crate) fn research_waiver_offers(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    tech: &TechnologyId,
) -> Vec<(usize, ResearchWaiver)> {
    tables()
        .filter_map(|table| table.research_waiver_offer)
        .filter_map(|hook| hook(state, content, player, tech))
        .enumerate()
        .map(|(index, waiver)| (index, waiver))
        .collect()
}

/// Tell the module that offered the research waiver (`index`, from [`research_waiver_offers`])
/// that it was used: it pays the cost.
pub(crate) fn research_waiver_paid(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    tech: &TechnologyId,
    index: usize,
    payment: &str,
) -> bool {
    if let Some(hook) = tables()
        .filter(|table| {
            table
                .research_waiver_offer
                .is_some_and(|offer| offer(state, content, player, tech).is_some())
        })
        .nth(index)
        .and_then(|table| table.research_waiver_paid)
    {
        return hook(state, content, player, tech, payment);
    }
    false
}

/// The unit id a module makes `player` use for `base_type` in place of `chosen_id`, if any.
pub(crate) fn unit_form_override(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
    chosen_id: &str,
) -> Option<UnitTypeId> {
    tables()
        .filter_map(|table| table.unit_form_override)
        .find_map(|hook| hook(state, content, sources, player, base_type, chosen_id))
}

/// Let every module write its initiative overrides as the strategy phase ends.
pub(crate) fn strategy_phase_ended(state: &mut GameState) {
    for hook in tables().filter_map(|table| table.strategy_phase_ended) {
        hook(state);
    }
}

/// Every module's token-free ways to follow `card`'s secondary.
pub(crate) fn secondary_waivers(
    state: &GameState,
    content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    card: &str,
) -> Vec<(usize, SecondaryWaiver)> {
    // The index (position among the modules that have this hook) rides in the offered option id,
    // so two modules may both use the same short waiver id.
    let mut found = Vec::new();
    for (index, hook) in tables()
        .filter_map(|table| table.secondary_waivers)
        .enumerate()
    {
        for waiver in hook(state, content, follower, primary, card) {
            found.push((index, waiver));
        }
    }
    found
}

/// Tell the module that offered the waiver (`index`, as returned by [`secondary_waivers`]) that
/// it was taken.
pub(crate) fn secondary_waived(
    state: &mut GameState,
    content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    index: usize,
    waiver: &str,
) {
    if let Some(hook) = tables()
        .filter_map(|table| table.secondary_waived)
        .nth(index)
    {
        hook(state, content, follower, primary, waiver);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tables_are_neutral() {
        let mut state = crate::fixtures::seated_game(
            &[("a", "sol"), ("b", "hacan")],
            ti4_model::content_types::DEFAULT,
        );
        let before = state.initiative_overrides.clone();
        strategy_phase_ended(&mut state);
        assert_eq!(state.initiative_overrides, before);
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        assert!(
            secondary_waivers(&state, ContentStore::embedded(), &a, &b, "Technology").is_empty()
        );
        let content = ContentStore::embedded();
        let tech = TechnologyId::new("pds");
        assert!(extra_prerequisite_colours(&state, content, &a).is_empty());
        assert!(research_waiver_offers(&state, content, &a, &tech).is_empty());
        assert_eq!(
            unit_form_override(
                &state,
                content,
                ti4_model::content_types::DEFAULT,
                &a,
                "cruiser",
                "cruiser1"
            ),
            None
        );
    }

    #[test]
    fn a_waiver_is_routed_back_to_the_module_that_offered_it() {
        let mut state = crate::fixtures::seated_game(
            &[("a", "sol"), ("b", "hacan")],
            ti4_model::content_types::DEFAULT,
        );
        let content = ContentStore::embedded();
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let hooks = StrategyHooks {
            secondary_waivers: Some(|_, _, _, _, card| {
                vec![SecondaryWaiver {
                    id: "w".to_owned(),
                    label: format!("free {card}"),
                    worded: None,
                }]
            }),
            secondary_waived: Some(|state, _, follower, _, waiver| {
                state
                    .faction_marks
                    .insert(format!("test:{follower}"), waiver.to_owned());
            }),
            ..StrategyHooks::NONE
        };
        with_test_hooks(hooks, || {
            let offered = secondary_waivers(&state, content, &a, &b, "Trade");
            assert_eq!(offered.len(), 1);
            assert_eq!(offered[0].1.label, "free Trade");
            secondary_waived(&mut state, content, &a, &b, offered[0].0, "w");
        });
        assert_eq!(
            state.faction_marks.get("test:a").map(String::as_str),
            Some("w")
        );
    }

    #[test]
    fn the_test_table_is_restored_after_a_panic() {
        let hooks = StrategyHooks {
            strategy_phase_ended: Some(|_| {}),
            ..StrategyHooks::NONE
        };
        let result = std::panic::catch_unwind(|| with_test_hooks(hooks, || panic!("boom")));
        assert!(result.is_err());
        assert!(test_table().is_none());
    }
}
