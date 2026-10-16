//! Faction hooks for production, placement, payment and action cards (`production.rs`, `action_cards.rs`, `laws.rs`, `supply.rs`).
//!
//! Owned by one wave-B package at a time (`plans/BASE_FACTIONS_PLAN_2026-10-02.md`). Each
//! field is optional and called for every module; dispatch functions live here beside the
//! fields, iterate [`super::MODULES`] in order, and are what the shared engine calls. The rules
//! on `super::Hooks` (atomicity, ordering, ownership) apply.
//!
//! Every hook defaults to the neutral value (identity on a fold, `false` on an "any", `0` on a
//! sum), so with every module empty the shared engine behaves exactly as before BF-00b-economy.
//! Hooks are called for every player and every module: check your own condition.
//!
//! Typed events added by this package (new names, never an existing type at a new place):
//!
//! | Event | Emitted by | Payload |
//! |---|---|---|
//! | `UNITS_PRODUCED` | `production::ProductionWindow` when a use of PRODUCTION ends having produced 1+ unit | `player`, `system`, `source` ("production" / "ability"), `count`, `units` (array of `{unit_type, place}`; `place` is `"space"` or a planet id) |
//! | `ACTION_CARDS_DRAWN` | `action_cards::draw_announced` | `player`, `count` (actually drawn, extra draws included), `source` |
//! | `TRADE_GOODS_GAINED` | `supply::gain_trade_goods_announced` / `_via` | `player`, `amount` (actually gained), `source` |

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{ActionCardId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;

use crate::choice::{ChoiceOption, IllegalChoice, Table};
use crate::production::Spend;

/// Hooks for this area. A module sets only the ones it needs (`..EconomyHooks::NONE`).
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::type_complexity,
    reason = "plain fn-pointer table; aliases would only move the signatures elsewhere"
)]
pub struct EconomyHooks {
    /// What one planet is worth, to this player, as a payment face or as the resources a
    /// production unit on it reads. Receives the value so far (printed value plus attachments and
    /// laws); returns the adjusted value. `kind` is the face asked for.
    ///
    /// For Winnu `htp` Hegemonic Trade Policy ("swap the resource and influence values of 1 planet
    /// you control during that use of Production"): return the planet's other value while the
    /// swap is live. Called from `production::capacity` (the resources a space dock reads) and
    /// `production::payment_faces` (every payment face), both for the *owner of the units / the
    /// payer*. The module owns the knowledge of which planet is swapped and for how long.
    pub planet_spend_value:
        Option<fn(&GameState, &ContentStore, &PlayerId, &PlanetId, Spend, i64) -> i64>,
    /// What one trade good is worth when `player` spends it; receives the worth so far. Called by
    /// `production::trade_good_worth` (production payment, `available`), where the value so far is
    /// 1, or 2 while the hard-coded `mc` technology check is still there; and by `payment::plans` /
    /// `Plan::worth_for` (votes, objective costs), where it starts from 1 and the hard-coded `mc`
    /// check is not applied. `mc` is currently hard-coded in production only; the Mentak package
    /// moves it onto this hook and removes the hard-code in one step (do not both, or it doubles).
    pub trade_good_worth: Option<fn(&GameState, &PlayerId, i64) -> i64>,
    /// Whether a producer may not produce a unit: `(state, content, player, unit_base_type,
    /// producer_base_type)`, e.g. `("infantry", "spacedock")` for Arborec `mitosis` ("Your space
    /// docks cannot produce infantry"). Read in `production::placements`, so a planet whose only
    /// producers are barred is not a spot for the unit, and in `ProductionWindow::build_options`
    /// the barred producers' PRODUCTION does not count toward what that unit may use.
    /// Ability-based production passes "ability" as the producer: blanket bans apply,
    /// while restrictions specific to a producing unit apply only to that unit.
    pub cannot_produce: Option<fn(&GameState, &ContentStore, &PlayerId, &str, &str) -> bool>,
    /// Whether a game effect may not place this resolved unit type from reinforcements.  This is
    /// deliberately separate from `cannot_produce`: effects that place units are not PRODUCTION.
    pub effect_placement_forbidden:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &UnitTypeId) -> bool>,
    /// Form a captured unit returns to its owner's reinforcements in.  Identity defaults to the
    /// captured type; a transforming unit may return in its unflipped form.
    pub captured_unit_return_form:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &UnitTypeId) -> UnitTypeId>,
    /// Places in *other* systems where `player` may put a unit of `unit_base_type` produced in
    /// `system`: `(state, content, sources, player, system, unit_base_type) -> [(system, planet)]`,
    /// `None` for the space area. For Saar's commander ("When you produce fighters or infantry: You
    /// may place each of those units at any of your space docks that are not blockaded"). Read by
    /// `ProductionWindow::spots`, only when the unit has a legal spot in the producing system.
    #[allow(clippy::type_complexity)]
    pub production_destinations: Option<
        fn(
            &GameState,
            &ContentStore,
            SourceSet,
            &PlayerId,
            &SystemId,
            &str,
        ) -> Vec<(SystemId, Option<PlanetId>)>,
    >,
    /// Action cards drawn *in addition* to `requested` when `player` draws `requested >= 1`. For
    /// Yssaril `scheming` ("When you draw 1 or more action cards, draw 1 additional action card").
    /// Called by `action_cards::draw` before the first card leaves the deck.
    pub action_card_draw_bonus: Option<fn(&GameState, &ContentStore, &PlayerId, usize) -> usize>,
    /// Before a requested action-card draw is expanded by bonuses or draws a card.  A module may
    /// ask, cancel, or otherwise alter the request atomically.
    pub action_card_draw_requested: Option<
        fn(
            &mut GameState,
            &ContentStore,
            &mut Table,
            &PlayerId,
            usize,
        ) -> Result<(), IllegalChoice>,
    >,
    /// After `player` drew (the cards just drawn, extra draws included) and before the hand limit
    /// is enforced. For Yssaril `scheming` ("Then, choose and discard 1 action card from your
    /// hand"). Not called when nothing was requested. Atomic: ask first, mutate after. Any choice
    /// asked from here is a new decision site and must be registered in
    /// `tests/decision_delivery_inventory.rs` (`PRODUCERS` and `OBSERVED_ASKS`).
    pub action_cards_drawn: Option<
        fn(
            &mut GameState,
            &ContentStore,
            &mut Table,
            &PlayerId,
            &[ActionCardId],
        ) -> Result<(), IllegalChoice>,
    >,
    /// The action-card hand limit for `player`; receives the limit after laws (Sanctions) have
    /// capped it. For Yssaril `crafty` ("any number of action cards in your hand. Game effects
    /// cannot prevent you from using this ability"): return `usize::MAX`, which also overrides
    /// Sanctions because modules run after the law cap. Called by
    /// `action_cards::enforce_hand_limit`.
    pub action_card_limit: Option<fn(&GameState, &ContentStore, &PlayerId, usize) -> usize>,
    /// Whether `player` cannot play action cards right now. For Yssaril `tp` Transparasteel
    /// Plating ("During your turn of the action phase, players that have passed cannot play action
    /// cards"): the module checks that its owner is the active player and `player` has passed.
    /// Read by `laws::action_cards_forbidden` (so `action_cards::is_playable`, the component
    /// action gate) and exposed as [`action_cards_forbidden`] for the reaction-window gate
    /// `reactions::playable_now`, which is not in this package's files (see the evidence).
    pub action_cards_forbidden: Option<fn(&GameState, &PlayerId) -> bool>,
    /// PRODUCTION added to `system` for `player` as if from a unit of theirs (BF-F3 package K).
    /// Summed into `production::capacity`; the system then counts as having a producer for ships
    /// (which are placed in the space area). Muaat Magmus Reactor ("each supernova that contains 1
    /// or more of your units gains PRODUCTION 5"), Creuss Particle Synthesis (each wormhole in a
    /// system with your ships gains PRODUCTION 1). Sum over modules; `0` is neutral. Called for every
    /// player and system: check your own condition.
    pub extra_production:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &SystemId) -> i64>,
    /// PRODUCTION added to one *planet* of `system` as if from a unit of `player` there. Summed into
    /// `production::capacity`, and a planet with a positive value is a placement spot for ground
    /// forces and structures (`production::placements`, subject to the same planet gates as a real
    /// producer). Argent Aerie Hololattice (PRODUCTION 1 for each planet with 1+ of your structures).
    pub extra_production_planet:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &SystemId, &PlanetId) -> i64>,
    /// Reduction of the combined cost of units `player` produces in `system` (BF-F3 package K),
    /// applied as part of the use's discount pool, so it never reduces below zero. Creuss Particle
    /// Synthesis ("reduce the combined cost ... by 1 for each wormhole in that system"), Argent
    /// Hololattice. Sum over modules.
    pub production_cost_reduction: Option<fn(&GameState, &PlayerId, &SystemId) -> i64>,
    /// Whether `player` draws an extra exploration card when exploring `planet`, and chooses which
    /// to resolve (the other is discarded). Naaz Distant Suns (BF-F3 package H). Any module's `true`.
    pub explore_extra_draw:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &PlanetId) -> bool>,
    /// After `player` finishes exploring `planet` (the card resolved and gains applied), called once
    /// per module at the end of `exploration::explore_with`. Naaz Pre-Fab Arcologies ("ready that
    /// planet"). Atomic: mutate only.
    pub explored: Option<fn(&mut GameState, &ContentStore, SourceSet, &PlayerId, &PlanetId)>,
    /// What `player` may do instead of placing a PDS on `planet` ("When you would place a PDS on a
    /// planet, you may ... instead": Titans Hecatoncheires). Called after the player has picked a
    /// PDS spot, so it is only consulted for a PDS that could be placed; each option's id is
    /// namespaced by its module and must not be `pds`. Offered beside the PDS, never in place of
    /// it. Concatenated over modules in order. Pure: ask nothing, mutate nothing.
    pub pds_placement_alternative: Option<
        fn(
            &GameState,
            &ContentStore,
            SourceSet,
            &PlayerId,
            &SystemId,
            &PlanetId,
        ) -> Vec<ChoiceOption>,
    >,
    /// Carry out the alternative `option` (one of this module's own ids from
    /// [`Self::pds_placement_alternative`]) in place of the PDS. Atomic: re-check everything, return
    /// `false` and leave the state untouched when it cannot happen; `true` once it has.
    pub pds_placement_alternative_performed: Option<
        fn(&mut GameState, &ContentStore, SourceSet, &PlayerId, &SystemId, &PlanetId, &str) -> bool,
    >,
    /// Whether this module may pay for one unit of type `unit` in `system` by returning one captured
    /// unit of that type instead of spending resources ("When you produce a unit: You may return 1
    /// captured unit of that type to produce that unit without spending resources", Cabal
    /// Amalgamation). Consulted while the production window builds its options, so the exchange is
    /// offered beside the paid build and is never a late rejection. Matched by unit type, like every
    /// other capture in the engine. Pure: ask nothing, mutate nothing.
    pub production_unit_exchange:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &SystemId, &str) -> bool>,
    /// Carry out that exchange for one unit of `unit`: return one captured unit of that type to its
    /// owner's reinforcements. Atomic: re-check everything, return `false` and leave the state
    /// untouched when it cannot happen; `true` once it has. No resources are spent; the production
    /// limit is spent by the caller, which is all the card waives.
    pub production_unit_exchange_performed:
        Option<fn(&mut GameState, &ContentStore, SourceSet, &PlayerId, &SystemId, &str) -> bool>,
}

