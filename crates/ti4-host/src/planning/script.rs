//! Scripts that are sent to the live game: each seat's questions are answered from its script.
//!
//! A script is what the page of a player keeps of a draft or of a staged step ([`DraftStep`]).
//! A host keeps it only while the game answers its seat from it ([`ScriptRuns`]); one function
//! answers a question from a script, in a run on a copy and in the live game:
//! [`script_answer`]. The host sends what [`ScriptRuns`] reports as [`Ended`] to the seat
//! (`ServerMessage::DraftApplied`). Before a host sends a script, it plays it on a copy of the
//! game ([`check_script`]): a script that the game refuses is refused before anything of it is
//! in the game.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, SeatObservation, Table,
};
use ti4_engine::game::Game;
use ti4_model::id::PlayerId;

use super::draft::{DraftStep, ScriptAnswer, needless, script_answer};
use super::fork::TACTICAL_COMPLETE;
use super::steps::INTERRUPTED;
use crate::protocol::server::ScriptStopped;
use crate::view::reaction_modes::{Filtered, Modes, filter};

/// What the game says when it ends an action that a script still has entries for.
pub const ACTION_OVER: &str = "The action is over before this.";

/// What the game says when the first question is not one the script answers.
pub const NO_ANSWER: &str = "The script has no answer for this choice.";

/// Whether `events` has the end of a tactical action.
#[must_use]
pub fn tactical_complete(events: &[String]) -> bool {
    events.iter().any(|event| event == TACTICAL_COMPLETE)
}

/// A script that changes nothing: the game refuses it before its first answer.
#[must_use]
pub fn refused(script: &[DraftStep], reason: String) -> ScriptStopped {
    ScriptStopped {
        applied: 0,
        remaining: script.to_vec(),
        interrupted: false,
        reason,
    }
}

/// The first answer of a script that is sent: the answer to the choice that the game waits in.
#[derive(Debug, Clone)]
pub struct First {
    pub option: ChoiceOption,
    /// The first entry that is still to play.
    pub next: usize,
    /// The script in the order it goes on in (a payment may change its place).
    pub script: Vec<DraftStep>,
}

/// The answer of `script` to `choice` of `seat`, the choice that the game waits in. `Err` is why
/// the game refuses the script: nothing of it is in the game.
pub fn first_answer(
    script: &[DraftStep],
    choice: &Choice,
    seat: &PlayerId,
) -> Result<First, String> {
    match script_answer(script, 0, choice, seat) {
        ScriptAnswer::Answer {
            option,
            next,
            script: reordered,
        } => Ok(First {
            option,
            next,
            script: reordered.unwrap_or_else(|| script.to_vec()),
        }),
        ScriptAnswer::Done => Err(NO_ANSWER.to_owned()),
        ScriptAnswer::Interrupted { reason } | ScriptAnswer::Mismatch { reason, .. } => Err(reason),
    }
}

/// What a host does when the game does not offer what an entry of a running script asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnMismatch {
    /// The entries before it are in the game and stay there (the server: other people saw them).
    Stop,
    /// The whole script is taken back: the host goes back to the choice it began at (a local
    /// game, whose checkpoint restores the dice).
    TakeBack,
}

/// A script that is being sent to the live game.
#[derive(Debug, Clone)]
struct ScriptRun {
    /// The nonce of the choice that the script began with, to name the script in its result.
    nonce: String,
    /// The script as it was sent: what a refusal says is not in the game.
    sent: Vec<DraftStep>,
    /// A payment may change its place among the steps around it ([`script_answer`]).
    script: Vec<DraftStep>,
    /// How many entries are in the game.
    next: usize,
    /// Whose turn it was. The script does not outlive that turn.
    active: Option<PlayerId>,
}

impl ScriptRun {
    fn stopped(&self, reason: &str) -> ScriptStopped {
        ScriptStopped {
            applied: self.next,
            remaining: self.script[self.next..].to_vec(),
            interrupted: true,
            reason: if reason.is_empty() {
                INTERRUPTED
            } else {
                reason
            }
            .to_owned(),
        }
    }

