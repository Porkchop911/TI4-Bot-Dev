//! The event journal of a game: how a resolved decision or a die roll becomes a [`GameEvent`],
//! and how a replay of a redo stack reattaches the events it recorded before.
//!
//! Pure functions over the data of a session (its logs and counters). Who stores the events,
//! and to whom they are sent, is the host's: the server keeps them in a file and sends them to
//! sockets, the browser host keeps them in memory and posts them to the page.

use ti4_engine::choice::DecisionRecord;
use ti4_model::state::Phase;

use crate::clock::Clock;
use crate::protocol::server::{
    DecisionFacts, EventVisibility, GameEvent, GameEventKind, PublicRoll, action_id_for,
    decision_grouping,
};
use crate::protocol::state_fact::Observed;

/// What the caller says about an event it records; the journal fills in the rest.
#[derive(Debug, Clone)]
pub struct EventDraft {
    pub visibility: EventVisibility,
    pub event: GameEventKind,
    pub version: Option<u64>,
    /// Decisions resolved when the event happened.
    pub decision_count: usize,
    /// What the event says about the decision it resolves; empty for every other event.
    pub facts: DecisionFacts,
}

/// What to do with an event that a replay of a redo stack produces again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciled {
    /// Record it as a new event.
    Fresh,
    /// It is already the last event of the log: record nothing.
    Duplicate,
    /// It was recorded before the redo: move the event at this index of the redo events back to
    /// the log.
    Reattach(usize),
}

/// A replay may traverse an automatic phase step before it reaches the next human choice. When
/// `history_active` and a redo stack exists, the original event of such a step is reattached
/// instead of duplicating its ID.
#[must_use]
pub fn reconcile_with_redo(
    history_active: bool,
    redo_nonempty: bool,
    event_log: &[GameEvent],
    redo_events: &[GameEvent],
    event: &GameEventKind,
    decision_count: usize,
) -> Reconciled {
    if !(history_active
        && redo_nonempty
        && matches!(
            event,
            GameEventKind::PhaseTransition { .. } | GameEventKind::GameFinished { .. }
        ))
    {
        return Reconciled::Fresh;
    }
    if event_log.last().is_some_and(|previous| {
        &previous.event == event && previous.decision_count == Some(decision_count)
    }) {
        return Reconciled::Duplicate;
    }
    let before_next_decision = redo_events
        .iter()
        .take_while(|entry| !matches!(entry.event, GameEventKind::DecisionResolved))
        .count();
    redo_events[..before_next_decision]
        .iter()
        .position(|entry| {
            &entry.event == event
                && entry.decision_count.unwrap_or(decision_count) == decision_count
        })
        .map_or(Reconciled::Fresh, Reconciled::Reattach)
}

/// Builds the event for `draft`. `counter` is the event counter after it was incremented for
/// this event; `decision_log` is the log as it stands.
#[must_use]
pub fn build_event(
    clock: &dyn Clock,
    game_id: &str,
    counter: u64,
    decision_log: &[DecisionRecord],
    draft: EventDraft,
) -> GameEvent {
    let EventDraft {
        visibility,
        event,
        version,
        decision_count,
        facts:
            DecisionFacts {
                detail,
                movement,
                seat_detail,
                fact,
            },
    } = draft;
    let action_id = (matches!(event, GameEventKind::DecisionResolved)
        && decision_log
            .get(decision_count.saturating_sub(1))
            .is_some_and(|record| {
                record.prompt == "action phase"
                    || record
                        .context
                        .as_ref()
                        .is_some_and(|context| context.phase == Phase::Action)
            }))
    .then(|| action_id_for(decision_log, decision_count))
    .flatten();
    let action_start_cursor = action_start(action_id.as_deref());
    let grouping = matches!(event, GameEventKind::DecisionResolved)
        .then(|| {
            decision_count
                .checked_sub(1)
                .and_then(|i| decision_log.get(i))
        })
        .flatten()
        .map(|record| decision_grouping(record, None, decision_log, decision_count));
    GameEvent {
        id: format!("{game_id}-{counter}"),
        timestamp: clock.utc_time_string(),
        version,
        visibility,
        event,
        decision_count: Some(decision_count),
        batch_id: None,
        batch_start_cursor: None,
        batch_end_cursor: None,
        action_id,
        action_start_cursor,
        actor: grouping.as_ref().and_then(|g| g.0.clone()),
        round: grouping.as_ref().and_then(|g| g.1),
        phase: grouping.as_ref().and_then(|g| g.2),
        action_type: grouping.as_ref().and_then(|g| g.3.clone()),
        action_actor: grouping.as_ref().and_then(|g| g.4.clone()),
        stage: grouping.as_ref().and_then(|g| g.5.clone()),
        detail,
        movement,
        fact,
        seat_detail,
        private_detail: None,
        roll: None,
    }
}

