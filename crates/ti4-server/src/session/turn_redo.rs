//! Turn redo (H7): replay a seat's last turn, then auto-play the round from other seats' records.
//!
//! The game seed is fixed. A request rewinds the live history to the start of the seat's turn;
//! the seat plays a new turn live; [`autoplay`] then replays the decisions that followed the old
//! turn, with the strict matcher of [`crate::session::splice`]: the other seats' recorded
//! decisions *and* the redoing seat's own recorded decisions that are not a turn start (reaction
//! windows, secondaries of other seats' strategy cards, agenda votes, picks...). It keeps each
//! only while it still fits and stops
//!
//! * when the redoing seat's next *turn* begins (control returns to it), or
//! * at the first decision that no longer fits (the seat the engine asks decides live), or
//! * when the recorded tail is exhausted.
//!
//! Whatever follows the stop stays in the saved alternate timeline.
//!
//! # Dice
//!
//! The dice and the deck shuffles come from per-domain seeded streams that only advance on a
//! draw. If the new turn drew more or fewer dice than the old one, a plain replay would hand the
//! recorded decisions that follow different faces. So the original timeline is replayed once to
//! read where every stream stood at each decision ([`prepare_tail`]); the auto-play then restores
//! those positions before each kept decision is answered, and stores them on the new history as
//! `rng_marks` so every later replay (recovery, undo, restart) reproduces the same dice.
//!
//! # Cards
//!
//! Decks are shuffled once at setup and drawn from the front, so forcing stream positions does
//! not help them. Instead the card each recorded draw took in the original timeline is read from
//! the same baseline replay (`ti4_model::deck_reserve`). While the new turn is played, draws skip
//! the cards reserved for that tail and take the next ones further down the deck; when the tail
//! is replayed, each recorded draw takes the very card it took originally, from wherever it sits
//! now. The reservations travel with the history in `rng_marks`. A reserved card that is gone is
//! a [`ConflictKind::ReservedCard`] conflict handing the decision to the seat the engine asks.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, Table};
use ti4_engine::game::Game;
use ti4_engine::rng::RngPositions;
use ti4_model::deck_reserve::{DeckSegment, OPEN, ReservedDraw};
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, Phase};

use crate::protocol::server::{GameEvent, GameEventKind};
use crate::protocol::splice::ConflictKind;
use crate::protocol::turn_redo::{TurnRedoConflict, TurnRedoStage, TurnRedoStop};
use crate::session::rng_force::{RngForce, RngMarks};
use crate::session::splice::{RawConflict, StrictDecider};
use crate::storage::BatchRecord;

/// How many of a seat's turns one request may rewind.
pub const MAX_TURNS_BACK: u8 = 2;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRedoError {
    #[error("{0} has no turn in this game to redo")]
    NoTurn(String),
    #[error("{0} has only {1} turn(s) to go back over")]
    NotEnoughTurns(String, usize),
    #[error("you can go back over at most {MAX_TURNS_BACK} of a seat's turns")]
    TooManyTurns,
    #[error(
        "the saved history no longer replays under the strict matcher, so a redo cannot be checked: {0}"
    )]
    BaselineDiverged(String),
    #[error("the new turn has not started or is not finished yet")]
    NewTurnNotComplete,
    #[error("the history does not contain the redone turn's start any more")]
    NewTurnMissing,
    #[error("the new turn's own decisions no longer replay: {0}")]
    PrefixDiverged(String),
}

// ---------------------------------------------------------------------------------------------
// Turn boundaries
// ---------------------------------------------------------------------------------------------

/// One seat's action-phase turn, read from the decision log alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSpan {
    pub seat: PlayerId,
    /// Index of the "action phase" decision that opened the turn.
    pub start: usize,
    /// One past the turn's last decision (reactions of other seats inside the turn included).
    pub end: usize,
    /// False only for the turn still in progress at the end of the log.
    pub complete: bool,
}

/// Options that leave the turn's action untouched: a trade or diplomacy contact, a payment on a
/// deal, or a leader printed "during the action phase" (the engine's `is_free_option`). After one
/// of these the turn menu is offered again within the same turn.
fn is_free_option(id: &str) -> bool {
    id.starts_with("component|trade|")
        || id.starts_with("component|leader|")
        || id.starts_with("component|diplomacy")
}