    fn end(self, seat: PlayerId, stopped: Option<ScriptStopped>) -> Ended {
        Ended {
            seat,
            nonce: self.nonce,
            stopped,
            taken_back: false,
        }
    }
}

/// A script that ended: the host tells its seat (`DraftApplied`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub seat: PlayerId,
    /// The nonce of the choice that the script began with.
    pub nonce: String,
    /// What is not in the game; `None` when the script was played whole.
    pub stopped: Option<ScriptStopped>,
    /// The game refused an entry and [`OnMismatch::TakeBack`] holds: the host goes back to the
    /// choice the script began at.
    pub taken_back: bool,
}

/// The scripts that are being sent to the game, by their seat.
#[derive(Debug, Clone)]
pub struct ScriptRuns {
    on_mismatch: OnMismatch,
    runs: BTreeMap<PlayerId, ScriptRun>,
}

impl ScriptRuns {
    #[must_use]
    pub const fn new(on_mismatch: OnMismatch) -> Self {
        Self {
            on_mismatch,
            runs: BTreeMap::new(),
        }
    }

    /// Whether a script of `seat` is running.
    #[must_use]
    pub fn running(&self, seat: &PlayerId) -> bool {
        self.runs.contains_key(seat)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Forgets every script, with nothing told (the game is gone or goes back).
    pub fn clear(&mut self) {
        self.runs.clear();
    }

    /// Begins to answer `seat` from `script`, whose first answer the caller gives: `next` and
    /// `script` are as [`script_answer`] left them after it. `sent` is the script as it was sent.
    /// A script that the first answer played whole ends at once.
    pub fn begin(
        &mut self,
        seat: &PlayerId,
        nonce: &str,
        sent: Vec<DraftStep>,
        script: Vec<DraftStep>,
        next: usize,
        active: Option<PlayerId>,
    ) -> Vec<Ended> {
        if next >= script.len() {
            return vec![Ended {
                seat: seat.clone(),
                nonce: nonce.to_owned(),
                stopped: None,
                taken_back: false,
            }];
        }
        self.runs.insert(
            seat.clone(),
            ScriptRun {
                nonce: nonce.to_owned(),
                sent,
                script,
                next,
                active,
            },
        );
        Vec::new()
    }

    /// The answer of the script of the asked seat to `choice`, when it has one, and the scripts
    /// that end. A question that the script does not answer ends it, and so does a question of
    /// another seat that is played by a person (the host asks only for those): the rest of that
    /// script is not in the game.
    pub fn answer(&mut self, choice: &Choice) -> (Option<ChoiceOption>, Vec<Ended>) {
        let seat = &choice.player;
        let others: Vec<PlayerId> = self.runs.keys().filter(|s| *s != seat).cloned().collect();
        let mut ended: Vec<Ended> = others
            .into_iter()
            .filter_map(|other| {
                let run = self.runs.remove(&other)?;
                let stopped = run.stopped(INTERRUPTED);
                Some(run.end(other, Some(stopped)))
            })
            .collect();
        let Some(run) = self.runs.get_mut(seat) else {
            return (None, ended);
        };
        match script_answer(&run.script, run.next, choice, seat) {
            ScriptAnswer::Answer {
                option,
                next,
                script,
            } => {
                run.next = next;
                if let Some(script) = script {
                    run.script = script;
                }
                if run.next >= run.script.len() {
                    let run = self.runs.remove(seat).expect("the script runs");
                    ended.push(run.end(seat.clone(), None));
                }
                (Some(option), ended)
            }
            ScriptAnswer::Done => {
                let run = self.runs.remove(seat).expect("the script runs");
                ended.push(run.end(seat.clone(), None));
                (None, ended)
            }
            ScriptAnswer::Interrupted { reason } => {
                // The entries before this one are in the game and stay there.
                let run = self.runs.remove(seat).expect("the script runs");
                let stopped = run.stopped(&reason);
                ended.push(run.end(seat.clone(), Some(stopped)));
                (None, ended)
            }
            ScriptAnswer::Mismatch { reason, .. } => {
                let run = self.runs.remove(seat).expect("the script runs");
                let end = match self.on_mismatch {
                    OnMismatch::Stop => {
                        let stopped = run.stopped(&reason);
                        run.end(seat.clone(), Some(stopped))
                    }
                    OnMismatch::TakeBack => {
                        let stopped = refused(&run.sent, reason);
                        Ended {
                            taken_back: true,
                            ..run.end(seat.clone(), Some(stopped))
                        }
                    }
                };
                ended.push(end);
                (None, ended)
            }
        }
    }

    /// A step of the game is complete, and `active` has the turn. A script whose action is over,
    /// or whose turn is, ends: what it has left is an end that the game did not ask for, or it is
    /// not in the game.
    pub fn after_step(&mut self, tactical_complete: bool, active: Option<&PlayerId>) -> Vec<Ended> {
        let over: Vec<PlayerId> = self
            .runs
            .iter()
            .filter(|(_, run)| tactical_complete || run.active.as_ref() != active)
            .map(|(seat, _)| seat.clone())
            .collect();
        over.into_iter()
            .filter_map(|seat| {
                let run = self.runs.remove(&seat)?;
                let stopped =
                    (!needless(&run.script[run.next..])).then(|| run.stopped(ACTION_OVER));
                Some(run.end(seat, stopped))
            })
            .collect()
    }
}

/// How many engine steps the check of a script takes at most, as a run of a draft.
const STEP_BOUND: usize = 64;

/// What a check of a script on a copy of the game found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// The game takes the script as far as the copy can tell: to its end, to a question that
    /// is not of the script, or to a decision of another seat.
    Plays,
    /// The game does not offer what entry `step` asks for.
    Refused { step: usize, reason: String },
    /// The copy did not come to the choice that the live game waits in.
    Unknown,
}

