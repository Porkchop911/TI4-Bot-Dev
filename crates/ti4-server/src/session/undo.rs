//! The undo of a seat on the server: the track of the session
//! ([`ti4_host::planning::undo::UndoTrack`] under the table rules) is told each answer of a seat,
//! a replayed one too ([`Watched`]). The undo itself replaces the session, as a change of
//! the host's history does (`GameRegistry::undo_player`).

use std::sync::{Arc, Mutex};

use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};

use crate::session::worker::SessionShared;

/// Tells the track of the session each answer of a seat, a replayed one too.
pub(crate) struct Watched {
    pub(crate) inner: Box<dyn Decider>,
    pub(crate) shared: Arc<Mutex<SessionShared>>,
    /// A person plays the seat.
    pub(crate) human: bool,
}

impl Watched {
    fn answered(&self, choice: &Choice, option: &ChoiceOption) {
        self.shared
            .lock()
            .expect("shared lock")
            .undo
            .answered(choice, option, self.human);
    }
}

impl Decider for Watched {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose(choice)?;
        self.answered(choice, &option);
        Ok(option)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose_seeing(choice, seen)?;
        self.answered(choice, &option);
        Ok(option)
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}