/// Turns of every seat, in order, found in `decisions`.
///
/// The engine does not mark turns, so the boundaries come from the log: a turn opens with the
/// seat's "action phase" menu and closes with its recorded "end your turn" (`end_turn`) or a
/// `pass`. An "end your turn" with a single option is settled without being asked and leaves no
/// record, so a turn also closes when another seat's menu opens, when the same seat's menu opens
/// again after it already took its action (consecutive turns once the others have passed), or
/// when the phase moves on.
#[must_use]
pub fn find_turns(decisions: &[DecisionRecord]) -> Vec<TurnSpan> {
    let mut turns: Vec<TurnSpan> = Vec::new();
    // (seat, start, took the main action)
    let mut open: Option<(PlayerId, usize, bool)> = None;
    let close = |turns: &mut Vec<TurnSpan>, open: &mut Option<(PlayerId, usize, bool)>, end| {
        if let Some((seat, start, _)) = open.take() {
            turns.push(TurnSpan {
                seat,
                start,
                end,
                complete: true,
            });
        }
    };
    for (i, record) in decisions.iter().enumerate() {
        if open.is_some()
            && record
                .context
                .as_ref()
                .is_some_and(|c| c.phase != Phase::Action)
        {
            close(&mut turns, &mut open, i);
        }
        if record.prompt == "action phase" {
            let continues = open
                .as_ref()
                .is_some_and(|(seat, _, main)| *seat == record.player && !main);
            if !continues {
                close(&mut turns, &mut open, i);
                open = Some((record.player.clone(), i, false));
            }
            if !is_free_option(&record.chosen)
                && let Some((_, _, main)) = open.as_mut()
            {
                *main = true;
            }
            if record.chosen == "pass" {
                close(&mut turns, &mut open, i + 1);
            }
        } else if record.prompt.starts_with("end your turn")
            && record.chosen == "end_turn"
            && open
                .as_ref()
                .is_some_and(|(seat, _, _)| *seat == record.player)
        {
            close(&mut turns, &mut open, i + 1);
        }
    }
    if let Some((seat, start, _)) = open {
        turns.push(TurnSpan {
            seat,
            start,
            end: decisions.len(),
            complete: false,
        });
    }
    turns
}

/// The part of the history a redo rewinds over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedoWindow {
    /// The redoing seat.
    pub seat: PlayerId,
    /// Where the rewind goes: the first decision of the seat's `turns`-th last turn.
    pub start: usize,
    /// One past the last decision of the window; the recorded tail that auto-play replays starts
    /// here. For a turn still in progress it is the end of the log (nothing to replay).
    pub end: usize,
    pub turns: u8,
}

/// The window for "redo `seat`'s last `turns` turns". A turn still in progress counts as the
/// seat's last turn.
///
/// # Errors
/// [`TurnRedoError`] when the seat has fewer turns than asked, or `turns` is 0 or above
/// [`MAX_TURNS_BACK`].
pub fn redo_window(
    decisions: &[DecisionRecord],
    seat: &PlayerId,
    turns: u8,
) -> Result<RedoWindow, TurnRedoError> {
    if turns == 0 || turns > MAX_TURNS_BACK {
        return Err(TurnRedoError::TooManyTurns);
    }
    let spans = find_turns(decisions);
    let mine: Vec<&TurnSpan> = spans.iter().filter(|t| &t.seat == seat).collect();
    if mine.is_empty() {
        return Err(TurnRedoError::NoTurn(seat.to_string()));
    }
    let n = usize::from(turns);
    if mine.len() < n {
        return Err(TurnRedoError::NotEnoughTurns(seat.to_string(), mine.len()));
    }
    let first = mine[mine.len() - n];
    Ok(RedoWindow {
        seat: seat.clone(),
        start: first.start,
        end: first.end,
        turns,
    })
}

/// Where, in the recorded `tail` that follows a redone turn, `seat`'s next turn begins: the index
/// of its "action phase" decision (derived like [`find_turns`]). Everything before it is replayed;
/// control goes back to the seat there. `None` when the tail holds no further turn of the seat.
#[must_use]
pub fn next_turn_start(tail: &[DecisionRecord], seat: &PlayerId) -> Option<usize> {
    find_turns(tail)
        .into_iter()
        .find(|turn| &turn.seat == seat)
        .map(|turn| turn.start)
}

