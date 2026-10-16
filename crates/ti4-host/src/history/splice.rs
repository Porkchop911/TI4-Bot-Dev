//! Read-only dry run of "remove (or change) one decision in the past, then replay the rest".
//!
//! H7 phase 1. The game seed is fixed: every run, baseline and edited, starts from the same
//! initial state, so nothing here re-seeds anything. Nothing here touches a live session,
//! the history, or storage; it replays in memory and reports.
//!
//! The matcher copies the design of `ti4-replayer`'s `ReplayScript::answer_for`
//! (actor, prompt, option set, typed context, chosen on offer) without taking that dependency.
//! It stops at the first recorded decision that does not fit.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use thiserror::Error;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, Table};
use ti4_engine::game::Game;
use ti4_model::state::GameState;

use crate::protocol::splice::{
    AlignmentChange, AlignmentNote, ConflictKind, RngCounters, RngIndicator, RngStatus,
    SpliceConflict, SpliceEdit, SplicePreview,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpliceError {
    #[error("decision {cursor} is out of range: the history has {len} decisions")]
    CursorOutOfRange { cursor: usize, len: usize },
    #[error("option {option_id:?} was not on offer at decision {cursor}")]
    OptionNotOffered { cursor: usize, option_id: String },
    #[error("{0}")]
    Replace(String),
}

/// One question the engine settled without asking, positioned by decisions applied so far.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AutoNote {
    pos: usize,
    seat: String,
    prompt: String,
    option_id: String,
}

/// A mismatch found at an index of the script, before it is mapped to an original cursor.
#[derive(Debug, Clone)]
pub(crate) struct RawConflict {
    pub(crate) index: usize,
    pub(crate) kind: ConflictKind,
    pub(crate) found_seat: Option<String>,
    pub(crate) found_prompt: Option<String>,
    pub(crate) found_offered: Option<Vec<String>>,
    pub(crate) added: Vec<String>,
    pub(crate) removed: Vec<String>,
    pub(crate) detail: String,
}

#[derive(Default)]
struct Shared {
    /// Decisions accepted so far.
    accepted: usize,
    reordered: Vec<usize>,
    conflict: Option<RawConflict>,
    /// The engine asked for a decision after the script ran out: a normal end of the replay.
    exhausted: bool,
    auto: Vec<AutoNote>,
}

pub(crate) struct StrictDecider {
    script: Vec<DecisionRecord>,
    shared: Arc<Mutex<Shared>>,
}

fn id_set(ids: &[String]) -> BTreeSet<&str> {
    ids.iter().map(String::as_str).collect()
}

