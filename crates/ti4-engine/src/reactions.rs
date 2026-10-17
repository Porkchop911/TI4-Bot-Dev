//! Action cards played in a timing window rather than as a turn action (M06-016).
//!
//! Forty-two of the corpus's action cards carry the window `Action` and are played as a
//! component action (22.1); [`crate::action_cards`] handles those. The other hundred say things
//! like "After an agenda is revealed" or "At the start of a combat round", and nothing could
//! play them at all.
//!
//! The missing piece was never the stack. [`crate::timing`] implements all of it — WHEN and
//! AFTER windows, LRR 1.19/1.20 ordering, frequency, absolute "cannot" effects, depth-first
//! nesting. What was missing is that **nothing outside a test ever called `Resolver::register`**.
//! Machinery that is built, proven in isolation and connected to nothing is indistinguishable,
//! in an event count, from machinery that was never written.
//!
//! This module is the connection.
//!
//! # Design
//!
//! One [`Ability`] per player per window, registered once when the game is seated, rather than
//! one per card in hand. Cards move constantly — drawn, played, discarded — and [`Resolver`]
//! deliberately has no unregister, because a "cannot" effect must not be removable (LRR 1.6).
//! Registering hands would mean either leaking stale abilities or building the removal path the
//! resolver refuses to have.
//!
//! So a reaction slot is standing furniture. Its *condition* asks whether this player currently
//! holds anything playable in this window, and its *effect* offers those cards and plays the
//! chosen one. The hand is read at resolution time, so it is always current.
//!
//! Frequency is [`Frequency::Unlimited`] rather than once-per-trigger. A player may hold two
//! cards for the same window, and 1.19's round-robin gives them a fresh opportunity after
//! anything else resolves; capping the slot would cap the player at one reaction per event,
//! which is not a rule.
//!
//! Windows are matched on the corpus's printed `window` text, not per alias. Twelve cards share
//! "After an agenda is revealed" and eight share "At the start of a combat round", so aliasing
//! the text is what keeps this a table rather than a hundred branches.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{ActionCardId, PlayerId, SystemId};
use ti4_model::state::{GameState, Player};

use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{
    Ability, Frequency, Relation, Resolver, TimingContext, TimingError, relation_name,
};

/// Whether a card's window applies to this player, given the event.
///
/// Guards may read `GameState` because a printed window can hinge on the board, not just on
/// the event: Crisis asks how many players have not passed, and Deadly Plot asks what the
/// holder voted for and predicted.
type Guard = fn(&Event, &PlayerId, &GameState) -> bool;

/// Where a printed window text hooks onto the engine's events.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// The typed event this window listens for.
    pub event: &'static str,
    /// Whether it resolves before or after that event.
    pub relation: Relation,
    /// Applies to the whole window; per-card narrowing is not modelled yet.
    pub guard: Option<Guard>,
}

/// The event's `player` names this player — "you" in the card text.
fn actor_is(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event.text("player") == Some(player.as_str())
}

/// Somebody else — "another player", "your opponent".
fn actor_is_not(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event
        .text("player")
        .is_some_and(|who| who != player.as_str())
}

/// Courageous to the End: the destroyed ship belongs to the holder and the event states that
/// the removal happened during a space combat. The effect source in `cause` is independent.
fn your_ship_destroyed_during_space_combat(
    event: &Event,
    player: &PlayerId,
    state: &GameState,
) -> bool {
    actor_is(event, player, state) && event.boolean("during_space_combat") == Some(true)
}

/// "When you are negotiating a transaction" — Black Market Dealings. The event names the
/// proposer as `player` and the other chair as `partner`; either chair at the table is
/// negotiating.
fn party_to_transaction(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event.text("player") == Some(player.as_str()) || event.text("partner") == Some(player.as_str())
}

/// "At the start of a combat round" and its kin — Morale Boost, Emergency Repairs, Skilled
/// Retreat, Reveal Prototype. Each acts on the player's own units in this combat, so only the two
/// combatants the event names may play one.
fn combatant(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event.text("attacker") == Some(player.as_str())
        || event.text("defender") == Some(player.as_str())
}

/// "At the start of a combat" (Reveal Prototype): a combatant, and only in the first round —
/// the round event fires every round, but a combat starts once.
fn combatant_in_first_round(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    combatant(event, player, state) && event.integer("round").is_none_or(|round| round == 1)
}

/// "When you would return your strategy card(s) during the status phase" — Political
/// Stability. The window fires per seat, named by that seat, and only a seat that
/// actually holds strategy cards has anything to keep.
fn holds_strategy_cards(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    actor_is(event, player, state)
        && state
            .player(player)
            .is_some_and(|seat| !seat.strategy_cards.is_empty())
}

/// "At the start of another player's turn, if they have a readied strategy card" —
/// Extreme Duress. The event names the seat whose turn is beginning, and it is that seat
/// — not the holder — that must have a readied card to be pressured over.
fn another_seat_has_a_readied_strategy_card(
    event: &Event,
    player: &PlayerId,
    state: &GameState,
) -> bool {
    actor_is_not(event, player, state)
        && state
            .active
            .as_ref()
            .and_then(|active| state.player(active))
            .is_some_and(Player::has_unused_strategy_card)
}

/// A rival's landing on a planet this player controls — "after another player commits units
/// to land on a planet you control." Both halves are named by the card text, so neither is
/// optional: the committer is someone else, and the planet's controller is this player.
fn commit_on_your_planet(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    actor_is_not(event, player, state)
        && event
            .text("controller")
            .is_some_and(|holder| holder == player.as_str())
}

/// "After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units
/// or abilities." The sustained ship belongs to someone else (the window's `actor_is_not`
/// half), but that half alone is not enough: any sustained hit on a rival's fleet would
/// qualify. The event's `producer` names the player whose unit or ability produced the
/// cancelled hit, and only the holder's own production counts.
fn direct_hit_guard(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    actor_is_not(event, player, state)
        && event
            .text("producer")
            .is_some_and(|who| who == player.as_str())
        // Dreadnought II and its kin cannot be destroyed by Direct Hit, so the card is not offered.
        && event.boolean("direct_hittable") != Some(false)
}

/// "When another player plays an action card other than 'Sabotage'": the committer is
/// someone else, and the card being played is not one of the four Sabotage copies — Sabotage
/// cancels other cards being played, not itself (1.15 would otherwise let a chain of
/// Sabotages spend the whole deck for nothing).
fn system_contains_player_units(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    if let Some(system) = event.text("system") {
        let sys = state.system_state(&SystemId::new(system));
        return sys.units.iter().any(|u| u.owner == *player);
    }
    false
}

fn system_contains_player_structures(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    if let Some(system) = event.text("system") {
        let sys = state.system_state(&SystemId::new(system));
        return sys.units.iter().any(|u| {
            u.owner == *player && (u.type_id.as_str() == "spacedock" || u.type_id.as_str() == "pds")
        });
    }
    false
}

fn system_contains_player_command_token(
    event: &Event,
    player: &PlayerId,
    state: &GameState,
) -> bool {
    if let Some(system) = event.text("system") {
        let sys = state.system_state(&SystemId::new(system));
        return sys.command_tokens.contains(player);
    }
    false
}

fn another_players_card_is_not_sabotage(
    event: &Event,
    player: &PlayerId,
    state: &GameState,
) -> bool {
    actor_is_not(event, player, state)
        && !is_sabotage_play(event)
        // Last Bastion commander: "Your action cards cannot be canceled by 'Sabotage'".
        && !event.text("player").is_some_and(|actor| {
            crate::promissory::has_commander_ability(
                state,
                &PlayerId::new(actor),
                "bastioncommander",
            )
        })
}

/// The `ACTION_CARD_PLAYED` payload names one of the four Sabotage copies.
fn is_sabotage_play(event: &Event) -> bool {
    matches!(
        event.text("card"),
        Some("sabo1" | "sabo2" | "sabo3" | "sabo4")
    )
}

/// "When another player is elected as the outcome of an agenda" (Confounding Legal Text).
///
/// The window reads the event's `elected_player` payload rather than the raw outcome in
/// `player`: the driver sets it only when the outcome is a real seat. An agenda that elects a
/// law, a planet, or nothing (For/Against) names no seat there, so the window is silent on it
/// — plain `actor_is_not` would match a law alias or "for" against every chair and offer the
/// card on an agenda that elects no one.
fn another_player_elected(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event
        .text("elected_player")
        .is_some_and(|who| who != player.as_str())
}

/// "When your last ship in the active system is destroyed": the destroyed ship belongs to
/// this player, and it was their last one in the system -- the `last` fact the combat window
/// recomputes from the board right before the emission (Crash Landing).
fn your_last_ship(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    actor_is(event, player, state) && event.boolean("last") == Some(true)
}

/// "If you voted for or predicted another outcome" (Deadly Plot), read at the moment the
/// agenda's outcome would be resolved. The event's `player` is the outcome about to be
/// resolved; the vote itself is not in the event, so the driver mirrors the ballot into
/// `GameState.agenda_votes` before the window opens (the ballot itself lives in the vote
/// window the driver holds). A prediction encodes its outcome up to a `|` separator, and a
/// player who predicted gave up the vote, so the two sources never double-count.
fn voted_or_predicted_another_outcome(event: &Event, player: &PlayerId, state: &GameState) -> bool {
    let Some(outcome) = event.text("player") else {
        return false;
    };
    let voted_other = state
        .agenda_votes
        .get(player)
        .is_some_and(|voted| voted.as_str() != outcome);
    let predicted_other = state
        .agenda_predictions
        .get(player)
        .is_some_and(|prediction| prediction.split('|').next() != Some(outcome));
    voted_other || predicted_other
}

/// "If there are at least 2 players who have not passed" (Crisis), counted at the moment a
/// turn ends. A player who just acted has not passed and still counts; a player who just
/// passed is marked before the turn moves, so the count is the one the window printed.
fn at_least_two_players_have_not_passed(
    _event: &Event,
    _player: &PlayerId,
    state: &GameState,
) -> bool {
    state.players.iter().filter(|seat| !seat.passed).count() >= 2
}

/// Anybody at all — the window applies whoever the event names.
fn anyone(_: &Event, _: &PlayerId, _state: &GameState) -> bool {
    true
}

const fn window(event: &'static str, relation: Relation) -> Window {
    Window {
        event,
        relation,
        guard: None,
    }
}

const fn guarded(event: &'static str, relation: Relation, guard: Guard) -> Window {
    Window {
        event,
        relation,
        guard: Some(guard),
    }
}

/// Printed window text to where it hooks.
///
/// Keyed by the exact corpus string, which is why [`unmapped_windows`] exists: a typo or a
/// reworded card would otherwise silently produce a card that can never be played, which is the
/// state this module exists to end.
///
/// Built once per process: the engine looks windows up several times per step.
#[must_use]
pub fn window_table() -> &'static BTreeMap<&'static str, Window> {
    static TABLE: std::sync::LazyLock<BTreeMap<&'static str, Window>> =
        std::sync::LazyLock::new(build_window_table);
    &TABLE
}