// ---------------------------------------------------------------------------------------------
// The recorded tail and the runs that replay it
// ---------------------------------------------------------------------------------------------

/// Everything about the decisions after the rewound window that the auto-play needs, read from
/// the timeline as it was when the redo was requested.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TailSource {
    /// Cursor, in that timeline, of the first tail decision (the end of the rewound window).
    pub start_cursor: usize,
    /// Other seats' (and the redoing seat's own later) recorded decisions after the window.
    pub decisions: Vec<DecisionRecord>,
    /// Where every stream stood when tail decision `j` was answered in that timeline.
    pub marks: BTreeMap<usize, RngPositions>,
    /// Where the streams stood when the window's last decision was answered. Restored when the
    /// new turn's last decision is answered, so draws made right after a turn ends match.
    pub prev_mark: Option<RngPositions>,
    /// The cards the original timeline's recorded decisions drew before the redoing seat's next
    /// turn, in order, tagged with the tail-relative index of the decision they belong to.
    #[serde(default)]
    pub reserved: Vec<ReservedDraw>,
    /// The events from the first tail decision on (decision numbers as in that timeline).
    pub events: Vec<GameEvent>,
}

struct RunShared {
    accepted: usize,
    conflict: Option<RawConflict>,
    exhausted: bool,
    handoff: bool,
    asking: Option<PlayerId>,
}

struct RedoDecider {
    script: Vec<DecisionRecord>,
    shared: Arc<Mutex<RunShared>>,
    /// `(script index, seat)`: stop before answering that decision when the seat is asked it.
    handoff: Option<(usize, PlayerId)>,
    force: RngForce,
}

impl Decider for RedoDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let mut shared = self.shared.lock().expect("redo shared lock");
        let index = shared.accepted;
        let fail = |reason: &str| IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: reason.to_owned(),
        };
        let Some(record) = self.script.get(index) else {
            shared.exhausted = true;
            shared.asking = Some(choice.player.clone());
            return Err(fail("turn redo script exhausted"));
        };
        if let Some((at, seat)) = &self.handoff
            && index >= *at
            && record.player == *seat
            && choice.player == *seat
        {
            shared.handoff = true;
            shared.asking = Some(seat.clone());
            return Err(fail("turn redo handoff"));
        }
        match StrictDecider::check(record, choice, index) {
            Ok(_reordered) => {
                shared.accepted += 1;
                self.force.before_answer(index);
                self.force.capture(index);
                Ok(choice.answer_for(&record.chosen).expect("checked on offer"))
            }
            Err(conflict) => {
                shared.asking = Some(choice.player.clone());
                shared.conflict = Some(*conflict);
                Err(fail("turn redo conflict"))
            }
        }
    }
}

struct RedoRun {
    accepted: usize,
    conflict: Option<RawConflict>,
    handoff: bool,
    asking: Option<PlayerId>,
    captured: BTreeMap<usize, RngPositions>,
    /// Every card drawn, tagged with the decision it belongs to.
    draws: Vec<ReservedDraw>,
    /// A reserved card that was no longer in the deck.
    reserve_failure: Option<ti4_model::deck_reserve::ReserveFailure>,
}