struct Probe {
    /// The decisions of the live game since the copy was made, which the copy plays again.
    tail: VecDeque<DecisionRecord>,
    /// The choice that the live game waits in. `None` once the copy came to it.
    pending: Option<Choice>,
    actor: PlayerId,
    /// The cards that the actor is never offered: the live game declines a window of such
    /// cards before the seat, and so before its script, is asked.
    never: Modes,
    script: Vec<DraftStep>,
    next: usize,
    verdict: Option<Check>,
}

impl Probe {
    fn answer(&mut self, choice: &Choice) -> Option<ChoiceOption> {
        if self.verdict.is_some() {
            return None;
        }
        if let Some(record) = self.tail.pop_front() {
            let option = (record.player == choice.player)
                .then(|| choice.option(&record.chosen).cloned())
                .flatten();
            if option.is_none() {
                self.verdict = Some(Check::Unknown);
            }
            return option;
        }
        // From here on the copy asks what the live game asks next: the actor is asked as its
        // seat is, behind its Never cards.
        let narrowed;
        let choice = if choice.player == self.actor {
            match filter(choice, &self.never) {
                Filtered::Declined { option, .. } => return Some(option),
                Filtered::Narrowed(choice) => {
                    narrowed = *choice;
                    &narrowed
                }
                Filtered::Unchanged => choice,
            }
        } else {
            choice
        };
        if let Some(pending) = self.pending.take()
            && (pending.player != choice.player || pending.ids() != choice.ids())
        {
            self.verdict = Some(Check::Unknown);
            return None;
        }
        if choice.player != self.actor {
            // What another seat decides is not known here: the script is fine so far.
            self.verdict = Some(Check::Plays);
            return None;
        }
        match script_answer(&self.script, self.next, choice, &self.actor) {
            ScriptAnswer::Answer {
                option,
                next,
                script,
            } => {
                self.next = next;
                if let Some(script) = script {
                    self.script = script;
                }
                Some(option)
            }
            ScriptAnswer::Done | ScriptAnswer::Interrupted { .. } => {
                self.verdict = Some(Check::Plays);
                None
            }
            ScriptAnswer::Mismatch { step, reason } => {
                self.verdict = Some(Check::Refused { step, reason });
                None
            }
        }
    }
}

