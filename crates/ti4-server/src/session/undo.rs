//! The undo of a seat: what a player may take back without the host.
//!
//! A seat may undo its own decisions back to, but not past, the first of: something hidden that
//! was shown, a random result, or a decision of another seat ([`UndoBarrier`]). The session
//! keeps the decisions after the last barrier ([`UndoTrack`]) while the game is played, and so
//! while its log is replayed: a game that is recovered knows them as the game that never
//! stopped. The undo itself replaces the session, as a change of the host's history does
//! (`GameRegistry::undo_player`).
//!
//! A barrier is found in two ways, and both are needed. The engine latches a die, a shuffle and
//! a look at hidden cards ([`ExecutionObservation`]); that is an event, which no view shows. A
//! card that is drawn from a deck in order sets no latch: only the view of the seat before and
//! after its decision shows it ([`learned_identity`]).

use std::sync::{Arc, Mutex};

use crate::session::reaction_modes::{Filtered, filter};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::observation::ExecutionObservation;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_view::planning::audit::learned_identity;
use ti4_view::planning::steps::PlanStep;
use ti4_view::projection::project_game_view;
use ti4_view::status::{UndoBarrier, ViewerRole};
use ti4_view::view::GameView;

use crate::session::worker::SessionShared;

/// The decisions of a game that its seat may still take back.
pub(crate) struct UndoTrack {
    /// The latch of the game. It is read and cleared at each look.
    observation: ExecutionObservation,
    /// How many decisions of the game were answered.
    count: usize,
    /// Who answered the last of them, when the track was told.
    last: Option<PlayerId>,
    /// The last decisions of the game, with what was asked: all of one seat that a person
    /// plays, and no barrier lies between them or after them.
    tail: Vec<(Choice, ChoiceOption)>,
    /// Why the decision before the tail cannot be taken back. `None` before the first barrier.
    barrier: Option<UndoBarrier>,
    /// The game at the last look.
    base: Option<GameState>,
    /// The view of the seat of the tail at the last look.
    known: Option<(PlayerId, GameView)>,
}

impl UndoTrack {
    pub(crate) fn new() -> Self {
        Self {
            observation: ExecutionObservation::default(),
            count: 0,
            last: None,
            tail: Vec::new(),
            barrier: None,
            base: None,
            known: None,
        }
    }

    /// The latch to bind to the game of the session.
    pub(crate) fn observation(&self) -> ExecutionObservation {
        self.observation.clone()
    }

    /// How many decisions of the game are known here.
    pub(crate) const fn count(&self) -> usize {
        self.count
    }

    fn cut(&mut self, barrier: UndoBarrier) {
        self.tail.clear();
        self.barrier = Some(barrier);
        self.known = None;
    }

    fn owner(&self) -> Option<&PlayerId> {
        self.tail.first().map(|(choice, _)| &choice.player)
    }

    /// A seat answered `choice`. Only the answers of a seat that a person plays are kept.
    fn answered(&mut self, choice: &Choice, option: &ChoiceOption, human: bool) {
        if !human || self.owner().is_some_and(|owner| owner != &choice.player) {
            self.cut(UndoBarrier::OtherSeat);
        }
        self.count += 1;
        self.last = Some(choice.player.clone());
        if human {
            self.tail.push((choice.clone(), option.clone()));
        }
    }