/// Replay `script` from the fixed initial state with the strict matcher, forcing `force` marks and
/// capturing where the streams stand at every answered decision and which cards were drawn.
fn run_redo(
    initial: &GameState,
    galaxy: Option<&Galaxy>,
    script: &[DecisionRecord],
    force_marks: &RngMarks,
    handoff: Option<(usize, PlayerId)>,
) -> RedoRun {
    let shared = Arc::new(Mutex::new(RunShared {
        accepted: 0,
        conflict: None,
        exhausted: false,
        handoff: false,
        asking: None,
    }));
    let force = RngForce::always(force_marks);
    let table = Table::with_default(Box::new(RedoDecider {
        script: script.to_vec(),
        shared: Arc::clone(&shared),
        handoff,
        force: force.clone(),
    }));
    let mut game = Game::with_table(initial.clone(), ContentStore::embedded(), table);
    if let Some(g) = galaxy {
        game = game.with_galaxy(g.clone());
    }
    force.attach(&mut game);
    let mut engine_end: Option<(ConflictKind, String)> = None;
    let cap = script.len().saturating_mul(64).saturating_add(4096);
    let mut steps = 0usize;
    loop {
        {
            let s = shared.lock().expect("redo shared lock");
            if s.accepted >= script.len() && s.exhausted {
                break;
            }
        }
        steps += 1;
        if steps > cap {
            engine_end = Some((
                ConflictKind::EngineError,
                "the replay did not settle within its step limit".to_owned(),
            ));
            break;
        }
        let result = game.step();
        {
            let s = shared.lock().expect("redo shared lock");
            if s.conflict.is_some() || s.handoff {
                break;
            }
            if game
                .state
                .deck_reserve
                .as_ref()
                .is_some_and(|r| r.failure().is_some())
            {
                break;
            }
            if let Some(err) = &result.error {
                if s.exhausted && s.accepted == script.len() {
                    break;
                }
                engine_end = Some((ConflictKind::EngineError, err.to_string()));
                break;
            }
            if result.finished {
                if s.accepted < script.len() {
                    engine_end = Some((
                        ConflictKind::EngineEnded,
                        "the game finished before every recorded decision was replayed".to_owned(),
                    ));
                }
                break;
            }
        }
    }
    force.finish(&game);
    let draws = game
        .state
        .deck_reserve
        .as_ref()
        .map(|r| r.draws().to_vec())
        .unwrap_or_default();
    let reserve_failure = game
        .state
        .deck_reserve
        .as_ref()
        .and_then(|r| r.failure().cloned());
    let mut s = shared.lock().expect("redo shared lock");
    let mut conflict = s.conflict.take();
    if let (None, Some((kind, detail))) = (&conflict, engine_end) {
        conflict = Some(RawConflict {
            index: s.accepted,
            kind,
            found_seat: None,
            found_prompt: None,
            found_offered: None,
            added: Vec::new(),
            removed: Vec::new(),
            detail,
        });
    }
    RedoRun {
        accepted: s.accepted,
        conflict,
        handoff: s.handoff,
        asking: s.asking.take(),
        captured: force.captured(),
        draws,
        reserve_failure,
    }
}

/// The unedited timeline replayed once, from which the tail after any window can be read.
///
/// A history that stops replaying under the strict matcher part-way (an old save) still yields a
/// baseline; windows that end before the stopping point work, later ones are refused.
pub struct TailBaseline {
    decisions: Vec<DecisionRecord>,
    events: Vec<GameEvent>,
    run: RedoRun,
}

impl TailBaseline {
    /// Replay `decisions` (with `marks`) from the fixed initial state, reading where every stream
    /// stands at each decision and the remaining deck sizes. `events` are the timeline's events
    /// (decision numbers as in `decisions`).
    #[must_use]
    pub fn new(
        initial: &GameState,
        galaxy: Option<&Galaxy>,
        decisions: &[DecisionRecord],
        marks: &RngMarks,
        events: &[GameEvent],
    ) -> Self {
        Self {
            decisions: decisions.to_vec(),
            events: events.to_vec(),
            run: run_redo(initial, galaxy, decisions, marks, None),
        }
    }

    /// How many decisions replayed under the strict matcher.
    #[must_use]
    pub fn replayed(&self) -> usize {
        self.run.accepted
    }

    /// The tail after `window`.
    ///
    /// # Errors
    /// [`TurnRedoError::BaselineDiverged`] when the history does not replay up to the end of the
    /// window; a history that stops replaying *after* the window just gets a shorter tail.
    pub fn tail(&self, window: &RedoWindow) -> Result<TailSource, TurnRedoError> {
        let base = &self.run;
        let end = window.end;
        if base.accepted < end {
            let why = base.conflict.as_ref().map_or_else(
                || "the replay ended early".to_owned(),
                |c| format!("{:?} at decision {}", c.kind, c.index),
            );
            return Err(TurnRedoError::BaselineDiverged(why));
        }
        let tail_end = base.accepted;
        let tail_len = tail_end - end;
        // Only the draws of decisions the auto-play can replay are reserved: those before the
        // seat's next turn. Whatever the original drew later is left to the live game.
        let stop =
            next_turn_start(&self.decisions[end..tail_end], &window.seat).unwrap_or(tail_len);
        let reserved = base
            .draws
            .iter()
            .filter(|d| d.tag >= end && d.tag < end + stop)
            .map(|d| ReservedDraw {
                tag: d.tag - end,
                ..d.clone()
            })
            .collect();
        let event_split = events_through(&self.events, 0, end);
        Ok(TailSource {
            start_cursor: end,
            decisions: self.decisions[end..tail_end].to_vec(),
            marks: (0..tail_len)
                .filter_map(|j| base.captured.get(&(end + j)).map(|m| (j, m.clone())))
                .collect(),
            prev_mark: end
                .checked_sub(1)
                .and_then(|i| base.captured.get(&i))
                .cloned(),
            reserved,
            events: self.events[event_split..].to_vec(),
        })
    }
}