#[allow(
    clippy::too_many_lines,
    reason = "one row per printed window: the table is the point, and splitting it hides the set"
)]
fn build_window_table() -> BTreeMap<&'static str, Window> {
    use Relation::{After, When};
    [
        (
            "After an agenda is revealed",
            window("AGENDA_REVEALED", After),
        ),
        (
            "When an agenda is revealed",
            window("AGENDA_REVEALED", When),
        ),
        (
            "At the start of the agenda phase",
            window("AGENDA_PHASE_BEGAN", After),
        ),
        (
            "At the start of a combat round",
            guarded("COMBAT_ROUND_STARTED", After, combatant),
        ),
        (
            "At the start or end of a combat round",
            guarded("COMBAT_ROUND_STARTED", After, combatant),
        ),
        (
            "At the start of the first round of a space combat",
            guarded("SPACE_COMBAT_STARTED", After, combatant),
        ),
        (
            "After you activate a system",
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After you activate a system that contains 1 or more of your ships",
            // Which ships are in the system is a content question (Upgrade needs a cruiser):
            // `action_cards::reaction_can_matter`.
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "When another player plays an action card other than 'Sabotage'",
            guarded("ACTION_CARD_PLAYED", When, another_players_card_is_not_sabotage),
        ),
        // Activation, seen from either chair. Five printed windows, one event: what separates
        // them is whose activation it was, which is exactly what the guard is for.
        (
            "After you activate a system that contains another player's ships",
            // Another player's ships: `action_cards::reaction_can_matter` (Rally).
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After you activate an anomaly",
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After another player activates a system that contains your units",
            guarded("SYSTEM_ACTIVATED", After, |e, p, s| {
                actor_is_not(e, p, s) && system_contains_player_units(e, p, s)
            }),
        ),
        (
            "After another player activates a system that contains 1 of your command tokens",
            guarded("SYSTEM_ACTIVATED", After, |e, p, s| {
                actor_is_not(e, p, s) && system_contains_player_command_token(e, p, s)
            }),
        ),
        (
            "After another player activates a system that contains 1 or more of your structures",
            guarded("SYSTEM_ACTIVATED", After, |e, p, s| {
                actor_is_not(e, p, s) && system_contains_player_structures(e, p, s)
            }),
        ),
        (
            "At the start of a combat",
            guarded("COMBAT_ROUND_STARTED", After, combatant_in_first_round),
        ),
        (
            "When another player chooses a strategy card during the strategy phase",
            guarded("STRATEGY_CARD_CHOSEN", After, actor_is_not),
        ),
        (
            "At the start of an invasion in a system that contains 1 or more of your opponents' PDS units",
            window("INVASION_BEGAN", After),
        ),
        (
            "After the active player moves ships into the active system during a tactical action",
            guarded("SHIP_MOVED", After, actor_is),
        ),
        (
            "After a player moves ships into a system that contains your ships",
            guarded("SHIP_MOVED", After, anyone),
        ),
        (
            "At the start of another player's turn, if they have a readied strategy card",
            guarded(
                "TURN_BEGAN",
                After,
                another_seat_has_a_readied_strategy_card,
            ),
        ),
        (
            "When you would return your strategy card(s) during the status phase",
            guarded("STRATEGY_CARDS_WOULD_RETURN", When, holds_strategy_cards),
        ),
        ("At the end of a player's turn, if you have passed", guarded("PLAYER_PASSED", After, actor_is)),
        (
            "When you are elected as the outcome of an agenda",
            guarded("AGENDA_RESOLVED", When, actor_is),
        ),
        (
            "When another player is elected as the outcome of an agenda",
            guarded("AGENDA_RESOLVED", When, another_player_elected),
        ),
        (
            "During the agenda phase when an outcome would be resolved",
            guarded(
                "AGENDA_RESOLVED",
                When,
                voted_or_predicted_another_outcome,
            ),
        ),
        (
            "When another player would perform a strategic action",
            guarded("STRATEGIC_ACTION_BEGAN", When, actor_is_not),
        ),
        (
            "At the end of any players turn, if there are at least 2 players who have not passed",
            guarded(
                "TURN_PASSED",
                After,
                at_least_two_players_have_not_passed,
            ),
        ),
        (
            "After you perform an action",
            guarded("ACTION_COMPLETED", After, actor_is),
        ),
        (
            "When you gain control of a planet",
            guarded("PLANET_CONTROL_GAINED", When, actor_is),
        ),
        (
            "After another player gains control of a planet you control",
            guarded("PLANET_CONTROL_GAINED", After, |e, p, s| {
                actor_is_not(e, p, s) && e.text("previous_owner") == Some(p.as_str())
            }),
        ),
        (
            "After you cast votes on an outcome of an agenda",
            guarded("VOTES_CAST", After, actor_is),
        ),
        ("After the speaker votes on an agenda", window("VOTES_CAST", After)),
        (
            "At the start of an invasion",
            window("INVASION_BEGAN", After),
        ),
        (
            "After you win a space combat",
            guarded("SPACE_COMBAT_WON", After, actor_is),
        ),
        (
            "After another player discards an action card that has a component action",
            guarded("ACTION_CARD_DISCARDED", After, actor_is_not),
        ),
        (
            "When you are negotiating a transaction",
            guarded("TRANSACTION_OPENED", When, party_to_transaction),
        ),
        (
            "When 1 or more of your units use PRODUCTION",
            guarded("PRODUCTION_USED", After, actor_is),
        ),
        // The eleven windows that had no moment. Each event names the player the window is written
        // from, so `actor_is` and `actor_is_not` divide one event between "yours" and "another
        // player's" rather than needing two events.
        //
        // `When` for a window that must resolve *before* the event's ordinary effect -- assigning
        // hits, losing the last ship -- and `After` for one that reacts to it having happened.
        (
            "After 1 of your ships is destroyed during a space combat",
            guarded(
                "SHIP_DESTROYED",
                After,
                your_ship_destroyed_during_space_combat,
            ),
        ),
        (
            "When your last ship in the active system is destroyed",
            guarded("SHIP_DESTROYED", When, your_last_ship),
        ),
        (
            "When one of your ships uses SUSTAIN DAMAGE during combat",
            guarded("SUSTAIN_DAMAGE_USED", When, actor_is),
        ),
        (
            "After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities",
            guarded("SUSTAIN_DAMAGE_USED", After, direct_hit_guard),
        ),
        (
            "Before you roll dice for ANTI-FIGHTER BARRAGE",
            guarded("ANTI_FIGHTER_BARRAGE_STARTED", When, actor_is),
        ),
        (
            "At the start of the strategy phase",
            window("STRATEGY_PHASE_BEGAN", After),
        ),
        (
            "Before you assign hits to your ships during a space combat",
            guarded("HITS_TO_ASSIGN", When, actor_is),
        ),
        (
            "Before you assign hits produced by another player's SPACE CANNON roll",
            guarded("SPACE_CANNON_HITS", When, actor_is),
        ),
        (
            "After your opponent declares a retreat during a space combat",
            guarded("RETREAT_DECLARED", After, actor_is_not),
        ),
        (
            "At the start of the 'Announce Retreats' step of space combat, if you are the defender",
            guarded("RETREAT_STEP_STARTED", After, actor_is),
        ),
        (
            "After your ground forces make combat rolls during a round of ground combat",
            guarded("GROUND_ROLLS_MADE", After, actor_is),
        ),
        (
            "After another player makes a BOMBARDMENT, SPACE CANNON, or ANTI-FIGHTER BARRAGE roll",
            guarded("UNIT_ABILITY_ROLLED", After, actor_is_not),
        ),
        (
            "After another player commits units to land on a planet you control",
            guarded("UNITS_COMMITTED", After, commit_on_your_planet),
        ),
        // Lie in Wait. `anyone`, deliberately: the window is about *neighbours* transacting, so
        // neither party to the deal is the player who plays the card -- `actor_is` is plainly wrong
        // and `actor_is_not` would let any non-party play it. Whether two neighbours have actually
        // traded is the card's own question, answered from the round's record.
        (
            "After 2 of your neighbors resolve a transaction",
            guarded("TRANSACTION_RESOLVED", After, anyone),
        ),
    ]
    .into_iter()
    .collect()
}

/// Typed events this engine actually emits.
///
/// Kept beside the window table because the two together decide whether a card can be played.
/// A window mapped to an event nobody emits is a table entry, not a connection — the distinction
/// this project keeps having to relearn.
pub const EMITTED_EVENTS: &[&str] = &[
    "ACTION_CARD_PLAYED",
    "AGENDA_RESOLVED",
    "ACTION_COMPLETED",
    "PLANET_CONTROL_GAINED",
    "PLAYER_PASSED",
    "SHIP_MOVED",
    "VOTES_CAST",
    "AGENDA_PHASE_BEGAN",
    "AGENDA_REVEALED",
    "COMBAT_ROUND_STARTED",
    "INVASION_BEGAN",
    "PRODUCTION_USED",
    "SPACE_COMBAT_STARTED",
    "SPACE_COMBAT_WON",
    "STRATEGY_CARD_CHOSEN",
    "STRATEGIC_ACTION_BEGAN",
    "SYSTEM_ACTIVATED",
    "SHIP_DESTROYED",
    "STRATEGY_PHASE_BEGAN",
    "SUSTAIN_DAMAGE_USED",
    "ANTI_FIGHTER_BARRAGE_STARTED",
    "GROUND_ROLLS_MADE",
    "HITS_TO_ASSIGN",
    "UNIT_ABILITY_ROLLED",
    "RETREAT_DECLARED",
    "RETREAT_STEP_STARTED",
    "SPACE_CANNON_HITS",
    "UNITS_COMMITTED",
    "TURN_PASSED",
    "TURN_BEGAN",
    "STRATEGY_CARDS_WOULD_RETURN",
    "ACTION_CARD_DISCARDED",
    "TRANSACTION_RESOLVED",
    "TRANSACTION_OPENED",
];

/// Printed windows that cannot yet be reacted to, with the reason.
///
/// Listed rather than counted, so the gap stays a set of reasons instead of a number.
///
/// Two different reasons appear here, and they want different work:
///
/// * **No such moment.** The window names a point inside a resolution step — between rolling and
///   assigning, or as a retreat is declared — and the step resolves whole. These need the step
///   decomposed before anything can be emitted.
/// * **Moment exists, binding deferred.** The event is emitted and carries what the window needs;
///   what is missing is the entry in [`window_table`] that lets a card subscribe. Adding one
///   changes what [`arm`] registers for *every* seat, which changes what deciders are asked and
///   therefore the behavioural baseline in `ti4-sim`. That baseline may only move through the
///   versioned process in `crates/ti4-sim/src/behavior.rs`, with the cause recorded and reviewed.
///   These bind when the cards that read them are implemented and the baseline moves once, rather
///   than moving it now for windows no implemented card can use.
#[must_use]
pub fn unsupported_windows() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::new()
}

/// Where this card hooks, or `None` if it is not a reaction card at all.
///
/// Looked up against the whole corpus deliberately. Which cards are in the deck is decided by
/// deck building; this only needs to know what a card says.
#[must_use]
pub fn window_for(content: &ContentStore, alias: &ActionCardId) -> Option<Window> {
    let printed = content
        .get(ContentType::ActionCards, alias.as_str())
        .and_then(|record| record.text("window"))?;
    if printed.trim() == "Action" {
        return None; // a component action, handled as a turn action
    }
    window_table().get(printed.trim()).copied()
}

