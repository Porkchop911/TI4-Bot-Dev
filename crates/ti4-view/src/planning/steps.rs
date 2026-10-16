//! The steps of a plan, and how a plan answers a question of the game.
//!
//! A step names what is moved, landed or produced, not the id of an option: the engine numbers
//! its options again after each answer. A draft and a plan that is sent to the live game are
//! answered here ([`super::draft::script_answer`]), so a draft is played as it will be applied.

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::id::PlayerId;

/// One step of a movement, of the landings of an invasion, or of a production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanStep {
    Move {
        origin: String,
        unit: String,
        damaged: bool,
        gravity_drive: bool,
        ionian: bool,
    },
    Load {
        /// The system that the ship starts in.
        origin: String,
        /// The system where the unit is picked up.
        pickup_system: String,
        unit: String,
        /// The planet, or `None` for the space area.
        source: Option<String>,
        damaged: bool,
        galvanized: bool,
    },
    DoneLoading,
    DoneMoving,
    /// One ground force lands on a planet of the active system.
    Land {
        planet: String,
        unit: String,
        damaged: bool,
        /// Where the unit is, when it is not the space area of the active system.
        #[serde(default)]
        from_system: Option<String>,
        #[serde(default)]
        from_planet: Option<String>,
    },
    DoneCommitting,
    /// One purchase of a unit: two fighters or infantry, one of any other unit.
    Produce {
        unit: String,
    },
    /// Where the units of the purchase before it go: `space`, or a planet.
    Place {
        destination: String,
    },
    /// The id of the option that pays: `exhaust|<planet>` or `trade_good`.
    Pay {
        source: String,
    },
    DoneProducing,
}