/// [`TailBaseline::new`] then [`TailBaseline::tail`], for one window.
///
/// # Errors
/// As [`TailBaseline::tail`].
pub fn prepare_tail(
    initial: &GameState,
    galaxy: Option<&Galaxy>,
    decisions: &[DecisionRecord],
    marks: &RngMarks,
    events: &[GameEvent],
    window: &RedoWindow,
) -> Result<TailSource, TurnRedoError> {
    TailBaseline::new(initial, galaxy, decisions, marks, events).tail(window)
}

/// How many leading events belong at or before decision `cursor`: those numbered at most `cursor`
/// (events from before cursors existed are numbered by counting decisions resolved so far,
/// starting from `base_count`).
#[must_use]
pub fn events_through(events: &[GameEvent], base_count: usize, cursor: usize) -> usize {
    let mut count = base_count;
    let mut split = 0;
    for event in events {
        if matches!(event.event, GameEventKind::DecisionResolved) {
            count += 1;
        }
        if event.decision_count.unwrap_or(count) > cursor {
            break;
        }
        split += 1;
    }
    split
}

// ---------------------------------------------------------------------------------------------
// Auto-play
// ---------------------------------------------------------------------------------------------

/// What [`autoplay`] decided.
#[derive(Debug, Clone)]
pub struct AutoplayResult {
    /// The new timeline: the redone window followed by the kept tail.
    pub decisions: Vec<DecisionRecord>,
    /// Stream positions to force while replaying `decisions` (kept tail and the join).
    pub marks: RngMarks,
    /// Length of the new turn's part (where the kept tail begins).
    pub prefix_len: usize,
    pub kept: usize,
    pub tail_total: usize,
    pub stop: TurnRedoStop,
    pub asking_seat: Option<String>,
    /// Where the random streams stood when the new turn's last decision was answered (after the
    /// restore, if one applied there). Without forcing this is the natural position, which a
    /// measurement compares with the original's to see whether the new turn shifted the dice.
    pub join_positions: Option<RngPositions>,
    /// Remapped events of the kept tail, to append to the prefix events.
    pub tail_events: Vec<GameEvent>,
}

