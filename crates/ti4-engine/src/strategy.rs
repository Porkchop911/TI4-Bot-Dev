//! Structural strategy-card actions.

use ti4_content::ContentStore;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::draft::strategy_card_label;

/// The historical action id used when a player has exactly one strategic action available.
pub const STRATEGIC_ACTION_ID: &str = "strategic";
/// The choice kind for an ordinary action-phase action.
pub const ACTION_KIND: &str = "action";
/// The id used to follow a strategic action's secondary.
pub const FOLLOW_SECONDARY_ID: &str = "follow";
/// The choice kind for a strategic-action secondary response.
pub const STRATEGY_KIND: &str = "strategy";

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

/// The recorded structural result of one eligible follower's secondary decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryResolution {
    /// The follower chose not to use the secondary.
    Declined,
    /// The follower accepted the secondary. Its shared token cost, when applicable, was paid;
    /// the game driver immediately invokes the content-specific effect.
    Followed,
    /// The follower had no strategy token and was not offered the secondary.
    Ineligible,
}

/// An error while resolving the follower window of a strategic action.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrategySecondaryError {
    #[error("the strategy-secondary window is complete")]
    Complete,
    #[error("follower {0} is no longer seated")]
    FollowerMissing(PlayerId),
    #[error("follower {0} no longer has a strategy token")]
    NoStrategyToken(PlayerId),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

fn secondary_choice(
    content: &ContentStore,
    card: &StrategyCardId,
    player: &PlayerId,
    costs_token: bool,
) -> Choice {
    let name = crate::strategy_cards::card_name(content, card.as_str())
        .unwrap_or_else(|| card.to_string());
    let contract = match card.as_str() {
        "te4construction" => Some((
            "spend a strategy token to place a structure",
            "decline",
            "place",
        )),
        _ => match name.as_str() {
            "Trade" => Some((
                "spend a strategy token to replenish commodities",
                "decline",
                "replenish",
            )),
            "Construction" => Some((
                "spend a strategy token to build a structure",
                "decline",
                "build",
            )),
            "Warfare" => Some((
                "spend a strategy token to produce at home",
                "decline",
                "produce",
            )),
            "Technology" => Some((
                "spend a strategy token and 4 resources to research",
                "decline",
                "spend",
            )),
            "Imperial" => Some((
                "spend a strategy token to draw a secret objective",
                "decline",
                "draw",
            )),
            "Diplomacy" => Some((
                "spend a strategy token to ready two planets",
                "decline",
                "ready",
            )),
            "Politics" => Some((
                "spend a strategy token to draw two action cards",
                "decline",
                "draw",
            )),
            _ => None,
        },
    };
    if let Some((prompt, no_label, yes_label)) = contract {
        return Choice::new(
            player.clone(),
            prompt,
            vec![
                ChoiceOption::labelled("no", STRATEGY_KIND, no_label),
                ChoiceOption::labelled("yes", STRATEGY_KIND, yes_label),
            ],
        );
    }
    Choice::new(
        player.clone(),
        format!("{card} secondary"),
        vec![
            ChoiceOption::decline(),
            ChoiceOption::labelled(
                FOLLOW_SECONDARY_ID,
                STRATEGY_KIND,
                if costs_token {
                    "spend a strategy token to resolve the secondary"
                } else {
                    "resolve the secondary"
                },
            ),
        ],
    )
}

/// The ordered follower window opened by a strategic action.
///
/// The owner is deliberately absent from `followers`: LRR 82.1 offers a secondary to
/// everyone else, clockwise from the primary player. The window keeps completion data
/// outside `GameState` until the M04 step driver can own open decision windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategySecondaryWindow {
    primary_player: PlayerId,
    card: StrategyCardId,
    followers: Vec<PlayerId>,
    next_follower: usize,
    resolutions: Vec<(PlayerId, SecondaryResolution)>,
}

impl StrategySecondaryWindow {
    /// The player resolving the primary.
    #[must_use]
    pub const fn primary_player(&self) -> &PlayerId {
        &self.primary_player
    }

    /// The strategy card whose secondary is being offered.
    #[must_use]
    pub const fn card(&self) -> &StrategyCardId {
        &self.card
    }

    /// Follower results in clockwise resolution order.
    #[must_use]
    pub fn resolutions(&self) -> &[(PlayerId, SecondaryResolution)] {
        &self.resolutions
    }