impl EconomyHooks {
    /// No hooks.
    pub const NONE: Self = Self {
        planet_spend_value: None,
        trade_good_worth: None,
        cannot_produce: None,
        effect_placement_forbidden: None,
        captured_unit_return_form: None,
        production_destinations: None,
        action_card_draw_bonus: None,
        action_card_draw_requested: None,
        action_cards_drawn: None,
        action_card_limit: None,
        action_cards_forbidden: None,
        extra_production: None,
        extra_production_planet: None,
        production_cost_reduction: None,
        explore_extra_draw: None,
        explored: None,
        pds_placement_alternative: None,
        pds_placement_alternative_performed: None,
        production_unit_exchange: None,
        production_unit_exchange_performed: None,
    };
}

#[cfg(test)]
thread_local! {
    /// Extra hooks a test registers beside the (empty) module table, per test thread.
    static TEST_HOOKS: std::cell::RefCell<Vec<EconomyHooks>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `run` with `extra` consulted after every module, on this thread only. For tests that prove
/// a call site reaches its hook while the real modules are still empty.
#[cfg(test)]
pub(crate) fn with_test_hooks<T>(extra: EconomyHooks, run: impl FnOnce() -> T) -> T {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_HOOKS.with(|hooks| {
                hooks.borrow_mut().pop();
            });
        }
    }
    TEST_HOOKS.with(|hooks| hooks.borrow_mut().push(extra));
    let _restore = Restore;
    run()
}

