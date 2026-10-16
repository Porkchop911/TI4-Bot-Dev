//! The undo of a seat: what a player may take back, and how far one undo goes back (one answer,
//! or a movement, the landings or a purchase whole).
//!
//! Both hosts keep the answers that may still be taken back in an [`UndoTrack`]. They differ
//! only in what cuts it ([`UndoRules`]):
//!
//! - [`UndoRules::Table`] (the server): a seat may undo its own decisions back to, but not past,
//!   the first of: something hidden that was shown, a random result, or a decision of another
//!   seat ([`UndoBarrier`]). The server keeps the decisions after the last barrier while the
//!   game is played, and so while its log is replayed: a game that is recovered knows them as
//!   the game that never stopped.
//! - [`UndoRules::Local`] (the browser host): only a change of the seats that the page plays
//!   cuts. A checkpoint restores the dice, so an undo cannot be used to roll again, and the
//!   page plays every seat that it answers for; an undo goes back over what other seats decided.
//!   The entries keep the number that the play loop gave each answer (`asked`), which the
//!   checkpoint reach needs.
//!
//! Under the table rules a barrier is found in two ways, and both are needed. The engine latches
//! a die, a shuffle and a look at hidden cards ([`ExecutionObservation`]); that is an event,
//! which no view shows. A card that is drawn from a deck in order sets no latch: only the view of
//! the seat before and after its decision shows it ([`learned_identity`]).

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::observation::ExecutionObservation;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::planning::audit::learned_identity;
use crate::planning::steps::PlanStep;
use crate::projection::project_game_view;
use crate::protocol::status::{UndoBarrier, ViewerRole};
use crate::protocol::view::GameView;
use crate::view::reaction_modes::{Filtered, Modes, filter};

/// One answer of a seat, as an undo reads it: what was asked, and what was chosen.
pub trait Answered {
    /// The subtype of the context of the choice (`""` without one).
    fn subtype(&self) -> &str;
    /// The kind of the chosen option.
    fn kind(&self) -> &str;
}

impl Answered for (Choice, ChoiceOption) {
    fn subtype(&self) -> &str {
        self.0
            .context
            .as_ref()
            .map_or("", |context| context.subtype.as_str())
    }

    fn kind(&self) -> &str {
        &self.1.kind
    }
}

/// Whether the choice at `at` is in the middle of a movement (a ship has moved, or a hold is
/// open), of the landings (the answer before it landed a unit) or of a production (a purchase
/// that is being paid and placed). An undo that opens such a choice again goes on: a movement
/// is taken back whole.
pub fn inside<A: Answered>(tail: &[A], at: usize) -> bool {
    let before = at.checked_sub(1).map(|index| &tail[index]);
    match tail[at].subtype() {
        "load_cargo" => true,
        "movement_step" => before.is_some_and(|answer| {
            answer.kind() == ti4_engine::tactical::MOVE_KIND || answer.subtype() == "load_cargo"
        }),
        "commit_ground_forces" => {
            before.is_some_and(|answer| answer.kind() == ti4_engine::invasion::COMMIT_KIND)
        }
        "produce_unit" | "pay_resources" | "place_unit" => tail[..at]
            .iter()
            .rev()
            .find(|answer| {
                answer.kind() != ti4_engine::production::PAY_KIND
                    && answer.kind() != ti4_engine::production::PLACE_KIND
            })
            .is_some_and(|answer| answer.kind() == ti4_engine::production::PRODUCE_KIND),
        _ => false,
    }
}

/// The place of the first answer that one undo takes back, at `fence` or later. An answer for
/// which `passed` holds is no answer of the player (a window that the game declines for the
/// Never cards of the seat): the game would decline it again, so the undo goes back over it to
/// the answer before.
pub fn start<A: Answered>(tail: &[A], fence: usize, passed: impl Fn(&A) -> bool) -> Option<usize> {
    let mut found = None;
    for at in (fence..tail.len()).rev() {
        if passed(&tail[at]) {
            continue;
        }
        found = Some(at);
        if !inside(tail, at) {
            break;
        }
    }
    found
}

/// What cuts the answers that an undo may take back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoRules {
    /// The server: another seat, a die, a shuffle and a revealed card cut.
    Table,
    /// The browser host: only a change of the page's seats cuts ([`UndoTrack::seats_changed`]).
    Local,
}

