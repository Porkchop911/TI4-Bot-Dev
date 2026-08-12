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
        if hand.len() <= HAND_LIMIT {
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