#[cfg(not(test))]
fn hooks() -> impl Iterator<Item = EconomyHooks> {
    super::MODULES.iter().map(|module| module.hooks.economy)
}

#[cfg(test)]
fn hooks() -> impl Iterator<Item = EconomyHooks> {
    let extra: Vec<EconomyHooks> = TEST_HOOKS.with(|hooks| hooks.borrow().clone());
    super::MODULES
        .iter()
        .map(|module| module.hooks.economy)
        .chain(extra)
}

/// Emit a new typed event through the caller's timing handle, unconditionally (as combat and
/// ground do). Returns whether the event survived (`true` when the caller has no timing handle:
/// nothing can react). Introducing these events shifts event ids and timing-log lines once.
pub(crate) fn emit(
    ctx: &mut crate::choice::Resolving<'_>,
    state: &mut GameState,
    event_type: &str,
    payload: std::collections::BTreeMap<String, serde_json::Value>,
) -> bool {
    ctx.emit(state, event_type, payload).unwrap_or(true)
}

// -- dispatch, called from the shared economy code ------------------------------------------------

pub(crate) fn planet_spend_value(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    planet: &PlanetId,
    kind: Spend,
    value: i64,
) -> i64 {
    hooks()
        .filter_map(|h| h.planet_spend_value)
        .fold(value, |value, f| {
            f(state, content, player, planet, kind, value)
        })
}

