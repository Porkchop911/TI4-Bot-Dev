//! Gaining command tokens into a pool of the player's choosing (LRR 52.4).
//!
//! Ported from the oracle's `Game.gain_tokens`. The oracle is explicit that this is one rule
//! with two callers — the status phase's gain step (81.5) and Leadership — and that having the
//! status phase quietly drop every token into the tactic pool "was an inconsistency, not a
//! simplification". So the window lives here rather than inside either caller.

use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};

/// The choice kind for placing a gained command token.
pub const POOL_KIND: &str = "pool";

/// Tokens each player gains at status step 81.5, before any modifier.
///
/// Sol's Versatile, Cybernetic Enhancements and the L1Z1X promissory note all change this in
/// the oracle. None of those are implemented, so the base is used unmodified; when they land
/// they modify the count handed to [`TokenGain::for_status`], not this constant.
pub const STATUS_TOKENS: u32 = 2;

/// A pool could not be selected from the state that was presented.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenGainError {
    #[error("the token-gain window is complete")]
    Complete,
    #[error("player {0} is no longer seated")]
    PlayerMissing(PlayerId),
    #[error("option id {0:?} does not name a command-token pool")]
    UnknownPool(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// One placed token, for the caller's report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPlacement {
    pub player: PlayerId,
    pub pool: TokenPool,
}

/// An ordered window granting tokens one at a time, each into a chosen pool.
///
/// One choice per token rather than one per player: LRR 52.4 places tokens individually, and a
/// player gaining two may legitimately split them across pools. Asking once for a count would
/// make that unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGain {
    /// Remaining grants, one entry per token still to be placed, in resolution order.
    pending: Vec<PlayerId>,
    placed: Vec<TokenPlacement>,
}

/// The pools a gained token may enter, in the oracle's option order.
const POOLS: [(&str, TokenPool, &str); 3] = [
    ("tactic_tokens", TokenPool::Tactic, "tactic pool"),
    ("fleet_tokens", TokenPool::Fleet, "fleet pool"),
    ("strategic_tokens", TokenPool::Strategic, "strategy pool"),
];

fn pool_for(option_id: &str) -> Option<TokenPool> {
    POOLS
        .iter()
        .find(|(id, _, _)| *id == option_id)
        .map(|(_, pool, _)| *pool)
}

impl TokenGain {
    /// A window granting `count` tokens to each player, in the given order.
    ///
    /// The order is the caller's: status step 81.5 grants in initiative order, which the oracle
    /// takes care to capture before strategy cards return at 81.8.
    #[must_use]
    pub fn new(players: &[PlayerId], count: u32) -> Self {
        let mut pending = Vec::new();
        for player in players {
            for _ in 0..count {
                pending.push(player.clone());
            }
        }
        // Reversed once so that `pop` takes from the front without shifting the vector.
        pending.reverse();
        Self {
            pending,
            placed: Vec::new(),
        }
    }

    /// The status phase's grant: [`STATUS_TOKENS`] to each player in initiative order.
    #[must_use]
    pub fn for_status(players: &[PlayerId]) -> Self {
        Self::new(players, STATUS_TOKENS)
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// The player whose token is next to place.
    #[must_use]
    pub fn next_player(&self) -> Option<&PlayerId> {
        self.pending.last()
    }

    /// Every token placed so far, in resolution order.
    #[must_use]
    pub fn placed(&self) -> &[TokenPlacement] {
        &self.placed
    }

    /// The choice for the next token, or `None` once the window is complete.
    #[must_use]
    pub fn pending_choice(&self) -> Option<Choice> {
        let player = self.next_player()?;
        Some(Choice::new(
            player.clone(),
            "gain a command token into which pool",
            POOLS
                .iter()
                .map(|(id, _, label)| ChoiceOption::labelled(*id, POOL_KIND, *label))
                .collect(),
        ))
    }

    /// Place the next token into the chosen pool.
    ///
    /// Validates against the generated choice before mutating, so a decider that answers with
    /// anything not offered cannot move a token.
    ///
    /// # Errors
    /// [`TokenGainError::Complete`] when no token remains, [`TokenGainError::IllegalChoice`]
    /// when the answer was not offered, [`TokenGainError::UnknownPool`] when it does not name a
    /// pool, and [`TokenGainError::PlayerMissing`] when the player has left the table.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        answer: ChoiceOption,
    ) -> Result<TokenPool, TokenGainError> {
        let choice = self.pending_choice().ok_or(TokenGainError::Complete)?;
        let option = validate(&choice, answer)?;
        let pool = pool_for(&option.id).ok_or_else(|| TokenGainError::UnknownPool(option.id))?;

        let player_id = choice.player;
        let player = state
            .player_mut(&player_id)
            .ok_or_else(|| TokenGainError::PlayerMissing(player_id.clone()))?;
        player.gain_token(pool, 1);

        self.pending.pop();
        self.placed.push(TokenPlacement {
            player: player_id,
            pool,
        });
        Ok(pool)
    }
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;

