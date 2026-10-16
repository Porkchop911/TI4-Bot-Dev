//! A draft of a tactical action or of a secondary, run in one call.
//!
//! A draft is a script: what the planner chose so far. A run plays the whole script on a
//! disposable [`Game::fork`] with a fresh hypothetical turn ([`PlanScope::Tactical`]) or with
//! the window of one follower ([`PlanScope::Secondary`]), and returns where it stopped and
//! what the planner is shown there. Nothing is kept between two runs, so a changed script is run
//! again from its start, and the caller needs no thread and no game of its own. The copy is
//! guarded by [`super::fork`], and what it shows is checked by [`super::audit`].
//!
//! The run covers the whole action: the activation, the movement, the landings of an invasion
//! and the production; or the whole secondary of the planner. It ends where the script does,
//! or where something random lies ahead: dice, a card that is drawn, a combat, a question of
//! another seat.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, Table};
use ti4_engine::game::Game;
use ti4_engine::observation::ExecutionObservation;

use ti4_model::content_types::SourceSet;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use super::audit::{
    PlanScope, SafePublication, StopReason, audit_offer, audit_secondary_offer, validate_knowledge,
};
use super::fork::{TACTICAL_COMPLETE, activity_stop, guard_fork};
use super::steps::{INTERRUPTED, PlanAnswer, PlanStep, plan_answer};
use crate::projection::{FactsContext, SessionUpdate, project_game_view, project_session_update};
use crate::protocol::status::ViewerRole;
use crate::protocol::view::{BoardTileView, GameView};

/// The nonce of the question a draft is open at. A draft is never answered by its nonce.
pub const DRAFT_NONCE: &str = "draft";

/// How many engine steps a run takes at most. A whole tactical action needs a few: most of its
/// questions are asked inside one step.
const STEP_BOUND: usize = 64;

/// An answer by the id of its option: the action of the turn menu, the system that is activated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ChosenOption {
    Choose { option_id: String },
}

/// One entry of a draft script: an answer by its id, or a step of the action ([`PlanStep`]) as
/// the page sends it to the live game. The steps that follow each other are one plan
/// ([`plan_answer`]); an answer by id between them is for what the game asks there (an ability
/// of the planner before the production).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DraftStep {
    Chosen(ChosenOption),
    Step(PlanStep),
}

impl DraftStep {
    /// Answer the open question with the option of this id.
    #[must_use]
    pub fn choose(option_id: impl Into<String>) -> Self {
        Self::Chosen(ChosenOption::Choose {
            option_id: option_id.into(),
        })
    }
}

impl From<PlanStep> for DraftStep {
    fn from(step: PlanStep) -> Self {
        Self::Step(step)
    }
}

/// What a script answers to a question of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptAnswer {
    /// Answer with this option. `next` is the first entry that is still to play. With `script`
    /// the script goes on in this order: a payment changed its place ([`plan_answer`]).
    Answer {
        option: ChoiceOption,
        next: usize,
        script: Option<Vec<DraftStep>>,
    },
    /// Every entry is played: the question is not of the script.
    Done,
    /// The game asks something that the next entry does not answer (a reaction window, another
    /// seat). Entries are left.
    Interrupted { reason: String },
    /// The game does not offer what entry `step` asks for.
    Mismatch { step: usize, reason: String },
}

/// What `script` answers to `choice`, from entry `next` on, for the seat `actor`.
///
/// The one answer of a script to a question, for a run of a draft and for a script that is sent
/// to the live game. The steps that follow each other are one plan ([`plan_answer`]); an answer
/// by id is for whatever the game asks the seat there. The pause between two steps of the action
/// is passed while entries are left.
#[must_use]
pub fn script_answer(
    script: &[DraftStep],
    mut next: usize,
    choice: &Choice,
    actor: &PlayerId,
) -> ScriptAnswer {
    let own = &choice.player == actor;
    let pause = choice
        .context
        .as_ref()
        .is_some_and(|context| context.subtype == "mid_action_pause");
    if own && pause && next < script.len() {
        let passes = choice
            .option(ti4_engine::game::CONTINUE_ACTION_ID)
            .filter(|option| option.kind == ti4_engine::game::CONTINUE_ACTION_KIND);
        if let Some(option) = passes {
            return ScriptAnswer::Answer {
                option: option.clone(),
                next,
                script: None,
            };
        }
    }
    if matches!(script.get(next), Some(DraftStep::Step(_))) {
        // The steps around the next one are one plan.
        let is_step = |entry: &DraftStep| matches!(entry, DraftStep::Step(_));
        let start = script[..next]
            .iter()
            .rposition(|entry| !is_step(entry))
            .map_or(0, |index| index + 1);
        let end = script[next..]
            .iter()
            .position(|entry| !is_step(entry))
            .map_or(script.len(), |offset| next + offset);
        let plan: Vec<PlanStep> = script[start..end]
            .iter()
            .filter_map(|entry| match entry {
                DraftStep::Step(step) => Some(step.clone()),
                DraftStep::Chosen(_) => None,
            })
            .collect();
        match plan_answer(&plan, next - start, choice, actor) {
            PlanAnswer::Answer {
                option,
                next,
                steps,
            } => {
                let script = steps.map(|steps| {
                    let mut order = script.to_vec();
                    order.splice(start..end, steps.into_iter().map(DraftStep::Step));
                    order
                });
                return ScriptAnswer::Answer {
                    option,
                    next: start + next,
                    script,
                };
            }
            // The game needs no more of these steps: what follows them is next.
            PlanAnswer::Done => next = end,
            PlanAnswer::Interrupted { reason } => return ScriptAnswer::Interrupted { reason },
            PlanAnswer::Mismatch { step, reason } => {
                return ScriptAnswer::Mismatch {
                    step: start + step,
                    reason,
                };
            }
        }
    }
    match script.get(next) {
        Some(DraftStep::Chosen(ChosenOption::Choose { option_id })) => {
            if !own {
                return ScriptAnswer::Interrupted {
                    reason: INTERRUPTED.to_owned(),
                };
            }
            match choice.option(option_id) {
                Some(option) => ScriptAnswer::Answer {
                    option: option.clone(),
                    next: next + 1,
                    script: None,
                },
                None => ScriptAnswer::Mismatch {
                    step: next,
                    reason: format!("The game does not offer \"{option_id}\" here."),
                },
            }
        }
        _ => ScriptAnswer::Done,
    }
}