pub(crate) fn trade_good_worth(state: &GameState, player: &PlayerId, worth: i64) -> i64 {
    hooks()
        .filter_map(|h| h.trade_good_worth)
        .fold(worth, |worth, f| f(state, player, worth))
}

pub(crate) fn cannot_produce(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    unit_base: &str,
    producer_base: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.cannot_produce)
        .any(|f| f(state, content, player, unit_base, producer_base))
}

/// Whether any faction rule bars this resolved unit from being placed by a game effect.
pub(crate) fn effect_placement_forbidden(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &UnitTypeId,
) -> bool {
    hooks()
        .filter_map(|h| h.effect_placement_forbidden)
        .any(|f| f(state, content, sources, player, unit))
}

/// Every alternative to placing a PDS on `planet` that `player` has, in module order.
pub(crate) fn pds_placement_alternatives(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Vec<ChoiceOption> {
    hooks()
        .filter_map(|h| h.pds_placement_alternative)
        .flat_map(|f| f(state, content, sources, player, system, planet))
        .collect()
}

/// Carry out the PDS alternative `option`; the first module that owns it and performs it wins.
/// `false` leaves the state untouched.
pub(crate) fn perform_pds_placement_alternative(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    option: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.pds_placement_alternative_performed)
        .any(|f| f(state, content, sources, player, system, planet, option))
}