    use super::*;
    use crate::setup::start_game;

    fn game() -> (GameState, [PlayerId; 2]) {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        (state, players)
    }

    fn pick(window: &TokenGain, id: &str) -> ChoiceOption {
        window.pending_choice().unwrap().option(id).unwrap().clone()
    }

    #[test]
    fn each_token_is_offered_separately_in_the_given_order() {
        let (_, players) = game();
        let window = TokenGain::for_status(&players);

        // Two tokens each, player a first: a, a, b, b.
        assert_eq!(window.next_player(), Some(&PlayerId::new("a")));
        assert_eq!(window.pending.len(), 4);
    }

    #[test]
    fn a_player_may_split_their_tokens_across_pools() {
        // The reason there is one choice per token rather than one per player.
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 2);
        let before = state.player(&players[0]).unwrap().clone();

        window
            .resolve(&mut state, pick(&window, "tactic_tokens"))
            .unwrap();
        window
            .resolve(&mut state, pick(&window, "fleet_tokens"))
            .unwrap();

        let after = state.player(&players[0]).unwrap();
        assert_eq!(after.tactic_tokens, before.tactic_tokens + 1);
        assert_eq!(after.fleet_tokens, before.fleet_tokens + 1);
        assert_eq!(after.strategic_tokens, before.strategic_tokens);
        assert!(window.is_complete());
    }

    #[test]
    fn every_pool_can_be_chosen() {
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 3);
        let before = state.player(&players[0]).unwrap().clone();

        for id in ["tactic_tokens", "fleet_tokens", "strategic_tokens"] {
            window.resolve(&mut state, pick(&window, id)).unwrap();
        }

        let after = state.player(&players[0]).unwrap();
        assert_eq!(after.tactic_tokens, before.tactic_tokens + 1);
        assert_eq!(after.fleet_tokens, before.fleet_tokens + 1);
        assert_eq!(after.strategic_tokens, before.strategic_tokens + 1);
    }

    #[test]
    fn the_window_grants_in_the_order_it_was_given() {
        let (mut state, players) = game();
        let mut window = TokenGain::for_status(&players);

        let mut order = Vec::new();
        while !window.is_complete() {
            order.push(window.next_player().unwrap().clone());
            window
                .resolve(&mut state, pick(&window, "tactic_tokens"))
                .unwrap();
        }

        assert_eq!(
            order,
            vec![
                PlayerId::new("a"),
                PlayerId::new("a"),
                PlayerId::new("b"),
                PlayerId::new("b"),
            ]
        );
    }

    #[test]
    fn an_answer_that_was_not_offered_moves_no_token() {
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 1);
        let before = state.clone();

        let error = window
            .resolve(&mut state, ChoiceOption::new("trade_goods", POOL_KIND))
            .unwrap_err();

        assert!(matches!(error, TokenGainError::IllegalChoice(_)));
        assert!(state.identical(&before), "state must not have moved");
        assert!(!window.is_complete(), "the token is still owed");
    }

    #[test]
    fn resolving_a_complete_window_is_refused() {
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 1);
        let option = pick(&window, "tactic_tokens");
        window.resolve(&mut state, option.clone()).unwrap();

        assert_eq!(
            window.resolve(&mut state, option),
            Err(TokenGainError::Complete)
        );
    }

    #[test]
    fn a_complete_window_offers_no_choice() {
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 1);
        window
            .resolve(&mut state, pick(&window, "fleet_tokens"))
            .unwrap();

        assert!(window.pending_choice().is_none());
        assert!(window.next_player().is_none());
    }

    #[test]
    fn placements_are_recorded_for_the_caller() {
        let (mut state, players) = game();
        let mut window = TokenGain::new(&players[..1], 2);
        window
            .resolve(&mut state, pick(&window, "strategic_tokens"))
            .unwrap();
        window
            .resolve(&mut state, pick(&window, "tactic_tokens"))
            .unwrap();

        assert_eq!(
            window.placed(),
            &[
                TokenPlacement {
                    player: PlayerId::new("a"),
                    pool: TokenPool::Strategic,
                },
                TokenPlacement {
                    player: PlayerId::new("a"),
                    pool: TokenPool::Tactic,
                },
            ]
        );
    }

    #[test]
    fn a_zero_token_grant_is_already_complete() {
        // Nothing in the base game grants zero, but an ability that reduces the count must
        // not leave an open window nobody can answer.
        let (_, players) = game();
        let window = TokenGain::new(&players, 0);
        assert!(window.is_complete());
        assert!(window.pending_choice().is_none());
    }
}