/// Whether the game needs no answer for these entries once the action is over: an end that it
/// did not ask for, a payment that it did not need.
#[must_use]
pub fn needless(entries: &[DraftStep]) -> bool {
    entries.iter().all(|entry| {
        matches!(
            entry,
            DraftStep::Step(
                PlanStep::DoneLoading
                    | PlanStep::DoneCommitting
                    | PlanStep::Pay { .. }
                    | PlanStep::Place { .. }
                    | PlanStep::DoneProducing
            )
        )
    })
}

/// Why a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DraftStop {
    /// The script is used up: the question of the update is open.
    Open,
    /// The game played the whole script, and the tactical action or the secondary is over.
    Complete,
    /// The game does not offer what step `step` of the script asks for.
    Refused { step: usize, reason: String },
    /// The run reached something a draft cannot show: dice, a card that is drawn, a combat, a
    /// question of another seat or one outside the audited offers.
    Stopped { reason: StopReason },
    /// No hypothetical turn can be made from this position, or the engine failed.
    Failed { reason: String },
}

/// Where a run of a script ended, and what the planner is shown there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DraftOutcome {
    /// The position where the run ended, with the open question when the stop is
    /// [`DraftStop::Open`]. The same shape as a live update.
    pub update: SessionUpdate,
    /// The question of the first ship to move, before any ship moved: what a whole movement is
    /// staged against. `None` when the run did not reach the movement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<SessionUpdate>,
    /// The question of the first ground force to land: what the landings are staged against.
    /// `None` when the run did not reach them, or the action has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invasion: Option<SessionUpdate>,
    /// The question of the first unit to produce: what a production is staged against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production: Option<SessionUpdate>,
    /// How many steps of the script the game took, or did not need.
    pub consumed: usize,
    pub stop: DraftStop,
}

/// What a draft is run on: the live game at a completed step, and the map for the facts.
pub struct DraftBase<'a, 'c> {
    pub game: &'a Game<'c>,
    pub content: &'a ContentStore,
    pub sources: SourceSet,
    pub tiles: &'a [BoardTileView],
    pub galaxy: Option<&'a Galaxy>,
    /// The strategic action of the game, as its host follows it: which secondary may be
    /// drafted. Without it no secondary is drafted.
    pub secondary: Option<&'a crate::view::secondary::SecondaryWatch>,
}

/// A question that passed the audit, on the position it was asked in.
#[derive(Clone)]
struct Offer {
    state: GameState,
    choice: Choice,
}

struct Run {
    player: PlayerId,
    scope: PlanScope,
    /// A payment may change its place among the steps around it ([`plan_answer`]).
    script: Vec<DraftStep>,
    /// How many entries of the script are done.
    next: usize,
    stop: Option<DraftStop>,
    baseline: GameView,
    /// The position of the question that is asked now. The offer callback fills it, and the
    /// decider takes it: a question without a position is not shown.
    seen: Option<GameState>,
    last: Option<Offer>,
    movement: Option<Offer>,
    invasion: Option<Offer>,
    production: Option<Offer>,
}

impl Run {
    fn end(&mut self, stop: DraftStop) {
        self.stop.get_or_insert(stop);
    }

    /// The answer of the script to `choice`, or the stop that it runs into.
    fn answer(
        &mut self,
        choice: &Choice,
        observation: &ExecutionObservation,
    ) -> Option<ChoiceOption> {
        let state = self.seen.take();
        if self.stop.is_some() {
            return None;
        }
        let reason = activity_stop(observation)
            .or_else(|| (choice.player != self.player).then_some(StopReason::OtherPlayerRequired));
        let offer = match (reason, state) {
            (Some(reason), _) => Err(reason),
            (None, None) => Err(StopReason::UnsupportedOffer),
            (None, Some(state)) => self.audited(state, choice),
        };
        let offer = match offer {
            Ok(offer) => offer,
            Err(reason) => {
                self.end(DraftStop::Stopped { reason });
                return None;
            }
        };
        let subtype = offer
            .choice
            .context
            .as_ref()
            .map_or("", |context| context.subtype.as_str());
        if subtype == "mid_action_pause" {
            // The pause between two steps of the action is no step of a draft: it goes on.
            return offer
                .choice
                .option(ti4_engine::game::CONTINUE_ACTION_ID)
                .cloned();
        }
        // The first question of a step is what the whole step is staged against.
        let first = match subtype {
            crate::view::tactical::MOVEMENT_SUBTYPE => Some(&mut self.movement),
            crate::view::tactical::INVASION_SUBTYPE => Some(&mut self.invasion),
            crate::view::tactical::PRODUCTION_SUBTYPE => Some(&mut self.production),
            _ => None,
        };
        if let Some(first) = first {
            first.get_or_insert_with(|| offer.clone());
        }
        let offered = self.last.insert(offer).choice.clone();
        match script_answer(&self.script, self.next, &offered, &self.player) {
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
            ScriptAnswer::Done => {
                self.next = self.script.len();
                self.end(DraftStop::Open);
                None
            }
            // A question of the planner that the script has no answer for is open, with the
            // rest of the script still to play.
            ScriptAnswer::Interrupted { .. } => {
                self.end(DraftStop::Open);
                None
            }
            ScriptAnswer::Mismatch { step, reason } => {
                self.end(DraftStop::Refused { step, reason });
                None
            }
        }
    }

    /// The action is over. The entries that are left must be such that the game needs no answer
    /// for them: an end that it did not ask for, a payment that it did not need.
    fn complete(&mut self) {
        // A secondary asks for no end: nothing of its script may be left.
        let rest = &self.script[self.next..];
        if match self.scope {
            PlanScope::Tactical => needless(rest),
            PlanScope::Secondary { .. } => rest.is_empty(),
        } {
            self.next = self.script.len();
            self.end(DraftStop::Complete);
        } else {
            self.end(DraftStop::Refused {
                step: self.next,
                reason: match self.scope {
                    PlanScope::Tactical => "The action is over before this.",
                    PlanScope::Secondary { .. } => "The secondary is over before this.",
                }
                .to_owned(),
            });
        }
    }

