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
use ti4_model::state::GameState;

use crate::event::Event;
use crate::timing::{Ability, Frequency, Relation, Resolver, TimingContext, TimingError};

/// The choice kind for a reaction offer.
pub const REACTION_KIND: &str = "reaction";

/// Whether a card's window applies to this player, given the event.
type Guard = fn(&Event, &PlayerId) -> bool;

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
fn actor_is(event: &Event, player: &PlayerId) -> bool {
    event.text("player") == Some(player.as_str())
}

/// Somebody else — "another player", "your opponent".
fn actor_is_not(event: &Event, player: &PlayerId) -> bool {
    event
        .text("player")
        .is_some_and(|who| who != player.as_str())
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
            guarded("ACTION_CARD_PLAYED", When, actor_is_not),
        ),
        (
            "At the start of an invasion",
            window("INVASION_BEGAN", After),
        ),
        (
            "After you win a space combat",
            guarded("SPACE_COMBAT_WON", After, actor_is),
        ),
        (
            "When 1 or more of your units use PRODUCTION",
            guarded("PRODUCTION_USED", After, actor_is),
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
    "STRATEGY_CARD_CHOSEN",
    "SYSTEM_ACTIVATED",
    "ACTION_CARD_PLAYED",
];

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
                    && window.guard.is_none_or(|guard| guard(event, player))
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

    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("card".to_owned(), alias.to_string().into());
    let announced = context.event_sequence.next("ACTION_CARD_PLAYED", payload)?;
    let announced = resolver.emit_with_context(context, announced, |_, _| {})?;
    if announced.cancelled {
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
    Ok(true)
}

/// One reaction opportunity, for one player, in one window.
fn slot(player: &PlayerId, event_type: &str, relation: Relation) -> Ability {
    let owner = player.clone();
    let condition_owner = player.clone();
    Ability::stateful(
        format!("reaction:{player}:{event_type}:{relation:?}"),
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
                    format!("play a reaction to {}", event.event_type),
                    options
                        .iter()
                        .map(|alias| {
                            crate::choice::ChoiceOption::labelled(
                                alias.to_string(),
                                REACTION_KIND,
                                alias.to_string(),
                            )
                        })
                        .collect(),
                );
                match context.table.ask(&choice) {
                    Ok(answer) => ActionCardId::new(answer.id),
                    Err(_) => return Ok(()),
                }
            };
            play(context, resolver, &owner, &chosen).map(|_| ())
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
        for (event_type, relation) in &windows {
            resolver.register([slot(&seat.id, event_type, *relation)]);
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
        let printed: std::collections::BTreeSet<String> = content
            .from_sources(ContentType::ActionCards, POK)
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
        assert!(
            !missing.is_empty(),
            "most reaction windows are still unmapped, and that must be visible"
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
}