pub(crate) fn captured_unit_return_form(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    unit: &UnitTypeId,
) -> UnitTypeId {
    hooks()
        .filter_map(|h| h.captured_unit_return_form)
        .fold(unit.clone(), |unit, f| {
            f(state, content, sources, owner, &unit)
        })
}

/// Every module's [`EconomyHooks::production_destinations`], sorted, deduplicated, and without
/// the producing system itself (its own spots are `production::placements`).
pub(crate) fn production_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    unit_base: &str,
) -> Vec<(SystemId, Option<PlanetId>)> {
    let mut out: Vec<(SystemId, Option<PlanetId>)> = hooks()
        .filter_map(|h| h.production_destinations)
        .flat_map(|f| f(state, content, sources, player, system, unit_base))
        .filter(|(at, _)| at != system)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether any module may produce one `unit` in `system` by returning a captured unit of that type
/// instead of paying for it.
pub(crate) fn production_unit_exchange(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    unit: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.production_unit_exchange)
        .any(|f| f(state, content, sources, player, system, unit))
}

/// Perform that exchange. The first module able to honour its own offer wins; `false` changes
/// nothing, which is how a production window drops an exchange that is no longer available.
pub(crate) fn perform_production_unit_exchange(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    unit: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.production_unit_exchange_performed)
        .any(|f| f(state, content, sources, player, system, unit))
}

/// Whether any module sets `cannot_produce` (lets `production` skip its per-producer walk).
pub(crate) fn has_cannot_produce() -> bool {
    hooks().any(|h| h.cannot_produce.is_some())
}

pub(crate) fn action_card_draw_bonus(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    requested: usize,
) -> usize {
    hooks()
        .filter_map(|h| h.action_card_draw_bonus)
        .map(|f| f(state, content, player, requested))
        .sum()
}

pub(crate) fn action_card_draw_requested(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
    requested: usize,
) -> Result<(), IllegalChoice> {
    for hook in hooks().filter_map(|h| h.action_card_draw_requested) {
        hook(state, content, table, player, requested)?;
    }
    Ok(())
}

pub(crate) fn action_cards_drawn(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
    drawn: &[ActionCardId],
) -> Result<(), IllegalChoice> {
    for f in hooks().filter_map(|h| h.action_cards_drawn) {
        f(state, content, table, player, drawn)?;
    }
    Ok(())
}

pub(crate) fn action_card_limit(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    limit: usize,
) -> usize {
    hooks()
        .filter_map(|h| h.action_card_limit)
        .fold(limit, |limit, f| f(state, content, player, limit))
}

/// Whether a faction module stops `player` playing action cards right now (`tp`). Public to the
/// crate so the reaction-window gate can call it.
pub(crate) fn action_cards_forbidden(state: &GameState, player: &PlayerId) -> bool {
    hooks()
        .filter_map(|h| h.action_cards_forbidden)
        .any(|f| f(state, player))
}

/// Extra PRODUCTION modules add to `system` for `player` (see [`EconomyHooks::extra_production`]).
pub(crate) fn extra_production(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    hooks()
        .filter_map(|h| h.extra_production)
        .map(|f| f(state, content, sources, player, system))
        .sum()
}