/// An answer of a seat that a person plays, as the track keeps it.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    /// The seat that answered.
    pub seat: PlayerId,
    /// Its place in the decision log of the game.
    pub decision: usize,
    /// The number that the play loop of the browser host gave the answer (0 on the server).
    pub asked: u64,
    /// The subtype of the context of the choice (`""` without one).
    pub subtype: String,
    /// The kind of the chosen option.
    pub kind: String,
    /// The game declined the window for the Never cards of the seat: no answer of the player.
    pub passed: bool,
    /// The step of a movement, the landings or a production that the answer was.
    pub step: Option<PlanStep>,
    /// Table rules: a choice that the seat may decline, read against its Never cards as they
    /// are at the undo.
    window: Option<Choice>,
}

impl UndoEntry {
    /// The answer `option` to `choice`, the decision at `decision` of the log.
    #[must_use]
    pub fn new(choice: &Choice, option: &ChoiceOption, decision: usize) -> Self {
        Self {
            seat: choice.player.clone(),
            decision,
            asked: 0,
            subtype: choice
                .context
                .as_ref()
                .map(|context| context.subtype.clone())
                .unwrap_or_default(),
            kind: option.kind.clone(),
            passed: false,
            step: PlanStep::of_option(option),
            window: choice
                .options
                .iter()
                .any(ChoiceOption::is_decline)
                .then(|| choice.clone()),
        }
    }

    /// An answer that is known only by its record: no option, no window.
    #[must_use]
    pub fn recorded(seat: PlayerId, decision: usize, subtype: String) -> Self {
        Self {
            seat,
            decision,
            asked: 0,
            subtype,
            kind: String::new(),
            passed: false,
            step: None,
            window: None,
        }
    }

    /// With the number that the play loop gave the answer.
    #[must_use]
    pub fn asked(mut self, asked: u64) -> Self {
        self.asked = asked;
        self
    }

    /// Whether the game declined the window for the Never cards (the browser host knows it when
    /// the answer is given).
    #[must_use]
    pub fn passed(mut self, passed: bool) -> Self {
        self.passed = passed;
        self
    }

    /// Whether the undo goes back over this answer to the one before it: the game declined
    /// the window, or would decline it now for the Never cards `never`.
    fn passed_for(&self, never: &Modes) -> bool {
        self.passed
            || self
                .window
                .as_ref()
                .is_some_and(|choice| matches!(filter(choice, never), Filtered::Declined { .. }))
    }
}

impl Answered for UndoEntry {
    fn subtype(&self) -> &str {
        &self.subtype
    }

    fn kind(&self) -> &str {
        &self.kind
    }
}

/// The decisions of a game that its seats may still take back.
pub struct UndoTrack {
    rules: UndoRules,
    /// The latch of the game (table rules). It is read and cleared at each look.
    observation: ExecutionObservation,
    /// How many decisions of the game were answered.
    count: usize,
    /// Who answered the last of them, when the track was told.
    last: Option<PlayerId>,
    /// The answers of the seats that a person plays, in order. Table rules: all of one seat,
    /// and no barrier lies between them or after them. Local rules: every answer of the page.
    entries: Vec<UndoEntry>,
    /// Local rules: an answer before this decision stays (the last change of the page's seats).
    since: usize,
    /// Why the decision before the entries cannot be taken back. `None` before the first barrier.
    barrier: Option<UndoBarrier>,
    /// The game at the last look.
    base: Option<GameState>,
    /// The view of the seat of the entries at the last look.
    known: Option<(PlayerId, GameView)>,
}

impl UndoTrack {
    #[must_use]
    pub fn new(rules: UndoRules) -> Self {
        Self {
            rules,
            observation: ExecutionObservation::default(),
            count: 0,
            last: None,
            entries: Vec::new(),
            since: 0,
            barrier: None,
            base: None,
            known: None,
        }
    }

    /// The latch to bind to the game (table rules).
    #[must_use]
    pub fn observation(&self) -> ExecutionObservation {
        self.observation.clone()
    }

    /// How many decisions of the game are known here.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// The answers kept, in order (local rules: every answer of the page).
    #[must_use]
    pub fn entries(&self) -> &[UndoEntry] {
        &self.entries
    }

    fn cut(&mut self, barrier: UndoBarrier) {
        self.entries.clear();
        self.barrier = Some(barrier);
        self.known = None;
    }

    fn owner(&self) -> Option<&PlayerId> {
        self.entries.first().map(|entry| &entry.seat)
    }