struct ProbeDecider(Arc<Mutex<Probe>>);

impl Decider for ProbeDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.0
            .lock()
            .expect("probe lock")
            .answer(choice)
            .ok_or_else(|| IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "the check of the script stops here".to_owned(),
            })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        _seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.choose(choice)
    }
}

/// Plays `script` for `actor` on `game`, a copy of the live game at its last completed step
/// (both hosts: the server's `DraftBasis`, the checkpoint of the wasm play loop).
///
/// The copy first plays `tail` again, the decisions of the live game since that step, and must
/// then ask `pending`, the choice that the live game waits in. From there the script answers.
/// The copy has the dice of the live game, so what it plays is what the live game will play,
/// up to the first decision of another seat. `never` is the Never cards of the actor.
pub fn check_script(
    mut game: Game<'_>,
    tail: Vec<DecisionRecord>,
    pending: &Choice,
    script: &[DraftStep],
    actor: &PlayerId,
    never: Modes,
) -> Check {
    let probe = Arc::new(Mutex::new(Probe {
        tail: tail.into(),
        pending: Some(pending.clone()),
        actor: actor.clone(),
        never,
        script: script.to_vec(),
        next: 0,
        verdict: None,
    }));
    // The table of the timing resolver stays as the copy has it: the live game leaves it alone
    // too, and its answers are not in the decision log.
    game.table = Table::with_default(Box::new(ProbeDecider(probe.clone())));
    let mut events = game.events.len();
    for _ in 0..STEP_BOUND {
        let result = game.step();
        let mut probe = probe.lock().expect("probe lock");
        if probe.verdict.is_some() {
            break;
        }
        if result.error.is_some() || probe.pending.is_some() && probe.tail.is_empty() {
            // The copy failed, or a step ended before the choice of the live game was asked.
            probe.verdict = Some(Check::Unknown);
            break;
        }
        if probe.pending.is_none() && tactical_complete(&game.events[events..]) {
            let left = &probe.script[probe.next..];
            probe.verdict = Some(if needless(left) {
                Check::Plays
            } else {
                Check::Refused {
                    step: probe.next,
                    reason: ACTION_OVER.to_owned(),
                }
            });
            break;
        }
        if result.finished || probe.next >= probe.script.len() {
            probe.verdict = Some(Check::Plays);
            break;
        }
        events = game.events.len();
    }
    drop(game);
    let verdict = probe.lock().expect("probe lock").verdict.clone();
    verdict.unwrap_or(Check::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::steps::PlanStep;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_model::state::Phase;

    /// A question of seat `player` with this subtype and these option ids.
    fn asked(player: &str, subtype: &str, options: &[&str]) -> Choice {
        let options = options
            .iter()
            .map(|id| ChoiceOption::new(*id, "x"))
            .collect();
        Choice::new(PlayerId::new(player), "a question", options).contextualized(
            DecisionContext::new(
                PlayerId::new(player),
                DecisionSource::Rule("test".to_owned()),
                subtype,
                Phase::Action,
                1,
            ),
        )
    }

    fn script() -> Vec<DraftStep> {
        vec![
            DraftStep::choose("tactical"),
            DraftStep::choose("18"),
            DraftStep::Step(PlanStep::DoneMoving),
        ]
    }

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    /// Runs with the script of seat `a` begun at entry `next` (the caller gave the first answers).
    fn running(on_mismatch: OnMismatch, script: Vec<DraftStep>, next: usize) -> ScriptRuns {
        let mut runs = ScriptRuns::new(on_mismatch);
        let ended = runs.begin(&a(), "n1", script.clone(), script, next, Some(a()));
        assert_eq!(ended, Vec::new(), "entries are left");
        runs
    }

    #[test]
    fn a_script_answers_its_seat_and_ends_when_it_is_played() {
        let mut runs = running(OnMismatch::Stop, script(), 1);
        let (option, ended) = runs.answer(&asked("a", "activate_system", &["18", "19"]));
        assert_eq!((option.map(|o| o.id), ended), (Some("18".into()), vec![]));
        let moving = asked("a", "movement_step", &["done_moving"]);
        let (option, ended) = runs.answer(&moving);
        assert_eq!(option.map(|o| o.id), Some("done_moving".into()));
        let end = ended.into_iter().next().expect("the script ended");
        assert_eq!(
            (end.seat, end.nonce.as_str(), end.stopped),
            (a(), "n1", None)
        );
        // The script is gone: the next question is of the player.
        assert_eq!(runs.answer(&moving), (None, vec![]));
        assert!(runs.is_empty());
    }

    #[test]
    fn a_question_that_the_script_does_not_answer_stops_it_with_the_rest() {
        let mut runs = running(OnMismatch::Stop, script(), 1);
        // The game offers other systems than the script wants.
        let (option, ended) = runs.answer(&asked("a", "activate_system", &["19", "20"]));
        assert_eq!(option, None);
        let stopped = ended[0].stopped.clone().expect("the script stopped");
        assert_eq!(stopped.applied, 1);
        assert_eq!(stopped.remaining, script()[1..]);
        assert!(stopped.interrupted, "its first entry is in the game");
        assert!(!ended[0].taken_back);
    }

    #[test]
    fn a_host_that_takes_a_refused_script_back_reports_it_whole() {
        let mut runs = running(OnMismatch::TakeBack, script(), 1);
        let (option, ended) = runs.answer(&asked("a", "activate_system", &["19", "20"]));
        assert_eq!(option, None);
        let stopped = ended[0].stopped.clone().expect("the script stopped");
        assert_eq!((stopped.applied, stopped.interrupted), (0, false));
        assert_eq!(stopped.remaining, script());
        assert!(ended[0].taken_back);
    }

    #[test]
    fn a_question_of_another_played_seat_stops_the_script() {
        let mut runs = running(OnMismatch::Stop, script(), 2);
        let (option, ended) = runs.answer(&asked("b", "reaction", &["sabotage", "decline"]));
        assert_eq!(option, None);
        let stopped = ended[0].stopped.clone().expect("the script stopped");
        assert_eq!((stopped.applied, stopped.interrupted), (2, true));
        assert_eq!(stopped.reason, INTERRUPTED);
        assert_eq!(stopped.remaining, script()[2..]);
    }

    #[test]
    fn a_script_ends_with_its_action_or_its_turn() {
        // An end that the game did not ask for is no loss.
        let mut ending = script();
        ending.push(DraftStep::Step(PlanStep::DoneProducing));
        let mut runs = running(OnMismatch::Stop, ending, 3);
        assert_eq!(
            runs.after_step(false, Some(&a())),
            vec![],
            "the action goes on"
        );
        let ended = runs.after_step(true, Some(&a()));
        assert_eq!(ended[0].stopped, None);

        // An answer that is left when the turn passes is not in the game.
        let mut runs = running(OnMismatch::Stop, script(), 1);
        let ended = runs.after_step(false, Some(&PlayerId::new("b")));
        let stopped = ended[0].stopped.clone().expect("the script stopped");
        assert_eq!((stopped.applied, stopped.reason.as_str()), (1, ACTION_OVER));
    }

    #[test]
    fn a_script_with_one_entry_is_played_by_its_first_answer() {
        let mut runs = ScriptRuns::new(OnMismatch::Stop);
        let pass = vec![DraftStep::choose("pass")];
        let ended = runs.begin(&a(), "n1", pass.clone(), pass, 1, Some(a()));
        assert_eq!(ended[0].stopped, None);
        assert!(runs.is_empty());
    }

    #[test]
    fn the_first_answer_refuses_a_script_that_has_none() {
        let choice = asked("a", "activate_system", &["19"]);
        assert!(first_answer(&[DraftStep::choose("18")], &choice, &a()).is_err());
        assert_eq!(
            first_answer(&[], &choice, &a()).map(|first| first.next),
            Err(NO_ANSWER.to_owned())
        );
    }
}
