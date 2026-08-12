//! Action cards: draw, hand limit, discard (LRR 2).
//!
//! Ported from the oracle's `engine/action_cards.py`: `draw`, `_first_of_each`,
//! `enforce_hand_limit`, `discard` and `unimplemented`.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;
use ti4_model::id::{ActionCardId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};

/// 2.4: seven cards in hand at the end of a turn.
pub const HAND_LIMIT: usize = 7;

/// The choice kind for discarding down to the limit.
pub const DISCARD_KIND: &str = "discard";

/// A card's printed name, falling back to its alias.
///
/// The alias is not the card. A card printed four times has four aliases — Morale Boost is
/// `mb1` to `mb4`, and so are Flank Speed, Skilled Retreat and Maneuvering Jets — so two copies
/// in one hand are two *different* aliases carrying identical text.
#[must_use]
pub fn name_of(content: &ContentStore, card: &ActionCardId) -> String {
    content
        .get(ContentType::ActionCards, card.as_str())
        .and_then(|record| record.text("name"))
        .unwrap_or_else(|| card.as_str())
        .to_owned()
}

/// The first index of each distinct *card* in a hand, keyed by printed name.
///
/// Keying on the alias would leave two copies looking distinct, and the hand would offer the
/// same card twice — which is not free, because a sampling decider draws per option.
#[must_use]
pub fn first_of_each(content: &ContentStore, hand: &[ActionCardId]) -> BTreeMap<String, usize> {
    let mut first = BTreeMap::new();
    for (index, card) in hand.iter().enumerate() {
        first.entry(name_of(content, card)).or_insert(index);
    }
    first
}

/// 2.3: take from the top of the deck into hand.
///
/// Returns what was drawn. An empty deck draws nothing rather than reshuffling: this engine
/// tracks no discard pile, so there is nothing to shuffle back, and inventing a fresh deck
/// would hand out cards that are already in someone's hand.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers the hand-limit discard with something not offered.
pub fn draw(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
    count: usize,
) -> Result<Vec<ActionCardId>, IllegalChoice> {
    let mut drawn = Vec::new();
    for _ in 0..count {
        if state.action_card_deck.is_empty() {
            break;
        }
        let top = state.action_card_deck.remove(0);
        if let Some(seat) = state.player_mut(player) {
            seat.action_cards.push(top.clone());
        }
        drawn.push(top);
    }
    enforce_hand_limit(state, content, table, player)?;
    Ok(drawn)
}

/// 2.4: over the limit, the player chooses which to discard.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn enforce_hand_limit(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    loop {
        let hand = state
            .player(player)
            .map(|seat| seat.action_cards.clone())
            .unwrap_or_default();
        // Sanctions caps the hand at three; without it the printed seven applies.
        if hand.len() <= crate::laws::action_card_limit(state, HAND_LIMIT) {
            return Ok(());
        }

        // One option per distinct card. Two copies of Maneuvering Jets are one decision
        // written twice, and the copy is not free: a sampling decider draws per option, so the
        // card held two of was likelier to be discarded than the one held one of, whatever it
        // thought of either.
        let distinct = first_of_each(content, &hand);
        let options: Vec<ChoiceOption> = distinct
            .iter()
            .map(|(name, index)| {
                ChoiceOption::labelled(index.to_string(), DISCARD_KIND, name.clone())
            })
            .collect();
        let choice = Choice::new(
            player.clone(),
            format!("over the hand limit — discard one of {}", hand.len()),
            options,
        );
        let answer = table.ask(&choice)?;
        let index = answer.id.parse::<usize>().unwrap_or(0);
        discard(state, player, index);
    }
}

/// Remove one card from a hand by index.
pub fn discard(state: &mut GameState, player: &PlayerId, index: usize) -> Option<ActionCardId> {
    let seat = state.player_mut(player)?;
    if index >= seat.action_cards.len() {
        return None;
    }
    Some(seat.action_cards.remove(index))
}

