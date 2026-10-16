//! Bluffing a reaction card: declared triggers, anti-griefing budgets and the live hold.
//!
//! Everything here is EPHEMERAL, presentation-level state. It is never written to the decision
//! log, the event log, `history.json`, snapshots, replay exports or `SessionConfig`; a restart of
//! the session (server restart, undo, redo, batch commit) simply forgets it and the client sends
//! its declaration again when it reconnects. The cooldown marker and the stall budgets reset with
//! it; that is accepted (a restart is operator- or vote-driven, not a lever a single player has).
//!
//! The hold is a wall-clock wait of the live engine thread at the moment a reaction window of a
//! declared kind opens for a seat that is not asked anything in it (see
//! `ti4_engine::choice::Table::window_skipped`). It never touches the engine's state, decisions or
//! random streams: its length comes from the operating system's entropy, not the game RNG.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::time::Duration;

use ti4_engine::timing::Relation;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::server::{ReactionIntentStateMsg, ServerMessage};
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;

use crate::session::SeatController;
use crate::session::worker::SessionShared;

/// Most triggers one seat may have declared at once.
pub const MAX_DECLARED_TRIGGERS: usize = 3;
/// Shortest and longest random stall.
pub const MIN_HOLD: Duration = Duration::from_secs(2);
pub const MAX_HOLD: Duration = Duration::from_secs(4);
/// No single stall ever exceeds this, whatever the other settings say.
pub const HARD_CAP: Duration = Duration::from_secs(8);
/// Total stall one seat may cause per round.
pub const SEAT_ROUND_BUDGET: Duration = Duration::from_secs(60);
/// Total stall one seat may cause per game.
pub const SEAT_GAME_BUDGET: Duration = Duration::from_secs(300);
/// Total stall all seats together may cause per game.
pub const TABLE_GAME_BUDGET: Duration = Duration::from_secs(600);
/// Most stalls one seat may cause per round.
pub const SEAT_ROUND_HOLDS: u32 = 8;

/// One user-facing kind of reaction window and the engine windows it covers.
pub struct TriggerGroup {
    pub id: &'static str,
    pub windows: &'static [(&'static str, Relation)],
}

use Relation::{After, When};

/// The declarable triggers. Every window in `ti4_engine::reactions::window_table` belongs to
/// exactly one group (checked by a test); the web client carries the same ids with their labels.
pub const TRIGGER_GROUPS: &[TriggerGroup] = &[
    TriggerGroup {
        id: "action_card_played",
        windows: &[
            ("ACTION_CARD_PLAYED", When),
            ("ACTION_CARD_DISCARDED", After),
        ],
    },
    TriggerGroup {
        id: "system_activated",
        windows: &[("SYSTEM_ACTIVATED", After)],
    },
    TriggerGroup {
        id: "movement",
        windows: &[("SHIP_MOVED", After), ("UNITS_COMMITTED", After)],
    },
    TriggerGroup {
        id: "space_combat",
        windows: &[
            ("SPACE_COMBAT_STARTED", After),
            ("COMBAT_ROUND_STARTED", After),
            ("ANTI_FIGHTER_BARRAGE_STARTED", When),
            ("SPACE_CANNON_HITS", When),
            ("HITS_TO_ASSIGN", When),
            ("SUSTAIN_DAMAGE_USED", When),
            ("SUSTAIN_DAMAGE_USED", After),
            ("SHIP_DESTROYED", When),
            ("SHIP_DESTROYED", After),
            ("SPACE_COMBAT_WON", After),
            ("RETREAT_DECLARED", After),
            ("RETREAT_STEP_STARTED", After),
            ("UNIT_ABILITY_ROLLED", After),
        ],
    },
    TriggerGroup {
        id: "ground_combat",
        windows: &[("INVASION_BEGAN", After), ("GROUND_ROLLS_MADE", After)],
    },
    TriggerGroup {
        id: "planet_control",
        windows: &[
            ("PLANET_CONTROL_GAINED", When),
            ("PLANET_CONTROL_GAINED", After),
        ],
    },
    TriggerGroup {
        id: "production",
        windows: &[("PRODUCTION_USED", After)],
    },
    TriggerGroup {
        id: "agenda",
        windows: &[
            ("AGENDA_PHASE_BEGAN", After),
            ("AGENDA_REVEALED", When),
            ("AGENDA_REVEALED", After),
            ("VOTES_CAST", After),
            ("AGENDA_RESOLVED", When),
        ],
    },
    TriggerGroup {
        id: "strategy_card",
        windows: &[
            ("STRATEGY_PHASE_BEGAN", After),
            ("STRATEGY_CARD_CHOSEN", After),
            ("STRATEGIC_ACTION_BEGAN", When),
            ("STRATEGY_CARDS_WOULD_RETURN", When),
        ],
    },
    TriggerGroup {
        id: "turn_end",
        windows: &[
            ("TURN_BEGAN", After),
            ("ACTION_COMPLETED", After),
            ("PLAYER_PASSED", After),
            ("TURN_PASSED", After),
        ],
    },
    TriggerGroup {
        id: "transaction",
        windows: &[
            ("TRANSACTION_OPENED", When),
            ("TRANSACTION_RESOLVED", After),
        ],
    },
];

