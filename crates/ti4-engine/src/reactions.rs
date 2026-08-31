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
use ti4_model::id::{ActionCardId, PlayerId};
use ti4_model::state::{GameState, Player};

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

/// "When you are negotiating a transaction" — Black Market Dealings. The event names the
/// proposer as `player` and the other chair as `partner`; either chair at the table is
/// negotiating.
fn party_to_transaction(event: &Event, player: &PlayerId, _state: &GameState) -> bool {
    event.text("player") == Some(player.as_str()) || event.text("partner") == Some(player.as_str())
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
}

/// "When another player plays an action card other than 'Sabotage'": the committer is
/// someone else, and the card being played is not one of the four Sabotage copies — Sabotage
/// cancels other cards being played, not itself (1.15 would otherwise let a chain of
/// Sabotages spend the whole deck for nothing).
fn another_players_card_is_not_sabotage(
    event: &Event,
    player: &PlayerId,
    state: &GameState,
) -> bool {
    actor_is_not(event, player, state) && !is_sabotage_play(event)
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
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one row per printed window: the table is the point, and splitting it hides the set"
)]
pub fn window_table() -> BTreeMap<&'static str, Window> {
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
            window("COMBAT_ROUND_STARTED", After),
        ),
        (
            "At the start or end of a combat round",
            window("COMBAT_ROUND_STARTED", After),
        ),
        (
            "At the start of the first round of a space combat",
            window("SPACE_COMBAT_STARTED", After),
        ),
        (
            "After you activate a system",
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After you activate a system that contains 1 or more of your ships",
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
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After you activate an anomaly",
            guarded("SYSTEM_ACTIVATED", After, actor_is),
        ),
        (
            "After another player activates a system that contains your units",
            guarded("SYSTEM_ACTIVATED", After, actor_is_not),
        ),
        (
            "After another player activates a system that contains 1 of your command tokens",
            guarded("SYSTEM_ACTIVATED", After, actor_is_not),
        ),
        (
            "After another player activates a system that contains 1 or more of your structures",
            guarded("SYSTEM_ACTIVATED", After, actor_is_not),
        ),
        ("At the start of a combat", window("COMBAT_ROUND_STARTED", After)),
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
            guarded("PLANET_CONTROL_GAINED", After, actor_is_not),
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
            guarded("SHIP_DESTROYED", After, actor_is),
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
/// resolves "you" against "another player". A per-card playability check — 22.3, whether the
/// effect has anything to act on — belongs here too and is not modelled yet: no card in this
/// engine has an effect to be blocked on.
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
    seat.action_cards
        .iter()
        .filter(|alias| {
            window_for(content, alias).is_some_and(|window| {
                window.event == event.event_type
                    && window.relation == relation
                    && window.guard.is_none_or(|guard| guard(event, player, state))
            })
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
) -> Result<bool, TimingError> {
    let held = context
        .state
        .player(player)
        .and_then(|seat| seat.action_cards.iter().position(|held| held == alias));
    let Some(index) = held else {
        return Ok(false);
    };
    crate::action_cards::discard(context.state, player, index);
    announce(context, resolver, player, alias)
}

/// Announce a card as played, then resolve it.
///
/// Shared by the reaction window and the component action (22.1), because the announcement is the
/// same event either way: Sabotage cancels *another card being played*, and it hooks the WHEN
/// window of `ACTION_CARD_PLAYED`, which only exists if every path announces before resolving.
///
/// # Errors
/// [`TimingError`] when the announcement cannot be resolved.
pub fn announce(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
    player: &PlayerId,
    alias: &ActionCardId,
) -> Result<bool, TimingError> {
    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("card".to_owned(), alias.to_string().into());
    let announced = context.event_sequence.next("ACTION_CARD_PLAYED", payload)?;
    let announced = resolver.emit_with_context(context, announced, |_, _| {})?;
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
        effect(context, player);
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
    for (system, owner, unit_type) in std::mem::take(&mut context.state.pending_destructions) {
        let remaining = crate::combat::ships_of(
            context.state,
            context.content,
            context.sources,
            &owner,
            &system,
        )
        .len();
        // The same handoff the combat window's own emissions make: a reacting effect that
        // needs to know which ship was destroyed cannot read the event once the window runs.
        context.state.last_ship_destroyed =
            Some((system.clone(), owner.clone(), unit_type.clone()));
        let mut payload = BTreeMap::new();
        payload.insert("system".to_owned(), system.to_string().into());
        payload.insert("player".to_owned(), owner.to_string().into());
        payload.insert("unit".to_owned(), unit_type.to_string().into());
        payload.insert("last".to_owned(), (remaining == 0).into());
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
    context.state.discarded_action_cards.push(alias.clone());
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
            let chosen = if options.len() == 1 {
                first
            } else {
                let choice = crate::choice::Choice::new(
                    owner.clone(),
                    format!(
                        "play an action card ({} {})",
                        relation_name(relation),
                        event.event_type
                    ),
                    reaction_card_options(context.content, &options),
                );
                match context.ask_seeing(&choice) {
                    Ok(answer) => ActionCardId::new(answer.id),
                    Err(_) => return Ok(()),
                }
            };
            let played = play(context, resolver, &owner, &chosen)?;
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

    for seat in &state.players {
        let owner_name = crate::promissory::faction_name(state, &seat.id);
        for (event_type, relation) in &windows {
            resolver.register([slot(&owner_name, &seat.id, event_type, *relation)]);
        }
    }
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
                )
            ]
        );

        // The chosen card was actually played: the hand lost it and the state carries its effect.
        assert_eq!(
            state.player(&player()).unwrap().action_cards,
            [ActionCardId::new("silence_space")]
        );
    }
}