/// Cards in hand that could be played into this event's window, right now.
///
/// The window decides whether the card speaks to this event at all, and the window's guard
/// resolves "you" against "another player". The per-card playability check — 22.3, whether the
/// effect has anything to act on — is [`crate::action_cards::reaction_can_matter`]: cards that
/// share a printed window (Bunker and Blitz) need different things from it.
#[must_use]
pub fn playable_now(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    event: &Event,
    relation: Relation,
) -> Vec<ActionCardId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    // A faction card that stops this player playing action cards (Yssaril Transparasteel
    // Plating). Deliberately not `laws::action_cards_forbidden`: Political Censure is not enforced
    // in reaction windows today and this must not start it.
    if crate::factions::hooks_economy::action_cards_forbidden(state, player) {
        return Vec::new();
    }
    // Cards lying on Garbozia are played "as if they were in your hand", so they are read beside
    // it rather than through a path of their own -- every guard below applies to them unchanged.
    let salvaged = crate::legendary::salvaged(state, player);
    seat.action_cards
        .iter()
        .chain(salvaged.iter())
        .filter(|alias| {
            // Check if the card is set to Never in the player's reaction modes.
            if seat.reaction_card_modes.get(*alias) == Some(&ti4_model::state::ReactionMode::Never)
            {
                return false;
            }
            window_for(content, alias).is_some_and(|window| {
                window.event == event.event_type
                    && window.relation == relation
                    && window.guard.is_none_or(|guard| guard(event, player, state))
            }) && crate::action_cards::reaction_can_matter(state, content, player, alias, event)
        })
        .cloned()
        .collect()
}

/// Play a reaction card: discard it, announce it, then resolve it.
///
/// Discarded first and announced before resolving, because Sabotage cancels *another card being
/// played* — it hooks the WHEN window of `ACTION_CARD_PLAYED`, which only exists if the card is
/// announced before its effect runs. The card is still spent when cancelled: 1.15 lets a WHEN
/// ability cancel the event, not un-spend the card.
///
/// # Errors
/// [`TimingError`] when announcing the play cannot be resolved.
pub fn play(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
    player: &PlayerId,
    alias: &ActionCardId,
    window: Option<&Event>,
) -> Result<bool, TimingError> {
    if !spend(context, player, alias) {
        // Not in hand and not on Garbozia: there was nothing to play.
        return Ok(false);
    }
    announce(context, resolver, player, alias, window)
}

/// Take the card out of the hand (or off Garbozia) into the discard pile. `false` when the
/// player holds no such card.
fn spend(context: &mut TimingContext<'_>, player: &PlayerId, alias: &ActionCardId) -> bool {
    let held = context
        .state
        .player(player)
        .and_then(|seat| seat.action_cards.iter().position(|held| held == alias));
    if let Some(index) = held {
        crate::action_cards::discard(context.state, player, index);
        true
    } else {
        crate::legendary::purge_salvaged(context.state, player, alias)
    }
}

/// Announce a card as played, then resolve it.
///
/// Shared by the reaction window and the component action (22.1), because the announcement is the
/// same event either way: Sabotage cancels *another card being played*, and it hooks the WHEN
/// window of `ACTION_CARD_PLAYED`, which only exists if every path announces before resolving.
///
/// `window` is the event whose window the card was played in (`None` outside any window, as a
/// component action): what the card will do is read against it, for the questions of the
/// announcement's own windows (display only, `DecisionContext::pending_effect`).
///
/// # Errors
/// [`TimingError`] when the announcement cannot be resolved.
pub fn announce(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
    player: &PlayerId,
    alias: &ActionCardId,
    window: Option<&Event>,
) -> Result<bool, TimingError> {
    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("card".to_owned(), alias.to_string().into());
    let announced = context.event_sequence.next("ACTION_CARD_PLAYED", payload)?;
    // What the card will do, as its windows show it before it resolves. A pure read: it changes
    // no state, option or event.
    let id = announced.id;
    if let Some(facts) = crate::reaction_preview::pending_facts(
        context.state,
        context.content,
        context.sources,
        player,
        window.unwrap_or(&announced),
        alias,
    ) {
        resolver.hold_pending_effect(id, facts);
    }
    context
        .state
        .action_card_plays
        .push((player.clone(), alias.clone()));
    let announced = resolver.emit_with_context(context, announced, |_, _| {});
    resolver.release_pending_effect(id);
    let announced = announced?;
    // The card is still spent when cancelled: 1.15 lets a WHEN ability cancel the event, not
    // un-spend the card — and a spent card is a discarded one, so the discard is announced on
    // both exits. Reverse Engineer reads exactly this moment.
    if announced.cancelled {
        announce_discard(context, resolver, player, alias)?;
        return Ok(false);
    }

    // A card with no registered effect is announced unresolved rather than passed off as having
    // done something. This is the registry design used everywhere else here: a gap is visible.
    if let Some(effect) = crate::action_cards::effect_for(alias) {
        crate::action_cards::with_action_card_source(alias, || effect(context, player));
    } else {
        let mut payload = BTreeMap::new();
        payload.insert("card".to_owned(), alias.to_string().into());
        let unresolved = context
            .event_sequence
            .next("ACTION_CARD_UNRESOLVED", payload)?;
        resolver.emit_with_context(context, unresolved, |_, _| {})?;
    }
    // The card left the hand before the play was announced, so it is in the discard pile now —
    // recorded before the event, so a card played into the window finds it there.
    announce_discard(context, resolver, player, alias)?;
    // An effect may have staged a destruction (Direct Hit destroys the sustained ship from
    // inside the window, where it holds no resolver): announce each removal through the game's
    // resolver now, so the event's own WHEN and AFTER windows open around it. The ship is off
    // the board before this runs, so `last` is read from the position a reacting card would see.
    for (system, owner, unit_type, cause, during_space_combat) in
        std::mem::take(&mut context.state.pending_destructions)
    {
        // The same handoff the combat window's own emissions make: a reacting effect that
        // needs to know which ship was destroyed cannot read the event once the window runs.
        context.state.last_ship_destroyed =
            Some((system.clone(), owner.clone(), unit_type.clone()));
        let payload = crate::combat::ship_destroyed_payload(
            &mut *context.state,
            context.content,
            context.sources,
            &system,
            &owner,
            &unit_type,
            &cause,
            during_space_combat,
        );
        let destroyed = context.event_sequence.next("SHIP_DESTROYED", payload)?;
        resolver.emit_with_context(context, destroyed, |_, _| {})?;
    }
    Ok(true)
}

/// Record the just-played card in the discard pile and open the window that reads it.
///
/// Every path into [`announce`] has already removed the card from its owner's hand, so the
/// pile push is the discard itself — a steal or a transaction never reaches here, and those
/// cards never enter the pile. The push precedes the event so that a card played into the
/// window (Reverse Engineer) finds its target in the pile, and the event names the discarded
/// card's alias because the window's printed qualifier — "that has a component action" — is
/// a fact about the card, which the effect checks against the content, and the pile lookup
/// needs the very card that left play rather than whichever card shares its name.
///
/// # Errors
/// [`TimingError`] when the announcement cannot be resolved.
fn announce_discard(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
    player: &PlayerId,
    alias: &ActionCardId,
) -> Result<(), TimingError> {
    // Data Skimmer: another player's discard is placed on the breakthrough, not the pile.
    crate::action_cards::discarded(context.state, player, alias, true);
    context.state.last_action_discarded = Some((player.clone(), alias.clone()));
    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("card".to_owned(), alias.to_string().into());
    let discarded = context
        .event_sequence
        .next("ACTION_CARD_DISCARDED", payload)?;
    resolver.emit_with_context(context, discarded, |_, _| {})?;
    Ok(())
}

/// Open the windows for the card events faction hooks staged
/// (`factions::hooks_cards::{discard_chosen, take_revealed_action_card}`), oldest first, and
/// return how many were announced.
///
/// A hook runs with a `TimingContext`, which has no resolver, so it can only stage; whoever owns
/// both a context and the resolver calls this after the hook returns. A staged discard becomes
/// `ACTION_CARD_DISCARDED` exactly as a played card's does (the pile and
/// `last_action_discarded` are set by [`announce_discard`]); a staged take becomes the new
/// `ACTION_CARD_TAKEN` (`player` = taker, `from`; deliberately no card id). With nothing staged — every game
/// without a faction module — this does nothing at all.
///
/// # Errors
/// [`TimingError`] when a window cannot be resolved.
pub fn announce_staged_card_events(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
) -> Result<usize, TimingError> {
    use crate::factions::hooks_cards::{StagedCardEvent, drain_staged, has_staged};
    if !has_staged(context.state) {
        return Ok(0);
    }
    let staged = drain_staged(context.state);
    let count = staged.len();
    for event in staged {
        match event {
            StagedCardEvent::Discarded { player, card } => {
                announce_discard(context, resolver, &player, &card)?;
            }
            StagedCardEvent::Taken { player, from, .. } => {
                let mut payload = BTreeMap::new();
                payload.insert("player".to_owned(), player.to_string().into());
                payload.insert("from".to_owned(), from.to_string().into());
                // No `card`: which card moved is known to the taker and the owner only, and
                // a typed event is evaluated in every seat's window.
                let taken = context.event_sequence.next("ACTION_CARD_TAKEN", payload)?;
                resolver.emit_with_context(context, taken, |_, _| {})?;
            }
        }
    }
    Ok(count)
}

/// The choice kind the oracle offers reaction cards under (engine/reactions.py:320–324).
pub const ACTION_CARD_KIND: &str = "action_card";

/// The options the inner card choice offers: one per *card*, not per alias.
///
/// Mirrors the oracle's `{name: a for a in reversed(available)}.values()` (engine/reactions.py:
/// 319–324): a card printed four times has four aliases, and holding two copies must not offer
/// it twice — a bot that samples per option would draw extra weight from the duplicate. The
/// reverse walk fixes each name's slot; the alias kept is the first one in hand order.
fn reaction_card_options(
    content: &ContentStore,
    options: &[ActionCardId],
) -> Vec<crate::choice::ChoiceOption> {
    let mut slots: Vec<(String, ActionCardId)> = Vec::new();
    for alias in options.iter().rev() {
        let name = crate::action_cards::name_of(content, alias);
        match slots.iter_mut().find(|(slot_name, _)| *slot_name == name) {
            Some(slot) => slot.1 = alias.clone(),
            None => slots.push((name, alias.clone())),
        }
    }
    slots
        .into_iter()
        .map(|(_, alias)| {
            crate::choice::ChoiceOption::labelled(
                alias.to_string(),
                ACTION_CARD_KIND,
                format!("play {}", crate::action_cards::name_of(content, &alias)),
            )
            .with("card", alias.to_string())
            .with("card_name", crate::action_cards::name_of(content, &alias))
        })
        .collect()
}