    /// Looks at the game where it asks a seat, and after each of its steps: `decisions` is the
    /// length of its log. What happened since the last look is a barrier when the engine
    /// latched it, or when the seat of the tail knows a card now that it did not know.
    pub(crate) fn observe(&mut self, decisions: usize, state: &GameState) {
        let activity = self.observation.take();
        if decisions != self.count {
            // A decision that no seat of the session answered.
            self.count = decisions;
            self.last = None;
            self.cut(UndoBarrier::OtherSeat);
        }
        if activity.randomness {
            self.cut(UndoBarrier::Random);
        } else if activity.hidden_information {
            self.cut(UndoBarrier::Revealed);
        } else if let Some(owner) = self.owner().cloned() {
            let viewer = ViewerRole::Player(owner.clone());
            let view = project_game_view(state, &viewer);
            let before = match (self.known.take(), &self.base) {
                (Some((seat, known)), _) if seat == owner => Some(known),
                (_, Some(base)) => Some(project_game_view(base, &viewer)),
                (_, None) => None,
            };
            if before.is_some_and(|before| learned_identity(&before, &view)) {
                self.cut(UndoBarrier::Revealed);
            } else {
                self.known = Some((owner, view));
            }
        }
        self.base = Some(state.clone());
    }

    /// The place in the tail of the first answer that one undo of `seat` takes back.
    fn start(
        &self,
        seat: &PlayerId,
        never: &crate::session::reaction_modes::Modes,
    ) -> Option<usize> {
        (self.owner() == Some(seat))
            .then(|| start(&self.tail, never))
            .flatten()
    }

    /// Whether `seat` has a decision to take back. `never` is its Never cards as they are now.
    pub(crate) fn can_undo(
        &self,
        seat: &PlayerId,
        never: &crate::session::reaction_modes::Modes,
    ) -> bool {
        self.start(seat, never).is_some()
    }

    /// What one undo of `seat` does: how many decisions the game keeps, and the steps of a
    /// plan that are taken back, in order.
    ///
    /// # Errors
    ///
    /// The barrier that the last decision of the game lies behind for this seat; `None` when
    /// the game has no decision to take back.
    pub(crate) fn plan(
        &self,
        seat: &PlayerId,
        never: &crate::session::reaction_modes::Modes,
    ) -> Result<(usize, Vec<PlanStep>), Option<UndoBarrier>> {
        let Some(start) = self.start(seat, never) else {
            // The last decision is of another seat, whatever was rolled after it.
            return Err(if self.count > 0 && self.last.as_ref() != Some(seat) {
                Some(UndoBarrier::OtherSeat)
            } else {
                self.barrier
            });
        };
        let steps = self.tail[start..]
            .iter()
            .filter_map(|(_, option)| PlanStep::of_option(option))
            .collect();
        Ok((self.count - self.tail.len() + start, steps))
    }
}

fn subtype(choice: &Choice) -> &str {
    choice
        .context
        .as_ref()
        .map_or("", |context| context.subtype.as_str())
}

/// Whether the choice at `at` is in the middle of a movement (a ship has moved, or a hold is
/// open), of the landings (the answer before it landed a unit) or of a production (a purchase
/// that is being paid and placed). An undo that opens such a choice again goes on: a movement
/// is taken back whole, as a local game takes it back (`insideMovement` of the wasm transport).
fn inside(tail: &[(Choice, ChoiceOption)], at: usize) -> bool {
    let before = at.checked_sub(1).map(|index| &tail[index]);
    match subtype(&tail[at].0) {
        "load_cargo" => true,
        "movement_step" => before.is_some_and(|(choice, option)| {
            option.kind == ti4_engine::tactical::MOVE_KIND || subtype(choice) == "load_cargo"
        }),
        "commit_ground_forces" => {
            before.is_some_and(|(_, option)| option.kind == ti4_engine::invasion::COMMIT_KIND)
        }
        "produce_unit" | "pay_resources" | "place_unit" => tail[..at]
            .iter()
            .rev()
            .find(|(_, option)| {
                option.kind != ti4_engine::production::PAY_KIND
                    && option.kind != ti4_engine::production::PLACE_KIND
            })
            .is_some_and(|(_, option)| option.kind == ti4_engine::production::PRODUCE_KIND),
        _ => false,
    }
}