    /// Whether a position tells the planner nothing new.
    fn known(
        &self,
        state: &GameState,
        choice: Option<Choice>,
    ) -> Result<Option<Choice>, StopReason> {
        let viewer = ViewerRole::Player(self.player.clone());
        let publication = SafePublication {
            position: project_game_view(state, &viewer),
            choice,
            events: Vec::new(),
        };
        validate_knowledge(&self.baseline, &publication, &self.player, &self.scope)?;
        Ok(publication.choice)
    }

    /// The question as the planner may see it: only the audited options, on a position that
    /// tells nothing new.
    fn audited(&self, state: GameState, choice: &Choice) -> Result<Offer, StopReason> {
        let enabled = match &self.scope {
            PlanScope::Tactical => audit_offer(choice),
            PlanScope::Secondary { primary, card } => audit_secondary_offer(choice, primary, card),
        }?;
        let mut choice = choice.clone();
        choice.options.retain(|option| enabled.contains(&option.id));
        let choice = self
            .known(&state, Some(choice))?
            .expect("the audited choice");
        Ok(Offer { state, choice })
    }
}

fn refused(choice: &Choice) -> IllegalChoice {
    IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: "the draft stops here".to_owned(),
    }
}

struct ScriptDecider {
    run: Arc<Mutex<Run>>,
    observation: ExecutionObservation,
    /// False for the table of the timing resolver, which asks without a position.
    observed: bool,
}

impl ScriptDecider {
    fn decide(&self, choice: &Choice, observed: bool) -> Result<ChoiceOption, IllegalChoice> {
        let mut run = self.run.lock().expect("draft lock");
        if !observed {
            run.seen = None;
        }
        run.answer(choice, &self.observation)
            .ok_or_else(|| refused(choice))
    }
}

impl Decider for ScriptDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.decide(choice, false)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        _seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.decide(choice, self.observed)
    }
}

