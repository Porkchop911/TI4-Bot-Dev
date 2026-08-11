//! Structural strategy-card actions.

use ti4_content::ContentStore;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::draft::strategy_card_label;

/// The historical action id used when a player has exactly one strategic action available.
pub const STRATEGIC_ACTION_ID: &str = "strategic";
/// The choice kind for an ordinary action-phase action.
pub const ACTION_KIND: &str = "action";

/// A strategic action could not be selected from the state that was presented.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrategyActionError {
    #[error("player {0} has no unused strategy card")]
    NoUnusedStrategyCard(PlayerId),
    #[error("strategy action id {0:?} was malformed")]
    MalformedActionId(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// Legal structural strategic actions for one player.
///
/// A one-card holding preserves the oracle's bare `strategic` id for compatibility. A
/// multi-card holding names the selected card in `strategic|<card-id>` so the selected
/// strategic action is a real decision rather than an arbitrary fallback.
#[must_use]
pub fn strategic_action_options(
    state: &GameState,
    content: &ContentStore,
    player_id: &PlayerId,
) -> Option<Choice> {
    let player = state.player(player_id)?;
    let unused = player.unused_strategy_cards();
    if unused.is_empty() {
        return None;
    }
    let options = if unused.len() == 1 {
        vec![ChoiceOption::labelled(
            STRATEGIC_ACTION_ID,
            ACTION_KIND,
            "take your strategic action",
        )]
    } else {
        unused
            .into_iter()
            .map(|card| {
                ChoiceOption::labelled(
                    format!("{STRATEGIC_ACTION_ID}|{}", card.as_str()),
                    ACTION_KIND,
                    format!(
                        "take the strategic action of {}",
                        strategy_card_label(content, card.as_str())
                    ),
                )
            })
            .collect()
    };
    Some(Choice::new(player_id.clone(), "action phase", options))
}

/// Resolve the structural part of a selected strategic action.
///
/// Card-specific primary and secondary effects are deliberately outside this package. The
/// selected card is exhausted only after this structural primary has finished; M04-009
/// extends this same boundary with secondaries before retaining that exhaustion state.
///
/// # Errors
/// [`StrategyActionError::IllegalChoice`] if `answer` was not offered, or
/// [`StrategyActionError::NoUnusedStrategyCard`] if the player has no available card.
pub fn take_strategic_action(
    state: &mut GameState,
    content: &ContentStore,
    player_id: &PlayerId,
    answer: ChoiceOption,
) -> Result<StrategyCardId, StrategyActionError> {
    let choice = strategic_action_options(state, content, player_id)
        .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?;
    let answer = validate(&choice, answer)?;
    let player = state
        .player(player_id)
        .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?;
    let unused = player.unused_strategy_cards();
    let card = if answer.id == STRATEGIC_ACTION_ID {
        unused
            .first()
            .copied()
            .cloned()
            .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?
    } else {
        let (_, named) = answer
            .id
            .split_once('|')
            .ok_or_else(|| StrategyActionError::MalformedActionId(answer.id.clone()))?;
        unused
            .into_iter()
            .find(|card| card.as_str() == named)
            .cloned()
            .ok_or_else(|| StrategyActionError::MalformedActionId(answer.id.clone()))?
    };

    // The option was generated from this player's unused holding, so no other player or
    // exhausted card can be changed here.
    let exhausted = state.exhaust_strategy_card(player_id, card.clone());
    debug_assert!(exhausted, "the checked player must hold the checked card");
    Ok(card)
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    use super::*;
    use crate::draft::{strategy_options, take_strategy_card};
    use crate::setup::start_game;

    fn drafted_three_player_game() -> GameState {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        while let Some(choice) = strategy_options(&state, ContentStore::embedded()) {
            take_strategy_card(
                &mut state,
                ContentStore::embedded(),
                choice.options[0].clone(),
            )
            .unwrap();
        }
        state
    }

    #[test]
    fn a_player_with_one_unused_card_keeps_the_legacy_bare_action_id() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        for _ in 0..2 {
            let choice = strategy_options(&state, ContentStore::embedded()).unwrap();
            take_strategy_card(
                &mut state,
                ContentStore::embedded(),
                choice.options[0].clone(),
            )
            .unwrap();
        }

        let choice =
            strategic_action_options(&state, ContentStore::embedded(), &PlayerId::new("a"))
                .unwrap();

        assert_eq!(choice.ids(), vec!["strategic"]);
    }

    #[test]
    fn each_unused_card_is_a_distinct_action_in_a_multi_card_holding() {
        let state = drafted_three_player_game();
        let choice =
            strategic_action_options(&state, ContentStore::embedded(), &PlayerId::new("a"))
                .unwrap();
        let held = &state.player(&PlayerId::new("a")).unwrap().strategy_cards;

        assert_eq!(
            choice.ids(),
            held.iter()
                .map(|card| format!("strategic|{card}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn spending_one_of_two_cards_leaves_the_other_unspent() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let first = state.player(&player).unwrap().strategy_cards[0].clone();
        let second = state.player(&player).unwrap().strategy_cards[1].clone();

        assert_eq!(
            take_strategic_action(
                &mut state,
                ContentStore::embedded(),
                &player,
                ChoiceOption::new(format!("strategic|{first}"), ACTION_KIND),
            )
            .unwrap(),
            first
        );
        let seat = state.player(&player).unwrap();
        assert!(seat.exhausted_strategy_cards.contains(&first));
        assert!(!seat.exhausted_strategy_cards.contains(&second));
        assert_eq!(
            strategic_action_options(&state, ContentStore::embedded(), &player)
                .unwrap()
                .ids(),
            vec![STRATEGIC_ACTION_ID]
        );
    }

    #[test]
    fn an_invented_strategic_action_is_atomic() {
        let mut state = drafted_three_player_game();
        let before = state.clone();
        let player = PlayerId::new("a");

        let error = take_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &player,
            ChoiceOption::new("strategic|invented", ACTION_KIND),
        )
        .unwrap_err();

        assert!(matches!(error, StrategyActionError::IllegalChoice(_)));
        assert!(state.identical(&before));
    }
}