impl StrictDecider {
    /// The strict check, in the order the replayer uses: actor, prompt, context, chosen on offer,
    /// then the menu, then quantities. `Ok(true)` means it fits with the menu in another order.
    #[expect(
        clippy::too_many_lines,
        reason = "the matcher keeps its ordered checks visible in one place, like the replayer's"
    )]
    pub(crate) fn check(
        record: &DecisionRecord,
        choice: &Choice,
        index: usize,
    ) -> Result<bool, Box<RawConflict>> {
        let found_offered: Vec<String> = choice.ids().into_iter().map(str::to_owned).collect();
        let conflict = |kind: ConflictKind, detail: String| {
            Box::new(RawConflict {
                index,
                kind,
                found_seat: Some(choice.player.to_string()),
                found_prompt: Some(choice.prompt.clone()),
                found_offered: Some(found_offered.clone()),
                added: Vec::new(),
                removed: Vec::new(),
                detail,
            })
        };
        if record.player != choice.player {
            return Err(conflict(
                ConflictKind::Actor,
                format!(
                    "engine asked {}, record says {}",
                    choice.player, record.player
                ),
            ));
        }
        if record.prompt != choice.prompt {
            return Err(conflict(
                ConflictKind::Prompt,
                format!(
                    "recorded {:?}, engine asked {:?}",
                    record.prompt, choice.prompt
                ),
            ));
        }
        // A record written before contexts existed has none; there is nothing to bind then.
        let offered_context = choice
            .context
            .as_ref()
            .map(ti4_engine::decision_context::DecisionContext::without_display_fields);
        let mut quantity_changed = false;
        if let Some(recorded) = &record.context {
            let Some(offered) = &offered_context else {
                return Err(conflict(
                    ConflictKind::Context,
                    "the engine offers no context, the record has one".to_owned(),
                ));
            };
            // `outstanding` is compared separately: paying differently earlier legitimately
            // changes what is owed later, which is a soft difference rather than another question.
            let mut a = recorded.clone();
            let mut b = offered.clone();
            quantity_changed = a.outstanding != b.outstanding;
            a.outstanding.clear();
            b.outstanding.clear();
            if a != b {
                return Err(conflict(
                    ConflictKind::Context,
                    format!(
                        "recorded subtype {:?} (round {}, {:?}) vs offered subtype {:?} (round {}, {:?})",
                        recorded.subtype,
                        recorded.round,
                        recorded.phase,
                        offered.subtype,
                        offered.round,
                        offered.phase
                    ),
                ));
            }
        }
        if choice.answer_for(&record.chosen).is_none() {
            return Err(conflict(
                ConflictKind::ChosenNotOffered,
                format!("{:?} is not on offer", record.chosen),
            ));
        }
        let (recorded_set, found_set) = (id_set(&record.offered), id_set(&found_offered));
        if recorded_set != found_set || record.offered.len() != found_offered.len() {
            if recorded_set != found_set {
                let mut c = conflict(
                    ConflictKind::OptionsChanged,
                    format!(
                        "{} option(s) recorded, {} offered",
                        record.offered.len(),
                        found_offered.len()
                    ),
                );
                c.added = found_set
                    .difference(&recorded_set)
                    .map(|s| (*s).to_owned())
                    .collect();
                c.removed = recorded_set
                    .difference(&found_set)
                    .map(|s| (*s).to_owned())
                    .collect();
                return Err(c);
            }
            // Same ids with a different count means duplicate ids; treat as an options change.
            return Err(conflict(
                ConflictKind::OptionsChanged,
                "the same ids are offered a different number of times".to_owned(),
            ));
        }
        if quantity_changed {
            return Err(conflict(
                ConflictKind::QuantityChanged,
                "the quantities still owed or available differ".to_owned(),
            ));
        }
        // Same set; a different order is kept and noted.
        Ok(record.offered != found_offered)
    }
}

impl Decider for StrictDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let mut shared = self.shared.lock().expect("splice shared lock");
        let index = shared.accepted;
        let Some(record) = self.script.get(index) else {
            // The script is exhausted: the engine is simply asking the next live question.
            shared.exhausted = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "splice script exhausted".to_owned(),
            });
        };
        match Self::check(record, choice, index) {
            Ok(reordered) => {
                if reordered {
                    shared.reordered.push(index);
                }
                shared.accepted += 1;
                Ok(choice.answer_for(&record.chosen).expect("checked on offer"))
            }
            Err(conflict) => {
                let reason = format!("splice conflict: {:?}", conflict.kind);
                shared.conflict = Some(*conflict);
                Err(IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason,
                })
            }
        }
    }
}

/// Remaining cards per deck (objectives, relics, agendas, action cards, secrets, exploration).
pub(crate) fn deck_lengths(s: &GameState) -> BTreeMap<String, i64> {
    let mut decks = BTreeMap::new();
    let len = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    decks.insert("objective".to_owned(), len(s.objective_deck.len()));
    decks.insert("relic".to_owned(), len(s.relic_deck.len()));
    decks.insert("agenda".to_owned(), len(s.agenda_deck.len()));
    decks.insert("action_card".to_owned(), len(s.action_card_deck.len()));
    decks.insert("secret".to_owned(), len(s.secret_deck.len()));
    for (kind, deck) in &s.exploration_decks {
        decks.insert(format!("exploration:{kind}"), len(deck.len()));
    }
    decks
}

/// Counters read from public engine surfaces after each decision.
fn counters(game: &Game) -> RngCounters {
    let decks = deck_lengths(&game.state);
    let len = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    let history = game.dice().history();
    let faces: usize = history
        .iter()
        .map(|r| {
            if r.rerolled.is_empty() {
                r.faces.len()
            } else {
                r.rerolled.len()
            }
        })
        .sum();
    RngCounters {
        dice_rolls: len(history.len()),
        dice_faces: len(faces),
        decks,
    }
}