/// The cursor of the selection that `action_id` names.
fn action_start(action_id: Option<&str>) -> Option<usize> {
    action_id?
        .strip_prefix("action_")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

/// Where a roll of the action phase belongs: the action under way after `cursor` decisions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RollAction {
    pub action_id: Option<String>,
    pub action_start_cursor: Option<usize>,
    pub action_type: Option<String>,
    pub action_actor: Option<ti4_model::id::PlayerId>,
}

/// The [`RollAction`] of a roll after `cursor` decisions of `decision_log`. Empty outside the
/// action phase.
#[must_use]
pub fn roll_action(decision_log: &[DecisionRecord], cursor: usize, phase: Phase) -> RollAction {
    if phase != Phase::Action {
        return RollAction::default();
    }
    let action_id = action_id_for(decision_log, cursor);
    let grouping = cursor
        .checked_sub(1)
        .and_then(|i| decision_log.get(i))
        .map(|record| decision_grouping(record, None, decision_log, cursor));
    RollAction {
        action_start_cursor: action_start(action_id.as_deref()),
        action_id,
        action_type: grouping.as_ref().and_then(|g| g.3.clone()),
        action_actor: grouping.and_then(|g| g.4),
    }
}

/// Whether an event of the log already shows `roll` (a replay after a restart, a batch that
/// recorded its own).
#[must_use]
pub fn roll_is_known(event_log: &[GameEvent], roll: &PublicRoll) -> bool {
    event_log.iter().any(|event| {
        event
            .roll
            .as_ref()
            .is_some_and(|known| known.index == roll.index)
    })
}

/// The cursor that the history file validates for a roll event: decisions resolved so far.
#[must_use]
pub fn roll_cursor(event_log: &[GameEvent]) -> usize {
    event_log
        .iter()
        .filter(|event| matches!(event.event, GameEventKind::DecisionResolved))
        .count()
}

/// The public event of one die roll.
#[must_use]
pub fn build_roll_event(
    clock: &dyn Clock,
    game_id: &str,
    counter: u64,
    decision_log: &[DecisionRecord],
    cursor: usize,
    round: u32,
    phase: Phase,
    version: Option<u64>,
    roll: PublicRoll,
) -> GameEvent {
    let action = roll_action(decision_log, cursor, phase);
    GameEvent {
        id: format!("{game_id}-{counter}"),
        timestamp: clock.utc_time_string(),
        version,
        visibility: EventVisibility::Public,
        event: GameEventKind::DiceRolled,
        decision_count: Some(cursor),
        batch_id: None,
        batch_start_cursor: None,
        batch_end_cursor: None,
        action_id: action.action_id,
        action_start_cursor: action.action_start_cursor,
        actor: roll.roller.clone(),
        round: Some(round),
        phase: Some(phase),
        action_type: action.action_type,
        action_actor: action.action_actor,
        stage: None,
        detail: Some(roll.describe()),
        movement: None,
        fact: None,
        seat_detail: None,
        private_detail: None,
        roll: Some(roll),
    }
}

/// The public event of one fact of the state ([`crate::protocol::state_fact`]), grouped as a
/// roll after `cursor` decisions is.
#[must_use]
pub fn build_state_event(
    clock: &dyn Clock,
    game_id: &str,
    counter: u64,
    decision_log: &[DecisionRecord],
    cursor: usize,
    round: u32,
    phase: Phase,
    version: Option<u64>,
    observed: Observed,
) -> GameEvent {
    let action = roll_action(decision_log, cursor, phase);
    GameEvent {
        id: format!("{game_id}-{counter}"),
        timestamp: clock.utc_time_string(),
        version,
        visibility: EventVisibility::Public,
        event: GameEventKind::StateChanged,
        decision_count: Some(cursor),
        batch_id: None,
        batch_start_cursor: None,
        batch_end_cursor: None,
        action_id: action.action_id,
        action_start_cursor: action.action_start_cursor,
        actor: observed.actor,
        round: Some(round),
        phase: Some(phase),
        action_type: action.action_type,
        action_actor: action.action_actor,
        stage: None,
        detail: crate::protocol::state_fact::describe(&observed.fact),
        movement: None,
        fact: Some(observed.fact),
        seat_detail: None,
        private_detail: None,
        roll: None,
    }
}

/// What a live decision at log position `index` did to the redo stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoOutcome {
    /// There is no redo stack.
    Untouched,
    /// The decision is the recorded one coming back; it was consumed with its events.
    Consumed,
    /// A different decision: the redo stack and its events were dropped. The caller drops what
    /// else was recorded past `index` (batches, forced random positions).
    Forked,
}