    /// Whether every follower has been recorded and the primary card exhausted.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.next_follower == self.followers.len()
    }

    /// Inspect the next offered secondary without changing the game state.
    ///
    /// A game driver needs this to expose legal options without recording tokenless followers
    /// merely because a client looked at a choice. [`Self::next_choice`] remains the mutating
    /// resolver used when a step actually advances the window.
    #[must_use]
    pub fn pending_choice(&self, state: &GameState, content: &ContentStore) -> Option<Choice> {
        self.followers[self.next_follower..]
            .iter()
            .find(|player_id| secondary_eligible(state, content, player_id, &self.card))
            .map(|player_id| {
                let costs_token = secondary_costs_token(content, &self.card)
                    && !secondary_is_free(state, content, player_id, &self.card);
                secondary_choice(content, &self.card, player_id, costs_token)
            })
    }

    /// Return the next eligible follower's choice, recording tokenless followers as skipped.
    ///
    /// A content-specific secondary may later impose further eligibility checks. This generic
    /// structural window has only the shared strategy-token gate.
    pub fn next_choice(&mut self, state: &mut GameState, content: &ContentStore) -> Option<Choice> {
        while let Some(player_id) = self.followers.get(self.next_follower).cloned() {
            if secondary_eligible(state, content, &player_id, &self.card) {
                let costs_token = secondary_costs_token(content, &self.card)
                    && !secondary_is_free(state, content, &player_id, &self.card);
                return Some(secondary_choice(
                    content,
                    &self.card,
                    &player_id,
                    costs_token,
                ));
            }
            self.resolutions
                .push((player_id, SecondaryResolution::Ineligible));
            self.next_follower += 1;
        }
        self.exhaust_primary(state);
        None
    }

    /// Resolve the current follower's answer and return its structural result.
    ///
    /// # Errors
    /// [`StrategySecondaryError::IllegalChoice`] if the answer was not offered, or
    /// [`StrategySecondaryError::Complete`] once all followers are resolved.
    pub fn take_choice(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        answer: ChoiceOption,
    ) -> Result<SecondaryResolution, StrategySecondaryError> {
        let choice = self
            .next_choice(state, content)
            .ok_or(StrategySecondaryError::Complete)?;
        let answer = validate(&choice, answer)?;
        let resolution = if answer.is_decline() || answer.id == "no" {
            SecondaryResolution::Declined
        } else {
            let costs_token = secondary_costs_token(content, &self.card)
                && !secondary_is_free(state, content, &choice.player, &self.card);
            if costs_token {
                let player = state.player_mut(&choice.player).ok_or_else(|| {
                    StrategySecondaryError::FollowerMissing(choice.player.clone())
                })?;
                if !player.spend_token(TokenPool::Strategic) {
                    return Err(StrategySecondaryError::NoStrategyToken(choice.player));
                }
            }
            SecondaryResolution::Followed
        };
        self.resolutions.push((choice.player, resolution));
        self.next_follower += 1;
        if self.is_complete() {
            self.exhaust_primary(state);
        }
        Ok(resolution)
    }

    fn exhaust_primary(&self, state: &mut GameState) {
        if self.is_complete() {
            let exhausted = state.exhaust_strategy_card(&self.primary_player, self.card.clone());
            debug_assert!(
                exhausted,
                "the primary holder must retain the selected card"
            );
        }
    }
}

fn secondary_costs_token(content: &ContentStore, card: &StrategyCardId) -> bool {
    crate::strategy_cards::card_name(content, card.as_str()).as_deref() != Some("Leadership")
}

fn secondary_is_free(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &StrategyCardId,
) -> bool {
    crate::strategy_cards::card_name(content, card.as_str()).is_some_and(|name| {
        crate::faction_abilities::secondary_is_free(state, content, player, &name)
    })
}

fn secondary_eligible(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &StrategyCardId,
) -> bool {
    state.player(player).is_some_and(|seat| {
        !secondary_costs_token(content, card)
            || secondary_is_free(state, content, player, card)
            || seat.strategic_tokens > 0
    })
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
/// Card-specific effects live in [`crate::strategy_cards`]. This operation retains the older
/// structural convenience API; the driven [`crate::game::Game`] uses
/// [`begin_strategic_action`] and invokes those effects.
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
    let card = selected_strategic_card(state, content, player_id, answer)?;

    // This convenience operation is retained for the M04-008 structural-primary API. New
    // action drivers must use `begin_strategic_action` so the primary stays ready while the
    // secondary window is open.
    let exhausted = state.exhaust_strategy_card(player_id, card.clone());
    debug_assert!(exhausted, "the checked player must hold the checked card");
    Ok(card)
}