    /// A seat answered `choice`, the next decision of the game. Only the answers of a seat that
    /// a person plays are kept.
    pub fn answered(&mut self, choice: &Choice, option: &ChoiceOption, human: bool) {
        self.push(UndoEntry::new(choice, option, self.count), human);
    }

    /// A seat gave the answer `entry`. Table rules: an answer of a seat that no person plays,
    /// or of another seat than the entries', cuts. Local rules: nothing cuts here.
    pub fn push(&mut self, mut entry: UndoEntry, human: bool) {
        if self.rules == UndoRules::Table
            && (!human || self.owner().is_some_and(|owner| owner != &entry.seat))
        {
            self.cut(UndoBarrier::OtherSeat);
        }
        self.count = entry.decision + 1;
        self.last = Some(entry.seat.clone());
        if human {
            if self.rules == UndoRules::Local {
                entry.window = None;
            }
            self.entries.push(entry);
        }
    }

    /// Local rules: from the decision `decision` on, the page plays other seats. An answer from
    /// before it is not taken back.
    pub fn seats_changed(&mut self, decision: usize) {
        self.since = decision;
    }

    /// Local rules: the game went back to a checkpoint where the page had given `keep`
    /// answers. Returns the answers that went, in order.
    pub fn take_back(&mut self, keep: usize) -> Vec<UndoEntry> {
        self.entries.split_off(keep.min(self.entries.len()))
    }