/// Replay the other seats' recorded decisions after the redone turn.
///
/// `current` is the live history (it ends with the new turn, possibly followed by decisions that
/// were made since, which are replaced). `window_start` is where the rewind went.
///
/// # Errors
/// [`TurnRedoError`] when the seat's new turn is missing or not finished, or its own decisions no
/// longer replay.
#[expect(
    clippy::too_many_lines,
    reason = "one pass: build the script, run it, then classify where it stopped"
)]
pub fn autoplay(
    initial: &GameState,
    galaxy: Option<&Galaxy>,
    current: &[DecisionRecord],
    current_marks: &RngMarks,
    window_start: usize,
    seat: &PlayerId,
    source: &TailSource,
) -> Result<AutoplayResult, TurnRedoError> {
    let spans = find_turns(current);
    let new_turn = spans
        .iter()
        .find(|t| &t.seat == seat && t.start == window_start)
        .ok_or(TurnRedoError::NewTurnMissing)?;
    if !new_turn.complete {
        return Err(TurnRedoError::NewTurnNotComplete);
    }
    let prefix_len = new_turn.end;
    let mut script = current[..prefix_len].to_vec();
    script.extend(source.decisions.iter().cloned());

    let mut force: RngMarks = current_marks
        .iter()
        .filter(|(i, _)| **i < prefix_len)
        .map(|(i, m)| (*i, m.clone()))
        .collect();
    if let (Some(prev), Some(last)) = (&source.prev_mark, prefix_len.checked_sub(1)) {
        force.insert(last, prev.clone());
    }
    for (j, m) in &source.marks {
        force.insert(prefix_len + j, m.clone());
    }
    // Earlier redos of the chain keep governing the part of the history before this one; the
    // unfinished segment of this very redo (if the live game has one) is replaced.
    let mut earlier = current_marks.clone();
    earlier.truncate_to(window_start);
    force.deck_plan = earlier.deck_plan;
    let segment_at = force.deck_plan.len();
    force.deck_plan.push(DeckSegment {
        from: window_start,
        until: OPEN,
        reserved: source
            .reserved
            .iter()
            .map(|d| ReservedDraw {
                tag: prefix_len + d.tag,
                ..d.clone()
            })
            .collect(),
    });

    let stop_rel = next_turn_start(&source.decisions, seat);
    let run = run_redo(
        initial,
        galaxy,
        &script,
        &force,
        stop_rel.map(|j| (prefix_len + j, seat.clone())),
    );
    if run.accepted < prefix_len {
        let why = run.conflict.as_ref().map_or_else(
            || "the replay ended early".to_owned(),
            |c| format!("{:?} at decision {}: {}", c.kind, c.index, c.detail),
        );
        return Err(TurnRedoError::PrefixDiverged(why));
    }

    let tail_total = stop_rel.unwrap_or(source.decisions.len());
    let (final_len, stop, asking_seat) =
        if let Some(failure) = run.reserve_failure.as_ref().filter(|f| f.tag >= prefix_len) {
            // A card the original timeline handed out is not in the deck any more: the decision it
            // belongs to is not replayed; the seat it asks decides live.
            let cut = failure.tag;
            let record = &script[cut];
            let who = failure
                .recipient
                .clone()
                .unwrap_or_else(|| record.player.to_string());
            (
                cut,
                TurnRedoStop::Conflict {
                    conflict: TurnRedoConflict {
                        original_cursor: source.start_cursor + (cut - prefix_len),
                        kind: ConflictKind::ReservedCard,
                        seat: record.player.to_string(),
                        prompt: record.prompt.clone(),
                        detail: reserved_card_detail(&failure.deck, Some(&failure.card), &who),
                        deck: Some(failure.deck.clone()),
                        card: Some(failure.card.clone()),
                        recipient: Some(who),
                    },
                },
                Some(record.player.to_string()),
            )
        } else if run.handoff {
            (
                run.accepted,
                TurnRedoStop::Handoff {
                    seat: seat.to_string(),
                },
                Some(seat.to_string()),
            )
        } else if let Some(conflict) = &run.conflict {
            let rel = conflict.index.saturating_sub(prefix_len);
            let recorded = script.get(conflict.index);
            let seat_asked = conflict
                .found_seat
                .clone()
                .or_else(|| recorded.map(|r| r.player.to_string()));
            (
                run.accepted,
                TurnRedoStop::Conflict {
                    conflict: TurnRedoConflict {
                        original_cursor: source.start_cursor + rel,
                        kind: conflict.kind,
                        seat: seat_asked.clone().unwrap_or_default(),
                        prompt: conflict
                            .found_prompt
                            .clone()
                            .or_else(|| recorded.map(|r| r.prompt.clone()))
                            .unwrap_or_default(),
                        detail: describe(conflict.kind),
                        deck: None,
                        card: None,
                        recipient: None,
                    },
                },
                seat_asked,
            )
        } else {
            (
                run.accepted,
                TurnRedoStop::TailExhausted,
                run.asking.map(|p| p.to_string()),
            )
        };

    let kept = final_len - prefix_len;
    let mut marks = force;
    marks.deck_plan[segment_at].until = final_len;
    marks.truncate_to(final_len);
    let tail_events = remap_tail_events(source, kept, prefix_len);
    Ok(AutoplayResult {
        decisions: script[..final_len].to_vec(),
        marks,
        prefix_len,
        kept,
        tail_total,
        stop,
        asking_seat,
        join_positions: prefix_len
            .checked_sub(1)
            .and_then(|i| run.captured.get(&i))
            .cloned(),
        tail_events,
    })
}

/// Decks whose drawn cards stay hidden from the other seats until played.
fn deck_is_hidden(deck: &str) -> bool {
    matches!(deck, "action_card" | "secret")
}

