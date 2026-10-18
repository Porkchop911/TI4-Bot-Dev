//! Record a player's answers and replay them on a fresh game copy.
//!
//! The engine asks questions and lists the legal answers. A `Decider` chooses an
//! answer, and the engine carries it out. The code here saves those answers so
//! another game copy can follow the same plan later.
//!
//! An answer marked "consumed" has been used in this attempt. It does not mean
//! its changes have been accepted. To make a batch all-or-nothing, the caller
//! must try the whole batch on a game copy and throw that copy away if any part
//! fails. Keep the original game and full list of answers for another attempt.
//! That batch handling is not implemented here.
//!
//! Check replay progress after each engine step, even if the step reports success:
//! the engine sometimes catches an answer error and continues. Once replay fails,
//! it refuses to answer any more questions, but it cannot undo earlier changes.
//! A planning copy must never replace the live game. This code also does not stop
//! the engine from rolling dice or revealing hidden information. The [`audit`]
//! module has the checks of that publication boundary; [`draft`] puts them
//! around these adapters and an ordinary disposable engine fork.

pub mod audit;
pub mod draft;
pub mod fork;
pub mod script;
pub mod steps;
pub mod undo;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, validate};
use ti4_engine::decision_context::DecisionContext;
use ti4_model::id::PlayerId;

/// Saves who was asked, what question was asked, and which option was selected.
///
/// The engine does not use one uniform representation for selections: some
/// options carry important information in their ID, others in their payload.
/// IDs alone are insufficient, so ID, kind, and payload must all match on replay.
/// Display labels are presentation and are not part of the saved identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedDecision {
    pub player: PlayerId,
    pub prompt: String,
    pub context: Option<DecisionContext>,
    pub option_id: String,
    pub kind: String,
    pub payload: BTreeMap<String, Value>,
}

impl RecordedDecision {
    pub fn from_answer(choice: &Choice, answer: &ChoiceOption) -> Self {
        Self {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            context: choice.context.clone(),
            option_id: answer.id.clone(),
            kind: answer.kind.clone(),
            payload: answer.payload.clone(),
        }
    }

    pub fn matches_question(&self, choice: &Choice) -> bool {
        // Prefer the structured description of why the engine is asking. Prompt
        // wording may change without changing that meaning. Fall back to exact
        // text only when neither question has context; missing context on just
        // one side is a mismatch, not permission to make a weaker comparison.
        self.player == choice.player
            && match (&self.context, &choice.context) {
                (Some(expected), Some(actual)) => expected == actual,
                (None, None) => self.prompt == choice.prompt,
                _ => false,
            }
    }

    pub fn matches_replay_question(&self, choice: &Choice) -> bool {
        if let (Some(expected), Some(actual)) = (&self.context, &choice.context) {
            let mut rebound = expected.clone();
            if expected.subtype == "commit_ground_forces"
                && expected.invasion_seq.is_some()
                && actual.invasion_seq.is_some()
            {
                rebound.invasion_seq = actual.invasion_seq;
            }
            if matches!(
                expected.subtype.as_str(),
                "produce_unit" | "pay_resources" | "place_unit"
            ) {
                // Costs and remaining capacity are recomputed by the fresh engine.
                rebound.outstanding.clone_from(&actual.outstanding);
            }
            return self.player == choice.player && rebound == *actual;
        }
        self.matches_question(choice)
    }

    /// Resolve only the exact recorded instruction against a fresh live offer.
    pub fn offered_option(&self, choice: &Choice) -> Option<ChoiceOption> {
        if !self.matches_question(choice) {
            return None;
        }
        let mut matches = choice.options.iter().filter(|option| {
            option.id == self.option_id
                && option.kind == self.kind
                && option.payload == self.payload
        });
        match (matches.next(), matches.next()) {
            (Some(option), None) => Some(option.clone()),
            _ => None,
        }
    }

    /// Movement/cargo/landing vector indexes and hold counters are execution details.
    /// Reconstruction may rebind those, but keeps every other semantic field,
    /// including boost choices, unit condition, and the cargo's actual pickup
    /// system (which may differ from the carrier's origin). Live application stays exact.
    pub fn matches_replay_option(&self, option: &ChoiceOption) -> bool {
        let subtype = self
            .context
            .as_ref()
            .map(|context| context.subtype.as_str());
        let volatile: &[&str] = match (subtype, self.kind.as_str()) {
            (Some("movement_step"), "move") => &[],
            (Some("commit_ground_forces"), "commit") => &[],
            (Some("produce_unit"), _) => &[
                "cost",
                "printed_cost",
                "discount",
                "placed",
                "credit",
                "available_resources",
                "free_this_use",
                "credit_used",
                "owed",
                "production_spent",
                "fleet_ships",
                "capacity_provided",
                "capacity_needed",
                "capacity_role",
                "space_placement",
                "capacity_used",
                "fleet_headroom_after",
                "capacity_free_after",
                "fleet_excess_after",
                "capacity_excess_after",
            ],
            (Some("place_unit"), _) => &[
                "placed",
                "capacity_used",
                "fleet_headroom_after",
                "capacity_free_after",
                "fleet_excess_after",
                "capacity_excess_after",
            ],
            (Some("pay_resources"), _) => &["owed"],
            (Some("load_cargo"), "load" | "decline") => &[
                "capacity_remaining",
                "loaded_ground",
                "loaded_fighters",
                "ground_available",
            ],
            _ => {
                return self.option_id == option.id
                    && self.kind == option.kind
                    && self.payload == option.payload;
            }
        };
        self.kind == option.kind
            && (!matches!(
                subtype,
                Some("produce_unit" | "place_unit" | "pay_resources")
            ) || self.option_id == option.id)
            && (self.kind != "decline" || self.option_id == option.id)
            && self
                .payload
                .iter()
                .filter(|(key, _)| !volatile.contains(&key.as_str()))
                .eq(option
                    .payload
                    .iter()
                    .filter(|(key, _)| !volatile.contains(&key.as_str())))
    }

