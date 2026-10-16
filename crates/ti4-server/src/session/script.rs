//! A script of a seat: run as a draft on a copy of the game, or sent to the live game.
//!
//! A script is what the page of a player keeps of a draft or of a staged step
//! ([`ti4_host::planning::draft::DraftStep`]). The server keeps none: a run plays the whole
//! script on a copy and forgets it ([`run_draft`]), and a script that is sent lives only while
//! the game answers its questions from it ([`ti4_host::planning::script::ScriptRuns`]). Here the
//! server tells each seat what became of its script. The check of a script on a copy before it
//! is sent is [`ti4_host::planning::script::check_script`].

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_host::planning::draft::DraftStep;
use ti4_host::planning::script::Ended;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::server::{DraftAppliedMsg, ServerMessage};
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;

use crate::session::worker::SessionShared;

impl SessionShared {
    /// Tells the connections of each seat what became of its script.
    fn tell_scripts(&mut self, ended: Vec<Ended>) {
        for end in ended {
            let message = ServerMessage::DraftApplied(DraftAppliedMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.game_id.clone(),
                nonce: end.nonce,
                stopped: end.stopped,
            });
            self.subscribers.retain(|_, subscriber| {
                !matches!(&subscriber.viewer, ViewerRole::Player(viewer) if *viewer == end.seat)
                    || subscriber.tx.try_send(message.clone()).is_ok()
            });
        }
    }

    /// Begins to answer `seat` from `script`, whose first answer is given by the caller: `next`
    /// and `script` are as [`ti4_host::planning::draft::script_answer`] left them after it.
    pub(crate) fn begin_script(
        &mut self,
        seat: &PlayerId,
        nonce: &str,
        script: Vec<DraftStep>,
        next: usize,
    ) {
        let active = self.latest_state.active.clone();
        let ended = self
            .scripts
            .begin(seat, nonce, script.clone(), script, next, active);
        self.tell_scripts(ended);
    }

    /// The answer of the script of the asked seat to `choice`, when it has one
    /// ([`ti4_host::planning::script::ScriptRuns::answer`]).
    pub(crate) fn script_option(&mut self, choice: &Choice) -> Option<ChoiceOption> {
        let (option, ended) = self.scripts.answer(choice);
        self.tell_scripts(ended);
        option
    }

    /// A step of the game is complete: a script whose action or turn is over ends.
    pub(crate) fn scripts_after_step(&mut self, tactical_complete: bool) {
        let active = self.latest_state.active.clone();
        let ended = self.scripts.after_step(tactical_complete, active.as_ref());
        self.tell_scripts(ended);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::mpsc;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_host::planning::steps::PlanStep;
    use ti4_model::state::{GameState, Phase};

    use crate::session::worker::Subscriber;

    fn shared() -> SessionShared {
        let ids = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = GameState::new(&ids, &[], BTreeMap::new(), None, 1);
        state.active = Some(PlayerId::new("a"));
        SessionShared::new("g".to_owned(), state)
    }

    fn listen(shared: &mut SessionShared, id: u64, seat: &str) -> mpsc::Receiver<ServerMessage> {
        let (tx, rx) = mpsc::sync_channel(8);
        let viewer = ViewerRole::Player(PlayerId::new(seat));
        shared.subscribers.insert(id, Subscriber { viewer, tx });
        rx
    }

    fn told(rx: &mpsc::Receiver<ServerMessage>) -> Option<DraftAppliedMsg> {
        match rx.try_recv() {
            Ok(ServerMessage::DraftApplied(message)) => Some(message),
            Ok(other) => panic!("another message: {other:?}"),
            Err(_) => None,
        }
    }

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

    #[test]
    fn a_script_answers_its_seat_and_is_told_only_to_that_seat_when_it_ends() {
        let mut shared = shared();
        let a = listen(&mut shared, 1, "a");
        let b = listen(&mut shared, 2, "b");
        let seat = PlayerId::new("a");
        // The first answer was given by the caller.
        shared.begin_script(&seat, "n1", script(), 1);
        assert_eq!(told(&a), None, "entries are left");

        let system = asked("a", "activate_system", &["18", "19"]);
        assert_eq!(
            shared.script_option(&system).map(|o| o.id),
            Some("18".into())
        );
        let moving = asked("a", "movement_step", &["done_moving"]);
        assert_eq!(
            shared.script_option(&moving).map(|o| o.id),
            Some("done_moving".into())
        );
        let end = told(&a).expect("the seat is told");
        assert_eq!((end.nonce.as_str(), end.stopped), ("n1", None));
        assert_eq!(told(&b), None, "another seat is told nothing");
        // The script is gone: the next question is of the player.
        assert_eq!(shared.script_option(&moving), None);
        assert_eq!(told(&a), None);
    }

    #[test]
    fn a_script_with_one_entry_is_played_by_its_first_answer() {
        let mut shared = shared();
        let a = listen(&mut shared, 1, "a");
        shared.begin_script(
            &PlayerId::new("a"),
            "n1",
            vec![DraftStep::choose("pass")],
            1,
        );
        assert_eq!(told(&a).expect("told").stopped, None);
        assert!(shared.scripts.is_empty());
    }
}