impl PlanStep {
    /// The question that this step answers, by the subtype of its context.
    #[must_use]
    pub const fn subtype(&self) -> &'static str {
        match self {
            Self::Move { .. } | Self::DoneMoving => "movement_step",
            Self::Load { .. } | Self::DoneLoading => "load_cargo",
            Self::Land { .. } | Self::DoneCommitting => "commit_ground_forces",
            Self::Produce { .. } | Self::DoneProducing => "produce_unit",
            Self::Place { .. } => "place_unit",
            Self::Pay { .. } => "pay_resources",
        }
    }

    /// A step that only follows a purchase: the game may need none of them.
    const fn follows(&self) -> bool {
        matches!(
            self,
            Self::Pay { .. } | Self::Place { .. } | Self::DoneProducing
        )
    }

    /// Whether an offered option is what this step asks for.
    #[must_use]
    pub fn matches_option(&self, option: &ChoiceOption) -> bool {
        let text = |key: &str| option.payload.get(key).and_then(serde_json::Value::as_str);
        let flag = |key: &str| {
            option
                .payload
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        match self {
            Self::Move {
                origin,
                unit,
                damaged,
                gravity_drive,
                ionian,
            } => {
                option.kind == ti4_engine::tactical::MOVE_KIND
                    && text("origin") == Some(origin)
                    && text("unit") == Some(unit)
                    && flag("damaged") == *damaged
                    && flag("gravity_drive") == *gravity_drive
                    && flag("ionian") == *ionian
            }
            Self::Load {
                origin,
                pickup_system,
                unit,
                source,
                damaged,
                galvanized,
            } => {
                option.kind == ti4_engine::transit::LOAD_KIND
                    && text("system") == Some(origin)
                    && text("pickup_system") == Some(pickup_system)
                    && text("unit") == Some(unit)
                    && text("source") == source.as_deref()
                    && flag("damaged") == *damaged
                    && flag("galvanized") == *galvanized
            }
            Self::DoneLoading => option.id == "done_loading",
            Self::DoneMoving => option.id == "done_moving",
            Self::Land {
                planet,
                unit,
                damaged,
                from_system,
                from_planet,
            } => {
                option.kind == ti4_engine::invasion::COMMIT_KIND
                    && text("planet") == Some(planet)
                    && text("unit") == Some(unit)
                    && flag("damaged") == *damaged
                    && text("from_system") == from_system.as_deref()
                    && text("from_planet") == from_planet.as_deref()
            }
            Self::DoneCommitting => option.id == "done_committing",
            Self::Produce { unit } => {
                option.kind == ti4_engine::production::PRODUCE_KIND
                    && text("unit") == Some(unit)
                    && !flag("exchange")
            }
            Self::Place { destination } => {
                option.kind == ti4_engine::production::PLACE_KIND
                    && text("destination") == Some(destination)
            }
            Self::Pay { source } => {
                option.kind == ti4_engine::production::PAY_KIND && option.id == *source
            }
            Self::DoneProducing => option.id == "done_producing",
        }
    }

    /// The step that an offered option is: the inverse of [`Self::matches_option`]. `None` for
    /// an option that no step asks for (an answer to a card, the start of an action).
    #[must_use]
    pub fn of_option(option: &ChoiceOption) -> Option<Self> {
        let text = |key: &str| {
            option
                .payload
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let flag = |key: &str| {
            option
                .payload
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        let step = match option.kind.as_str() {
            ti4_engine::tactical::MOVE_KIND => Self::Move {
                origin: text("origin")?,
                unit: text("unit")?,
                damaged: flag("damaged"),
                gravity_drive: flag("gravity_drive"),
                ionian: flag("ionian"),
            },
            ti4_engine::transit::LOAD_KIND => Self::Load {
                origin: text("system")?,
                pickup_system: text("pickup_system")?,
                unit: text("unit")?,
                source: text("source"),
                damaged: flag("damaged"),
                galvanized: flag("galvanized"),
            },
            ti4_engine::invasion::COMMIT_KIND => Self::Land {
                planet: text("planet")?,
                unit: text("unit")?,
                damaged: flag("damaged"),
                from_system: text("from_system"),
                from_planet: text("from_planet"),
            },
            ti4_engine::production::PRODUCE_KIND if !flag("exchange") => Self::Produce {
                unit: text("unit")?,
            },
            ti4_engine::production::PLACE_KIND => Self::Place {
                destination: text("destination")?,
            },
            ti4_engine::production::PAY_KIND => Self::Pay {
                source: option.id.clone(),
            },
            _ => match option.id.as_str() {
                "done_loading" => Self::DoneLoading,
                "done_moving" => Self::DoneMoving,
                "done_committing" => Self::DoneCommitting,
                "done_producing" => Self::DoneProducing,
                _ => return None,
            },
        };
        Some(step)
    }

    fn describe(&self) -> String {
        match self {
            Self::Move { unit, origin, .. } => format!("move {unit} from {origin}"),
            Self::Load {
                unit,
                pickup_system,
                ..
            } => format!("load {unit} at {pickup_system}"),
            Self::DoneLoading => "close the hold".to_owned(),
            Self::DoneMoving => "finish the movement".to_owned(),
            Self::Land { unit, planet, .. } => format!("land {unit} on {planet}"),
            Self::DoneCommitting => "finish the landings".to_owned(),
            Self::Produce { unit } => format!("produce {unit}"),
            Self::Place { destination } => format!("place the units on {destination}"),
            Self::Pay { source } => format!("pay with {source}"),
            Self::DoneProducing => "finish the production".to_owned(),
        }
    }
}

/// What a plan answers to a question of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAnswer {
    /// Answer with this option. `next` is the first step that is still to do. With `steps` the
    /// plan goes on in this order: the step that answered was not the next one.
    Answer {
        option: ChoiceOption,
        next: usize,
        steps: Option<Vec<PlanStep>>,
    },
    /// Every step is done: the question is not of the plan.
    Done,
    /// The game asks something else first (a reaction window, another seat).
    Interrupted { reason: String },
    /// The game does not offer what step `step` asks for.
    Mismatch { step: usize, reason: String },
}

pub const INTERRUPTED: &str = "The game asks for another decision first.";

/// What a plan answers to `choice`, from step `next` on.
///
/// A hold that closed itself needs no `done_loading`; a hold that the plan does not load is
/// declined; after the last ground force the game does not ask for `done_committing`. The pause
/// between two steps of the action is passed while steps are left. Any other question stops
/// the plan.
///
/// A production has rules of its own. The game asks for a payment after a purchase, as often
/// as the purchase needs: the answer is the first payment of the plan that is offered, wherever
/// it is in the plan. A place that the game settled, a payment that it did not need and an end
/// that it did not ask for are left out.
#[must_use]
pub fn plan_answer(
    steps: &[PlanStep],
    mut next: usize,
    choice: &Choice,
    actor: &PlayerId,
) -> PlanAnswer {
    let subtype = choice
        .context
        .as_ref()
        .map_or("", |context| context.subtype.as_str());
    let own = &choice.player == actor;
    let pick = |step: &PlanStep, index: usize, at: usize| {
        let mut found = choice
            .options
            .iter()
            .filter(|option| step.matches_option(option));
        match (found.next(), found.next()) {
            (Some(option), None) => PlanAnswer::Answer {
                option: option.clone(),
                next: at,
                steps: None,
            },
            (first, _) => PlanAnswer::Mismatch {
                step: index,
                reason: format!(
                    "The game does not offer to {}{}.",
                    step.describe(),
                    if first.is_some() {
                        " in one way only"
                    } else {
                        ""
                    }
                ),
            },
        }
    };
    if own && subtype == "mid_action_pause" && next < steps.len() {
        // The plan goes on to its next step, whatever that is.
        let passes = choice
            .option(ti4_engine::game::CONTINUE_ACTION_ID)
            .filter(|option| option.kind == ti4_engine::game::CONTINUE_ACTION_KIND);
        if let Some(option) = passes {
            return PlanAnswer::Answer {
                option: option.clone(),
                next,
                steps: None,
            };
        }
    }
    if own && subtype == "movement_step" {
        while steps.get(next) == Some(&PlanStep::DoneLoading) {
            next += 1;
        }
    }
    if !own || subtype != "commit_ground_forces" {
        while steps.get(next) == Some(&PlanStep::DoneCommitting) {
            next += 1;
        }
    }
    // The bill of a purchase of the plan: the step before is that purchase, or a part of its
    // payment.
    let bill = next > 0
        && matches!(
            steps.get(next - 1),
            Some(PlanStep::Produce { .. } | PlanStep::Pay { .. })
        );
    if own && subtype == "pay_resources" && bill {
        let offered = |step: &PlanStep| {
            matches!(step, PlanStep::Pay { .. })
                && choice
                    .options
                    .iter()
                    .any(|option| step.matches_option(option))
        };
        let Some(at) = (next..steps.len()).find(|index| offered(&steps[*index])) else {
            return PlanAnswer::Mismatch {
                step: next,
                reason: "The staged payment does not cover the cost.".to_owned(),
            };
        };
        return match pick(&steps[at], at, next + 1) {
            PlanAnswer::Answer { option, next, .. } => {
                let mut order = steps.to_vec();
                let paying = order.remove(at);
                order.insert(next - 1, paying);
                PlanAnswer::Answer {
                    option,
                    next,
                    steps: Some(order),
                }
            }
            other => other,
        };
    }
    if own && subtype == "produce_unit" {
        // The game placed the units of the last purchase itself, and needs no more payment
        // for it.
        while matches!(steps.get(next), Some(PlanStep::Place { .. })) {
            next += 1;
        }
        while matches!(steps.get(next), Some(PlanStep::Pay { .. })) {
            next += 1;
        }
    } else if !(own && matches!(subtype, "pay_resources" | "place_unit"))
        && steps[next.min(steps.len())..].iter().all(PlanStep::follows)
    {
        // The game ended the production itself: the limit is used.
        next = steps.len();
    }
    let Some(step) = steps.get(next) else {
        return PlanAnswer::Done;
    };
    if own && subtype == "load_cargo" && step.subtype() != "load_cargo" {
        // A hold that the plan does not load.
        return pick(&PlanStep::DoneLoading, next, next);
    }
    if !own || subtype != step.subtype() {
        return PlanAnswer::Interrupted {
            reason: INTERRUPTED.to_owned(),
        };
    }
    pick(step, next, next + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_model::state::Phase;

    fn seat() -> PlayerId {
        PlayerId::new("a")
    }

    fn asked(subtype: &str, options: Vec<ChoiceOption>) -> Choice {
        Choice::new(seat(), "a question", options).contextualized(DecisionContext::new(
            seat(),
            DecisionSource::Rule("test".to_owned()),
            subtype,
            Phase::Action,
            1,
        ))
    }

    fn produce(unit: &str) -> PlanStep {
        PlanStep::Produce {
            unit: unit.to_owned(),
        }
    }

    fn pay(source: &str) -> PlanStep {
        PlanStep::Pay {
            source: source.to_owned(),
        }
    }

    fn land(planet: &str) -> PlanStep {
        PlanStep::Land {
            planet: planet.to_owned(),
            unit: "infantry".to_owned(),
            damaged: false,
            from_system: None,
            from_planet: None,
        }
    }

    fn paying(id: &str) -> ChoiceOption {
        ChoiceOption::new(id, "pay")
    }

    fn answered(answer: &PlanAnswer) -> (&str, usize) {
        match answer {
            PlanAnswer::Answer { option, next, .. } => (option.id.as_str(), *next),
            other => panic!("no answer: {other:?}"),
        }
    }

    #[test]
    fn the_steps_have_the_wire_shape_of_the_page() {
        let step: PlanStep = serde_json::from_str(
            r#"{"kind":"land","planet":"jord","unit":"infantry","damaged":false,"from_system":null,"from_planet":null}"#,
        )
        .unwrap();
        assert_eq!(step, land("jord"));
        assert_eq!(
            serde_json::to_string(&PlanStep::DoneProducing).unwrap(),
            r#"{"kind":"done_producing"}"#
        );
    }

    #[test]
    fn a_hold_that_the_plan_does_not_load_is_declined() {
        let steps = [PlanStep::DoneMoving];
        let hold = asked(
            "load_cargo",
            vec![
                ChoiceOption::new("load|0", "load"),
                ChoiceOption::new("done_loading", "decline"),
            ],
        );
        assert_eq!(
            answered(&plan_answer(&steps, 0, &hold, &seat())),
            ("done_loading", 0)
        );
        // A hold that closed itself needs no answer of the plan.
        let steps = [PlanStep::DoneLoading, PlanStep::DoneMoving];
        let moving = asked(
            "movement_step",
            vec![ChoiceOption::new("done_moving", "decline")],
        );
        assert_eq!(
            answered(&plan_answer(&steps, 0, &moving, &seat())),
            ("done_moving", 2)
        );
    }

    #[test]
    fn the_end_of_the_landings_is_left_out_when_the_game_does_not_ask_for_it() {
        let steps = [land("jord"), PlanStep::DoneCommitting, produce("carrier")];
        let commit = ChoiceOption::new("commit|0", "commit")
            .with("planet", "jord")
            .with("unit", "infantry")
            .with("damaged", false);
        let landing = asked("commit_ground_forces", vec![commit]);
        assert_eq!(
            answered(&plan_answer(&steps, 0, &landing, &seat())),
            ("commit|0", 1)
        );
        let producing = asked(
            "produce_unit",
            vec![ChoiceOption::new("produce|carrier", "produce").with("unit", "carrier")],
        );
        assert_eq!(
            answered(&plan_answer(&steps, 1, &producing, &seat())),
            ("produce|carrier", 3)
        );
    }

    #[test]
    fn the_pause_between_two_steps_is_passed_while_steps_are_left() {
        let pause = asked(
            "mid_action_pause",
            vec![ChoiceOption::new(
                ti4_engine::game::CONTINUE_ACTION_ID,
                ti4_engine::game::CONTINUE_ACTION_KIND,
            )],
        );
        let steps = [PlanStep::DoneMoving, land("jord")];
        assert_eq!(
            answered(&plan_answer(&steps, 1, &pause, &seat())),
            (ti4_engine::game::CONTINUE_ACTION_ID, 1)
        );
        assert_eq!(plan_answer(&steps, 2, &pause, &seat()), PlanAnswer::Done);
    }

    #[test]
    fn a_payment_is_found_anywhere_in_the_plan() {
        let steps = [
            produce("carrier"),
            produce("cruiser"),
            pay("exhaust|jord"),
            pay("trade_good"),
            PlanStep::DoneProducing,
        ];
        let bill = asked("pay_resources", vec![paying("trade_good")]);
        let PlanAnswer::Answer {
            option,
            next,
            steps: Some(order),
        } = plan_answer(&steps, 1, &bill, &seat())
        else {
            panic!("no answer");
        };
        assert_eq!((option.id.as_str(), next), ("trade_good", 2));
        assert_eq!(
            order,
            [
                produce("carrier"),
                pay("trade_good"),
                produce("cruiser"),
                pay("exhaust|jord"),
                PlanStep::DoneProducing,
            ]
        );
        // What the plan does not pay with is not taken.
        let other = asked("pay_resources", vec![paying("exhaust|vefut")]);
        assert!(matches!(
            plan_answer(&steps, 1, &other, &seat()),
            PlanAnswer::Mismatch { step: 1, .. }
        ));
    }

    #[test]
    fn what_the_game_settled_itself_is_left_out() {
        // A payment that the game did not need, before the end.
        let steps = [
            produce("carrier"),
            pay("exhaust|jord"),
            PlanStep::DoneProducing,
        ];
        let producing = asked(
            "produce_unit",
            vec![ChoiceOption::new("done_producing", "decline")],
        );
        assert_eq!(
            answered(&plan_answer(&steps, 1, &producing, &seat())),
            ("done_producing", 3)
        );
        // The game ended the production itself: any other question is not of the plan.
        let later = asked("action_menu", vec![ChoiceOption::new("pass", "action")]);
        assert_eq!(plan_answer(&steps, 1, &later, &seat()), PlanAnswer::Done);
    }

    #[test]
    fn another_question_stops_the_plan() {
        let steps = [land("jord")];
        let window = asked("reaction_window", vec![ChoiceOption::new("pass", "pass")]);
        assert!(matches!(
            plan_answer(&steps, 0, &window, &seat()),
            PlanAnswer::Interrupted { .. }
        ));
        let mut other = asked("commit_ground_forces", Vec::new());
        other.player = PlayerId::new("b");
        assert!(matches!(
            plan_answer(&steps, 0, &other, &seat()),
            PlanAnswer::Interrupted { .. }
        ));
    }

    #[test]
    fn an_option_is_the_step_that_asks_for_it() {
        let options = [
            ChoiceOption::new("m1", "move")
                .with("origin", "18")
                .with("unit", "cruiser")
                .with("gravity_drive", true),
            ChoiceOption::new("l1", "load")
                .with("system", "18")
                .with("pickup_system", "19")
                .with("unit", "infantry")
                .with("source", "jord")
                .with("damaged", true),
            ChoiceOption::new("c1", "commit")
                .with("planet", "jord")
                .with("unit", "infantry")
                .with("from_system", "19"),
            ChoiceOption::new("p1", "produce").with("unit", "cruiser2"),
            ChoiceOption::new("place|jord", "place").with("destination", "jord"),
            ChoiceOption::new("exhaust|jord", "pay"),
            ChoiceOption::new("done_loading", "decline"),
            ChoiceOption::new("done_moving", "decline"),
            ChoiceOption::new("done_committing", "decline"),
            ChoiceOption::new("done_producing", "decline"),
        ];
        for option in &options {
            let step = PlanStep::of_option(option).expect("a step");
            assert!(step.matches_option(option), "{step:?}");
        }
        assert_eq!(
            PlanStep::of_option(&options[1]),
            Some(PlanStep::Load {
                origin: "18".to_owned(),
                pickup_system: "19".to_owned(),
                unit: "infantry".to_owned(),
                source: Some("jord".to_owned()),
                damaged: true,
                galvanized: false,
            })
        );
        // An exchange is no purchase, and an answer to a card is no step.
        let exchange = ChoiceOption::new("x", "produce")
            .with("unit", "cruiser")
            .with("exchange", true);
        assert_eq!(PlanStep::of_option(&exchange), None);
        assert_eq!(PlanStep::of_option(&ChoiceOption::decline()), None);
        assert_eq!(
            PlanStep::of_option(&ChoiceOption::new("tactical", "action")),
            None
        );
    }
}
