//! Remote human decider implementation connecting engine decision sites to client channels.

use std::sync::{Arc, Mutex, mpsc};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_host::clock::NonceSource as _;
use ti4_model::id::PlayerId;

use crate::protocol::server::ActionAcceptedMsg;
use crate::protocol::status::RejectionReason;
use crate::session::ready_secondary;
use crate::session::worker::{PendingDecision, PendingSubmissionState, SessionShared};

/// Incoming choice submission from a client.
#[derive(Debug)]
pub struct ChoiceSubmission {
    pub seat: PlayerId,
    pub nonce: String,
    pub expected_version: u64,
    pub option_id: String,
    pub reply_tx: mpsc::Sender<Result<ActionAcceptedMsg, RejectionReason>>,
}

/// An engine decider that routes choices to a remote human player.
///
/// It generates an opaque nonce, records the pending choice on the session,
/// notifies connected viewers, and blocks on its per-seat inbox until an answer
/// is submitted and validated against the engine-offered options.
pub struct RemoteHumanDecider {
    seat: PlayerId,
    shared: Arc<Mutex<SessionShared>>,
    inbox_rx: mpsc::Receiver<ChoiceSubmission>,
}

impl RemoteHumanDecider {
    #[must_use]
    pub fn new(
        seat: PlayerId,
        _game_id: String,
        shared: Arc<Mutex<SessionShared>>,
        inbox_rx: mpsc::Receiver<ChoiceSubmission>,
    ) -> Self {
        Self {
            seat,
            shared,
            inbox_rx,
        }
    }
}

/// The nonces of a server: random, so no other player can guess the token of a pending choice.
/// (`ti4-host` leaves the source of nonces to its host.)
#[derive(Debug, Clone, Copy, Default)]
pub struct RandomNonce;

impl ti4_host::clock::NonceSource for RandomNonce {
    fn next_nonce(&self) -> String {
        format!("{:016x}", rand::random::<u64>())
    }
}

impl Decider for RemoteHumanDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let nonce = RandomNonce.next_nonce();

        let reached = {
            let mut shared = self.shared.lock().expect("session shared lock");
            shared.game_version += 1;
            if let Some(previous) = shared.pending_decision.take()
                && previous.reply_tx.is_some()
            {
                shared.in_flight_submissions.push_back(previous);
            }
            shared.pending_decision = Some(PendingDecision {
                seat: self.seat.clone(),
                nonce: nonce.clone(),
                game_version: shared.game_version,
                choice: choice.clone(),
                submission_state: PendingSubmissionState::AwaitingSubmission,
                submitted_option_id: None,
                reply_tx: None,
            });
            // A script that the seat sent answers before the seat is asked.
            if let Some(option) = shared.script_option(choice) {
                let (reply_tx, _reply_rx) = mpsc::channel();
                let pending = shared.pending_decision.as_mut().expect("pending decision");
                pending.submission_state = PendingSubmissionState::Reserved;
                pending.submitted_option_id = Some(option.id.clone());
                pending.reply_tx = Some(reply_tx);
                return Ok(option);
            }
            shared.planning.begin_secondary_application(choice);
            if let Some(option) = shared.planning.application_option(choice) {
                let (reply_tx, _reply_rx) = mpsc::channel();
                let pending = shared.pending_decision.as_mut().expect("pending decision");
                pending.submission_state = PendingSubmissionState::Reserved;
                pending.submitted_option_id = Some(option.id.clone());
                pending.reply_tx = Some(reply_tx);
                shared.planning.application_answered(choice, &option);
                return Ok(option);
            }
            // A ready draft of the secondary that the seat is asked is sent by the server. The
            // choice is reserved while the draft is checked on a copy, which needs no lock.
            if ready_secondary::may_send(&shared, &self.seat, choice) {
                let pending = shared.pending_decision.as_mut().expect("pending decision");
                pending.submission_state = PendingSubmissionState::Reserved;
                Some(shared.latest_state.round)
            } else {
                shared.broadcast_state_update();
                shared.broadcast_pending_decision(choice, &nonce);
                None
            }
        };
        if let Some(round) = reached {
            let send = ready_secondary::on_reach(&self.shared, &self.seat, choice, round);
            let mut shared = self.shared.lock().expect("session shared lock");
            let ours = !shared.stopped
                && shared
                    .pending_decision
                    .as_ref()
                    .is_some_and(|pending| pending.nonce == nonce);
            match send {
                Some(send) if ours => {
                    let (reply_tx, _reply_rx) = mpsc::channel();
                    let pending = shared.pending_decision.as_mut().expect("pending decision");
                    pending.submitted_option_id = Some(send.option.id.clone());
                    pending.reply_tx = Some(reply_tx);
                    shared.note_answer(&self.seat, &nonce);
                    shared.begin_script(&self.seat, &nonce, send.script, send.next);
                    let book = shared.staged.clone();
                    drop(shared);
                    ready_secondary::spent(&self.shared, &book, &self.seat, send.revision);
                    return Ok(send.option);
                }
                _ => {
                    if ours {
                        shared
                            .pending_decision
                            .as_mut()
                            .expect("pending decision")
                            .submission_state = PendingSubmissionState::AwaitingSubmission;
                    }
                    shared.broadcast_state_update();
                    shared.broadcast_pending_decision(choice, &nonce);
                }
            }
        }

        while let Ok(submission) = self.inbox_rx.recv() {
            // If the session was stopped externally, abort cleanly
            if self.shared.lock().expect("shared lock").stopped {
                return Err(IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason: "session was stopped".to_owned(),
                });
            }

            // Submission identity and option validity were atomically reserved by GameSession.
            if let Some(opt) = choice.answer_for(&submission.option_id) {
                let mut shared = self.shared.lock().expect("shared lock");
                let pending = shared
                    .pending_decision
                    .as_mut()
                    .expect("reserved pending decision");
                pending.submitted_option_id = Some(submission.option_id);
                pending.reply_tx = Some(submission.reply_tx);
                shared.planning.application_answered(choice, &opt);

                return Ok(opt);
            }

            // Option ID not in the offered set
            let _ = submission
                .reply_tx
                .send(Err(RejectionReason::UnknownOption {
                    option_id: submission.option_id,
                }));
        }

        // Channel disconnected
        Err(IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: "inbox channel closed".to_owned(),
        })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // A nested choice can pause a step before the worker publishes a new state.
        // Keep the public combat boundary current even while the rest of the state
        // waits for that step to finish (notably between victory and invasion).
        let mut shared = self.shared.lock().expect("session shared lock");
        shared.latest_state.active_space_combat = seen.space_battle();
        shared.latest_state.active_invasion = seen.invasion();
        drop(shared);
        self.choose(choice)
    }
}