/// Begin a strategic action and open its ordered generic-secondary window.
///
/// The caller applies the primary before driving the returned follower window. The selected card
/// is exhausted only when every follower, including tokenless skipped followers, is recorded.
///
/// # Errors
/// [`StrategyActionError::IllegalChoice`] if `answer` was not offered, or
/// [`StrategyActionError::NoUnusedStrategyCard`] if the player has no available card.
pub fn begin_strategic_action(
    state: &mut GameState,
    content: &ContentStore,
    player_id: &PlayerId,
    answer: ChoiceOption,
) -> Result<StrategySecondaryWindow, StrategyActionError> {
    let card = selected_strategic_card(state, content, player_id, answer)?;
    Ok(StrategySecondaryWindow {
        primary_player: player_id.clone(),
        card,
        followers: state
            .clockwise_from(player_id)
            .into_iter()
            .skip(1)
            .collect(),
        next_follower: 0,
        resolutions: Vec::new(),
    })
}

fn selected_strategic_card(
    state: &GameState,
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
    fn followers_resolve_clockwise_pay_or_decline_then_exhaust_the_primary_card() {
        let mut state = drafted_three_player_game();
        let primary = PlayerId::new("a");
        let first_follower = PlayerId::new("b");
        let second_follower = PlayerId::new("c");
        let card = state
            .player(&primary)
            .unwrap()
            .strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    != Some("Leadership")
            })
            .expect("a three-player hand includes a token-costing card")
            .clone();
        let first_tokens = state.player(&first_follower).unwrap().strategic_tokens;

        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();

        assert!(
            !state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card),
            "the card remains ready while followers decide"
        );
        let choice = window
            .next_choice(&mut state, ContentStore::embedded())
            .unwrap();
        assert_eq!(choice.player, first_follower);
        assert_eq!(choice.ids(), vec!["no", "yes"]);
        assert_eq!(
            window
                .take_choice(
                    &mut state,
                    ContentStore::embedded(),
                    ChoiceOption::new("yes", "strategy"),
                )
                .unwrap(),
            SecondaryResolution::Followed
        );
        assert_eq!(
            state.player(&first_follower).unwrap().strategic_tokens,
            first_tokens - 1
        );

        let choice = window
            .next_choice(&mut state, ContentStore::embedded())
            .unwrap();
        assert_eq!(choice.player, second_follower);
        assert_eq!(
            window
                .take_choice(
                    &mut state,
                    ContentStore::embedded(),
                    ChoiceOption::new("no", "strategy"),
                )
                .unwrap(),
            SecondaryResolution::Declined
        );

        assert!(window.is_complete());
        assert!(
            state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card),
            "the primary card exhausts after every follower has completed"
        );
    }

    #[test]
    fn tokenless_followers_are_recorded_ineligible_and_close_the_window() {
        let mut state = drafted_three_player_game();
        let primary = PlayerId::new("a");
        let card = state
            .player(&primary)
            .unwrap()
            .strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    != Some("Leadership")
            })
            .expect("a three-player hand includes a token-costing card")
            .clone();
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .strategic_tokens = 0;
        state
            .player_mut(&PlayerId::new("c"))
            .unwrap()
            .strategic_tokens = 0;
        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();

        assert!(
            window
                .next_choice(&mut state, ContentStore::embedded())
                .is_none()
        );
        assert_eq!(
            window.resolutions(),
            &[
                (PlayerId::new("b"), SecondaryResolution::Ineligible),
                (PlayerId::new("c"), SecondaryResolution::Ineligible),
            ]
        );
        assert!(window.is_complete());
        assert!(
            state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card)
        );
    }

    #[test]
    fn an_invented_secondary_response_is_atomic() {
        let mut state = drafted_three_player_game();
        let primary = PlayerId::new("a");
        let card = state.player(&primary).unwrap().strategy_cards[0].clone();
        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();
        let before = state.clone();

        let error = window
            .take_choice(
                &mut state,
                ContentStore::embedded(),
                ChoiceOption::new("invented", STRATEGY_KIND),
            )
            .unwrap_err();

        assert!(matches!(error, StrategySecondaryError::IllegalChoice(_)));
        assert!(state.identical(&before));
        assert!(window.resolutions().is_empty());
        assert!(!window.is_complete());
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