// -- the component action (22.1) -----------------------------------------------------------------

/// The kind of a component-action option.
pub const ACTION_KIND: &str = "component";

/// The prefix of an option that plays an action card as a turn action.
const PLAY_PREFIX: &str = "action_card|";

/// Whether a card's printed window makes it a component action (22.1).
#[must_use]
pub fn is_component_action(content: &ContentStore, alias: &ActionCardId) -> bool {
    content
        .get(ContentType::ActionCards, alias.as_str())
        .and_then(|record| record.text("window"))
        .is_some_and(|window| window.trim() == "Action")
}

/// 22.3: whether this card could be played right now.
///
/// A card nobody has modelled has no stated requirement, so it stays offered — which is what
/// keeps an unimplemented card countable rather than invisible. The alternative hides the gap
/// behind a card that simply never appears.
#[must_use]
pub fn is_playable(state: &GameState, player: &PlayerId, _alias: &ActionCardId) -> bool {
    !crate::laws::action_cards_forbidden(state, player)
}

/// Component actions this player could take with the cards in hand (22.1).
///
/// Indexed by hand position rather than by alias, because a hand may hold two copies of the
/// same card and naming the alias would make them one option — which a sampling decider would
/// then draw half as often as it should.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    seat.action_cards
        .iter()
        .enumerate()
        .filter(|(_, alias)| is_component_action(content, alias))
        .filter(|(_, alias)| is_playable(state, player, alias))
        .map(|(index, alias)| {
            crate::choice::ChoiceOption::labelled(
                format!("{PLAY_PREFIX}{index}"),
                ACTION_KIND,
                format!("play {}", name_of(content, alias)),
            )
        })
        .collect()
}

/// Play an action card as a component action. Returns `false` for an option that is not one.
///
/// # Errors
/// [`crate::timing::TimingError`] when announcing the play cannot be resolved.
pub fn perform(
    context: &mut crate::timing::TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> Result<bool, crate::timing::TimingError> {
    let Some(index) = option
        .id
        .strip_prefix(PLAY_PREFIX)
        .and_then(|index| index.parse::<usize>().ok())
    else {
        return Ok(false);
    };
    let held = context
        .state
        .player(player)
        .and_then(|seat| seat.action_cards.get(index).cloned());
    let Some(alias) = held else {
        return Ok(false);
    };
    if !is_component_action(context.content, &alias) || !is_playable(context.state, player, &alias)
    {
        return Ok(false);
    }

    // 22.3: the card leaves the hand whether or not its effect is modelled. It was genuinely
    // played, and pretending otherwise would let a bot hold it for ever.
    discard(context.state, player, index);
    crate::reactions::announce(context, resolver, player, &alias)?;
    Ok(true)
}

// -- effects (M06-016b) --------------------------------------------------------------------------

/// What playing an action card does.
///
/// A card with no entry here is played, announced, and reports itself unresolved — the registry
/// design used throughout this engine. A card that silently did nothing would be indistinguishable
/// from one that worked.
pub type Effect = fn(&mut crate::timing::TimingContext<'_>, &PlayerId);

/// Morale Boost: "+1 to the result of each of your unit's combat rolls during this combat round."
///
/// Scoped to [`GameState::combat_round_seq`] rather than a flag, so the bonus expires with the
/// round it was played in. A flag would improve every later round of the same combat too.
fn morale_boost(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let round = context.state.combat_round_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.combat_bonus_round = Some(round);
    }
}

/// Flank Speed: "+1 to the move value of each of your ships during this tactical action."
///
/// Scoped to [`GameState::activation_seq`] for the same reason: the card says *this* tactical
/// action, and an unscoped bonus would follow the fleet for the rest of the game.
fn flank_speed(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.move_bonus_activation = Some(activation);
    }
}