/// Extra PRODUCTION modules add to one planet (see [`EconomyHooks::extra_production_planet`]).
pub(crate) fn extra_production_planet(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> i64 {
    hooks()
        .filter_map(|h| h.extra_production_planet)
        .map(|f| f(state, content, sources, player, system, planet))
        .sum()
}

/// Combined-cost reduction for production in `system` (see [`EconomyHooks::production_cost_reduction`]).
pub fn production_cost_reduction(state: &GameState, player: &PlayerId, system: &SystemId) -> i64 {
    hooks()
        .filter_map(|h| h.production_cost_reduction)
        .map(|f| f(state, player, system))
        .sum::<i64>()
        .max(0)
}

/// Whether any module grants `player` an extra exploration draw on `planet`.
pub(crate) fn explore_extra_draw(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
) -> bool {
    hooks()
        .filter_map(|h| h.explore_extra_draw)
        .any(|f| f(state, content, sources, player, planet))
}

/// Tell every module `player` finished exploring `planet`.
pub(crate) fn explored(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
) {
    for f in hooks().filter_map(|h| h.explored) {
        f(state, content, sources, player, planet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(name: &str) -> PlayerId {
        PlayerId::new(name)
    }

    #[test]
    fn every_dispatch_is_neutral_with_no_module_hooks() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a", "b"]);
        let (a, planet) = (pid("a"), PlanetId::new("p"));
        let sources = ti4_model::content_types::DEFAULT;
        assert_eq!(
            planet_spend_value(&state, content, &a, &planet, Spend::Resources, 3),
            3
        );
        assert_eq!(trade_good_worth(&state, &a, 1), 1);
        assert!(!cannot_produce(
            &state,
            content,
            &a,
            "infantry",
            "spacedock"
        ));
        assert!(!effect_placement_forbidden(
            &state,
            content,
            sources,
            &a,
            &UnitTypeId::new("infantry")
        ));
        assert_eq!(action_card_draw_bonus(&state, content, &a, 2), 0);
        assert_eq!(action_card_limit(&state, content, &a, 7), 7);
        assert!(!action_cards_forbidden(&state, &a));
        let system = SystemId::new("s");
        assert_eq!(extra_production(&state, content, sources, &a, &system), 0);
        assert_eq!(
            extra_production_planet(&state, content, sources, &a, &system, &planet),
            0
        );
        assert_eq!(production_cost_reduction(&state, &a, &system), 0);
        assert!(!explore_extra_draw(&state, content, sources, &a, &planet));
    }

    #[test]
    fn a_test_hook_is_visible_only_inside_its_scope() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a"]);
        let a = pid("a");
        let doubled = EconomyHooks {
            trade_good_worth: Some(|_, _, worth| worth * 2),
            action_card_limit: Some(|_, _, _, _| usize::MAX),
            ..EconomyHooks::NONE
        };
        with_test_hooks(doubled, || {
            assert_eq!(trade_good_worth(&state, &a, 1), 2);
            assert_eq!(action_card_limit(&state, content, &a, 3), usize::MAX);
        });
        assert_eq!(trade_good_worth(&state, &a, 1), 1, "the scope has ended");
    }

    #[test]
    fn value_hooks_fold_in_order_and_flag_hooks_are_any() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a"]);
        let (a, planet) = (pid("a"), PlanetId::new("p"));
        let sources = ti4_model::content_types::DEFAULT;
        let add_two = EconomyHooks {
            planet_spend_value: Some(|_, _, _, _, _, value| value + 2),
            action_card_draw_bonus: Some(|_, _, _, _| 1),
            ..EconomyHooks::NONE
        };
        let times_ten = EconomyHooks {
            planet_spend_value: Some(|_, _, _, _, _, value| value * 10),
            cannot_produce: Some(|_, _, _, unit, _| unit == "infantry"),
            action_cards_forbidden: Some(|_, _| true),
            effect_placement_forbidden: Some(|_, _, _, _, unit| unit.as_str() == "mech"),
            ..EconomyHooks::NONE
        };
        with_test_hooks(add_two, || {
            with_test_hooks(times_ten, || {
                // registration order: (1 + 2) * 10
                assert_eq!(
                    planet_spend_value(&state, content, &a, &planet, Spend::Influence, 1),
                    30
                );
                assert!(cannot_produce(&state, content, &a, "infantry", "spacedock"));
                assert!(!cannot_produce(&state, content, &a, "cruiser", "spacedock"));
                assert_eq!(action_card_draw_bonus(&state, content, &a, 1), 1);
                assert!(action_cards_forbidden(&state, &a));
                assert!(effect_placement_forbidden(
                    &state,
                    content,
                    sources,
                    &a,
                    &UnitTypeId::new("mech")
                ));
            });
        });
    }
}
