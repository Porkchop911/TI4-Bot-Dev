//! Remote human decider implementation connecting engine decision sites to client channels.

use std::sync::{Arc, Mutex, mpsc};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::id::PlayerId;

use crate::protocol::server::ActionAcceptedMsg;
use crate::protocol::status::RejectionReason;
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

impl Decider for RemoteHumanDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let nonce = format!("{:016x}", rand::random::<u64>());

        {
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
            shared.broadcast_state_update();
            shared.broadcast_pending_decision(choice, &nonce);
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
            if let Some(opt) = choice.options.iter().find(|o| o.id == submission.option_id) {
                let mut shared = self.shared.lock().expect("shared lock");
                let pending = shared
                    .pending_decision
                    .as_mut()
                    .expect("reserved pending decision");
                pending.submitted_option_id = Some(submission.option_id);
                pending.reply_tx = Some(submission.reply_tx);

                return Ok(opt.clone());
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