/// The sentence for a reserved card that is gone, naming the card only when `card` is given.
#[must_use]
pub fn reserved_card_detail(deck: &str, card: Option<&str>, recipient: &str) -> String {
    let what = card.map_or_else(
        || format!("a {}", deck_label(deck)),
        |card| format!("the {} {card}", deck_label(deck)),
    );
    format!("{what} reserved for {recipient} is no longer in the deck")
}

/// What `viewer` may read in the status of a redo: a card drawn from a hidden deck (action card,
/// secret objective) is named only to the seat it was reserved for; everyone else is told that
/// some card of that deck is missing.
pub fn redact_for_viewer(
    outcome: &mut crate::protocol::turn_redo::TurnRedoOutcome,
    viewer: &PlayerId,
) {
    let TurnRedoStop::Conflict { conflict } = &mut outcome.stop else {
        return;
    };
    let (Some(deck), Some(recipient)) = (conflict.deck.clone(), conflict.recipient.clone()) else {
        return;
    };
    if deck_is_hidden(&deck) && recipient != viewer.as_str() {
        conflict.card = None;
        conflict.detail = reserved_card_detail(&deck, None, &recipient);
    }
}

/// A deck name as a noun for a message.
fn deck_label(deck: &str) -> String {
    match deck {
        "action_card" => "action card".to_owned(),
        "secret" => "secret objective".to_owned(),
        "objective" => "public objective".to_owned(),
        "relic" => "relic".to_owned(),
        "agenda" => "agenda".to_owned(),
        other => other.strip_prefix("exploration:").map_or_else(
            || other.to_owned(),
            |kind| format!("{kind} exploration card"),
        ),
    }
}

/// The reservation segment a freshly requested redo carries while its new turn is played live:
/// the new turn's draws skip every card the recorded tail will draw. The tags are placeholders
/// beyond any decision index (the final ones are assigned when auto-play knows the turn's length).
#[must_use]
pub fn open_segment(window_start: usize, source: &TailSource) -> DeckSegment {
    DeckSegment {
        from: window_start,
        until: OPEN,
        reserved: source
            .reserved
            .iter()
            .map(|d| ReservedDraw {
                tag: usize::MAX - 2,
                ..d.clone()
            })
            .collect(),
    }
}

/// A plain-language reason for a conflict kind that names no recorded choice or menu.
fn describe(kind: ConflictKind) -> String {
    match kind {
        ConflictKind::Actor => "the game now asks a different seat than the recorded decision",
        ConflictKind::Prompt => "the game now asks a different question than the recorded decision",
        ConflictKind::Context => "the recorded decision belongs to a different situation now",
        ConflictKind::ChosenNotOffered => "the recorded answer is not available any more",
        ConflictKind::OptionsChanged => {
            "the options on offer are different from when it was recorded"
        }
        ConflictKind::QuantityChanged => {
            "the amounts owed or available differ from when it was recorded"
        }
        ConflictKind::EngineEnded => "the game ended before the recorded decisions did",
        ConflictKind::EngineError => "the game could not continue with the recorded decisions",
        ConflictKind::ReservedCard => "a card the original timeline drew is no longer in the deck",
    }
    .to_owned()
}