fn counters_delta(later: &RngCounters, earlier: &RngCounters) -> RngCounters {
    let keys: BTreeSet<&String> = later.decks.keys().chain(earlier.decks.keys()).collect();
    RngCounters {
        dice_rolls: later.dice_rolls - earlier.dice_rolls,
        dice_faces: later.dice_faces - earlier.dice_faces,
        decks: keys
            .into_iter()
            .map(|k| {
                (
                    k.clone(),
                    later.decks.get(k).copied().unwrap_or(0)
                        - earlier.decks.get(k).copied().unwrap_or(0),
                )
            })
            .filter(|(_, v)| *v != 0)
            .collect(),
    }
}

/// Everything one in-memory run produced.
struct Run {
    accepted: usize,
    reordered: Vec<usize>,
    conflict: Option<RawConflict>,
    auto: Vec<AutoNote>,
    /// Counters keyed by number of decisions applied.
    snaps: BTreeMap<usize, RngCounters>,
}

/// Replay `script` from the fixed initial state with the strict matcher.
fn run(initial: &GameState, galaxy: Option<&Galaxy>, script: &[DecisionRecord]) -> Run {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let decider = Box::new(StrictDecider {
        script: script.to_vec(),
        shared: Arc::clone(&shared),
    });
    let mut table = Table::with_default(decider);
    let observer = Arc::clone(&shared);
    table.on_auto_resolved(move |note| {
        let mut s = observer.lock().expect("splice shared lock");
        let pos = s.accepted;
        s.auto.push(AutoNote {
            pos,
            seat: note.player.to_string(),
            prompt: note.prompt.clone(),
            option_id: note.option_id.clone(),
        });
    });
    let mut game = Game::with_table(initial.clone(), ContentStore::embedded(), table);
    if let Some(g) = galaxy {
        game = game.with_galaxy(g.clone());
    }
    let mut snaps = BTreeMap::new();
    snaps.insert(0, counters(&game));
    let mut engine_end: Option<(ConflictKind, String)> = None;
    while shared.lock().expect("splice shared lock").accepted < script.len() {
        let before = game.table.log.records.len();
        let result = game.step();
        let now = game.table.log.records.len();
        if now != before {
            snaps.insert(now, counters(&game));
        }
        if shared
            .lock()
            .expect("splice shared lock")
            .conflict
            .is_some()
        {
            break;
        }
        if let Some(err) = result.error {
            let s = shared.lock().expect("splice shared lock");
            if s.exhausted && s.accepted == script.len() {
                // The engine moved on to its next question after the last recorded decision.
                break;
            }
            drop(s);
            engine_end = Some((ConflictKind::EngineError, err.to_string()));
            break;
        }
        if result.finished && game.table.log.records.len() < script.len() {
            engine_end = Some((
                ConflictKind::EngineEnded,
                "the game finished before every recorded decision was replayed".to_owned(),
            ));
            break;
        }
    }
    let mut s = shared.lock().expect("splice shared lock");
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
    Run {
        accepted: s.accepted,
        reordered: std::mem::take(&mut s.reordered),
        conflict,
        auto: std::mem::take(&mut s.auto),
        snaps,
    }
}

/// The unedited history replayed once, reusable across many edits of the same game.
pub struct SpliceBaseline {
    initial: GameState,
    galaxy: Option<Galaxy>,
    decisions: Vec<DecisionRecord>,
    run: Run,
}

impl SpliceBaseline {
    /// Replay the recorded history from the seed.
    ///
    /// A history that no longer replays cleanly (an old save) still yields a baseline; edits
    /// compared against it simply have fewer comparison points.
    #[must_use]
    pub fn new(initial: &GameState, galaxy: Option<&Galaxy>, decisions: &[DecisionRecord]) -> Self {
        Self {
            initial: initial.clone(),
            galaxy: galaxy.cloned(),
            decisions: decisions.to_vec(),
            run: run(initial, galaxy, decisions),
        }
    }

    /// How many recorded decisions the unedited replay reproduced.
    #[must_use]
    pub fn replayed(&self) -> usize {
        self.run.accepted
    }