/// Nav Suite: "during the Movement step of this tactical action, ignore the effect of anomalies."
///
/// All of them, including a gravity rift's +1 and its destruction roll. A rift's bonus is as much
/// an effect of an anomaly as a supernova's bar, so the card gives up the one along with the other.
fn nav_suite(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.anomalies_ignored_activation = Some(activation);
    }
}

/// In The Silence Of Space: "choose 1 system; during this tactical action, your ships in the
/// chosen system can move through systems that contain other players' ships."
///
/// The permission is tied to where the ships *start*, not to what they pass through, so the chosen
/// origin is stored and checked against the origin of each route.
fn in_the_silence_of_space(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let origins: Vec<ti4_model::id::SystemId> = context
        .state
        .systems_with_units_of(player)
        .into_iter()
        .filter(|system| !context.state.ships_of(player, system).is_empty())
        .cloned()
        .collect();
    let Some(first) = origins.first().cloned() else {
        return; // no ships anywhere, so there is nothing to free
    };
    let chosen = if origins.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "In The Silence Of Space: whose ships ignore blockades",
            origins
                .iter()
                .map(|system| {
                    crate::choice::ChoiceOption::labelled(
                        system.to_string(),
                        "silence",
                        format!("ships in {system} may pass blockades"),
                    )
                })
                .collect(),
        );
        match context.table.ask(&choice) {
            Ok(answer) => ti4_model::id::SystemId::new(answer.id),
            Err(_) => return,
        }
    };
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.silence_activation = Some(activation);
        seat.silence_system = Some(chosen);
    }
}

/// Skilled Retreat: "move all of your ships from the active system into an adjacent system that
/// does not contain another player's ships. The space combat ends in a draw. Then place a command
/// token in that system."
///
/// The movement, the stranding of cargo beyond capacity and the token are exactly a retreat, so
/// [`crate::combat::retreat_to`] does them. What the card changes is *where* it may go and that
/// the result is a draw rather than a win for whoever stayed.
fn skilled_retreat(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let Some(galaxy) = context.galaxy else {
        return; // without a map there is no "adjacent", so the card cannot resolve
    };
    let destinations = crate::combat::skilled_retreat_destinations(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
        &system,
    );
    let Some(first) = destinations.first().cloned() else {
        return;
    };
    let chosen = if destinations.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "Skilled Retreat: withdraw to which system",
            destinations
                .iter()
                .map(|system| {
                    crate::choice::ChoiceOption::labelled(
                        system.to_string(),
                        "skilled_retreat",
                        format!("withdraw to {system}"),
                    )
                })
                .collect(),
        );
        match context.table.ask(&choice) {
            Ok(answer) => ti4_model::id::SystemId::new(answer.id),
            Err(_) => return,
        }
    };

    crate::combat::retreat_to(
        context.state,
        context.content,
        context.sources,
        player,
        &system,
        &chosen,
    );
    context.state.combat_draw_round = Some(context.state.combat_round_seq);
}

/// Imperial Rider: "predict aloud an outcome of this agenda. If your prediction is correct, gain
/// 1 victory point." The cost is the vote: a player who predicts cannot vote on that agenda.
///
/// The prediction is read from the event rather than from the game, because the card is played
/// into the `AGENDA_REVEALED` window and the outcomes are what that event carries.
fn imperial_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let choices: Vec<String> = context.state.agenda_choices.clone();
    let Some(first) = choices.first().cloned() else {
        return; // nothing to predict, so the card cannot resolve (22.3)
    };
    let predicted = if choices.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "Imperial Rider: predict the agenda outcome",
            choices
                .iter()
                .map(|outcome| {
                    crate::choice::ChoiceOption::labelled(
                        outcome.clone(),
                        "prediction",
                        format!("predict {outcome}"),
                    )
                })
                .collect(),
        );
        match context.table.ask(&choice) {
            Ok(answer) => answer.id,
            Err(_) => return,
        }
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), predicted);
}

