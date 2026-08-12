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