    /// Why the unedited history stopped replaying under the strict matcher, if it did. When this
    /// is `Some`, every preview of this game is measured against a history that does not itself
    /// replay, so the caller should say so rather than present the numbers as meaningful.
    #[must_use]
    pub fn replay_conflict(&self) -> Option<SpliceConflict> {
        self.run
            .conflict
            .as_ref()
            .map(|raw| self.conflict(raw, raw.index))
    }

    #[must_use]
    pub fn decisions(&self) -> &[DecisionRecord] {
        &self.decisions
    }

    /// Dry-run one edit. Replays the edited list from the same seed; changes nothing.
    ///
    /// # Errors
    /// [`SpliceError`] for a cursor outside the history or an option that was not on offer.
    pub fn preview(&self, edit: &SpliceEdit) -> Result<SplicePreview, SpliceError> {
        let n = self.decisions.len();
        let k = edit.cursor();
        if k >= n {
            return Err(SpliceError::CursorOutOfRange { cursor: k, len: n });
        }
        let (script, remove) = match edit {
            SpliceEdit::Remove { .. } => {
                let mut s = self.decisions.clone();
                s.remove(k);
                (s, true)
            }
            SpliceEdit::Replace { option_id, .. } => {
                let mut s = self.decisions.clone();
                if !s[k].offered.contains(option_id) {
                    return Err(SpliceError::OptionNotOffered {
                        cursor: k,
                        option_id: option_id.clone(),
                    });
                }
                s[k].chosen.clone_from(option_id);
                (s, false)
            }
        };
        // Index of the edited list -> cursor of the original list.
        let to_original = |e: usize| if remove && e >= k { e + 1 } else { e };
        let edited = run(&self.initial, self.galaxy.as_ref(), &script);

        let later_decisions = n - k - 1;
        // Decisions accepted at or after the edit point. A Replace's own slot is index k.
        let applied_from_edit = edited.accepted.saturating_sub(k);
        let kept_later = if remove {
            applied_from_edit
        } else {
            applied_from_edit.saturating_sub(1)
        }
        .min(later_decisions);
        let first_conflict = edited.conflict.as_ref().map(|raw| {
            let mut conflict = self.conflict(raw, to_original(raw.index));
            if remove {
                let removed = &self.decisions[k];
                conflict.asks_removed_decision = raw.found_seat.as_deref()
                    == Some(removed.player.as_str())
                    && raw.found_prompt.as_deref() == Some(removed.prompt.as_str())
                    && raw.found_offered.as_ref() == Some(&removed.offered);
            }
            conflict
        });
        let kept_reordered = edited
            .reordered
            .iter()
            .map(|e| to_original(*e))
            .filter(|c| *c > k)
            .collect();

        Ok(SplicePreview {
            edit: edit.clone(),
            original_decisions: n,
            later_decisions,
            kept_later,
            dropped_later: later_decisions - kept_later,
            survives_to_end: first_conflict.is_none() && kept_later == later_decisions,
            kept_reordered,
            first_conflict,
            alignment: self.alignment(&edited, k, remove),
            rng: self.rng_indicator(&edited, k, remove),
        })
    }

    fn conflict(&self, raw: &RawConflict, cursor: usize) -> SpliceConflict {
        // The record the engine failed to match; past the end it is the last one.
        let record = self
            .decisions
            .get(cursor)
            .or_else(|| self.decisions.last())
            .expect("a conflict implies a non-empty history");
        SpliceConflict {
            cursor,
            kind: raw.kind,
            soft: raw.kind.is_soft(),
            seat: record.player.to_string(),
            prompt: record.prompt.clone(),
            expected_chosen: record.chosen.clone(),
            expected_offered: record.offered.clone(),
            found_seat: raw.found_seat.clone(),
            found_prompt: raw.found_prompt.clone(),
            found_offered: raw.found_offered.clone(),
            added: raw.added.clone(),
            removed: raw.removed.clone(),
            detail: raw.detail.clone(),
            asks_removed_decision: false,
        }
    }