/// Pay every correct Imperial Rider once the outcome is known, and clear the predictions.
///
/// Called when a vote closes. Clearing matters: a prediction left behind would pay again on the
/// next agenda, for a card that was spent on this one.
pub fn resolve_predictions(state: &mut GameState, outcome: &str) -> Vec<PlayerId> {
    let predictions = std::mem::take(&mut state.agenda_predictions);
    let mut paid = Vec::new();
    for (player, predicted) in predictions {
        if predicted == outcome {
            if let Some(seat) = state.player_mut(&player) {
                seat.victory_points =
                    (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
            }
            paid.push(player);
        }
    }
    paid
}

/// Apply every movement modifier this player currently owns to a set of rules.
///
/// One door, called wherever rules are built for a real move. Reading the fields at each
/// construction site instead would mean a card that works in the option list and not in the move
/// that follows, which is worse than one that does not work at all.
pub fn apply_movement_effects(
    rules: &mut crate::movement::MovementRules<'_>,
    state: &GameState,
    player: &PlayerId,
) {
    rules.rifts_ignored = crate::relics::ignores_gravity_rifts(state, player);
    let Some(seat) = state.player(player) else {
        return;
    };
    let this_activation = Some(state.activation_seq);
    if seat.anomalies_ignored_activation == this_activation {
        rules.anomalies_ignored = true;
    }
    if seat.silence_activation == this_activation {
        rules.ignore_enemy_ships_from = seat.silence_system.as_ref().map(ToString::to_string);
    }
}

/// The effect registered for a card, if this engine has one.
#[must_use]
pub fn effect_for(alias: &ActionCardId) -> Option<Effect> {
    match alias.as_str() {
        // Four physical copies each, resolved from the printed name rather than listed by hand:
        // the registry test catches a *wrong* alias, but nothing catches a *missing* one, and a
        // fourth copy left off a list stays unplayable for ever with no symptom.
        "mb1" | "mb2" | "mb3" | "mb4" => Some(morale_boost),
        "fs1" | "fs2" | "fs3" | "fs4" => Some(flank_speed),
        "imp_rider" => Some(imperial_rider),
        "nav_suite" => Some(nav_suite),
        "s_retreat1" | "s_retreat2" | "s_retreat3" | "s_retreat4" => Some(skilled_retreat),
        "silence_space" => Some(in_the_silence_of_space),
        _ => None,
    }
}

/// Aliases with a registered effect.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "fs1",
        "fs2",
        "fs3",
        "fs4",
        "imp_rider",
        "mb1",
        "mb2",
        "mb3",
        "mb4",
        "nav_suite",
        "s_retreat1",
        "s_retreat2",
        "s_retreat3",
        "s_retreat4",
        "silence_space",
    ]
}

/// This player's ships move one further during `activation`, from Flank Speed.
#[must_use]
pub fn move_bonus(state: &GameState, player: &PlayerId, activation: u32) -> i32 {
    i32::from(
        state
            .player(player)
            .is_some_and(|seat| seat.move_bonus_activation == Some(activation)),
    )
}

/// Cards this engine has no effect for.
///
/// Every action card is currently unimplemented; the list is exposed so the gap is queryable
/// rather than implied, in the same way `unregistered_objectives` is.
#[must_use]
pub fn unimplemented(content: &ContentStore) -> Vec<ActionCardId> {
    content
        .records(ContentType::ActionCards)
        .iter()
        .filter_map(|record| record.text("alias"))
        .map(ActionCardId::new)
        .collect()
}

#[cfg(test)]
mod tests {