/// One reaction opportunity, for one player, in one window.
///
/// `owner_name` is the faction name that appears in the ability id: the oracle builds
/// `f"reaction:{player}:{event_type}:{relation.value}"` (engine/reactions.py:332) and a Python
/// player's identity is its faction, with the relation lowercase (`"when"` / `"after"`).
fn slot(owner_name: &str, player: &PlayerId, event_type: &str, relation: Relation) -> Ability {
    let owner = player.clone();
    let condition_owner = player.clone();
    Ability::stateful(
        format!(
            "reaction:{owner_name}:{event_type}:{}",
            relation_name(relation)
        ),
        player.clone(),
        event_type,
        relation,
        Arc::new(move |event, resolver, context| {
            let options = playable_now(context.state, context.content, &owner, event, relation);
            let Some(first) = options.first().cloned() else {
                return Ok(());
            };
            let chosen = if options.iter().all(|card| {
                crate::action_cards::name_of(context.content, card)
                    == crate::action_cards::name_of(context.content, &first)
            }) {
                first
            } else {
                // "May": the player who opened this step can still back out of it. Declining
                // plays nothing and the window offers its slot again, so the outer Pass stays.
                let mut card_options = reaction_card_options(context.content, &options);
                card_options.push(crate::choice::ChoiceOption::decline());
                let offered: Vec<ActionCardId> = card_options
                    .iter()
                    .filter_map(|option| option.payload.get("card")?.as_str())
                    .map(ActionCardId::new)
                    .collect();
                let choice = crate::choice::Choice::new(
                    owner.clone(),
                    format!(
                        "play an action card ({} {})",
                        relation_name(relation),
                        event.event_type
                    ),
                    card_options,
                )
                .contextualized(
                    DecisionContext::new(
                        owner.clone(),
                        DecisionSource::Rule("22.1".to_owned()),
                        format!(
                            "play_reaction_{}_{}",
                            relation_name(relation),
                            event.event_type
                        ),
                        context.state.phase,
                        context.state.round,
                    )
                    .optional(true)
                    .about_battle(context.state)
                    .with_previews(crate::reaction_preview::previews_for(
                        context.state,
                        context.content,
                        context.sources,
                        &owner,
                        event,
                        offered.iter().map(|card| (card, card.to_string())),
                    ))
                    .with_pending_effect(resolver.pending_effect(event, &owner))
                    .with_trigger(
                        crate::decision_context::DecisionTrigger::from_event(
                            event,
                            relation_name(relation),
                            &resolver.emission_chain(event),
                        ),
                    ),
                );
                match context.ask_seeing(&choice) {
                    Ok(answer) if answer.is_decline() => return Ok(()),
                    Ok(answer) => ActionCardId::new(answer.id),
                    Err(_) => return Ok(()),
                }
            };
            // Rules version 3: a play in an ordinary window is only declared here (the card is
            // spent); it is announced, and Sabotage offered, once every player has declared.
            // The Sabotage window itself is the answer to a play and stays immediate.
            if context.state.rules_version >= ti4_model::state::RULES_SABOTAGE_TIMING
                && event.event_type != "ACTION_CARD_PLAYED"
            {
                if spend(context, &owner, &chosen) {
                    context
                        .state
                        .declared_card_plays
                        .push((owner.clone(), chosen));
                }
                return Ok(());
            }
            let played = play(context, resolver, &owner, &chosen, Some(event))?;
            // Sabotage: "Cancel that action card." The cancelled event is the one that
            // opened this window, and only this closure still holds it: a card's effect
            // signature carries no event, and by the time the played card's effect runs,
            // the triggering event is back in its own frame. A Sabotage spent in any other
            // window (none exists today) answers nothing, exactly as the window table says.
            if played
                && event.event_type == "ACTION_CARD_PLAYED"
                && crate::action_cards::name_of(context.content, &chosen) == "Sabotage"
            {
                event.cancel();
            }
            Ok(())
        }),
    )
    .with_optional(true)
    // A player may hold two cards for the same window, and 1.19 gives them a fresh
    // opportunity after anything else resolves. Capping the slot would cap the player at one
    // reaction per event, which is not a rule.
    .with_frequency(Frequency::Unlimited)
    .with_repeatable_in_window(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !playable_now(
            context.state,
            context.content,
            &condition_owner,
            event,
            relation,
        )
        .is_empty()
    }))
}

/// Register one standing reaction slot per seated player per window.
///
/// Called once when the game is seated. Registering per card in hand would leak stale abilities,
/// because the resolver has no unregister and must not have one.
pub fn arm(resolver: &mut Resolver, state: &GameState) {
    let mut windows: Vec<(&'static str, Relation)> = window_table()
        .values()
        .map(|window| (window.event, window.relation))
        .collect();
    windows.sort_unstable();
    windows.dedup();

    resolver.register(crate::leaders::ssruu_l1z1x_activation_abilities(state));
    resolver.register(crate::factions::borrowed_round_agents::abilities(state));

    for seat in &state.players {
        let owner_name = crate::promissory::faction_name(state, &seat.id);
        for (event_type, relation) in &windows {
            resolver.register([slot(&owner_name, &seat.id, event_type, *relation)]);
        }
        // Registered for every seat, because a technology can be gained mid-game and the resolver
        // has no late registration; the condition is what limits it to the holder.
        resolver.register([instinct_training(&owner_name, &seat.id)]);
        // The same for a leader that acts in a window rather than as an action.
        resolver.register([l1z1x_agent(&owner_name, &seat.id)]);
        // Per-faction modules (`crate::factions`), after the shared slots so existing ordering
        // in every window is unchanged.
        resolver.register(crate::factions::timing_abilities(
            state,
            &owner_name,
            &seat.id,
        ));
    }
}

/// Whether I48S can act on this activation: `owner` holds it readied, and the activating seat has
/// an infantry on a planet in the active system and a mech it may still place.
fn l1z1x_agent_ready(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
) -> bool {
    let agent = ti4_model::id::LeaderId::new("l1z1xagent");
    let readied = state.player(owner).is_some_and(|seat| {
        seat.leaders.get(&agent) == Some(&ti4_model::state::LeaderStatus::Readied)
    });
    let (Some(system), Some(target)) = (state.active_system.as_ref(), state.active.as_ref()) else {
        return false;
    };
    if !readied {
        return false;
    }
    let faction = state
        .player(target)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let Some(mech) = ti4_content::units::faction_unit(content, &faction, "mech", sources)
        .map(|kind| ti4_model::id::UnitTypeId::new(kind.id()))
    else {
        return false;
    };
    let types = ti4_content::units::catalogue(content, sources);
    let has_infantry = state.board.get(system).is_some_and(|board| {
        board.planet_units.values().flatten().any(|unit| {
            &unit.owner == target
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "infantry")
        })
    });
    has_infantry && crate::supply::allowed(state, content, sources, target, &mech, 1) > 0
}

/// I48S, the L1Z1X agent: "After a player activates a system: You may exhaust this card to allow
/// that player to replace 1 of their infantry in the active system with 1 mech from their
/// reinforcements." The effect was built but nothing offered it (reported 2026-09-23): an agent
/// with a timing window needs a standing slot on that window, like a reaction card.
fn l1z1x_agent(owner_name: &str, player: &PlayerId) -> Ability {
    let owner = player.clone();
    let condition_owner = player.clone();
    Ability::stateful(
        format!("leader:{owner_name}:l1z1xagent:SYSTEM_ACTIVATED:after"),
        player.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let target = context
                .state
                .active
                .clone()
                .map_or_else(|| "the active player".to_owned(), |seat| seat.to_string());
            let choice = crate::choice::Choice::new(
                owner.clone(),
                format!("I48S: exhaust to let {target} replace 1 infantry in the active system with a mech"),
                vec![
                    crate::choice::ChoiceOption::labelled(
                        "use".to_owned(),
                        "leader",
                        "exhaust the agent".to_owned(),
                    ),
                    crate::choice::ChoiceOption::decline(),
                ],
            )
            .contextualized(
                DecisionContext::new(
                    owner.clone(),
                    DecisionSource::Content("l1z1xagent".to_owned()),
                    "l1z1x_agent_swap",
                    context.state.phase,
                    context.state.round,
                )
                .with_trigger(crate::decision_context::DecisionTrigger::from_event(
                    event,
                    "after",
                    &resolver.emission_chain(event),
                )),
            );
            let Ok(answer) = context.ask_seeing(&choice) else {
                return Ok(());
            };
            if !answer.is_decline() {
                crate::leaders::use_leader(
                    context,
                    &owner,
                    &ti4_model::id::LeaderId::new("l1z1xagent"),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        l1z1x_agent_ready(context.state, context.content, context.sources, &condition_owner)
    }))
}

/// Whether `player` can use Instinct Training now: holds it ready, with a strategy token.
fn instinct_training_ready(state: &GameState, player: &PlayerId) -> bool {
    crate::technology::technology_text_ready(state, player, "it")
        && state
            .player(player)
            .is_some_and(|seat| seat.strategic_tokens > 0)
}

/// Instinct Training (Xxcha): "You may exhaust this card and spend 1 token from your strategy pool
/// when another player plays an action card; cancel that action card." Unlike Sabotage it can
/// cancel any card, Sabotage included. The card is still spent (1.15), as with Sabotage.
fn instinct_training(owner_name: &str, player: &PlayerId) -> Ability {
    let owner = player.clone();
    let condition_owner = player.clone();
    Ability::stateful(
        format!("technology:{owner_name}:it:ACTION_CARD_PLAYED:when"),
        player.clone(),
        "ACTION_CARD_PLAYED",
        Relation::When,
        Arc::new(move |event, resolver, context| {
            let card = event.text("card").unwrap_or_default().to_owned();
            let choice = crate::choice::Choice::new(
                owner.clone(),
                format!("Instinct Training: exhaust and spend a strategy token to cancel {card}"),
                vec![
                    crate::choice::ChoiceOption::labelled(
                        "use".to_owned(),
                        "technology",
                        "cancel it".to_owned(),
                    ),
                    crate::choice::ChoiceOption::decline(),
                ],
            )
            .contextualized(
                DecisionContext::new(
                    owner.clone(),
                    DecisionSource::Content("it".to_owned()),
                    "instinct_training_cancel",
                    context.state.phase,
                    context.state.round,
                )
                .with_trigger(
                    crate::decision_context::DecisionTrigger::from_event(
                        event,
                        "when",
                        &resolver.emission_chain(event),
                    ),
                ),
            );
            let Ok(answer) = context.ask_seeing(&choice) else {
                return Ok(());
            };
            if answer.is_decline() || !instinct_training_ready(context.state, &owner) {
                return Ok(());
            }
            if let Some(seat) = context.state.player_mut(&owner) {
                seat.strategic_tokens -= 1;
            }
            crate::technology::exhaust_technology_text(context.state, &owner, "it");
            crate::supply::note_strategy_token_spent(context.state, &owner, "instinct_training");
            event.cancel();
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        actor_is_not(event, &condition_owner, context.state)
            && instinct_training_ready(context.state, &condition_owner)
    }))
}

/// Reaction cards whose printed window this table maps.
#[must_use]
pub fn reachable(content: &ContentStore, sources: SourceSet) -> Vec<ActionCardId> {
    let table = window_table();
    content
        .from_sources(ContentType::ActionCards, sources)
        .filter(|record| record.text("window").is_some_and(|w| w.trim() != "Action"))
        .filter(|record| {
            record
                .text("window")
                .is_some_and(|w| table.contains_key(w.trim()))
        })
        .filter_map(|record| record.text("alias").map(ActionCardId::new))
        .collect()
}

/// Printed windows this table does not map, with how many cards each carries.
///
/// Reported rather than ignored: a reaction card whose window is unmapped can never be played,
/// and a silent one of those is the state this module exists to end.
#[must_use]
pub fn unmapped_windows(content: &ContentStore, sources: SourceSet) -> BTreeMap<String, usize> {
    let table = window_table();
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for record in content.from_sources(ContentType::ActionCards, sources) {
        let Some(printed) = record.text("window") else {
            continue;
        };
        if printed.trim() == "Action" || table.contains_key(printed.trim()) {
            continue;
        }
        *found.entry(printed.trim().to_owned()).or_insert(0) += 1;
    }
    found
}