/// Play `script` on a copy of the base, and tell where it ended: as the next turn of `player`,
/// or as its secondary of the strategic action that is resolved, by `scope`. The base is not
/// changed.
///
/// A secondary is drafted from the moment its card is played ([`crate::view::secondary`]): the copy
/// may have nothing of the primary yet, and nothing of the followers before `player`.
#[must_use]
pub fn run_draft(
    base: &DraftBase<'_, '_>,
    player: &PlayerId,
    scope: &PlanScope,
    script: &[DraftStep],
) -> DraftOutcome {
    let viewer = ViewerRole::Player(player.clone());
    let mut game = base.game.fork();
    let observation = guard_fork(&mut game, player);
    let run = Arc::new(Mutex::new(Run {
        player: player.clone(),
        scope: scope.clone(),
        script: script.to_vec(),
        next: 0,
        stop: None,
        baseline: project_game_view(&game.state, &viewer),
        seen: None,
        last: None,
        movement: None,
        invasion: None,
        production: None,
    }));
    let decider = |observed| {
        Box::new(ScriptDecider {
            run: run.clone(),
            observation: observation.clone(),
            observed,
        })
    };
    *game.timing.table_mut() = Table::with_default(decider(false));
    game.table = Table::with_default(decider(true));
    let seen = run.clone();
    game.table.on_observed_offer(move |_, state| {
        seen.lock().expect("draft lock").seen = Some(state.clone());
    });

    let ended = |run: &Arc<Mutex<Run>>| run.lock().expect("draft lock").stop.is_some();
    let prepared = match scope {
        PlanScope::Tactical => game
            .prepare_hypothetical_turn(player)
            .map_err(|error| error.to_string()),
        PlanScope::Secondary { primary, card } => {
            let allowed = base
                .secondary
                .is_some_and(|watch| watch.allows(scope, &base.game.state, base.content, player));
            if allowed {
                game.prepare_hypothetical_secondary(player, primary, card)
                    .map_err(|error| error.to_string())
            } else {
                Err("this secondary cannot be drafted now".to_owned())
            }
        }
    };
    if let Err(reason) = prepared {
        run.lock()
            .expect("draft lock")
            .end(DraftStop::Failed { reason });
    }
    let complete = match scope {
        PlanScope::Tactical => TACTICAL_COMPLETE,
        PlanScope::Secondary { .. } => ti4_engine::game::SECONDARY_PREVIEW_COMPLETE,
    };
    // The position after the last step that ended with no question open.
    let mut settled = None;
    let mut events = game.events.len();
    for _ in 0..STEP_BOUND {
        if ended(&run) {
            break;
        }
        let error = game.step().error;
        let mut run = run.lock().expect("draft lock");
        if run.stop.is_some() {
            break;
        }
        if let Some(error) = error {
            run.end(DraftStop::Failed {
                reason: error.to_string(),
            });
        } else if let Some(reason) =
            activity_stop(&observation).or_else(|| run.known(&game.state, None).err())
        {
            // The step did something that no question came after: a roll, a draw.
            run.end(DraftStop::Stopped { reason });
        } else {
            settled = Some(game.state.clone());
            if game.events[events..].iter().any(|event| event == complete) {
                run.complete();
            }
        }
        events = game.events.len();
    }
    drop(game);
    let mut run = run.lock().expect("draft lock");
    run.end(DraftStop::Stopped {
        reason: StopReason::StepLimit,
    });
    let stop = run.stop.clone().expect("the run ended");

    let context = FactsContext {
        content: base.content,
        sources: base.sources,
        galaxy: base.galaxy,
        secondary: None,
    };
    let closed =
        |state: &GameState| project_session_update(state, &viewer, None, base.tiles, &context);
    let shown = |offer: &Offer, open: bool| {
        if !open {
            return closed(&offer.state);
        }
        project_session_update(
            &offer.state,
            &viewer,
            Some((&offer.choice, DRAFT_NONCE)),
            base.tiles,
            &context,
        )
    };
    let first = |offer: &Option<Offer>| offer.as_ref().map(|offer| shown(offer, true));
    let update = match (&stop, &settled, &run.last) {
        // The board after the action.
        (DraftStop::Complete, Some(state), _) => closed(state),
        (_, _, Some(offer)) => shown(offer, stop == DraftStop::Open),
        _ => closed(&base.game.state),
    };
    DraftOutcome {
        update,
        movement: first(&run.movement),
        invasion: first(&run.invasion),
        production: first(&run.production),
        consumed: run.next,
        stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::secondary::{FollowerState, SecondaryWatch};
    use crate::view::tactical::TacticalFacts;
    use ti4_engine::choice::SeededRandom;
    use ti4_model::content_types::POK;
    use ti4_model::id::StrategyCardId;
    use ti4_model::state::Phase;

    /// A seeded game of six on the recommended map, played by random seats to its first action.
    fn game() -> (Game<'static>, Vec<BoardTileView>) {
        game_until(|game| game.state.phase == Phase::Action)
    }

    /// The same game, played by random seats until `reached`.
    fn game_until(reached: impl Fn(&Game<'static>) -> bool) -> (Game<'static>, Vec<BoardTileView>) {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template = crate::maps::default_template_for(content, &loader, 6, POK)
            .expect("a template for six");
        let (state, galaxy) =
            crate::map::create_game_with_template(content, &ids, 3, Some(&template))
                .expect("a game");
        let tiles = crate::map::build_board_tiles(content, &galaxy);
        let mut game = Game::with_seeded_random(state, content, 3)
            .with_sources(POK)
            .with_galaxy(galaxy);
        game.table = Table::with_default(Box::new(SeededRandom::new(3)));
        while !reached(&game) {
            assert_eq!(game.step().error, None);
        }
        (game, tiles)
    }

    /// A question of seat `player` with this subtype.
    fn asked(player: &str, subtype: &str, options: Vec<ChoiceOption>) -> Choice {
        use ti4_engine::decision_context::{DecisionContext, DecisionSource};
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

    #[test]
    fn a_script_answers_by_id_and_by_step_and_stops_at_another_question() {
        let seat = PlayerId::new("a");
        let script = [
            DraftStep::choose("tactical"),
            DraftStep::Step(PlanStep::DoneMoving),
            DraftStep::choose("use"),
            DraftStep::Step(PlanStep::DoneProducing),
        ];
        let id = |answer: &ScriptAnswer| match answer {
            ScriptAnswer::Answer { option, next, .. } => (option.id.clone(), *next),
            other => panic!("no answer: {other:?}"),
        };
        let menu = asked("a", "action_menu", vec![ChoiceOption::new("tactical", "x")]);
        assert_eq!(
            id(&script_answer(&script, 0, &menu, &seat)),
            ("tactical".to_owned(), 1)
        );
        // An answer by id that the game does not offer refuses the script; a step is not
        // answered by id.
        let other = asked("a", "action_menu", vec![ChoiceOption::new("pass", "x")]);
        assert!(matches!(
            script_answer(&script, 0, &other, &seat),
            ScriptAnswer::Mismatch { step: 0, .. }
        ));
        assert!(matches!(
            script_answer(&script, 1, &menu, &seat),
            ScriptAnswer::Interrupted { .. }
        ));
        let moving = asked(
            "a",
            "movement_step",
            vec![ChoiceOption::new("done_moving", "x")],
        );
        assert_eq!(
            id(&script_answer(&script, 1, &moving, &seat)),
            ("done_moving".to_owned(), 2)
        );
        // The pause is passed while entries are left, also before an answer by id.
        let pause = asked(
            "a",
            "mid_action_pause",
            vec![ChoiceOption::new(
                ti4_engine::game::CONTINUE_ACTION_ID,
                ti4_engine::game::CONTINUE_ACTION_KIND,
            )],
        );
        assert_eq!(
            id(&script_answer(&script, 2, &pause, &seat)),
            (ti4_engine::game::CONTINUE_ACTION_ID.to_owned(), 2)
        );
        assert_eq!(script_answer(&script, 4, &pause, &seat), ScriptAnswer::Done);
        // A question of another seat is never answered.
        let theirs = asked("c", "ability", vec![ChoiceOption::new("use", "x")]);
        assert!(matches!(
            script_answer(&script, 2, &theirs, &seat),
            ScriptAnswer::Interrupted { .. }
        ));
        let ability = asked("a", "ability", vec![ChoiceOption::new("use", "x")]);
        assert_eq!(
            id(&script_answer(&script, 2, &ability, &seat)),
            ("use".to_owned(), 3)
        );
        // The game ended the production itself: the end of the script is left out.
        assert_eq!(script_answer(&script, 3, &menu, &seat), ScriptAnswer::Done);
    }

    #[test]
    fn a_draft_opens_while_the_strategy_cards_are_chosen() {
        // Two seats have a card, four have none.
        let (game, tiles) = game_until(|game| {
            let seats = game.state.players.iter();
            seats.filter(|seat| !seat.strategy_cards.is_empty()).count() == 2
        });
        assert_eq!(game.state.phase, Phase::Strategy);
        let before = game.state.clone();

        let mut script = vec![choose("tactical")];
        let activation = run(&game, &tiles, &script);
        assert_eq!(activation.stop, DraftStop::Open);
        let Some(TacticalFacts::Activation(facts)) = &activation.update.tactical else {
            panic!("no activation facts");
        };
        let target = facts
            .systems
            .iter()
            .find(|reach| reach.ships > 0)
            .expect("a system in range");
        script.push(choose(&target.system));
        let movement = run(&game, &tiles, &script);
        assert_eq!(movement.stop, DraftStop::Open);
        let Some(TacticalFacts::Movement(facts)) = &movement.update.tactical else {
            panic!("no movement facts");
        };
        let ship = facts
            .ships
            .iter()
            .find(|ship| ship.r#move.is_some())
            .expect("a ship that can move");
        let plan = ship.r#move.as_ref().unwrap();
        script.push(step(PlanStep::Move {
            origin: ship.origin.clone(),
            unit: ship.unit.clone(),
            damaged: ship.damaged,
            gravity_drive: plan.gravity_drive,
            ionian: plan.ionian,
        }));
        script.push(step(PlanStep::DoneLoading));
        script.push(step(PlanStep::DoneMoving));
        let moved = run(&game, &tiles, &script);
        assert_eq!(moved.stop, DraftStop::Complete);
        assert_eq!(moved.consumed, script.len());

        assert_eq!(game.state, before, "a draft changes nothing");
    }

    /// A seat that is not `a` and holds a card of this name or, with `None`, any card but
    /// Trade and Leadership; with its card.
    fn holder(game: &Game<'static>, name: Option<&str>) -> (PlayerId, StrategyCardId) {
        let content = ContentStore::embedded();
        let seats = game
            .state
            .players
            .iter()
            .filter(|seat| seat.id.as_str() != "a");
        seats
            .flat_map(|seat| seat.strategy_cards.iter().map(move |card| (seat, card)))
            .find(|(_, card)| {
                let printed = ti4_engine::strategy_cards::card_name(content, card.as_str());
                match (name, printed.as_deref()) {
                    (Some(name), printed) => printed == Some(name),
                    (None, printed) => !matches!(printed, Some("Trade" | "Leadership")),
                }
            })
            .map(|(seat, card)| (seat.id.clone(), card.clone()))
            .expect("a seat with such a card")
    }

    fn run_secondary(
        game: &Game<'static>,
        tiles: &[BoardTileView],
        watch: Option<&SecondaryWatch>,
        scope: &PlanScope,
        script: &[DraftStep],
    ) -> DraftOutcome {
        let base = DraftBase {
            game,
            content: ContentStore::embedded(),
            sources: POK,
            tiles,
            galaxy: game.galaxy(),
            secondary: watch,
        };
        run_draft(&base, &PlayerId::new("a"), scope, script)
    }

    #[test]
    fn a_secondary_is_drafted_from_the_moment_its_card_is_played() {
        let (game, tiles) = game();
        let before = game.state.clone();
        let (primary, card) = holder(&game, None);
        let content = ContentStore::embedded();
        let viewer = ViewerRole::Player(PlayerId::new("a"));
        let watch = SecondaryWatch::default();
        assert_eq!(watch.facts(&game.state, content, &viewer), None);

        // The engine announces the card inside the step that resolves its primary: the game
        // of a draft is the one before that step.
        watch.announce(&primary, &card);
        let facts = watch
            .facts(&game.state, content, &viewer)
            .expect("the action");
        assert_eq!((&facts.primary, &facts.card), (&primary, &card));
        assert!(!facts.open, "the primary is not resolved yet");
        assert!(facts.draft);
        let order: Vec<PlayerId> = game
            .state
            .clockwise_from(&primary)
            .into_iter()
            .skip(1)
            .collect();
        let seats: Vec<PlayerId> = facts.followers.iter().map(|f| f.seat.clone()).collect();
        assert_eq!(seats, order);
        assert!(
            facts
                .followers
                .iter()
                .all(|follower| follower.state == FollowerState::Waiting)
        );
        // The primary player has nothing to draft, and neither has a spectator.
        let other = |viewer| watch.facts(&game.state, content, &viewer).expect("facts");
        assert!(!other(ViewerRole::Player(primary.clone())).draft);
        assert!(!other(ViewerRole::Spectator).draft);

        let scope = PlanScope::Secondary {
            primary: primary.clone(),
            card: card.clone(),
        };
        let asked = run_secondary(&game, &tiles, Some(&watch), &scope, &[]);
        assert_eq!(asked.stop, DraftStop::Open);
        let choice = &asked.update.pending_choice.as_ref().expect("asked").choice;
        assert_eq!(choice.player, PlayerId::new("a"));
        assert_eq!(
            choice.details.get("kind").and_then(|kind| kind.as_str()),
            Some("strategy_secondary")
        );
        assert_eq!(
            choice.details.get("card").and_then(|card| card.as_str()),
            Some(card.as_str())
        );

        // The first option of the question is the pass.
        let pass = choose(&choice.options[0].id);
        let passed = run_secondary(
            &game,
            &tiles,
            Some(&watch),
            &scope,
            std::slice::from_ref(&pass),
        );
        assert_eq!(passed.stop, DraftStop::Complete);
        assert_eq!(passed.consumed, 1);
        assert_eq!(passed.update.pending_choice, None);

        // Nothing is left of a script when the secondary is over.
        let long = [pass.clone(), pass];
        let over = run_secondary(&game, &tiles, Some(&watch), &scope, &long);
        assert!(matches!(over.stop, DraftStop::Refused { step: 1, .. }));
        let wrong = run_secondary(&game, &tiles, Some(&watch), &scope, &[choose("nothing")]);
        assert!(matches!(wrong.stop, DraftStop::Refused { step: 0, .. }));

        assert_eq!(game.state, before, "a draft changes nothing");
    }

    #[test]
    fn a_secondary_is_drafted_only_of_the_card_that_is_played() {
        let (game, tiles) = game();
        let (primary, card) = holder(&game, None);
        let scope = PlanScope::Secondary {
            primary: primary.clone(),
            card: card.clone(),
        };
        let failed = |outcome: DraftOutcome| matches!(outcome.stop, DraftStop::Failed { .. });
        // No card is played, or the host does not follow its game.
        let watch = SecondaryWatch::default();
        assert!(failed(run_secondary(
            &game,
            &tiles,
            Some(&watch),
            &scope,
            &[]
        )));
        assert!(failed(run_secondary(&game, &tiles, None, &scope, &[])));
        // Another card is played.
        let (other_seat, other) = holder(&game, Some("Trade"));
        watch.announce(&other_seat, &other);
        assert!(failed(run_secondary(
            &game,
            &tiles,
            Some(&watch),
            &scope,
            &[]
        )));
        // Trade is not drafted before its window: the primary changes what it costs.
        let trade = PlanScope::Secondary {
            primary: other_seat.clone(),
            card: other.clone(),
        };
        let content = ContentStore::embedded();
        let viewer = ViewerRole::Player(PlayerId::new("a"));
        let facts = watch.facts(&game.state, content, &viewer).expect("facts");
        assert_eq!(facts.name, "Trade");
        assert!(!facts.draft);
        assert!(failed(run_secondary(
            &game,
            &tiles,
            Some(&watch),
            &trade,
            &[]
        )));
    }

    #[test]
    fn the_watch_follows_the_window_of_the_followers_to_its_end() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template = crate::maps::default_template_for(content, &loader, 6, POK)
            .expect("a template for six");
        let (state, galaxy) =
            crate::map::create_game_with_template(content, &ids, 3, Some(&template))
                .expect("a game");
        let mut game = Game::with_seeded_random(state, content, 3)
            .with_sources(POK)
            .with_galaxy(galaxy);
        game.table = Table::with_default(Box::new(SeededRandom::new(3)));
        let watch = SecondaryWatch::default();
        watch.bind(&mut game);
        let spectator = ViewerRole::Spectator;

        // To a step that ends inside a window, with a follower recorded and one to ask.
        let mut seen = None;
        for _ in 0..2_000 {
            assert_eq!(game.step().error, None);
            watch.completed_step(&game);
            let facts = watch.facts(&game.state, content, &spectator);
            assert_eq!(facts.is_some(), game.open_secondary().is_some());
            let Some(facts) = facts else {
                if seen.is_some() {
                    break;
                }
                continue;
            };
            assert!(facts.open);
            let (primary, card, unresolved) = game.open_secondary().expect("the window");
            assert_eq!((&facts.primary, &facts.card), (primary, card));
            let waiting: Vec<&PlayerId> = facts
                .followers
                .iter()
                .filter(|follower| follower.state == FollowerState::Waiting)
                .map(|follower| &follower.seat)
                .collect();
            assert_eq!(waiting, unresolved.iter().collect::<Vec<_>>());
            assert_eq!(facts.followers.len(), 5);
            if waiting.len() < 5 && !waiting.is_empty() {
                seen = Some((facts, game.fork()));
            }
        }
        let (facts, base) = seen.expect("a window with a follower recorded");
        // A seat that is still to be asked drafts on; a seat that is recorded does not.
        let draws = |seat: &PlayerId| {
            watch_at(&base)
                .facts(&base.state, content, &ViewerRole::Player(seat.clone()))
                .expect("facts")
                .draft
        };
        for follower in &facts.followers {
            assert_eq!(
                draws(&follower.seat),
                follower.state == FollowerState::Waiting
            );
        }
    }

    /// A watch that is brought to a game that it did not follow.
    fn watch_at(game: &Game<'static>) -> SecondaryWatch {
        let watch = SecondaryWatch::default();
        watch.completed_step(game);
        watch
    }

    fn run(game: &Game<'static>, tiles: &[BoardTileView], script: &[DraftStep]) -> DraftOutcome {
        let base = DraftBase {
            game,
            content: ContentStore::embedded(),
            sources: POK,
            tiles,
            galaxy: game.galaxy(),
            secondary: None,
        };
        run_draft(&base, &PlayerId::new("a"), &PlanScope::Tactical, script)
    }

    fn step(step: PlanStep) -> DraftStep {
        step.into()
    }

    fn choose(id: &str) -> DraftStep {
        DraftStep::choose(id)
    }

    fn subtype(update: &SessionUpdate) -> &str {
        let choice = &update.pending_choice.as_ref().expect("a question").choice;
        &choice.context.as_ref().expect("a context").subtype
    }

    #[test]
    fn a_draft_goes_through_activation_and_movement_and_leaves_the_game_as_it_was() {
        let (game, tiles) = game();
        let before = game.state.clone();

        let menu = run(&game, &tiles, &[]);
        assert_eq!(menu.stop, DraftStop::Open);
        assert_eq!(subtype(&menu.update), "action_menu");
        let offered = &menu.update.pending_choice.as_ref().unwrap().choice.options;
        assert_eq!(offered.len(), 1, "only the tactical action is shown");
        // A draft has the facts of a live update, from the same place.
        let content = ContentStore::embedded();
        assert!(
            menu.update.tactical.is_none(),
            "a menu has no step to stage"
        );
        assert!(menu.update.exploration.is_some());
        assert!(menu.update.action_cards.is_some());
        assert!(menu.update.secret_objectives.is_some());
        assert_eq!(
            menu.update.score,
            Some(crate::view::score::project_score_facts(
                &game.state,
                content
            ))
        );
        assert_eq!(
            menu.update.agendas,
            Some(crate::view::agendas::project_agenda_facts(&game.state))
        );

        let mut script = vec![choose("tactical")];
        let activation = run(&game, &tiles, &script);
        assert_eq!(activation.stop, DraftStop::Open);
        assert_eq!(activation.consumed, 1);
        let Some(TacticalFacts::Activation(facts)) = &activation.update.tactical else {
            panic!("no activation facts");
        };
        let target = facts
            .systems
            .iter()
            .find(|reach| reach.ships > 0)
            .expect("a system in range");

        script.push(choose(&target.system));
        let movement = run(&game, &tiles, &script);
        assert_eq!(movement.stop, DraftStop::Open);
        assert_eq!(subtype(&movement.update), "movement_step");
        assert_eq!(movement.movement.as_ref(), Some(&movement.update));
        let Some(TacticalFacts::Movement(facts)) = &movement.update.tactical else {
            panic!("no movement facts");
        };
        assert_eq!(facts.active, target.system);
        let ship = facts
            .ships
            .iter()
            .find(|ship| ship.r#move.is_some())
            .expect("a ship that can move");
        let plan = ship.r#move.as_ref().unwrap();

        script.push(step(PlanStep::Move {
            origin: ship.origin.clone(),
            unit: ship.unit.clone(),
            damaged: ship.damaged,
            gravity_drive: plan.gravity_drive,
            ionian: plan.ionian,
        }));
        script.push(step(PlanStep::DoneLoading));
        script.push(step(PlanStep::DoneMoving));
        let moved = run(&game, &tiles, &script);
        assert_eq!(moved.stop, DraftStop::Complete);
        assert_eq!(moved.consumed, script.len());
        // The facts to stage against are still those from before the first ship moved.
        assert_eq!(moved.movement, movement.movement);
        let arrived = &moved.update.view.board.systems
            [&ti4_model::id::SystemId::new(target.system.as_str())]
            .units;
        assert!(
            arrived
                .iter()
                .any(|unit| unit.owner.as_str() == "a" && unit.unit_type.as_str() == ship.unit)
        );

        assert_eq!(game.state, before, "a draft changes nothing");
    }

    /// Seat A holds Flank Speed: the card is taken from wherever the game dealt it.
    fn with_flank_speed(game: &mut Game<'static>) {
        let card = ti4_model::id::ActionCardId::new("fs1");
        game.state.action_card_deck.retain(|held| *held != card);
        game.state
            .discarded_action_cards
            .retain(|held| *held != card);
        for seat in &mut game.state.players {
            seat.action_cards.retain(|held| *held != card);
        }
        let seat = game.state.player_mut(&PlayerId::new("a")).expect("seat a");
        seat.action_cards.push(card);
    }

    #[test]
    fn a_card_of_the_seat_is_asked_after_the_activation_and_played_or_declined() {
        let (mut game, tiles) = game();
        with_flank_speed(&mut game);
        let before = game.state.clone();
        let activation = run(&game, &tiles, &[choose("tactical")]);
        let Some(TacticalFacts::Activation(facts)) = &activation.update.tactical else {
            panic!("no activation facts");
        };
        let target = facts
            .systems
            .iter()
            .find(|reach| reach.ships > 0)
            .expect("a system in range");
        let mut script = vec![choose("tactical"), choose(&target.system)];

        // The window of the card is open: the script has no answer for it.
        let window = run(&game, &tiles, &script);
        assert_eq!(window.stop, DraftStop::Open);
        assert_eq!(window.consumed, 2);
        assert_eq!(window.movement, None, "the movement is not reached");
        let offered = &window.update.pending_choice.as_ref().unwrap().choice;
        let play = offered
            .options
            .iter()
            .find(|option| !option.is_decline())
            .expect("the card is offered");
        assert_eq!(
            play.payload.get("card").and_then(|card| card.as_str()),
            Some("fs1"),
            "{play:?}"
        );
        assert!(offered.options.iter().any(ChoiceOption::is_decline));

        // How far the ships of the seat come, with the card declined and with it played.
        let reach = |outcome: &DraftOutcome| {
            assert_eq!(outcome.stop, DraftStop::Open);
            assert_eq!(subtype(&outcome.update), "movement_step");
            let Some(TacticalFacts::Movement(facts)) = &outcome.update.tactical else {
                panic!("no movement facts");
            };
            facts
                .ships
                .iter()
                .filter(|ship| ship.r#move.is_some())
                .count()
        };
        script.push(choose("decline"));
        let declined = run(&game, &tiles, &script);
        assert_eq!(declined.consumed, 3);
        let hand = |outcome: &DraftOutcome| {
            let seats = &outcome.update.view.players;
            let seat = seats.iter().find(|seat| seat.id.as_str() == "a").unwrap();
            seat.held_action_cards.clone()
        };
        assert!(hand(&declined).iter().any(|card| card.as_str() == "fs1"));

        *script.last_mut().unwrap() = choose(&play.id);
        let played = run(&game, &tiles, &script);
        assert_eq!(played.consumed, 3);
        assert!(
            !hand(&played).iter().any(|card| card.as_str() == "fs1"),
            "the copy played the card"
        );
        assert!(reach(&played) >= reach(&declined));

        assert_eq!(game.state, before, "a draft changes nothing");
    }

    #[test]
    fn a_step_that_the_game_does_not_offer_is_named() {
        let (game, tiles) = game();
        let activation = run(&game, &tiles, &[choose("tactical")]);
        let Some(TacticalFacts::Activation(facts)) = &activation.update.tactical else {
            panic!("no activation facts");
        };
        let script = [
            choose("tactical"),
            choose(&facts.systems[0].system),
            step(PlanStep::Move {
                origin: "nowhere".to_owned(),
                unit: "carrier".to_owned(),
                damaged: false,
                gravity_drive: false,
                ionian: false,
            }),
            step(PlanStep::DoneMoving),
        ];
        let outcome = run(&game, &tiles, &script);
        assert!(
            matches!(outcome.stop, DraftStop::Refused { step: 2, .. }),
            "{:?}",
            outcome.stop
        );
        assert_eq!(outcome.consumed, 2);
        assert!(outcome.movement.is_some());

        let unknown = run(
            &game,
            &tiles,
            &[choose("tactical"), choose("no such system")],
        );
        assert!(matches!(unknown.stop, DraftStop::Refused { step: 1, .. }));
    }

    /// The late game of the scenarios: seat A has the turn, two space docks and many fleets.
    fn late_game() -> (Game<'static>, Vec<BoardTileView>) {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template = crate::maps::default_template_for(content, &loader, 8, POK)
            .expect("a template for eight");
        let (mut state, mut galaxy) =
            crate::map::create_game_with_template(content, &ids, 42, Some(&template))
                .expect("a game");
        crate::view::scenario::apply(
            crate::view::scenario::LATE_GAME,
            &mut state,
            &mut galaxy,
            content,
            POK,
        )
        .expect("the scenario applies");
        let tiles = crate::map::build_board_tiles(content, &galaxy);
        let mut game = Game::with_seeded_random(state, content, 42)
            .with_sources(POK)
            .with_galaxy(galaxy);
        game.table = Table::with_default(Box::new(SeededRandom::new(42)));
        (game, tiles)
    }

    /// The system of the second space dock, the free system with two planets, a system with a
    /// fleet of seat C, and the system north of the free one (`scenario::at`).
    const DOCK: &str = "72";
    const FREE: &str = "38";
    const ENEMY: &str = "32";
    const NORTH: &str = "63";

    #[test]
    fn a_draft_goes_on_to_the_production_and_ends_with_the_action() {
        let (game, tiles) = late_game();
        let before = game.state.clone();
        let mut script = vec![choose("tactical"), choose(DOCK), step(PlanStep::DoneMoving)];

        // AI Development Algorithm asks before the production: the question is open.
        let asked = run(&game, &tiles, &script);
        assert_eq!(asked.stop, DraftStop::Open);
        assert_eq!(subtype(&asked.update), "exhaust_for_production_discount");
        assert_eq!(asked.consumed, 3);
        script.push(choose("decline"));

        let open = run(&game, &tiles, &script);
        assert_eq!(open.stop, DraftStop::Open);
        assert_eq!(subtype(&open.update), "produce_unit");
        assert_eq!(open.production.as_ref(), Some(&open.update));
        assert_eq!(open.invasion, None, "nothing can land");
        let Some(TacticalFacts::Production(facts)) = &open.update.tactical else {
            panic!("no production facts");
        };
        let cruiser = facts
            .units
            .iter()
            .find(|unit| unit.unit == "cruiser2" && unit.unavailable.is_none())
            .expect("a cruiser");
        let produce = step(PlanStep::Produce {
            unit: cruiser.unit.clone(),
        });

        // A build that is not paid is refused at its payment.
        let mut unpaid = script.clone();
        unpaid.extend([produce.clone(), step(PlanStep::DoneProducing)]);
        let refused = run(&game, &tiles, &unpaid);
        assert!(
            matches!(refused.stop, DraftStop::Refused { step: 5, .. }),
            "{:?}",
            refused.stop
        );
        assert_eq!(refused.production, open.production);

        // Every planet pays; the game takes what it needs and the rest is left out.
        script.push(produce);
        script.extend(facts.planets.iter().map(|planet| {
            step(PlanStep::Pay {
                source: format!("exhaust|{}", planet.planet),
            })
        }));
        script.push(step(PlanStep::DoneProducing));
        let produced = run(&game, &tiles, &script);
        assert_eq!(produced.stop, DraftStop::Complete);
        assert_eq!(produced.consumed, script.len());
        assert_eq!(produced.production, open.production);
        assert_eq!(produced.update.pending_choice, None);
        let there = &produced.update.view.board.systems[&ti4_model::id::SystemId::new(DOCK)].units;
        let cruisers = |units: &[crate::protocol::view::PlacedUnitView]| {
            units
                .iter()
                .filter(|unit| unit.unit_type.as_str() == cruiser.unit)
                .count()
        };
        let was = &open.update.view.board.systems[&ti4_model::id::SystemId::new(DOCK)].units;
        assert_eq!(cruisers(there), cruisers(was) + 1);

        assert_eq!(game.state, before, "a draft changes nothing");
    }

    #[test]
    fn a_draft_ends_where_a_landing_draws_a_card() {
        let (game, tiles) = late_game();
        let mut script = vec![choose("tactical"), choose(FREE)];
        let movement = run(&game, &tiles, &script);
        let Some(TacticalFacts::Movement(facts)) = &movement.update.tactical else {
            panic!("no movement facts");
        };
        let ship = facts
            .ships
            .iter()
            .find(|ship| ship.origin == NORTH && ship.capacity > 0 && !ship.loads.is_empty())
            .expect("a carrier in the north");
        let plan = ship.r#move.as_ref().expect("it arrives");
        let cargo = ship
            .loads
            .iter()
            .map(|index| &facts.cargo[*index])
            .find(|cargo| cargo.unit.contains("infantry"))
            .expect("infantry to load");
        script.extend([
            step(PlanStep::Move {
                origin: ship.origin.clone(),
                unit: ship.unit.clone(),
                damaged: ship.damaged,
                gravity_drive: plan.gravity_drive,
                ionian: plan.ionian,
            }),
            step(PlanStep::Load {
                origin: ship.origin.clone(),
                pickup_system: cargo.system.clone(),
                unit: cargo.unit.clone(),
                source: cargo.source.clone(),
                damaged: cargo.damaged,
                galvanized: cargo.galvanized,
            }),
            step(PlanStep::DoneLoading),
            step(PlanStep::DoneMoving),
        ]);

        // The landings are open, staged against their first question.
        let landing = run(&game, &tiles, &script);
        assert_eq!(landing.stop, DraftStop::Open);
        assert_eq!(landing.consumed, script.len());
        assert_eq!(subtype(&landing.update), "commit_ground_forces");
        assert_eq!(landing.invasion.as_ref(), Some(&landing.update));
        let Some(TacticalFacts::Invasion(facts)) = &landing.update.tactical else {
            panic!("no invasion facts");
        };
        assert_eq!(facts.active, FREE);
        let planet = facts
            .planets
            .iter()
            .find(|planet| !planet.lands.is_empty())
            .expect("a planet to land on");
        let force = &facts.forces[planet.lands[0]];

        // Land nothing: the action is over, and the draft with it.
        let mut nothing = script.clone();
        nothing.push(step(PlanStep::DoneCommitting));
        assert_eq!(run(&game, &tiles, &nothing).stop, DraftStop::Complete);

        // A planet that nobody explored: the card that the landing draws ends the draft.
        script.extend([
            step(PlanStep::Land {
                planet: planet.planet.clone(),
                unit: force.unit.clone(),
                damaged: force.damaged,
                from_system: force.from_system.clone(),
                from_planet: force.from_planet.clone(),
            }),
            step(PlanStep::DoneCommitting),
        ]);
        let landed = run(&game, &tiles, &script);
        assert_eq!(
            landed.stop,
            DraftStop::Stopped {
                reason: StopReason::Uncertainty
            }
        );
        assert_eq!(landed.invasion, landing.invasion);
        assert_eq!(landed.update.pending_choice, None);
    }

    #[test]
    fn a_draft_ends_before_a_space_combat() {
        let (game, tiles) = late_game();
        let script = [
            choose("tactical"),
            choose(ENEMY),
            step(PlanStep::DoneMoving),
        ];
        let outcome = run(&game, &tiles, &script);
        // The guns of the system roll before the combat is reached.
        assert!(
            matches!(
                outcome.stop,
                DraftStop::Stopped {
                    reason: StopReason::Uncertainty | StopReason::UnsupportedSegment
                }
            ),
            "{:?}",
            outcome.stop
        );
        assert!(outcome.movement.is_some());
        assert_eq!(outcome.invasion, None);
        assert_eq!(outcome.update.pending_choice, None);
    }
}