    pub fn replay_option(&self, choice: &Choice) -> Option<ChoiceOption> {
        if !self.matches_replay_question(choice) {
            return None;
        }
        let mut matches = choice
            .options
            .iter()
            .filter(|option| self.matches_replay_option(option));
        match (matches.next(), matches.next()) {
            (Some(option), None) => Some(option.clone()),
            _ => None,
        }
    }
}

/// Kept outside the decider so answers survive disposal of the engine fork.
///
/// The engine owns the decider once it is installed. `Arc` lets the caller and
/// decider share ownership of the saved answers; `Mutex` protects their access.
pub type DecisionRecording = Arc<Mutex<Vec<RecordedDecision>>>;

/// Records successful answers from an existing input source.
///
/// ```text
/// Engine asks a question
///     -> RecordingDecider forwards it to the wrapped input source
///     -> Input source answers
///     -> RecordingDecider saves the question identity and selected answer
///     -> Engine receives the answer and continues
/// ```
///
/// The wrapped source can be a human-input decider or, as in the tests, a
/// `Scripted` decider supplying predefined answers.
pub struct RecordingDecider {
    inner: Box<dyn Decider>,
    recording: DecisionRecording,
}

impl RecordingDecider {
    /// Returns the adapter to install in the engine and a recording handle to
    /// keep outside it. The handle remains usable after the fork is discarded.
    #[must_use]
    pub fn new(inner: Box<dyn Decider>) -> (Self, DecisionRecording) {
        let recording = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                inner,
                recording: recording.clone(),
            },
            recording,
        )
    }

    fn record(&self, choice: &Choice, answer: ChoiceOption) -> Result<ChoiceOption, IllegalChoice> {
        let answer = validate(choice, answer)?;
        // Validate the selected ID, then record and return the actual offered
        // option. A wrapped decider's copy might contain a modified payload;
        // the engine's option is the authoritative instruction.
        let offered = choice.option(&answer.id).expect("validated answer").clone();
        self.recording
            .lock()
            .expect("decision recording lock")
            .push(RecordedDecision::from_answer(choice, &offered));
        Ok(offered)
    }
}

impl Decider for RecordingDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose(choice)?;
        self.record(choice, answer)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // Some input sources use the player's view of the board to decide.
        // Preserve that interface rather than routing them through choose(),
        // which receives only the question and options.
        let answer = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, answer)
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayStopReason {
    /// The next choice needs new input.
    Exhausted,
    QuestionMismatch,
    SelectionMissing,
    SelectionAmbiguous,
}

/// Progress observable outside the engine: how many answers were consumed,
/// which remain, and why replay stopped. A failing answer stays at the front.
///
/// Consumption describes this attempt only; it does not commit game changes.
/// For an atomic batch, the caller must retain the original full script separately.
/// If the attempt fails, discard its fork and retry from the original checkpoint
/// with the full script, not just this progress's remaining tail.
#[derive(Debug)]
pub struct ReplayProgress {
    consumed: usize,
    remaining: VecDeque<RecordedDecision>,
    stop_reason: Option<ReplayStopReason>,
}

impl ReplayProgress {
    #[must_use]
    pub const fn consumed(&self) -> usize {
        self.consumed
    }

    #[must_use]
    pub fn remaining(&self) -> &VecDeque<RecordedDecision> {
        &self.remaining
    }

    #[must_use]
    pub const fn stop_reason(&self) -> Option<ReplayStopReason> {
        self.stop_reason
    }
}

/// Shared ownership lets the caller inspect progress while the engine owns the
/// replay decider, and retain the unconsumed answers after disposing of the fork.
pub type ReplayStatus = Arc<Mutex<ReplayProgress>>;

