//! The strategic action that is resolved now: who played which card, and what each follower did
//! of its secondary. A follower may draft its secondary from the moment the card is played, so
//! a client is told of the card before the window of the followers is open.
//!
//! The state of a game does not say this: the engine announces the card inside the step that
//! resolves its primary ([`Game::on_strategic_action_chosen`]), and the window of the followers
//! is of the driver ([`Game::open_secondary`]). A host keeps one [`SecondaryWatch`] for its
//! game, and the facts of an update are made from it ([`crate::projection::fill_facts`]).
//!
//! Nothing here decides a rule. Who plays a strategy card and who follows it is public, so the
//! facts differ by viewer only in `draft`.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::game::Game;
use ti4_engine::strategy::SecondaryResolution;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::planning::audit::PlanScope;
use crate::protocol::status::ViewerRole;

/// What a follower did of the secondary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum FollowerState {
    /// The seat is still to be asked, or is asked now.
    Waiting,
    Followed,
    Passed,
    /// The seat had no command token in its strategy pool and was not asked.
    Ineligible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FollowerFact {
    pub seat: PlayerId,
    pub state: FollowerState,
}

/// The strategic action that is resolved now: who played which strategy card, and what each
/// follower did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SecondaryFacts {
    pub primary: PlayerId,
    pub card: StrategyCardId,
    /// The printed name of the card.
    pub name: String,
    /// The primary is resolved: the followers are asked, in their order.
    pub open: bool,
    /// The seats that may follow, in the order in which they are asked.
    pub followers: Vec<FollowerFact>,
    /// The viewer may draft its secondary now ([`PlanScope::Secondary`]).
    pub draft: bool,
}

/// The window of the followers, as the driver has it at a completed step.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Window {
    /// What the followers that are recorded did, in the order in which they were asked.
    resolved: Vec<(PlayerId, SecondaryResolution)>,
    /// The seats that are still to be asked, in their order.
    unresolved: Vec<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Round {
    primary: PlayerId,
    card: StrategyCardId,
    /// `None` while the primary is resolved.
    window: Option<Window>,
}

/// The strategic action of one game, as its host follows it. A clone watches the same game.
#[derive(Debug, Clone, Default)]
pub struct SecondaryWatch {
    round: Arc<Mutex<Option<Round>>>,
}

impl SecondaryWatch {
    /// Follow the cards that `game` announces. The binding is not forked: a copy of the game
    /// that goes on as the game (an undo) is bound again.
    pub fn bind(&self, game: &mut Game<'_>) {
        let watch = self.clone();
        game.on_strategic_action_chosen(move |primary, card| watch.announce(primary, card));
    }

    /// `card` is certain to resolve: its primary is resolved now. For a host that has a hook
    /// of its own on the game.
    pub fn announce(&self, primary: &PlayerId, card: &StrategyCardId) {
        *self.round.lock().expect("secondary lock") = Some(Round {
            primary: primary.clone(),
            card: card.clone(),
            window: None,
        });
    }

    /// A step of `game` is complete: follow the window of the followers, and forget the action
    /// with its window. Also brings the watch to a game that it did not follow (an undo).
    pub fn completed_step(&self, game: &Game<'_>) {
        *self.round.lock().expect("secondary lock") = (!game.state.finished)
            .then(|| game.open_secondary())
            .flatten()
            .map(|(primary, card, unresolved)| Round {
                primary: primary.clone(),
                card: card.clone(),
                window: Some(Window {
                    resolved: game.secondary_resolutions().to_vec(),
                    unresolved: unresolved.to_vec(),
                }),
            });
    }

    /// The facts of the action for `viewer`, on `state` of the same game.
    #[must_use]
    pub fn facts(
        &self,
        state: &GameState,
        content: &ContentStore,
        viewer: &ViewerRole,
    ) -> Option<SecondaryFacts> {
        let round = self.round.lock().expect("secondary lock").clone()?;
        let waits = |seat: PlayerId| FollowerFact {
            seat,
            state: FollowerState::Waiting,
        };
        let followers: Vec<FollowerFact> = match &round.window {
            // The seats that the window will ask (LRR 82.1): clockwise from the primary.
            None => state
                .clockwise_from(&round.primary)
                .into_iter()
                .skip(1)
                .map(waits)
                .collect(),
            Some(window) => window
                .resolved
                .iter()
                .map(|(seat, resolution)| FollowerFact {
                    seat: seat.clone(),
                    state: match resolution {
                        SecondaryResolution::Followed => FollowerState::Followed,
                        SecondaryResolution::Declined => FollowerState::Passed,
                        SecondaryResolution::Ineligible => FollowerState::Ineligible,
                    },
                })
                .chain(window.unresolved.iter().cloned().map(waits))
                .collect(),
        };
        let name = ti4_engine::strategy_cards::card_name(content, round.card.as_str())
            .unwrap_or_else(|| round.card.to_string());
        let draft =
            matches!(viewer, ViewerRole::Player(seat) if drafts(&round, &followers, &name, seat));
        Some(SecondaryFacts {
            primary: round.primary,
            card: round.card,
            open: round.window.is_some(),
            followers,
            name,
            draft,
        })
    }

    /// Whether `player` may draft what `scope` names now. A tactical action is not of the
    /// watch; a secondary must be the one of the action that is resolved, for a seat that is
    /// still to be asked.
    #[must_use]
    pub fn allows(
        &self,
        scope: &PlanScope,
        state: &GameState,
        content: &ContentStore,
        player: &PlayerId,
    ) -> bool {
        let PlanScope::Secondary { primary, card } = scope else {
            return true;
        };
        self.facts(state, content, &ViewerRole::Player(player.clone()))
            .is_some_and(|facts| facts.draft && &facts.primary == primary && &facts.card == card)
    }
}

/// Whether `seat` may draft its secondary of `round`. Trade waits for its window: the primary
/// changes what the secondary of a follower costs and gives.
fn drafts(round: &Round, followers: &[FollowerFact], name: &str, seat: &PlayerId) -> bool {
    (round.window.is_some() || name != "Trade")
        && followers
            .iter()
            .any(|follower| &follower.seat == seat && follower.state == FollowerState::Waiting)
}