/// The place of the first answer that one undo takes back. A window that the game declines
/// for the Never cards of the seat is no answer of the player: the game would decline it
/// again, so the undo goes back over it to the answer before.
fn start(
    tail: &[(Choice, ChoiceOption)],
    never: &crate::session::reaction_modes::Modes,
) -> Option<usize> {
    let mut found = None;
    for at in (0..tail.len()).rev() {
        if matches!(filter(&tail[at].0, never), Filtered::Declined { .. }) {
            continue;
        }
        found = Some(at);
        if !inside(tail, at) {
            break;
        }
    }
    found
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_model::state::Phase;

    fn seat() -> PlayerId {
        PlayerId::new("a")
    }

    fn asked(player: &str, subtype: &str, option: ChoiceOption) -> (Choice, ChoiceOption) {
        let player = PlayerId::new(player);
        let choice = Choice::new(player.clone(), "a question", vec![option.clone()])
            .contextualized(DecisionContext::new(
                player,
                DecisionSource::Rule("test".to_owned()),
                subtype,
                Phase::Action,
                1,
            ));
        (choice, option)
    }

    fn ship(unit: &str) -> ChoiceOption {
        ChoiceOption::new(format!("move|{unit}"), "move")
            .with("origin", "18")
            .with("unit", unit)
    }

    fn track(answers: &[(Choice, ChoiceOption)]) -> UndoTrack {
        let mut track = UndoTrack::new();
        for (choice, option) in answers {
            track.answered(choice, option, true);
        }
        track
    }

    fn state() -> GameState {
        let ids = [PlayerId::new("a"), PlayerId::new("b")];
        GameState::new(&ids, &[], std::collections::BTreeMap::new(), None, 1)
    }

    #[test]
    fn one_undo_takes_back_one_answer_and_a_movement_whole() {
        let none = crate::session::reaction_modes::Modes::new();
        let answers = [
            asked("a", "action_menu", ChoiceOption::new("tactical", "action")),
            asked("a", "activate", ChoiceOption::new("18", "system")),
            asked("a", "movement_step", ship("carrier")),
            asked(
                "a",
                "load_cargo",
                ChoiceOption::new("done_loading", "decline"),
            ),
            asked("a", "movement_step", ship("cruiser")),
        ];
        // The activation alone.
        assert_eq!(track(&answers[..2]).plan(&seat(), &none), Ok((1, vec![])));
        // The game asks for the next ship: the undo goes back to the first.
        let (kept, steps) = track(&answers).plan(&seat(), &none).unwrap();
        assert_eq!(kept, 2);
        assert_eq!(
            steps.iter().map(PlanStep::subtype).collect::<Vec<_>>(),
            ["movement_step", "load_cargo", "movement_step"]
        );
        // The game asks the hold of the carrier: the carrier goes back too.
        assert_eq!(track(&answers[..3]).plan(&seat(), &none).unwrap().0, 2);
    }

    #[test]
    fn a_purchase_goes_back_with_what_paid_and_placed_it() {
        let none = crate::session::reaction_modes::Modes::new();
        let answers = [
            asked(
                "a",
                "commit_ground_forces",
                ChoiceOption::new("done_committing", "decline"),
            ),
            asked(
                "a",
                "produce_unit",
                ChoiceOption::new("p", "produce").with("unit", "cruiser"),
            ),
            asked("a", "pay_resources", ChoiceOption::new("trade_good", "pay")),
            asked(
                "a",
                "place_unit",
                ChoiceOption::new("place|space", "place").with("destination", "space"),
            ),
        ];
        // The next purchase is asked: one undo takes the last answer and its purchase.
        let (kept, steps) = track(&answers).plan(&seat(), &none).unwrap();
        assert_eq!(kept, 1);
        assert_eq!(steps.len(), 3);
        // Before a purchase, the end of the landings is one answer.
        assert_eq!(track(&answers[..1]).plan(&seat(), &none).unwrap().0, 0);
    }

    #[test]
    fn a_window_that_is_declined_for_a_never_card_goes_with_the_answer_before_it() {
        let never: crate::session::reaction_modes::Modes =
            std::iter::once(("Sabotage".to_owned(), ti4_model::state::ReactionMode::Never))
                .collect();
        let window = {
            let player = seat();
            let card = ChoiceOption::new("card", ti4_engine::reactions::ACTION_CARD_KIND)
                .with("card_name", "Sabotage");
            let choice = Choice::new(player, "a window", vec![card, ChoiceOption::decline()]);
            (choice, ChoiceOption::decline())
        };
        assert!(matches!(
            filter(&window.0, &never),
            Filtered::Declined { .. }
        ));
        let answers = [
            asked("a", "activate", ChoiceOption::new("18", "system")),
            window.clone(),
        ];
        let track_of = track(&answers);
        assert_eq!(track_of.plan(&seat(), &never).unwrap().0, 0);
        // Asked again, the window is an answer of the player.
        assert_eq!(
            track_of
                .plan(&seat(), &crate::session::reaction_modes::Modes::new())
                .unwrap()
                .0,
            1
        );
        // A window alone is nothing to take back.
        let alone = track(&[window]);
        assert!(!alone.can_undo(&seat(), &never));
        assert_eq!(alone.plan(&seat(), &never), Err(None));
    }

    #[test]
    fn a_decision_of_another_seat_a_die_and_a_reveal_are_barriers() {
        let none = crate::session::reaction_modes::Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let other = asked("b", "activate", ChoiceOption::new("19", "system"));

        // Another person decided: its seat may undo, this seat may not.
        let mut track = UndoTrack::new();
        track.answered(&own.0, &own.1, true);
        track.answered(&other.0, &other.1, true);
        assert_eq!(
            track.plan(&seat(), &none),
            Err(Some(UndoBarrier::OtherSeat))
        );
        assert_eq!(track.plan(&PlayerId::new("b"), &none), Ok((1, vec![])));

        // A seat that the server plays decided.
        let mut track = UndoTrack::new();
        track.answered(&own.0, &own.1, true);
        track.answered(&other.0, &other.1, false);
        assert_eq!(
            track.plan(&seat(), &none),
            Err(Some(UndoBarrier::OtherSeat))
        );
        assert_eq!(track.count(), 2);

        // The latch of the engine, read at the next look. It is clear after it.
        for (latch, barrier) in [(true, UndoBarrier::Random), (false, UndoBarrier::Revealed)] {
            let mut track = UndoTrack::new();
            track.observe(0, &state());
            track.answered(&own.0, &own.1, true);
            if latch {
                track.observation().randomness();
            } else {
                track.observation().hidden_information();
            }
            track.observe(1, &state());
            assert_eq!(track.plan(&seat(), &none), Err(Some(barrier)));
            track.answered(&own.0, &own.1, true);
            track.observe(2, &state());
            assert_eq!(track.plan(&seat(), &none), Ok((1, vec![])));
        }

        // A decision that the track was not told of.
        let mut track = UndoTrack::new();
        track.answered(&own.0, &own.1, true);
        track.observe(2, &state());
        assert!(!track.can_undo(&seat(), &none));
    }

    #[test]
    fn a_card_that_the_seat_holds_after_its_decision_is_a_barrier() {
        let none = crate::session::reaction_modes::Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let mut before = state();
        let mut track = UndoTrack::new();
        track.observe(0, &before);
        track.answered(&own.0, &own.1, true);
        track.observe(1, &before);
        assert!(track.can_undo(&seat(), &none));

        // The seat draws a card: no latch, the deck is in order.
        track.answered(&own.0, &own.1, true);
        before
            .player_mut(&seat())
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("sabotage_1"));
        track.observe(2, &before);
        assert_eq!(track.plan(&seat(), &none), Err(Some(UndoBarrier::Revealed)));

        // A card of another seat is not known to this one: no barrier.
        track.answered(&own.0, &own.1, true);
        before
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("sabotage_2"));
        track.observe(3, &before);
        assert_eq!(track.plan(&seat(), &none), Ok((2, vec![])));
    }
}