/// Supplies fresh engine options until the first mismatch or unanswered choice.
/// A stopped decider never resumes or falls back to choosing an arbitrary option.
///
/// For each question, only the next recorded decision is considered:
///
/// ```text
/// Does the player match?
///     -> Does the question match?
///     -> Is there exactly one matching offered option (ID, kind, payload)?
///     -> Return that fresh option and consume the recorded decision
/// ```
///
/// The entire offered option list need not be unchanged. Unrelated options may
/// appear or disappear as long as the recorded selection still matches uniquely.
///
/// For example, a script may say: take a tactical action, activate system 19,
/// move a destroyer from system 20, finish movement. If the destroyer disappears,
/// the attempt supplies the first two answers, then stops at the third. Progress
/// reports two consumed and two remaining, but an atomic caller discards the
/// entire trial fork, undoing the activation and its cost by disposal. It keeps
/// the original full script for a new attempt. No different ship is substituted,
/// and replay never searches ahead for another answer.
pub struct ReplayDecider {
    progress: ReplayStatus,
    semantic_planning: bool,
}

impl ReplayDecider {
    /// Returns the adapter to install in the engine and a progress handle to
    /// inspect after each attempted step.
    #[must_use]
    pub fn new(decisions: Vec<RecordedDecision>) -> (Self, ReplayStatus) {
        let progress = Arc::new(Mutex::new(ReplayProgress {
            consumed: 0,
            remaining: decisions.into(),
            stop_reason: None,
        }));
        (
            Self {
                progress: progress.clone(),
                semantic_planning: false,
            },
            progress,
        )
    }

    pub fn for_planning(decisions: Vec<RecordedDecision>) -> (Self, ReplayStatus) {
        let (mut decider, status) = Self::new(decisions);
        decider.semantic_planning = true;
        (decider, status)
    }
}

impl Decider for ReplayDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let mut progress = self.progress.lock().expect("replay progress lock");
        // Check the latch before doing any matching. An engine path may catch a
        // failed answer and ask another question within the same step; even if
        // that later question matches, it must not restart this failed attempt.
        // The latch prevents further answers; the caller discarding the trial
        // fork prevents partial changes from being committed.
        let result = if let Some(reason) = progress.stop_reason {
            Err(reason)
        } else if let Some(expected) = progress.remaining.front() {
            if !(if self.semantic_planning {
                expected.matches_replay_question(choice)
            } else {
                expected.matches_question(choice)
            }) {
                Err(ReplayStopReason::QuestionMismatch)
            } else {
                let mut matching = choice.options.iter().filter(|option| {
                    if self.semantic_planning {
                        expected.matches_replay_option(option)
                    } else {
                        option.id == expected.option_id
                            && option.kind == expected.kind
                            && option.payload == expected.payload
                    }
                });
                match (matching.next(), matching.next()) {
                    // Clone the fresh offered option, not the saved instruction.
                    (Some(option), None) => Ok(option.clone()),
                    (None, _) => Err(ReplayStopReason::SelectionMissing),
                    _ => Err(ReplayStopReason::SelectionAmbiguous),
                }
            }
        } else {
            Err(ReplayStopReason::Exhausted)
        };

        match result {
            Ok(option) => {
                // Consume only after both the question and selection match.
                // This records attempt progress, not a batch commit.
                progress.remaining.pop_front();
                progress.consumed += 1;
                Ok(option)
            }
            Err(reason) => {
                // Preserve the failing answer and the rest of the script. This
                // status remains authoritative even if the engine catches the
                // error below and returns a successful StepResult.
                progress.stop_reason = Some(reason);
                Err(IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason: format!("planning replay stopped: {reason:?}"),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::decision_context::DecisionSource;
    use ti4_model::state::Phase;

    #[test]
    fn semantic_movement_rebinds_indexes_without_substituting_conditions_boosts_or_ambiguous_units()
    {
        let player = PlayerId::new("b");
        let option = ChoiceOption::new("move|origin|0", "move")
            .with("origin", "origin")
            .with("unit", "carrier")
            .with("damaged", false)
            .with("capacity", 4)
            .with("gravity_drive", false);
        let question = Choice::new(player.clone(), "movement", vec![option.clone()])
            .contextualized(DecisionContext::new(
                player,
                DecisionSource::Rule("89.2".into()),
                "movement_step",
                Phase::Action,
                1,
            ));
        let recorded = RecordedDecision::from_answer(&question, &option);
        let mut fresh = option.clone();
        fresh.id = "move|origin|2".into();
        let mut offered = question.clone();
        offered.options = vec![fresh.clone()];
        assert_eq!(recorded.replay_option(&offered), Some(fresh.clone()));
        assert_eq!(
            recorded.offered_option(&offered),
            None,
            "live application stays exact"
        );
        for changed in [
            fresh.clone().with("damaged", true),
            fresh.clone().with("gravity_drive", true),
            fresh.clone().with("ionian", true),
            fresh.clone().with("unit", "carrier2"),
            fresh.clone().with("origin", "elsewhere"),
        ] {
            offered.options = vec![changed];
            assert!(recorded.replay_option(&offered).is_none());
        }
        offered.options = vec![fresh, option];
        let (mut replay, progress) = ReplayDecider::for_planning(vec![recorded]);
        assert!(replay.choose(&offered).is_err());
        let progress = progress.lock().unwrap();
        assert_eq!(
            progress.stop_reason(),
            Some(ReplayStopReason::SelectionAmbiguous)
        );
        assert_eq!(progress.remaining().len(), 1);
    }
}