/// Mapped windows whose event nothing in this engine emits yet.
///
/// A window can be in the table and still be unreachable, because the table says where a card
/// *would* hook. Until the subsystem emits that typed event, the card is as unplayable as one
/// with no window at all — and far easier to mistake for finished.
#[must_use]
pub fn windows_without_an_event(emitted: &[&str]) -> Vec<&'static str> {
    let mut missing: Vec<&'static str> = window_table()
        .values()
        .map(|window| window.event)
        .filter(|event| !emitted.contains(event))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    /// An event of one type, naming one player.
    fn event(event_type: &str, who: &str) -> Event {
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), who.to_owned().into());
        Event::new(1, event_type, payload)
    }

    /// An `ACTION_CARD_PLAYED`-shaped event: the player who played, and the card played.
    fn event_with_card(event_type: &str, who: &str, card: &str) -> Event {
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), who.to_owned().into());
        payload.insert("card".to_owned(), card.to_owned().into());
        Event::new(1, event_type, payload)
    }

    /// A `SYSTEM_ACTIVATED` event: the player who activated, and the system.
    fn system_activation_event(who: &str, system: &str) -> Event {
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), who.to_owned().into());
        payload.insert("system".to_owned(), system.to_owned().into());
        Event::new(1, "SYSTEM_ACTIVATED", payload)
    }

    #[test]
    fn a_component_action_is_not_a_reaction() {
        // Forty-two cards read "Action". Treating one as a reaction would offer it in a window
        // where its own rules say it costs a turn.
        let content = ContentStore::embedded();
        let action_card = content
            .from_sources(ContentType::ActionCards, POK)
            .find(|record| record.text("window") == Some("Action"))
            .and_then(|record| record.text("alias").map(ActionCardId::new))
            .expect("the corpus has component-action cards");

        assert!(window_for(content, &action_card).is_none());
    }

    #[test]
    fn a_reaction_card_hooks_where_its_printed_window_says() {
        let content = ContentStore::embedded();
        let flank_speed = ActionCardId::new("fs1"); // "After you activate a system"
        let hooked = window_for(content, &flank_speed).expect("a mapped window");

        assert_eq!(hooked.event, "SYSTEM_ACTIVATED");
        assert_eq!(hooked.relation, Relation::After);
    }

    #[test]
    fn you_means_you() {
        // The guard is the whole difference between "after you activate" and "after anyone
        // activates". Without it every player reacts to every activation.
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().action_cards = vec![ActionCardId::new("fs1")];

        let mine = event("SYSTEM_ACTIVATED", "a");
        let theirs = event("SYSTEM_ACTIVATED", "b");

        assert_eq!(
            playable_now(&state, content, &player(), &mine, Relation::After),
            vec![ActionCardId::new("fs1")]
        );
        assert!(
            playable_now(&state, content, &player(), &theirs, Relation::After).is_empty(),
            "another player's activation is not yours"
        );
    }

    #[test]
    fn courageous_requires_the_destroyed_ship_to_be_lost_during_space_combat() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().action_cards = vec![ActionCardId::new("courageous")];
        let destroyed = |during_space_combat: bool| {
            let mut payload = BTreeMap::new();
            payload.insert("player".to_owned(), "a".into());
            payload.insert("system".to_owned(), "18".into());
            payload.insert("unit".to_owned(), "cruiser".into());
            payload.insert("cause".to_owned(), "action_card:direct_hit".into());
            payload.insert("during_space_combat".to_owned(), during_space_combat.into());
            Event::new(1, "SHIP_DESTROYED", payload)
        };

        assert_eq!(
            playable_now(
                &state,
                content,
                &player(),
                &destroyed(true),
                Relation::After,
            ),
            vec![ActionCardId::new("courageous")],
            "the effect source remains Direct Hit while the independent window fact qualifies"
        );
        assert!(
            playable_now(
                &state,
                content,
                &player(),
                &destroyed(false),
                Relation::After,
            )
            .is_empty(),
            "the same effect outside combat does not open Courageous"
        );
    }

    #[test]
    fn only_the_combatants_may_play_a_combat_round_card() {
        // Skilled Retreat moves "all of your ships" out of the combat; a seat with no ships there
        // was once offered it, and played it for nothing.
        let content = ContentStore::embedded();
        let c = PlayerId::new("c");
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        for seat in ["a", "c"] {
            state.player_mut(&PlayerId::new(seat)).unwrap().action_cards =
                vec![ActionCardId::new("s_retreat1")];
        }
        let mut payload = BTreeMap::new();
        payload.insert("system".to_owned(), "69".to_owned().into());
        payload.insert("round".to_owned(), 1.into());
        payload.insert("attacker".to_owned(), "a".to_owned().into());
        payload.insert("defender".to_owned(), "b".to_owned().into());
        let round = Event::new(1, "COMBAT_ROUND_STARTED", payload);

        assert_eq!(
            playable_now(&state, content, &player(), &round, Relation::After),
            vec![ActionCardId::new("s_retreat1")],
            "a combatant may play it"
        );
        assert!(
            playable_now(&state, content, &c, &round, Relation::After).is_empty(),
            "a bystander may not"
        );
    }

    #[test]
    fn sabotage_reacts_only_to_a_card_that_is_not_sabotage() {
        // The window's tail: "other than 'Sabotage'". The four copies cancel other cards
        // being played, not each other — a chain of Sabotages would spend the whole deck
        // for nothing. The guard reads the played card's alias off the event payload.
        let content = ContentStore::embedded();
        let b = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&b).unwrap().action_cards = vec![ActionCardId::new("sabo1")];

        let other_card = event_with_card("ACTION_CARD_PLAYED", "a", "fs1");
        let sabotage_play = event_with_card("ACTION_CARD_PLAYED", "a", "sabo2");

        assert_eq!(
            playable_now(&state, content, &b, &other_card, Relation::When),
            vec![ActionCardId::new("sabo1")],
            "another player's card is a legal target"
        );
        assert!(
            playable_now(&state, content, &b, &sabotage_play, Relation::When).is_empty(),
            "Sabotage does not answer a Sabotage"
        );
    }

    /// An `INVASION_BEGAN` event: `invader` invades system 18.
    fn invasion_of_18(invader: &str) -> Event {
        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), invader.to_owned().into());
        payload.insert("system".to_owned(), "18".to_owned().into());
        Event::new(1, "INVASION_BEGAN", payload)
    }

    #[test]
    fn bunker_is_offered_only_to_a_defender_who_controls_a_planet_in_the_system() {
        // "-4 to each BOMBARDMENT roll against planets you control": without a planet of the
        // holder's in the invaded system, or as the invader itself, the card cannot matter.
        let content = ContentStore::embedded();
        let (a, b, c) = (PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c"));
        let system = SystemId::new("18");
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        for seat in [&a, &b, &c] {
            state.player_mut(seat).unwrap().action_cards = vec![ActionCardId::new("bunker")];
        }
        crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        let invasion = invasion_of_18("a");

        assert!(
            playable_now(&state, content, &b, &invasion, Relation::After).is_empty(),
            "no planet of b's in the system"
        );

        state
            .system_mut(&system)
            .set_control(ti4_model::id::PlanetId::new("mecatol"), b.clone());
        assert_eq!(
            playable_now(&state, content, &b, &invasion, Relation::After),
            vec![ActionCardId::new("bunker")],
            "b controls a planet the invader can bombard"
        );
        assert!(
            playable_now(&state, content, &c, &invasion, Relation::After).is_empty(),
            "a bystander with no planet there"
        );

        // The invader, even holding a planet in the system, is never bombarding itself.
        state
            .system_mut(&system)
            .set_control(ti4_model::id::PlanetId::new("other"), a.clone());
        assert!(
            playable_now(&state, content, &a, &invasion, Relation::After).is_empty(),
            "the invader"
        );
    }

    #[test]
    fn blitz_is_offered_only_to_the_invader_with_a_ship_that_lacks_bombardment() {
        let content = ContentStore::embedded();
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let system = SystemId::new("18");
        let mut state = crate::fixtures::game(&["a", "b"]);
        for seat in [&a, &b] {
            state.player_mut(seat).unwrap().action_cards = vec![ActionCardId::new("blitz")];
        }
        crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        crate::fixtures::put(&mut state, &system, "fighter", &a, 2);
        let invasion = invasion_of_18("a");

        assert!(
            playable_now(&state, content, &a, &invasion, Relation::After).is_empty(),
            "a dreadnought already bombards and fighters never gain it"
        );
        crate::fixtures::put(&mut state, &system, "cruiser", &a, 1);
        assert_eq!(
            playable_now(&state, content, &a, &invasion, Relation::After),
            vec![ActionCardId::new("blitz")]
        );
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);
        assert!(
            playable_now(&state, content, &b, &invasion, Relation::After).is_empty(),
            "only the invader bombards"
        );
    }

    #[test]
    fn fighter_prototype_is_offered_only_to_a_combatant_with_fighters() {
        // An unguarded Fighter Prototype let any seat play it at the start of any space combat;
        // that play is what then opened Sabotage for everyone else "at the start of combat".
        let content = ContentStore::embedded();
        let (a, b, c) = (PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c"));
        let system = SystemId::new("18");
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        for seat in [&a, &b, &c] {
            state.player_mut(seat).unwrap().action_cards = vec![ActionCardId::new("f_prototype")];
        }
        crate::fixtures::put(&mut state, &system, "carrier", &a, 1);
        crate::fixtures::put(&mut state, &system, "fighter", &a, 2);
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);
        crate::fixtures::put(&mut state, &system, "fighter", &c, 1);
        let mut payload = BTreeMap::new();
        payload.insert("system".to_owned(), "18".to_owned().into());
        payload.insert("player".to_owned(), "a".to_owned().into());
        payload.insert("attacker".to_owned(), "a".to_owned().into());
        payload.insert("defender".to_owned(), "b".to_owned().into());
        let started = Event::new(1, "SPACE_COMBAT_STARTED", payload);

        assert_eq!(
            playable_now(&state, content, &a, &started, Relation::After),
            vec![ActionCardId::new("f_prototype")]
        );
        assert!(
            playable_now(&state, content, &b, &started, Relation::After).is_empty(),
            "a combatant without fighters"
        );
        assert!(
            playable_now(&state, content, &c, &started, Relation::After).is_empty(),
            "a bystander"
        );
    }

    #[test]
    fn sabotage_is_never_offered_outside_a_played_card_window() {
        // Sabotage hooks only the WHEN window of another player's ACTION_CARD_PLAYED: the start
        // of a space combat, a combat round, or an invasion never offers it by itself.
        let content = ContentStore::embedded();
        let b = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&b).unwrap().action_cards = vec![ActionCardId::new("sabo1")];
        let mut payload = BTreeMap::new();
        payload.insert("system".to_owned(), "18".to_owned().into());
        payload.insert("player".to_owned(), "a".to_owned().into());
        payload.insert("attacker".to_owned(), "a".to_owned().into());
        payload.insert("defender".to_owned(), "b".to_owned().into());
        payload.insert("round".to_owned(), 1.into());
        for event_type in [
            "SPACE_COMBAT_STARTED",
            "COMBAT_ROUND_STARTED",
            "INVASION_BEGAN",
        ] {
            let opened = Event::new(1, event_type, payload.clone());
            for relation in [Relation::When, Relation::After] {
                assert!(
                    playable_now(&state, content, &b, &opened, relation).is_empty(),
                    "{event_type}"
                );
            }
        }
        assert!(
            playable_now(
                &state,
                content,
                &b,
                &event_with_card("ACTION_CARD_PLAYED", "b", "fs1"),
                Relation::When
            )
            .is_empty(),
            "not the holder's own card"
        );
    }

    #[test]
    fn activation_cards_are_offered_only_where_their_printed_condition_holds() {
        // Harness Energy needs an anomaly; Upgrade a cruiser of yours; Rally another player's
        // ships. The old ship test matched only type ids ending in "ship" (the flagship).
        let content = ContentStore::embedded();
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&a).unwrap().action_cards = ["harness", "upgrade", "rally"]
            .map(ActionCardId::new)
            .to_vec();
        let offered = |state: &GameState, system: &str| {
            playable_now(
                state,
                content,
                &a,
                &system_activation_event("a", system),
                Relation::After,
            )
        };
        assert!(
            offered(&state, "18").is_empty(),
            "Mecatol: no anomaly, no ships"
        );
        assert_eq!(
            offered(&state, "41"),
            vec![ActionCardId::new("harness")],
            "a gravity rift"
        );
        let mecatol = SystemId::new("18");
        crate::fixtures::put(&mut state, &mecatol, "cruiser", &a, 1);
        crate::fixtures::put(&mut state, &mecatol, "dreadnought", &b, 1);
        assert_eq!(
            offered(&state, "18"),
            vec![ActionCardId::new("upgrade"), ActionCardId::new("rally")]
        );
    }

    #[test]
    fn reveal_prototype_is_offered_in_the_first_round_only() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().action_cards =
            vec![ActionCardId::new("reveal_prototype")];
        let round = |n: i64| {
            let mut payload = BTreeMap::new();
            payload.insert("system".to_owned(), "18".to_owned().into());
            payload.insert("round".to_owned(), n.into());
            payload.insert("attacker".to_owned(), "a".to_owned().into());
            payload.insert("defender".to_owned(), "b".to_owned().into());
            Event::new(1, "COMBAT_ROUND_STARTED", payload)
        };
        assert_eq!(
            playable_now(&state, content, &player(), &round(1), Relation::After),
            vec![ActionCardId::new("reveal_prototype")]
        );
        assert!(playable_now(&state, content, &player(), &round(2), Relation::After).is_empty());
    }

    #[test]
    fn a_card_for_a_different_window_is_not_offered() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player()).unwrap().action_cards = vec![ActionCardId::new("fs1")];

        let agenda = event("AGENDA_REVEALED", "a");

        assert!(
            playable_now(&state, content, &player(), &agenda, Relation::After).is_empty(),
            "an activation card does not answer an agenda"
        );
    }

    #[test]
    fn the_relation_is_part_of_the_window() {
        // Nineteen cards split on it: seven read "When an agenda is revealed" and twelve "After".
        // Ignoring the relation offers both sets in both windows.
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        let after_card = content
            .from_sources(ContentType::ActionCards, POK)
            .find(|record| record.text("window") == Some("After an agenda is revealed"))
            .and_then(|record| record.text("alias").map(ActionCardId::new))
            .expect("the corpus has one");
        state.player_mut(&player()).unwrap().action_cards = vec![after_card.clone()];
        let agenda = event("AGENDA_REVEALED", "a");

        assert_eq!(
            playable_now(&state, content, &player(), &agenda, Relation::After),
            vec![after_card]
        );
        assert!(
            playable_now(&state, content, &player(), &agenda, Relation::When).is_empty(),
            "an AFTER card must not answer the WHEN window"
        );
    }

    #[test]
    fn every_mapped_window_is_a_window_the_corpus_prints() {
        // The table is keyed on exact printed text. A typo produces a key nothing matches, and
        // the cards it was meant to cover stay unplayable with nothing to say so.
        let content = ContentStore::embedded();
        // The whole corpus, not one scope. `window_for` looks a card up against everything a
        // card might say — which cards are in a deck is decided by deck building — and Rescue
        // prints its window only in the Thunder's Edge source.
        let printed: std::collections::BTreeSet<String> = content
            .from_sources(ContentType::ActionCards, ti4_model::content_types::FULL)
            .filter_map(|record| record.text("window"))
            .map(|window| window.trim().to_owned())
            .collect();

        for key in window_table().keys() {
            assert!(
                printed.contains(*key),
                "{key:?} is in the table but no card prints it"
            );
        }
    }

    #[test]
    fn the_unmapped_windows_are_reported_rather_than_ignored() {
        let missing = unmapped_windows(ContentStore::embedded(), POK);
        // The window table has grown to cover every printed window in the corpus, so there is
        // nothing left that could be silently unplayable: the report is empty, and it would
        // stop being empty the day a card ships with a window nobody mapped.
        assert!(
            missing.is_empty(),
            "these printed windows have no table entry and their cards can never be played: {missing:?}"
        );
        for key in window_table().keys() {
            assert!(!missing.contains_key(*key));
        }
    }

    #[test]
    fn a_mapped_window_with_no_event_is_still_unreachable() {
        // The distinction this project keeps having to relearn: a table entry is not a
        // connection. Only SYSTEM_ACTIVATED and STRATEGY_CARD_CHOSEN are emitted today.
        let blind = windows_without_an_event(&[]);
        assert!(blind.contains(&"AGENDA_REVEALED"));

        let with_activation = windows_without_an_event(&["SYSTEM_ACTIVATED"]);
        assert!(
            !with_activation.contains(&"SYSTEM_ACTIVATED"),
            "an emitted event is reachable"
        );
    }

    // P1-d: reaction option identity aligned to the oracle (engine/reactions.py:316–324, 332;
    // engine/timing.py:55–63), commit 37061c5.

    /// One recorded option surface: id, kind and label.
    type OptionSurface = (String, String, String);
    /// One recorded choice: prompt plus its offered options in order.
    type RecordedAsk = (String, Vec<OptionSurface>);

    /// A decider that records every choice it is asked to answer, answering from a queue of ids.
    struct Recording {
        wanted: std::collections::VecDeque<String>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>,
    }

    impl Recording {
        fn new(wanted: &[&str]) -> (Self, std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>) {
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            (
                Self {
                    wanted: wanted.iter().map(|id| (*id).to_owned()).collect(),
                    seen: seen.clone(),
                },
                seen,
            )
        }

        fn record(&self, choice: &crate::choice::Choice) {
            self.seen.borrow_mut().push((
                choice.prompt.clone(),
                choice
                    .options
                    .iter()
                    .map(|option| (option.id.clone(), option.kind.clone(), option.label.clone()))
                    .collect(),
            ));
        }
    }

    impl crate::choice::Decider for Recording {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            self.record(choice);
            let Some(wanted) = self.wanted.pop_front() else {
                return Err(crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: "<script exhausted>".to_owned(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                });
            };
            choice.option(&wanted).cloned().ok_or_else(|| {
                crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted,
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                }
            })
        }
    }

    #[test]
    fn the_reaction_slot_id_uses_the_faction_and_lowercase_relation() {
        // engine/reactions.py:332 builds f"reaction:{player}:{event_type}:{relation.value}",
        // and a Python player's identity is its faction name (live trace:
        // reaction:hacan:SYSTEM_ACTIVATED:after). Rust used the seat id plus Debug-formatted
        // capitalization.
        let ability = slot("hacan", &player(), "SYSTEM_ACTIVATED", Relation::After);
        assert_eq!(ability.id, "reaction:hacan:SYSTEM_ACTIVATED:after");

        let when = slot("sol", &player(), "AGENDA_REVEALED", Relation::When);
        assert_eq!(when.id, "reaction:sol:AGENDA_REVEALED:when");
    }

    #[test]
    fn a_reaction_offer_is_one_option_per_printed_card() {
        // engine/reactions.py:319–324 dedupes by printed name. Flank Speed is printed four times
        // (fs1..fs4), so holding two copies must offer one option, not two — and the kept alias
        // is the first in hand order.
        let content = ContentStore::embedded();
        let options = reaction_card_options(
            content,
            &[ActionCardId::new("fs1"), ActionCardId::new("fs4")],
        );
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].id, "fs1");

        // The reverse walk fixes each name's slot: hand [silence_space, fs1] offers in the order
        // [fs1, silence_space], exactly as Python's reversed() dict comprehension does.
        let options = reaction_card_options(
            content,
            &[ActionCardId::new("silence_space"), ActionCardId::new("fs1")],
        );
        assert_eq!(
            options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            ["fs1", "silence_space"]
        );
    }

    #[test]
    fn the_inner_choice_labels_cards_in_played_form() {
        // engine/reactions.py:320–324: kind "action_card", label f"play {known[a].name}".
        let content = ContentStore::embedded();
        let options = reaction_card_options(
            content,
            &[ActionCardId::new("fs1"), ActionCardId::new("silence_space")],
        );
        // The reverse walk puts the last held card first: [fs1, silence] offers
        // [silence_space, fs1].
        assert_eq!(options[0].label, "play In The Silence Of Space");
        assert_eq!(options[1].label, "play Flank Speed");
        for option in &options {
            assert_eq!(option.kind, "action_card");
        }
    }

    #[test]
    fn the_inner_choice_is_asked_with_the_oracle_prompt_and_surface() {
        // End to end: the outer window ask ("after SYSTEM_ACTIVATED") and the inner card choice
        // (engine/reactions.py:316) must both surface through one table, with the oracle wording.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().action_cards =
            vec![ActionCardId::new("silence_space"), ActionCardId::new("fs1")];
        // Nobody else holds a card that could hook the nested ACTION_CARD_PLAYED announcement.
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .clear();

        // Third answer: the slot is repeatable in window (1.19), so after playing Flank Speed
        // the resolver re-offers it while In The Silence Of Space is still playable; decline.
        let (decider, seen) =
            Recording::new(&["reaction:hacan:SYSTEM_ACTIVATED:after", "fs1", "decline"]);
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut resolver = Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b")],
            Some(PlayerId::new("a")),
            crate::choice::Table::default(),
        );
        resolver.register([slot(
            "hacan",
            &player(),
            "SYSTEM_ACTIVATED",
            Relation::After,
        )]);

        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state: &mut state,
            content,
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };

        let mut payload = BTreeMap::new();
        payload.insert("player".to_owned(), "a".into());
        let event = context
            .event_sequence
            .next("SYSTEM_ACTIVATED", payload)
            .unwrap();
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .unwrap();

        let asks = seen.borrow().clone();
        assert_eq!(
            asks.len(),
            3,
            "outer ask, inner card choice, repeatable re-offer; got {asks:?}"
        );

        // The outer asks keep the shape they already had: prompt, ability option, decline —
        // including the repeatable re-offer after the first resolution.
        assert_eq!(asks[0].0, "after SYSTEM_ACTIVATED");
        assert_eq!(asks[2].0, "after SYSTEM_ACTIVATED");
        let ids: Vec<_> = asks[0].1.iter().map(|(id, _, _)| id.as_str()).collect();
        assert!(
            ids.contains(&"reaction:hacan:SYSTEM_ACTIVATED:after"),
            "got {ids:?}"
        );

        // The inner ask is the aligned surface.
        assert_eq!(asks[1].0, "play an action card (after SYSTEM_ACTIVATED)");
        let offered: Vec<_> = asks[1]
            .1
            .iter()
            .map(|(id, kind, label)| (id.as_str(), kind.as_str(), label.as_str()))
            .collect();
        assert_eq!(
            offered,
            [
                ("fs1", "action_card", "play Flank Speed"),
                (
                    "silence_space",
                    "action_card",
                    "play In The Silence Of Space"
                ),
                ("decline", "decline", "Decline"),
            ]
        );

        // The chosen card was actually played: the hand lost it and the state carries its effect.
        assert_eq!(
            state.player(&player()).unwrap().action_cards,
            [ActionCardId::new("silence_space")]
        );
    }

    #[test]
    fn the_inner_card_choice_can_be_declined_without_playing() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().action_cards =
            vec![ActionCardId::new("silence_space"), ActionCardId::new("fs1")];
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .clear();
        let asked = asked_for(
            state,
            "SYSTEM_ACTIVATED",
            payload(&[("player", "a".into())]),
            &[
                "reaction:generic:SYSTEM_ACTIVATED:after",
                "decline",
                "decline",
            ],
        );
        let inner = asked
            .iter()
            .find(|c| c.prompt.starts_with("play an action card"))
            .expect("the inner card choice is asked");
        assert!(inner.options.iter().any(|o| o.is_decline()));
        assert!(inner.context.as_ref().is_some_and(|c| c.optional));
    }

    /// Emit `event_type` with `payload` through an armed resolver and return every decision the
    /// table asked, context included.
    fn asked_for(
        mut state: GameState,
        event_type: &str,
        payload: BTreeMap<String, serde_json::Value>,
        answers: &[&str],
    ) -> Vec<crate::choice::Choice> {
        let (decider, seen) = crate::choice::Capturing::new(Box::new(
            crate::choice::Scripted::new(answers.iter().map(|answer| (*answer).to_owned())),
        ));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
        let mut resolver = Resolver::new(
            seats.clone(),
            seats.first().cloned(),
            crate::choice::Table::default(),
        );
        arm(&mut resolver, &state);
        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state: &mut state,
            content,
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };
        let event = context.event_sequence.next(event_type, payload).unwrap();
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .unwrap();
        let asked = seen.borrow().clone();
        asked
    }

    fn payload(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn only_trigger(asked: &[crate::choice::Choice]) -> crate::decision_context::DecisionTrigger {
        asked
            .iter()
            .find_map(|choice| choice.context.as_ref().and_then(|c| c.trigger.clone()))
            .expect("a reaction decision carries its trigger")
    }

    #[test]
    fn a_played_card_opens_sabotage_with_who_played_what() {
        use crate::decision_context::TriggerKind;
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sabo1")];
        let asked = asked_for(
            state,
            "ACTION_CARD_PLAYED",
            payload(&[("player", "a".into()), ("card", "fs1".into())]),
            &["decline"],
        );
        let trigger = only_trigger(&asked);
        assert_eq!(trigger.kind, TriggerKind::ActionCardPlayed);
        assert_eq!(trigger.actor, Some(PlayerId::new("a")));
        assert_eq!(trigger.card.as_deref(), Some("fs1"));
        assert_eq!(trigger.relation, "when");
        assert_eq!(trigger.event_type, "ACTION_CARD_PLAYED");
        assert_eq!(trigger.event_id, 1);
        assert!(trigger.chain.is_empty());
        let context = asked[0].context.as_ref().unwrap();
        assert_eq!(
            context.actor,
            PlayerId::new("b"),
            "the asked seat is the reactor"
        );
    }

    /// Who was asked what, in order, when `a` picks a strategy card and `b` and `c` each hold a
    /// card for that window while `d` holds Sabotage. `b` and `c` always play; `d` always passes.
    fn declaration_round(rules_version: u32) -> Vec<(String, String)> {
        struct Plays(std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>>);
        impl crate::choice::Decider for Plays {
            fn choose(
                &mut self,
                choice: &crate::choice::Choice,
            ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
                self.0
                    .borrow_mut()
                    .push((choice.player.to_string(), choice.prompt.clone()));
                let pick = choice
                    .options
                    .iter()
                    .find(|o| o.id.starts_with("reaction:") && choice.player.as_str() != "d");
                Ok(pick
                    .or_else(|| choice.options.iter().find(|o| o.is_decline()))
                    .unwrap_or(&choice.options[0])
                    .clone())
            }
        }
        let mut state = crate::fixtures::game(&["a", "b", "c", "d"]);
        state.rules_version = rules_version;
        for (id, card) in [("b", "disgrace"), ("c", "disgrace"), ("d", "sabo1")] {
            state.player_mut(&PlayerId::new(id)).unwrap().action_cards =
                vec![ActionCardId::new(card)];
        }
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = crate::choice::Table::with_default(Box::new(Plays(log.clone())));
        let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
        let mut resolver = Resolver::new(
            seats.clone(),
            seats.first().cloned(),
            crate::choice::Table::default(),
        );
        arm(&mut resolver, &state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state: &mut state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };
        let event = context
            .event_sequence
            .next("STRATEGY_CARD_CHOSEN", payload(&[("player", "a".into())]))
            .unwrap();
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .unwrap();
        let asked = log.borrow().clone();
        asked
    }

    /// Positions of the first Sabotage question and of the declarers' own questions.
    fn sabotage_and_declarers(asked: &[(String, String)]) -> (usize, Vec<usize>) {
        let sabotage_at = asked
            .iter()
            .position(|(_, prompt)| prompt.contains("ACTION_CARD_PLAYED"))
            .expect("Sabotage is offered");
        let declarers = asked
            .iter()
            .enumerate()
            .filter(|(_, (who, prompt))| {
                (who == "b" || who == "c") && prompt.contains("STRATEGY_CARD_CHOSEN")
            })
            .map(|(i, _)| i)
            .collect();
        (sabotage_at, declarers)
    }

    #[test]
    fn sabotage_waits_until_every_player_has_declared_their_cards() {
        // Rules version 2: a window collects every player's card play first; only then are
        // the declared plays announced one by one, each opening its Sabotage window.
        let asked = declaration_round(ti4_model::state::RULES_SABOTAGE_TIMING);
        let (sabotage_at, declarers) = sabotage_and_declarers(&asked);
        assert!(declarers.len() >= 2, "both declare: {asked:?}");
        assert!(
            declarers.iter().take(2).all(|i| *i < sabotage_at),
            "both players declare before any Sabotage window: {asked:?}"
        );
    }

    #[test]
    fn a_legacy_game_opens_sabotage_right_after_each_play() {
        let asked = declaration_round(1);
        let (sabotage_at, declarers) = sabotage_and_declarers(&asked);
        assert!(
            declarers[0] < sabotage_at && sabotage_at < declarers[1],
            "{asked:?}"
        );
    }

    fn previews_of(asked: &[crate::choice::Choice]) -> Vec<crate::decision_context::EffectPreview> {
        asked
            .iter()
            .flat_map(|choice| choice.context.iter().flat_map(|c| c.previews.clone()))
            .collect()
    }

    #[test]
    fn sabotage_says_which_card_it_would_cancel_and_whose() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sabo1")];
        let asked = asked_for(
            state,
            "ACTION_CARD_PLAYED",
            payload(&[("player", "a".into()), ("card", "fs1".into())]),
            &["decline"],
        );
        let previews = previews_of(&asked);
        assert_eq!(previews.len(), 1);
        let preview = &previews[0];
        assert_eq!(preview.effect, "cancel_card");
        assert_eq!(preview.card, "sabo1");
        assert_eq!(preview.seats, vec![PlayerId::new("a")]);
        assert_eq!(preview.refs, vec!["fs1".to_owned()]);
        let offered = asked[0]
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect::<Vec<_>>();
        assert!(
            offered.contains(&preview.option),
            "the preview names an option of the same decision"
        );
        // Display only: neither the canonical form nor a recorded decision holds it.
        let context = asked[0].context.as_ref().unwrap();
        assert!(!context.canonical().contains("cancel_card"));
        assert!(context.without_display_fields().previews.is_empty());
    }

    #[test]
    fn shields_holding_says_how_many_of_the_incoming_hits_it_cancels() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sh1")];
        let asked = asked_for(
            state,
            "HITS_TO_ASSIGN",
            payload(&[
                ("player", "b".into()),
                ("system", "27".into()),
                ("hits", 3.into()),
            ]),
            &["decline"],
        );
        let previews = previews_of(&asked);
        assert_eq!(previews.len(), 1, "{asked:?}");
        assert_eq!(previews[0].effect, "cancel_hits");
        assert_eq!(previews[0].amount, Some(2));
        assert_eq!(previews[0].of, Some(3));
        assert_eq!(previews[0].system, Some(SystemId::new("27")));
    }

    #[test]
    fn the_inner_card_choice_has_a_preview_for_each_offered_card() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards = vec![
            ActionCardId::new("sabo1"),
            ActionCardId::new("sabo2"),
            ActionCardId::new("stability"),
        ];
        let asked = asked_for(
            state,
            "ACTION_CARD_PLAYED",
            payload(&[("player", "a".into()), ("card", "fs1".into())]),
            &["decline"],
        );
        // Only Sabotage answers this window; the preview never names another card.
        let previews = previews_of(&asked);
        assert!(previews.iter().all(|p| p.card.starts_with("sabo")));
    }

    /// The questions asked when `a` plays `card` in the window of `window` (its type and payload)
    /// and `b` holds Sabotage: `b` plays it, so the card is cancelled and never resolves.
    fn sabotage_asked_over(
        mut state: GameState,
        card: &str,
        window: (&str, BTreeMap<String, serde_json::Value>),
    ) -> Vec<crate::choice::Choice> {
        struct Sabotages(std::rc::Rc<std::cell::RefCell<Vec<crate::choice::Choice>>>);
        impl crate::choice::Decider for Sabotages {
            fn choose(
                &mut self,
                choice: &crate::choice::Choice,
            ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
                self.0.borrow_mut().push(choice.clone());
                let pick = choice
                    .options
                    .iter()
                    .find(|o| o.id.starts_with("reaction:") && choice.player.as_str() == "b");
                Ok(pick
                    .or_else(|| choice.options.iter().find(|o| o.is_decline()))
                    .unwrap_or(&choice.options[0])
                    .clone())
            }
        }
        state.player_mut(&PlayerId::new("a")).unwrap().action_cards = vec![ActionCardId::new(card)];
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sabo1")];
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut table = crate::choice::Table::with_default(Box::new(Sabotages(seen.clone())));
        let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
        let mut resolver = Resolver::new(
            seats.clone(),
            seats.first().cloned(),
            crate::choice::Table::default(),
        );
        arm(&mut resolver, &state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state: &mut state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };
        let window = context.event_sequence.next(window.0, window.1).unwrap();
        let played = play(
            &mut context,
            &mut resolver,
            &PlayerId::new("a"),
            &ActionCardId::new(card),
            Some(&window),
        )
        .unwrap();
        assert!(!played, "Sabotage cancels the card");
        let asked = seen.borrow().clone();
        asked
    }

    /// The Sabotage question of `asked`: the one `b` answers in the window of the played card.
    fn sabotage_question(
        asked: &[crate::choice::Choice],
    ) -> &crate::decision_context::DecisionContext {
        asked
            .iter()
            .filter_map(|choice| choice.context.as_ref())
            .find(|context| context.subtype == "reaction_when_ACTION_CARD_PLAYED")
            .expect("b is asked whether to sabotage")
    }

    #[test]
    fn the_sabotage_window_says_what_the_played_card_will_do() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.last_sustain = Some((
            SystemId::new("27"),
            PlayerId::new("b"),
            ti4_model::id::UnitTypeId::new("dreadnought"),
            PlayerId::new("a"),
            true,
        ));
        let asked = sabotage_asked_over(
            state,
            "dh1",
            (
                "SUSTAIN_DAMAGE_USED",
                payload(&[("player", "b".into()), ("system", "27".into())]),
            ),
        );
        let context = sabotage_question(&asked);
        let pending = context
            .pending_effect
            .as_ref()
            .expect("Direct Hit has a preview");
        assert_eq!(pending.card, "dh1");
        assert_eq!(pending.option, "", "it belongs to no option of the window");
        assert_eq!(pending.effect, "destroy_unit");
        assert_eq!(
            crate::phrases::said(&pending.phrase, "b"),
            "Destroys your dreadnought in System 27, the ship that just sustained damage."
        );
        // The offered Sabotage keeps its own preview, and the pending one is display only.
        assert!(context.previews.iter().all(|p| p.card == "sabo1"));
        assert!(!context.canonical().contains("destroy_unit"));
        assert!(context.without_display_fields().pending_effect.is_none());
    }

    #[test]
    fn a_played_card_without_a_preview_has_no_pending_effect() {
        let asked = sabotage_asked_over(
            crate::fixtures::game(&["a", "b"]),
            "mining_initiative",
            ("TURN_BEGAN", payload(&[("player", "a".into())])),
        );
        assert!(sabotage_question(&asked).pending_effect.is_none());
        // And no other question carries one.
        assert!(
            asked
                .iter()
                .filter_map(|choice| choice.context.as_ref())
                .all(|context| context.pending_effect.is_none())
        );
    }

    #[test]
    fn a_system_activation_names_the_player_and_the_system_on_both_steps() {
        use crate::decision_context::TriggerKind;
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&PlayerId::new("a")).unwrap().action_cards =
            vec![ActionCardId::new("silence_space"), ActionCardId::new("fs1")];
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .clear();
        let asked = asked_for(
            state,
            "SYSTEM_ACTIVATED",
            payload(&[("player", "a".into()), ("system", "27".into())]),
            &["reaction:generic:SYSTEM_ACTIVATED:after", "fs1", "decline"],
        );
        let outer = asked[0].context.as_ref().unwrap().trigger.clone().unwrap();
        assert_eq!(outer.kind, TriggerKind::SystemActivated);
        assert_eq!(outer.actor, Some(PlayerId::new("a")));
        assert_eq!(outer.system, Some(SystemId::new("27")));
        assert_eq!(outer.relation, "after");
        let inner = asked
            .iter()
            .find(|choice| {
                choice
                    .context
                    .as_ref()
                    .is_some_and(|c| c.subtype == "play_reaction_after_SYSTEM_ACTIVATED")
            })
            .expect("several differently named cards ask the inner step");
        assert_eq!(
            inner.context.as_ref().unwrap().trigger.as_ref(),
            Some(&outer),
            "the inner decision repeats the outer's trigger"
        );
    }

    #[test]
    fn a_fleet_arrival_names_the_system_and_what_arrived() {
        use crate::decision_context::TriggerKind;
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&PlayerId::new("a")).unwrap().action_cards =
            vec![ActionCardId::new("experimental")];
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .clear();
        let asked = asked_for(
            state,
            "SHIP_MOVED",
            payload(&[
                ("player", "a".into()),
                ("system", "27".into()),
                (
                    "units",
                    serde_json::json!([{"owner": "a", "unit_type": "cruiser", "count": 2}]),
                ),
            ]),
            &["decline"],
        );
        let trigger = only_trigger(&asked);
        assert_eq!(trigger.kind, TriggerKind::ShipMoved);
        assert_eq!(trigger.system, Some(SystemId::new("27")));
        assert_eq!(trigger.units.len(), 1);
        assert_eq!(trigger.units[0].unit_type, "cruiser");
        assert_eq!(trigger.units[0].count, 2);
    }

    #[test]
    fn a_strategic_action_names_the_strategy_card() {
        use crate::decision_context::TriggerKind;
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards
            .clear();
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("coup")];
        let asked = asked_for(
            state,
            "STRATEGIC_ACTION_BEGAN",
            payload(&[("player", "a".into()), ("card", "pok1leadership".into())]),
            &["decline"],
        );
        let trigger = only_trigger(&asked);
        assert_eq!(trigger.kind, TriggerKind::StrategicActionBegan);
        assert_eq!(trigger.actor, Some(PlayerId::new("a")));
        assert_eq!(trigger.card.as_deref(), Some("pok1leadership"));
        assert_eq!(trigger.relation, "when");
    }

    #[test]
    fn every_mapped_window_event_has_a_trigger_kind() {
        // A new window can never silently fall back to `other`.
        for window in window_table().values() {
            assert_ne!(
                crate::trigger::kind_of(window.event),
                crate::decision_context::TriggerKind::Other,
                "{} has no trigger kind",
                window.event
            );
        }
    }

    #[test]
    fn the_trigger_is_not_part_of_the_recorded_decision_or_the_context_canon() {
        use crate::decision_context::DecisionTrigger;
        let event = event_with_card("ACTION_CARD_PLAYED", "a", "fs1");
        let plain = DecisionContext::new(
            PlayerId::new("b"),
            DecisionSource::Reaction("ACTION_CARD_PLAYED".to_owned()),
            "reaction_when_ACTION_CARD_PLAYED",
            ti4_model::state::Phase::Action,
            1,
        );
        let with = plain
            .clone()
            .with_trigger(DecisionTrigger::from_event(&event, "when", &[]));
        assert_eq!(plain.canonical(), with.canonical());
        let choice = crate::choice::Choice::new(
            PlayerId::new("b"),
            "when ACTION_CARD_PLAYED",
            vec![crate::choice::ChoiceOption::decline()],
        )
        .contextualized(with.clone());
        let mut log = crate::choice::DecisionLog::default();
        log.record(&choice, &crate::choice::ChoiceOption::decline());
        assert_eq!(log.records[0].context.as_ref().unwrap().trigger, None);
        let record = crate::choice::DecisionRecord {
            player: PlayerId::new("b"),
            prompt: "when ACTION_CARD_PLAYED".to_owned(),
            chosen: "decline".to_owned(),
            offered: vec!["decline".to_owned()],
            context: Some(with),
        };
        let stripped = crate::choice::DecisionRecord {
            context: Some(plain),
            ..record.clone()
        };
        for version in [
            crate::fingerprint::CanonicalHashVersion::V1,
            crate::fingerprint::CanonicalHashVersion::V2,
        ] {
            assert_eq!(
                crate::fingerprint::decision_hash(version, &record),
                crate::fingerprint::decision_hash(version, &stripped)
            );
        }
    }

    #[test]
    fn an_old_context_without_a_trigger_still_reads_and_a_trigger_round_trips() {
        let old = r#"{"version":1,"actor":"b","source":{"Reaction":"X"},"subtype":"reaction_when_X","phase":"action","round":1,"optional":true,"target":null,"outstanding":[]}"#;
        let context: DecisionContext = serde_json::from_str(old).expect("old shape reads");
        assert!(context.trigger.is_none());
        let json = serde_json::to_string(&context).unwrap();
        assert!(!json.contains("trigger"), "absent stays absent on the wire");
        let event = event_with_card("ACTION_CARD_PLAYED", "a", "fs1");
        let with = context.with_trigger(crate::decision_context::DecisionTrigger::from_event(
            &event,
            "when",
            &[],
        ));
        let back: DecisionContext =
            serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
        assert_eq!(back, with);
    }

    #[test]
    fn hits_and_votes_never_name_the_wrong_seat_or_an_outcome() {
        use crate::decision_context::{DecisionTrigger, TriggerKind};
        let mut hits = BTreeMap::new();
        hits.insert("player".to_owned(), "victim".into());
        hits.insert("gunner".to_owned(), "shooter".into());
        hits.insert("hits".to_owned(), 3.into());
        let cannon =
            DecisionTrigger::from_event(&Event::new(5, "SPACE_CANNON_HITS", hits), "when", &[1, 2]);
        assert_eq!(cannon.kind, TriggerKind::SpaceCannonHits);
        assert_eq!(cannon.actor, Some(PlayerId::new("shooter")));
        assert_eq!(cannon.subject, Some(PlayerId::new("victim")));
        assert_eq!(cannon.hits, Some(3));
        assert_eq!(cannon.chain, [1, 2]);
        let mut votes = BTreeMap::new();
        votes.insert("player".to_owned(), "a".into());
        votes.insert("outcome".to_owned(), "for".into());
        let cast = DecisionTrigger::from_event(&Event::new(6, "VOTES_CAST", votes), "after", &[]);
        assert_eq!(cast.kind, TriggerKind::VotesCast);
        let json = serde_json::to_string(&cast).unwrap();
        assert!(
            !json.contains("outcome") && !json.contains("\"for\""),
            "{json}"
        );
        let mut agenda = BTreeMap::new();
        agenda.insert("player".to_owned(), "an outcome".into());
        agenda.insert("elected_player".to_owned(), "b".into());
        let resolved =
            DecisionTrigger::from_event(&Event::new(7, "AGENDA_RESOLVED", agenda), "when", &[]);
        assert_eq!(
            resolved.actor, None,
            "player here is the outcome, not a seat"
        );
        assert_eq!(resolved.subject, Some(PlayerId::new("b")));
    }

    #[test]
    fn reaction_conditions_are_checked_before_offering() {
        // Decoy Operation window: "After another player activates a system that contains
        // 1 or more of your structures". The card should only be offered if the activated
        // system actually contains the player's structures, not always when another player activates.
        let content = ContentStore::embedded();
        let b = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);

        // Player B holds Decoy Operation
        state.player_mut(&b).unwrap().action_cards = vec![ActionCardId::new("decoy")];

        // Player A activates system 33 (Corneeq)
        let activation = system_activation_event("a", "33");

        // Player B has no structures in system 33, so Decoy Operation should not be offered
        assert!(
            playable_now(&state, content, &b, &activation, Relation::After).is_empty(),
            "Decoy Operation should not be offered when the system contains no structures"
        );

        // Place a Space Dock for player B in system 33
        let spacedock =
            ti4_model::Unit::new(ti4_model::id::UnitTypeId::new("spacedock"), b.clone());
        state.system_mut(&SystemId::new("33")).units.push(spacedock);

        // Now Decoy Operation should be offered
        assert_eq!(
            playable_now(&state, content, &b, &activation, Relation::After),
            vec![ActionCardId::new("decoy")],
            "Decoy Operation should be offered when the system contains the player's structures"
        );
    }

    /// BF-00h-cards: staged card events are announced oldest first; with none staged nothing runs.
    #[test]
    fn staged_card_events_are_announced_through_their_windows() {
        use crate::factions::hooks_cards::{
            RevealScope, discard_chosen, reveal_hand, take_revealed_action_card,
        };
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&PlayerId::new("a")).unwrap().action_cards =
            vec![ActionCardId::new("bribery")];
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("dh1")];
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut table = crate::choice::Table::new();
        let announced = crate::fixtures::with_context(&mut state, POK, None, &mut table, |ctx| {
            assert_eq!(
                announce_staged_card_events(ctx, &mut resolver).unwrap(),
                0,
                "nothing staged, nothing emitted"
            );
            assert!(resolver.log().is_empty());
            let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
            assert!(discard_chosen(ctx.state, &a, &ActionCardId::new("bribery")));
            reveal_hand(ctx.state, &a, &b, RevealScope::Choice, "mi");
            assert!(take_revealed_action_card(
                ctx.state,
                &a,
                &b,
                &ActionCardId::new("dh1")
            ));
            announce_staged_card_events(ctx, &mut resolver).unwrap()
        });
        assert_eq!(announced, 2);
        let log = resolver.log().join(
            "
",
        );
        let discarded = log
            .find("emit ACTION_CARD_DISCARDED")
            .expect("discard announced");
        let taken = log.find("emit ACTION_CARD_TAKEN").expect("take announced");
        assert!(discarded < taken, "oldest first: {log}");
        assert_eq!(
            state.discarded_action_cards,
            [ActionCardId::new("bribery")],
            "the pile is set when the discard is announced"
        );
        assert_eq!(
            state.last_action_discarded,
            Some((PlayerId::new("a"), ActionCardId::new("bribery")))
        );
    }
}