/// The trigger group an engine window belongs to, if any.
#[must_use]
pub fn group_for_window(event_type: &str, relation: Relation) -> Option<&'static str> {
    TRIGGER_GROUPS
        .iter()
        .find(|group| {
            group
                .windows
                .iter()
                .any(|(event, rel)| *event == event_type && *rel == relation)
        })
        .map(|group| group.id)
}

/// Every limit of the feature. The defaults are the named constants above; tests shorten them.
#[derive(Debug, Clone)]
pub struct BluffPolicy {
    pub max_triggers: usize,
    pub min_hold: Duration,
    pub max_hold: Duration,
    pub hard_cap: Duration,
    pub seat_round_budget: Duration,
    pub seat_game_budget: Duration,
    pub table_game_budget: Duration,
    pub seat_round_holds: u32,
    /// How often wall-clock waits look at the stop flag (the channel wakes them sooner).
    pub poll: Duration,
}

impl Default for BluffPolicy {
    fn default() -> Self {
        Self {
            max_triggers: MAX_DECLARED_TRIGGERS,
            min_hold: MIN_HOLD,
            max_hold: MAX_HOLD,
            hard_cap: HARD_CAP,
            seat_round_budget: SEAT_ROUND_BUDGET,
            seat_game_budget: SEAT_GAME_BUDGET,
            table_game_budget: TABLE_GAME_BUDGET,
            seat_round_holds: SEAT_ROUND_HOLDS,
            poll: Duration::from_millis(50),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct SeatBluff {
    declared: BTreeSet<String>,
    /// Round of the last accepted change of a non-empty declaration (the cooldown marker).
    last_change_round: Option<u32>,
    round: u32,
    round_spent: Duration,
    round_holds: u32,
    game_spent: Duration,
}

pub(crate) struct ActiveHold {
    pub(crate) seat: PlayerId,
    pub(crate) wake: mpsc::Sender<()>,
}

/// All bluff state of one live session.
#[derive(Default)]
pub struct BluffBook {
    pub policy: BluffPolicy,
    seats: BTreeMap<PlayerId, SeatBluff>,
    table_spent: Duration,
    pub(crate) active: Option<ActiveHold>,
}

impl BluffBook {
    /// The seat's declared trigger ids (for tests and projection).
    #[must_use]
    pub fn declared(&self, seat: &PlayerId) -> BTreeSet<String> {
        self.seats
            .get(seat)
            .map(|s| s.declared.clone())
            .unwrap_or_default()
    }

    /// Whether a hold is running right now.
    #[must_use]
    pub fn holding(&self) -> Option<&PlayerId> {
        self.active.as_ref().map(|active| &active.seat)
    }

    /// Wake a running hold so it ends now (Pass, stop).
    pub fn wake(&self) {
        if let Some(active) = &self.active {
            let _ = active.wake.send(());
        }
    }

    pub(crate) fn budget_left(&self, seat: &PlayerId, round: u32) -> Duration {
        let policy = &self.policy;
        let default = SeatBluff::default();
        let state = self.seats.get(seat).unwrap_or(&default);
        let round_spent = if state.round == round {
            state.round_spent
        } else {
            Duration::ZERO
        };
        let round_holds = if state.round == round {
            state.round_holds
        } else {
            0
        };
        if round_holds >= policy.seat_round_holds {
            return Duration::ZERO;
        }
        policy
            .seat_round_budget
            .saturating_sub(round_spent)
            .min(policy.seat_game_budget.saturating_sub(state.game_spent))
            .min(policy.table_game_budget.saturating_sub(self.table_spent))
    }

    fn budget_used_up(&self, seat: &PlayerId, round: u32) -> bool {
        self.budget_left(seat, round) < self.policy.min_hold
    }

    pub(crate) fn account(&mut self, seat: &PlayerId, round: u32, spent: Duration) {
        let state = self.seats.entry(seat.clone()).or_default();
        if state.round != round {
            state.round = round;
            state.round_spent = Duration::ZERO;
            state.round_holds = 0;
        }
        state.round_spent += spent;
        state.round_holds += 1;
        state.game_spent += spent;
        self.table_spent += spent;
    }
}

/// Why a seat cannot bluff right now, if it cannot.
fn ineligible_reason(shared: &SessionShared, seat: &PlayerId) -> Option<String> {
    if !matches!(shared.seats.get(seat), Some(SeatController::Human)) {
        return Some("only a human seat can bluff".to_owned());
    }
    let cards = shared
        .latest_state
        .player(seat)
        .map_or(0, |player| player.action_cards.len());
    if cards == 0 {
        return Some("you hold no action cards, and hand size is public".to_owned());
    }
    let never = shared
        .reaction_modes
        .get(seat)
        .is_some_and(|set| !set.lock().expect("never set lock").is_empty());
    if never {
        return Some(
            "you set a card to Never offer to speed the game up, so you cannot also stall it"
                .to_owned(),
        );
    }
    None
}

/// Normalise a declaration: known ids only, no repeats, at most `max` of them.
fn normalise(triggers: &[String], max: usize) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for id in triggers {
        if !TRIGGER_GROUPS.iter().any(|group| group.id == id) {
            return Err(format!("unknown reaction trigger '{id}'"));
        }
        out.insert(id.clone());
    }
    if out.len() > max {
        return Err(format!("you can declare at most {max} triggers"));
    }
    Ok(out)
}

impl SessionShared {
    /// What `seat` is told about its own bluff settings. Private to the seat.
    #[must_use]
    pub fn reaction_intent_state(&self, seat: &PlayerId) -> ReactionIntentStateMsg {
        let round = self.latest_state.round;
        let reason = ineligible_reason(self, seat);
        let seat_state = self.bluff.seats.get(seat);
        let locked_until_round = seat_state
            .and_then(|s| s.last_change_round)
            .filter(|marker| round <= *marker)
            .map(|marker| marker + 1);
        ReactionIntentStateMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: self.game_id.clone(),
            triggers: self.bluff.declared(seat).into_iter().collect(),
            max_triggers: self.bluff.policy.max_triggers,
            eligible: reason.is_none(),
            ineligible_reason: reason,
            locked_until_round,
            budget_used_up: self.bluff.budget_used_up(seat, round),
            holding: self.bluff.holding() == Some(seat),
        }
    }

    /// Tell one seat (all its connections) its own bluff state. Nobody else is sent anything.
    pub fn send_reaction_intent_state(&mut self, seat: &PlayerId) {
        let message = ServerMessage::ReactionIntentState(self.reaction_intent_state(seat));
        self.subscribers.retain(|_, subscriber| {
            if subscriber.viewer == ViewerRole::Player(seat.clone()) {
                subscriber.tx.try_send(message.clone()).is_ok()
            } else {
                true
            }
        });
    }

    /// Record a seat's declaration. Idempotent; the first declaration of a game is free, a later
    /// change waits for a new round; clearing is always allowed.
    ///
    /// # Errors
    /// A client-facing message when the declaration is invalid, the seat may not bluff, or the
    /// change is still locked.
    pub fn set_reaction_intent(
        &mut self,
        seat: &PlayerId,
        triggers: &[String],
    ) -> Result<(), String> {
        let declared = normalise(triggers, self.bluff.policy.max_triggers)?;
        let round = self.latest_state.round;
        let current = self.bluff.declared(seat);
        if declared == current {
            // Reconnect resend or a no-op: nothing changes, nothing counts.
            self.send_reaction_intent_state(seat);
            return Ok(());
        }
        if !declared.is_empty() {
            if let Some(reason) = ineligible_reason(self, seat) {
                return Err(format!("You cannot bluff: {reason}"));
            }
            if let Some(marker) = self.bluff.seats.get(seat).and_then(|s| s.last_change_round)
                && round <= marker
            {
                return Err(format!(
                    "Your bluffed reactions can change again after round {marker} (next change in round {})",
                    marker + 1
                ));
            }
        }
        let entry = self.bluff.seats.entry(seat.clone()).or_default();
        if !declared.is_empty() {
            entry.last_change_round = Some(round);
        }
        entry.declared = declared;
        self.send_reaction_intent_state(seat);
        Ok(())
    }

    /// The bluffer ends its own running hold early. Ignored when it is not the held seat.
    pub fn pass_reaction_hold(&self, seat: &PlayerId) {
        if self.bluff.holding() == Some(seat) {
            self.bluff.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_window_belongs_to_exactly_one_trigger_group() {
        let mut bad = Vec::new();
        for window in ti4_engine::reactions::window_table().values() {
            let groups = TRIGGER_GROUPS
                .iter()
                .filter(|group| {
                    group
                        .windows
                        .iter()
                        .any(|(event, rel)| *event == window.event && *rel == window.relation)
                })
                .count();
            if groups != 1 {
                bad.push(format!("{} {:?} x{groups}", window.event, window.relation));
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
        let ids: BTreeSet<_> = TRIGGER_GROUPS.iter().map(|g| g.id).collect();
        assert_eq!(ids.len(), TRIGGER_GROUPS.len(), "ids are unique");
        assert!(
            TRIGGER_GROUPS
                .iter()
                .all(|g| g.id.len() <= ti4_host::protocol::client::MAX_TRIGGER_ID_BYTES)
        );
    }

    #[test]
    fn budgets_are_per_round_per_game_and_per_table() {
        let seat = PlayerId::new("a");
        let mut book = BluffBook::default();
        book.policy.seat_round_budget = Duration::from_secs(10);
        book.policy.seat_game_budget = Duration::from_secs(14);
        book.policy.table_game_budget = Duration::from_secs(100);
        assert_eq!(book.budget_left(&seat, 1), Duration::from_secs(10));
        book.account(&seat, 1, Duration::from_secs(9));
        // Less than a minimum hold left in the round: used up, and later holds are skipped.
        assert!(book.budget_used_up(&seat, 1));
        // A new round refills the round budget but not the game budget.
        assert_eq!(book.budget_left(&seat, 2), Duration::from_secs(5));
        book.account(&seat, 2, Duration::from_secs(4));
        assert!(book.budget_used_up(&seat, 3), "the game budget is gone");
        // Another seat is untouched by the first seat's spending, except through the table.
        let other = PlayerId::new("b");
        assert_eq!(book.budget_left(&other, 1), Duration::from_secs(10));
        book.policy.table_game_budget = Duration::from_secs(14);
        assert!(book.budget_used_up(&other, 1), "the table budget is shared");
    }

    #[test]
    fn a_seat_cannot_hold_more_often_than_the_round_limit() {
        let seat = PlayerId::new("a");
        let mut book = BluffBook::default();
        book.policy.seat_round_holds = 2;
        book.account(&seat, 1, Duration::from_millis(1));
        assert!(!book.budget_used_up(&seat, 1));
        book.account(&seat, 1, Duration::from_millis(1));
        assert!(book.budget_used_up(&seat, 1));
        assert!(!book.budget_used_up(&seat, 2));
    }
}