    /// Resolve one card's effect against a state, through a real timing context.
    fn play_effect(state: &mut GameState, alias: &str, player: &PlayerId) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        effect(&mut context, player);
    }

    #[test]
    fn morale_boost_expires_with_the_round_it_was_played_in() {
        // "During this combat round." Held as the round number rather than a flag, because a
        // flag would improve every later round of the same combat as well.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        state.combat_round_seq = 4;

        play_effect(&mut state, "mb1", &player);

        assert_eq!(
            state.player(&player).unwrap().combat_bonus_round,
            Some(4),
            "the bonus belongs to the round it was played in"
        );
        assert_ne!(
            state.player(&player).unwrap().combat_bonus_round,
            Some(5),
            "and not to the next one"
        );
    }

    #[test]
    fn flank_speed_expires_with_the_tactical_action_it_was_played_in() {
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        state.activation_seq = 3;

        play_effect(&mut state, "fs1", &player);

        assert_eq!(move_bonus(&state, &player, 3), 1, "this activation");
        assert_eq!(move_bonus(&state, &player, 4), 0, "not the next one");
        assert_eq!(
            move_bonus(&state, &PlayerId::new("b"), 3),
            0,
            "and not somebody else's ships"
        );
    }

    #[test]
    fn every_copy_of_a_card_carries_the_same_effect() {
        // Morale Boost is four physical cards and so is Flank Speed. A list written by hand
        // catches a wrong alias but never a missing one, and the only symptom of a fourth copy
        // left off is a slightly lower play rate.
        let content = ContentStore::embedded();
        for name in ["Morale Boost", "Flank Speed", "Skilled Retreat"] {
            let copies: Vec<ActionCardId> = content
                .from_sources(
                    ti4_model::content_types::ContentType::ActionCards,
                    ti4_model::content_types::POK,
                )
                .filter(|record| record.text("name") == Some(name))
                .filter_map(|record| record.text("alias").map(ActionCardId::new))
                .collect();
            assert!(copies.len() > 1, "{name} is printed more than once");
            for alias in copies {
                assert!(
                    effect_for(&alias).is_some(),
                    "{alias} is a copy of {name} and must carry its effect"
                );
            }
        }
    }

    #[test]
    fn a_correct_prediction_is_worth_a_point_and_a_wrong_one_is_not() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "for".to_owned());
        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "against".to_owned());

        let paid = resolve_predictions(&mut state, "for");

        assert_eq!(paid, vec![PlayerId::new("a")]);
        assert_eq!(state.player(&PlayerId::new("a")).unwrap().victory_points, 1);
        assert_eq!(state.player(&PlayerId::new("b")).unwrap().victory_points, 0);
    }

    #[test]
    fn predictions_do_not_survive_the_agenda_they_were_made_on() {
        // The card is spent on one agenda. A prediction left behind pays again on the next.
        let mut state = crate::fixtures::game(&["a"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "for".to_owned());

        resolve_predictions(&mut state, "for");
        assert!(state.agenda_predictions.is_empty());

        let paid_again = resolve_predictions(&mut state, "for");
        assert!(paid_again.is_empty(), "and cannot pay a second time");
        assert_eq!(state.player(&PlayerId::new("a")).unwrap().victory_points, 1);
    }

    /// The first card in the corpus whose printed window makes it a component action.
    fn a_component_action_card() -> ActionCardId {
        ContentStore::embedded()
            .from_sources(
                ti4_model::content_types::ContentType::ActionCards,
                ti4_model::content_types::POK,
            )
            .find(|record| record.text("window") == Some("Action"))
            .and_then(|record| record.text("alias").map(ActionCardId::new))
            .expect("the corpus has component-action cards")
    }

    #[test]
    fn only_a_card_that_says_action_is_a_component_action() {
        // 22.1. Offering a reaction card as a turn action would let a player spend a turn on a
        // card whose own text says it is played in somebody else's window.
        let content = ContentStore::embedded();
        assert!(is_component_action(content, &a_component_action_card()));
        assert!(
            !is_component_action(content, &ActionCardId::new("fs1")),
            "Flank Speed is played after an activation, not as a turn"
        );
    }

    #[test]
    fn a_component_action_card_in_hand_is_offered() {
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards =
            vec![ActionCardId::new("fs1"), card.clone()];

        let offered = available_actions(&state, content, &player);

        assert_eq!(offered.len(), 1, "one of the two is a turn action");
        assert!(
            offered[0].id.ends_with('1'),
            "offered by hand position, so two copies are two options: {}",
            offered[0].id
        );
    }

    #[test]
    fn two_copies_of_one_card_are_two_options() {
        // Indexed by hand position rather than by alias. Naming the alias would collapse them
        // into one option, which a sampling decider then draws half as often as it should.
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards = vec![card.clone(), card];

        assert_eq!(available_actions(&state, content, &player).len(), 2);
    }

    #[test]
    fn political_censure_stops_its_owner_playing_action_cards() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player).unwrap().action_cards = vec![a_component_action_card()];
        assert_eq!(available_actions(&state, content, &player).len(), 1);

        state.enact_law("censure", "a");
        assert!(
            available_actions(&state, content, &player).is_empty(),
            "the elected owner cannot play action cards"
        );
        assert_eq!(
            available_actions(&state, content, &PlayerId::new("b")).len(),
            0,
            "and b has no cards either way"
        );
    }

    #[test]
    fn playing_a_component_action_spends_the_card_even_with_no_effect() {
        // 22.3: it was genuinely played. Leaving an unmodelled card in hand would let a bot
        // hold it for ever and would hide the gap behind a card that never leaves.
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards = vec![card.clone()];

        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolver = crate::timing::Resolver::default();
        let option = available_actions(&state, content, &player)
            .into_iter()
            .next()
            .expect("it is offered");
        let played = {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content,
                sources: ti4_model::content_types::POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            perform(&mut context, &mut resolver, &player, &option).unwrap()
        };

        assert!(played);
        assert!(
            state.player(&player).unwrap().action_cards.is_empty(),
            "the card left the hand"
        );
        assert!(
            resolver
                .log()
                .iter()
                .any(|line| line.contains("ACTION_CARD_UNRESOLVED")),
            "and said it had no effect: {:?}",
            resolver.log()
        );
    }

    #[test]
    fn the_alias_list_and_the_effect_table_agree() {
        // Two places record the same fact: `effect_for` decides behaviour and
        // `registered_aliases` feeds the coverage ledger. They drifted the first time a card was
        // added — the effect worked and the ledger under-reported by four — so this ties them.
        let content = ContentStore::embedded();
        let listed: std::collections::BTreeSet<&str> = registered_aliases().into_iter().collect();

        for record in content.from_sources(
            ti4_model::content_types::ContentType::ActionCards,
            ti4_model::content_types::POK,
        ) {
            let Some(alias) = record.text("alias") else {
                continue;
            };
            let has_effect = effect_for(&ActionCardId::new(alias)).is_some();
            assert_eq!(
                has_effect,
                listed.contains(alias),
                "{alias}: effect_for says {has_effect}, registered_aliases says {}",
                listed.contains(alias)
            );
        }
    }

    #[test]
    fn every_registered_alias_is_a_real_action_card() {
        for alias in registered_aliases() {
            assert!(
                ContentStore::embedded()
                    .get(ti4_model::content_types::ContentType::ActionCards, alias)
                    .is_some(),
                "{alias} is not an action card the corpus knows"
            );
        }
    }

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn hand(state: &GameState) -> Vec<ActionCardId> {
        state.player(&player()).unwrap().action_cards.clone()
    }

    fn set_hand(state: &mut GameState, cards: &[&str]) {
        state.player_mut(&player()).unwrap().action_cards =
            cards.iter().map(|c| ActionCardId::new(*c)).collect();
    }

    /// Two aliases of the same printed card, if the corpus has such a pair.
    fn a_duplicated_card() -> Option<(String, String)> {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::ActionCards) {
            if let (Some(name), Some(alias)) = (record.text("name"), record.text("alias")) {
                by_name
                    .entry(name.to_owned())
                    .or_default()
                    .push(alias.to_owned());
            }
        }
        by_name
            .into_values()
            .find(|aliases| aliases.len() >= 2)
            .map(|aliases| (aliases[0].clone(), aliases[1].clone()))
    }

    #[test]
    fn drawing_takes_from_the_top() {
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck = vec![ActionCardId::new("c1"), ActionCardId::new("c2")];
        let mut table = Table::new();

        let drawn = draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            2,
        )
        .unwrap();

        assert_eq!(drawn.len(), 2);
        assert!(state.action_card_deck.is_empty());
        assert_eq!(hand(&state).len(), 2);
    }

    #[test]
    fn an_empty_deck_draws_nothing() {
        // No discard pile is tracked, so inventing a fresh deck would deal out cards that are
        // already in somebody's hand.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck.clear();
        let mut table = Table::new();

        let drawn = draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            3,
        )
        .unwrap();

        assert!(drawn.is_empty());
        assert!(hand(&state).is_empty());
    }

    #[test]
    fn sanctions_tightens_the_hand_limit_to_three() {
        // A law that is enacted but not enforced is a list nothing reads; this one bites.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.enact_law("sanctions", "for");
        state.action_card_deck = (0..10)
            .map(|n| ActionCardId::new(format!("c{n}")))
            .collect();
        let mut table = Table::new();

        draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            10,
        )
        .unwrap();

        assert_eq!(hand(&state).len(), 3, "not the printed seven");
    }

    #[test]
    fn a_hand_over_seven_discards_down() {
        // 2.4.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck = (0..10)
            .map(|n| ActionCardId::new(format!("c{n}")))
            .collect();
        let mut table = Table::new();

        draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            10,
        )
        .unwrap();

        assert_eq!(hand(&state).len(), HAND_LIMIT);
    }

    #[test]
    fn two_copies_of_one_card_are_one_discard_decision() {
        // The alias is not the card. Keying on the alias leaves two copies looking distinct,
        // and the hand offers the same card twice — which is not free, because a sampling
        // decider draws per option and would discard the card it held two of more often.
        let Some((first, second)) = a_duplicated_card() else {
            return;
        };
        let mut state = game(&["a"]);
        set_hand(&mut state, &[&first, &second]);

        let distinct = first_of_each(ContentStore::embedded(), &hand(&state));
        assert_eq!(distinct.len(), 1, "two aliases, one printed card");
    }

    #[test]
    fn distinct_cards_are_offered_separately() {
        let mut state = game(&["a"]);
        let two: Vec<String> = ContentStore::embedded()
            .records(ContentType::ActionCards)
            .iter()
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(2)
            .collect();
        set_hand(&mut state, &[&two[0], &two[1]]);

        let distinct = first_of_each(ContentStore::embedded(), &hand(&state));
        let names: std::collections::BTreeSet<String> = hand(&state)
            .iter()
            .map(|card| name_of(ContentStore::embedded(), card))
            .collect();
        assert_eq!(distinct.len(), names.len());
    }

    #[test]
    fn discarding_out_of_range_changes_nothing() {
        let mut state = game(&["a"]);
        set_hand(&mut state, &["c1"]);
        assert_eq!(discard(&mut state, &player(), 5), None);
        assert_eq!(hand(&state).len(), 1);
    }

    #[test]
    fn an_unknown_card_falls_back_to_its_alias() {
        // A missing display name is not a reason to lose the card.
        assert_eq!(
            name_of(ContentStore::embedded(), &ActionCardId::new("not_a_card")),
            "not_a_card"
        );
    }

    #[test]
    fn every_action_card_is_reported_unimplemented() {
        // The gap is queryable rather than implied: nothing plays an action card yet.
        let all = unimplemented(ContentStore::embedded());
        assert!(all.len() > 50, "the corpus has a full deck");
    }
}