/// A live decision was just accepted while a redo stack may exist.
///
/// Redo replays the recorded timeline. A decision equal to the head of the redo list (a random
/// bot re-deciding the same position: bots are a pure function of seed, seat and index) is that
/// recorded entry coming back, so it is consumed together with its recorded events and the rest
/// of the redo list stays. Any other decision forks history.
pub fn consume_or_fork_redo(
    redo_decisions: &mut Vec<DecisionRecord>,
    redo_events: &mut Vec<GameEvent>,
    record: &DecisionRecord,
) -> RedoOutcome {
    if redo_decisions.is_empty() {
        return RedoOutcome::Untouched;
    }
    if redo_decisions.first() == Some(record) {
        redo_decisions.remove(0);
        if let Some(resolved) = redo_events
            .iter()
            .position(|entry| matches!(entry.event, GameEventKind::DecisionResolved))
        {
            redo_events.drain(..=resolved);
        }
        if redo_decisions.is_empty() {
            redo_events.clear();
        }
        return RedoOutcome::Consumed;
    }
    // Only an accepted, different decision forks history; autonomous engine steps between
    // decisions leave redo available.
    redo_decisions.clear();
    redo_events.clear();
    RedoOutcome::Forked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;

    fn draft(event: GameEventKind, count: usize) -> EventDraft {
        EventDraft {
            visibility: EventVisibility::Public,
            event,
            version: Some(3),
            decision_count: count,
            facts: DecisionFacts::default(),
        }
    }

    #[test]
    fn an_event_carries_the_injected_time_and_a_counted_id() {
        let clock = ManualClock::new(0, 3 * 3600 + 4 * 60 + 5);
        let event = build_event(
            &clock,
            "g1",
            7,
            &[],
            draft(GameEventKind::DecisionResolved, 0),
        );
        assert_eq!(event.id, "g1-7");
        assert_eq!(event.timestamp, "03:04:05");
        assert_eq!(event.decision_count, Some(0));
    }

    #[test]
    fn without_a_redo_stack_every_event_is_fresh() {
        let kind = GameEventKind::GameFinished { winner: None };
        assert_eq!(
            reconcile_with_redo(true, false, &[], &[], &kind, 2),
            Reconciled::Fresh
        );
        assert_eq!(
            reconcile_with_redo(false, true, &[], &[], &kind, 2),
            Reconciled::Fresh
        );
    }

    #[test]
    fn a_replayed_event_is_reattached_and_a_repeat_is_dropped() {
        let clock = ManualClock::new(0, 0);
        let kind = GameEventKind::GameFinished { winner: None };
        let old = build_event(&clock, "g", 1, &[], draft(kind.clone(), 2));
        assert_eq!(
            reconcile_with_redo(true, true, &[], std::slice::from_ref(&old), &kind, 2),
            Reconciled::Reattach(0)
        );
        assert_eq!(
            reconcile_with_redo(true, true, std::slice::from_ref(&old), &[], &kind, 2),
            Reconciled::Duplicate
        );
    }

    #[test]
    fn a_roll_of_the_action_phase_belongs_to_the_action_under_way() {
        use ti4_model::id::PlayerId;
        let selection = DecisionRecord {
            player: PlayerId::new("p1"),
            prompt: "action phase".into(),
            chosen: "tactical".into(),
            offered: vec!["tactical".into()],
            context: None,
        };
        let activation = DecisionRecord {
            prompt: "activate".into(),
            chosen: "22".into(),
            offered: vec!["22".into()],
            context: Some(ti4_engine::decision_context::DecisionContext::new(
                PlayerId::new("p1"),
                ti4_engine::decision_context::DecisionSource::Rule("test".into()),
                "activate_system",
                Phase::Action,
                1,
            )),
            ..selection.clone()
        };
        let log = [activation.clone(), selection, activation];
        let roll = crate::protocol::server::public_roll(
            0,
            &ti4_engine::dice::Roll {
                reason: "space combat".into(),
                faces: vec![7],
                hits_on: Some(7),
                rerolled: std::collections::BTreeSet::new(),
                by: Some("p2".into()),
                system: None,
                outcome: None,
                subject: None,
            },
            Some("22"),
        )
        .unwrap();
        let clock = ManualClock::new(0, 0);
        let event = build_roll_event(
            &clock,
            "g",
            1,
            &log,
            3,
            1,
            Phase::Action,
            None,
            roll.clone(),
        );
        assert_eq!(event.action_id.as_deref(), Some("action_2"));
        assert_eq!(event.action_start_cursor, Some(1));
        assert_eq!(event.action_type.as_deref(), Some("tactical"));
        assert_eq!(event.action_actor, Some(PlayerId::new("p1")));
        assert_eq!(event.actor, Some(PlayerId::new("p2")));
        let status = build_roll_event(&clock, "g", 2, &log, 3, 1, Phase::Status, None, roll);
        assert_eq!(status.action_id, None);
        assert_eq!(status.action_type, None);
    }

    #[test]
    fn the_roll_cursor_counts_resolved_decisions() {
        let clock = ManualClock::new(0, 0);
        let events = vec![
            build_event(
                &clock,
                "g",
                1,
                &[],
                draft(GameEventKind::DecisionResolved, 1),
            ),
            build_event(
                &clock,
                "g",
                2,
                &[],
                draft(GameEventKind::DecisionResolved, 2),
            ),
        ];
        assert_eq!(roll_cursor(&events), 2);
    }
}