    /// Table rules: looks at the game where it asks a seat, and after each of its steps:
    /// `decisions` is the length of its log. What happened since the last look is a barrier
    /// when the engine latched it, or when the seat of the entries knows a card now that it did
    /// not know. Local rules: nothing to look at (a checkpoint restores the dice).
    pub fn observe(&mut self, decisions: usize, state: &GameState) {
        if self.rules == UndoRules::Local {
            return;
        }
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

    /// The place in [`Self::entries`] of the first answer that one undo of `seat` takes back
    /// ([`start`]). `never` is the Never cards of the seat as they are now: a window that the
    /// game declines for them is no answer of the player, so the undo goes back over it to the
    /// answer before. Table rules: only the seat of the entries may undo. Local rules: the page
    /// takes back its last answer, of whichever of its seats.
    #[must_use]
    pub fn start(&self, seat: &PlayerId, never: &Modes) -> Option<usize> {
        if self.rules == UndoRules::Table && self.owner() != Some(seat) {
            return None;
        }
        let fence = self
            .entries
            .partition_point(|entry| entry.decision < self.since);
        start(&self.entries, fence, |entry| entry.passed_for(never))
    }

    /// Whether `seat` has a decision to take back.
    #[must_use]
    pub fn can_undo(&self, seat: &PlayerId, never: &Modes) -> bool {
        self.start(seat, never).is_some()
    }

    /// The steps of a plan that an undo from the entry at `start` takes back, in order.
    #[must_use]
    pub fn steps_from(&self, start: usize) -> Vec<PlanStep> {
        self.entries[start..]
            .iter()
            .filter_map(|entry| entry.step.clone())
            .collect()
    }

    /// What one undo of `seat` does: how many decisions the game keeps, and the steps of a
    /// plan that are taken back, in order.
    ///
    /// # Errors
    ///
    /// The barrier that the last decision of the game lies behind for this seat; `None` when
    /// the game has no decision to take back.
    pub fn plan(
        &self,
        seat: &PlayerId,
        never: &Modes,
    ) -> Result<(usize, Vec<PlanStep>), Option<UndoBarrier>> {
        let Some(start) = self.start(seat, never) else {
            // The last decision is of another seat, whatever was rolled after it.
            return Err(if self.count > 0 && self.last.as_ref() != Some(seat) {
                Some(UndoBarrier::OtherSeat)
            } else {
                self.barrier
            });
        };
        Ok((self.entries[start].decision, self.steps_from(start)))
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
        let mut track = UndoTrack::new(UndoRules::Table);
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
        let none = Modes::new();
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
        let none = Modes::new();
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
        let never: Modes =
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
        assert_eq!(track_of.plan(&seat(), &Modes::new()).unwrap().0, 1);
        // A window alone is nothing to take back.
        let alone = track(&[window]);
        assert!(!alone.can_undo(&seat(), &never));
        assert_eq!(alone.plan(&seat(), &never), Err(None));
    }

    #[test]
    fn a_decision_of_another_seat_a_die_and_a_reveal_are_barriers() {
        let none = Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let other = asked("b", "activate", ChoiceOption::new("19", "system"));

        // Another person decided: its seat may undo, this seat may not.
        let mut track = UndoTrack::new(UndoRules::Table);
        track.answered(&own.0, &own.1, true);
        track.answered(&other.0, &other.1, true);
        assert_eq!(
            track.plan(&seat(), &none),
            Err(Some(UndoBarrier::OtherSeat))
        );
        assert_eq!(track.plan(&PlayerId::new("b"), &none), Ok((1, vec![])));

        // A seat that the server plays decided.
        let mut track = UndoTrack::new(UndoRules::Table);
        track.answered(&own.0, &own.1, true);
        track.answered(&other.0, &other.1, false);
        assert_eq!(
            track.plan(&seat(), &none),
            Err(Some(UndoBarrier::OtherSeat))
        );
        assert_eq!(track.count(), 2);

        // The latch of the engine, read at the next look. It is clear after it.
        for (latch, barrier) in [(true, UndoBarrier::Random), (false, UndoBarrier::Revealed)] {
            let mut track = UndoTrack::new(UndoRules::Table);
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
        let mut track = UndoTrack::new(UndoRules::Table);
        track.answered(&own.0, &own.1, true);
        track.observe(2, &state());
        assert!(!track.can_undo(&seat(), &none));
    }

    #[test]
    fn a_card_that_the_seat_holds_after_its_decision_is_a_barrier() {
        let none = Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let mut before = state();
        let mut track = UndoTrack::new(UndoRules::Table);
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

    /// The answers of the page under the local rules, at the given places of the log.
    fn local(answers: &[((Choice, ChoiceOption), usize)]) -> UndoTrack {
        let mut track = UndoTrack::new(UndoRules::Local);
        for (asked_number, ((choice, option), decision)) in (1_u64..).zip(answers) {
            track.push(
                UndoEntry::new(choice, option, *decision).asked(asked_number),
                true,
            );
        }
        track
    }

    #[test]
    fn locally_a_die_a_shuffle_and_another_seat_do_not_cut() {
        let none = Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let other = asked("c", "activate", ChoiceOption::new("19", "system"));
        let bot = asked("b", "activate", ChoiceOption::new("20", "system"));
        let mut track = local(&[(own.clone(), 0)]);
        track.observe(0, &state());
        // A seat that the page does not play decides, and the engine rolls and shuffles.
        track.push(UndoEntry::new(&bot.0, &bot.1, 1), false);
        track.observation().randomness();
        track.observation().hidden_information();
        track.observe(5, &state());
        assert_eq!(track.start(&seat(), &none), Some(0));
        // Another seat of the page answers (hot seat): one undo takes that answer back,
        // whichever seat is shown.
        track.push(UndoEntry::new(&other.0, &other.1, 6).asked(2), true);
        assert_eq!(track.start(&seat(), &none), Some(1));
        assert_eq!(track.start(&PlayerId::new("c"), &none), Some(1));
        assert_eq!(track.entries()[1].asked, 2);
        assert_eq!(track.entries()[1].decision, 6);
    }

    #[test]
    fn locally_a_change_of_the_seats_of_the_page_cuts() {
        let none = Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let mut track = local(&[(own.clone(), 0), (own.clone(), 2)]);
        assert_eq!(track.start(&seat(), &none), Some(1));
        // From decision 3 on the page plays seat c in place of seat a.
        track.seats_changed(3);
        assert_eq!(track.start(&seat(), &none), None);
        let other = asked("c", "activate", ChoiceOption::new("19", "system"));
        track.push(UndoEntry::new(&other.0, &other.1, 4), true);
        assert_eq!(track.start(&PlayerId::new("c"), &none), Some(2));
        // The answers before the change stay in the track: the history counts them.
        assert_eq!(track.entries().len(), 3);
        // A checkpoint before the last answer takes it out.
        let gone = track.take_back(2);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].seat, PlayerId::new("c"));
        assert_eq!(track.start(&PlayerId::new("c"), &none), None);
    }

    #[test]
    fn locally_a_window_declined_for_a_never_card_goes_with_the_answer_before_it() {
        let none = Modes::new();
        let own = asked("a", "activate", ChoiceOption::new("18", "system"));
        let window = asked("a", "window", ChoiceOption::decline());
        let mut track = local(&[(own, 0)]);
        track.push(UndoEntry::new(&window.0, &window.1, 1).passed(true), true);
        assert_eq!(track.start(&seat(), &none), Some(0));
    }
}