    /// Single-option questions (never logged) that differ between the original and the edited
    /// run over the part of the history both reached.
    fn alignment(&self, edited: &Run, k: usize, remove: bool) -> Vec<AlignmentNote> {
        // Original positions: a removal merges the two positions around the removed decision.
        let orig_pos = |p: usize| if remove && p > k { p - 1 } else { p };
        let reached = edited.accepted;
        let original: Vec<(usize, &AutoNote)> = self
            .run
            .auto
            .iter()
            .map(|a| (orig_pos(a.pos), a))
            .filter(|(p, _)| *p >= k && *p <= reached)
            .collect();
        let after: Vec<(usize, &AutoNote)> = edited
            .auto
            .iter()
            .map(|a| (a.pos, a))
            .filter(|(p, _)| *p >= k && *p <= reached)
            .collect();
        let key = |(p, a): &(usize, &AutoNote)| {
            (*p, a.seat.clone(), a.prompt.clone(), a.option_id.clone())
        };
        let mut notes = Vec::new();
        // Multiset difference in both directions, by (position, seat, prompt, option).
        let mut orig_keys: Vec<_> = original.iter().map(key).collect();
        let mut edit_keys: Vec<_> = after.iter().map(key).collect();
        orig_keys.sort();
        edit_keys.sort();
        let mut remaining = orig_keys.clone();
        for item in &edit_keys {
            if let Some(i) = remaining.iter().position(|o| o == item) {
                remaining.remove(i);
            } else {
                notes.push(Self::note(AlignmentChange::Appeared, item, remove, k));
            }
        }
        let mut remaining = edit_keys;
        for item in &orig_keys {
            if let Some(i) = remaining.iter().position(|o| o == item) {
                remaining.remove(i);
            } else {
                notes.push(Self::note(AlignmentChange::Vanished, item, remove, k));
            }
        }
        notes.sort_by(|a, b| (a.cursor, &a.prompt).cmp(&(b.cursor, &b.prompt)));
        notes
    }

    fn note(
        change: AlignmentChange,
        key: &(usize, String, String, String),
        remove: bool,
        k: usize,
    ) -> AlignmentNote {
        // Positions are in the edited numbering; report original cursors.
        let cursor = if remove && key.0 > k {
            key.0 + 1
        } else {
            key.0
        };
        AlignmentNote {
            change,
            cursor,
            seat: key.1.clone(),
            prompt: key.2.clone(),
            option_id: key.3.clone(),
        }
    }

    fn rng_indicator(&self, edited: &Run, k: usize, remove: bool) -> RngIndicator {
        // Counters are read after each engine step, and one step can log several nested
        // decisions, so the nearest step boundaries are used. That can only over-report
        // consumption (a neighbour in the same step is included), never hide it.
        let removed_consumed = if remove {
            let before = self.run.snaps.range(..=k).next_back();
            let after = self.run.snaps.range(k + 1..).next();
            match (before, after) {
                (Some((_, before)), Some((_, after))) => Some(counters_delta(after, before)),
                _ => None,
            }
        } else {
            None
        };
        let mut compared = 0;
        let mut first: Option<(usize, RngCounters)> = None;
        // Edited length L corresponds to original length L (+1 after a removal).
        for (len, snap) in edited.snaps.range(k..) {
            let orig_len = if remove { len + 1 } else { *len };
            // A replace compares from the replaced decision onward; a removal from the point
            // where the removed decision would have been applied.
            let Some(orig) = self.run.snaps.get(&orig_len) else {
                continue;
            };
            compared += 1;
            let diff = counters_delta(snap, orig);
            if !diff.is_zero() && first.is_none() {
                first = Some((orig_len.saturating_sub(1), diff));
            }
        }
        let consumed_nonzero = removed_consumed.as_ref().is_some_and(|c| !c.is_zero());
        let status = if first.is_some() || consumed_nonzero {
            RngStatus::NotNeutral
        } else if compared == 0 && removed_consumed.is_none() {
            RngStatus::Unknown
        } else {
            RngStatus::Neutral
        };
        RngIndicator {
            status,
            compared,
            removed_consumed,
            first_divergence_cursor: first.as_ref().map(|(c, _)| *c),
            first_divergence: first.map(|(_, d)| d),
        }
    }
}

/// One-shot dry run: replay the history, then the edit.
///
/// # Errors
/// [`SpliceError`] as for [`SpliceBaseline::preview`].
pub fn splice_preview(
    initial: &GameState,
    galaxy: Option<&Galaxy>,
    decisions: &[DecisionRecord],
    edit: &SpliceEdit,
) -> Result<SplicePreview, SpliceError> {
    SpliceBaseline::new(initial, galaxy, decisions).preview(edit)
}