/// The kept tail's events with decision numbers moved to the new timeline. Batch references are
/// dropped (a recorded batch is not a batch of the new timeline).
fn remap_tail_events(source: &TailSource, kept: usize, prefix_len: usize) -> Vec<GameEvent> {
    let end = source.start_cursor;
    let take = events_through(&source.events, end, end + kept);
    source.events[..take]
        .iter()
        .map(|event| {
            let mut e = event.clone();
            e.decision_count = event.decision_count.map(|c| c - end + prefix_len);
            e.batch_id = None;
            e.batch_start_cursor = None;
            e.batch_end_cursor = None;
            // An action id is `action_<n>`: the 1-based number of its first decision.
            let moved_id = event
                .action_id
                .as_deref()
                .and_then(|id| id.strip_prefix("action_"))
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n > end)
                .map(|n| format!("action_{}", n - end + prefix_len));
            e.action_start_cursor = event
                .action_start_cursor
                .filter(|c| *c >= end && moved_id.is_some())
                .map(|c| c - end + prefix_len);
            e.action_id = moved_id;
            e
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Persisted state
// ---------------------------------------------------------------------------------------------

/// The timeline as it was before the redo chain began. Restoring it brings back every field the
/// history replacement path writes, so the decision log is byte-identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternateTimeline {
    pub decisions: Vec<DecisionRecord>,
    pub redo: Vec<DecisionRecord>,
    pub events: Vec<GameEvent>,
    pub redo_events: Vec<GameEvent>,
    pub event_counter: u64,
    pub batches: Vec<BatchRecord>,
    #[serde(default)]
    pub rng_marks: RngMarks,
}

/// Saved beside the game while a redo is in flight (`turn_redo.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRedoRecord {
    /// The seat whose turn is redone.
    pub seat: PlayerId,
    pub requested_by: PlayerId,
    pub turns_back: u8,
    /// Requests chained onto the same saved original.
    pub redo_count: u32,
    /// Where the latest request rewound to (start of the new turn).
    pub rewound_to: usize,
    pub stage: TurnRedoStage,
    /// After auto-play: the live history length then.
    pub handoff_len: Option<usize>,
    /// Where the latest request's recorded tail comes from.
    pub source: TailSource,
    pub outcome: Option<crate::protocol::turn_redo::TurnRedoOutcome>,
    /// The timeline before the first redo of the chain.
    pub alternate: AlternateTimeline,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(player: &str, prompt: &str, chosen: &str) -> DecisionRecord {
        DecisionRecord {
            player: PlayerId::new(player),
            prompt: prompt.to_owned(),
            chosen: chosen.to_owned(),
            offered: vec![chosen.to_owned()],
            context: None,
        }
    }

    #[test]
    fn turns_close_on_end_turn_pass_and_the_next_seats_menu() {
        let log = vec![
            record("a", "action phase", "tactical"),
            record("a", "activate a system", "18"),
            record("a", "end your turn", "end_turn"),
            record("b", "action phase", "tactical"),
            record("b", "activate a system", "10"),
            // b's end of turn was a single option, so it is not recorded; c's menu closes it.
            record("c", "action phase", "pass"),
            record("a", "action phase", "component|leader|x"),
            record("a", "action phase", "strategic|pok1"),
            record("b", "reaction", "no"),
            record("a", "end your turn", "end_turn"),
            // a again with nothing recorded in between and no explicit end: a new turn.
            record("a", "action phase", "tactical"),
        ];
        let spans = find_turns(&log);
        let view: Vec<_> = spans
            .iter()
            .map(|t| (t.seat.as_str(), t.start, t.end, t.complete))
            .collect();
        assert_eq!(
            view,
            vec![
                ("a", 0, 3, true),
                ("b", 3, 5, true),
                ("c", 5, 6, true),
                ("a", 6, 10, true),
                ("a", 10, 11, false),
            ]
        );
    }

    #[test]
    fn a_free_option_keeps_the_turn_open() {
        let log = vec![
            record("a", "action phase", "component|trade|b"),
            record("a", "action phase", "tactical"),
            record("a", "activate a system", "1"),
        ];
        let spans = find_turns(&log);
        assert_eq!(spans.len(), 1);
        assert_eq!(
            (spans[0].start, spans[0].end, spans[0].complete),
            (0, 3, false)
        );
    }

    #[test]
    fn redo_window_counts_back_over_a_seats_own_turns() {
        let log = vec![
            record("a", "action phase", "tactical"),
            record("a", "end your turn", "end_turn"),
            record("b", "action phase", "tactical"),
            record("b", "end your turn", "end_turn"),
            record("a", "action phase", "tactical"),
            record("a", "end your turn", "end_turn"),
            record("b", "action phase", "tactical"),
        ];
        let a = PlayerId::new("a");
        let one = redo_window(&log, &a, 1).unwrap();
        assert_eq!((one.start, one.end), (4, 6));
        let two = redo_window(&log, &a, 2).unwrap();
        assert_eq!((two.start, two.end), (0, 2));
        assert_eq!(redo_window(&log, &a, 3), Err(TurnRedoError::TooManyTurns));
        assert!(matches!(
            redo_window(&log, &PlayerId::new("z"), 1),
            Err(TurnRedoError::NoTurn(_))
        ));
        // A turn still in progress is the seat's last turn.
        let b = redo_window(&log, &PlayerId::new("b"), 1).unwrap();
        assert_eq!((b.start, b.end), (6, 7));
    }
}
