//! Duha Menaimon (Arborec flagship) asks its production questions inside the system activation
//! that triggered it, so a batch plan, a recovery replay, or any other scripted run that stops at
//! the first of them ends in the middle of a step the engine rolls back as a whole. Every game
//! that seated Arborec ended in `POST /batches 409 "replay diverged"` (and a restart could not
//! recover it) until the server kept its own list of the answers it gave.
//!
//! The game here is played by a seeded random table, except that Arborec takes a tactical action
//! on its home system, where the flagship and a full purse wait.

use crate::support;

use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, DecisionLog, DecisionRecord, IllegalChoice, SeededRandom, Table,
};
use ti4_engine::game::Game;
use ti4_host::batch::{
    BatchKind, MovementPlan, MovementStep, SimulationMode, simulate_with, simulation_difference,
};
use ti4_host::replay::replay_session_forced;
use ti4_host::step_snapshot::StepSnapshot;
use ti4_model::id::PlayerId;
use ti4_server::session::SessionConfig;

const FLAGSHIP: &str = "unit:arborec:arborec_flagship:SYSTEM_ACTIVATED:after";

/// Steers the Arborec seat to its flagship's production offer and stops there.
struct ToTheProduction {
    arborec: PlayerId,
    home: String,
    random: SeededRandom,
    answered: Arc<Mutex<DecisionLog>>,
    offer: Arc<Mutex<Option<Choice>>>,
}

impl Decider for ToTheProduction {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if choice.player == self.arborec && choice.options.iter().any(|o| o.kind == "produce") {
            *self.offer.lock().unwrap() = Some(choice.clone());
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "stop at the production offer".into(),
            });
        }
        let wanted = (choice.player == self.arborec)
            .then(|| {
                ["tactical", FLAGSHIP, self.home.as_str()]
                    .into_iter()
                    .find_map(|id| choice.options.iter().find(|o| o.id == id))
            })
            .flatten();
        let option = match wanted {
            Some(option) => option.clone(),
            None => self.random.choose(choice)?,
        };
        self.answered.lock().unwrap().record(choice, &option);
        Ok(option)
    }
}

struct Stopped {
    config: SessionConfig,
    records: Vec<DecisionRecord>,
    copy: StepSnapshot,
    offer: Choice,
    state: ti4_model::state::GameState,
}

fn play_to_the_flagship_production(seed: u64) -> Stopped {
    let (mut state, galaxy) = support::factions_start(seed, ["arborec", "hacan", "jolnar", "sol"]);
    let arborec = PlayerId::new("a");
    let home = state.player(&arborec).unwrap().home_system.clone().unwrap();
    ti4_engine::fixtures::put(&mut state, &home, "arborec_flagship", &arborec, 1);
    state.player_mut(&arborec).unwrap().trade_goods += 20;
    let answered = Arc::new(Mutex::new(DecisionLog::default()));
    let offer = Arc::new(Mutex::new(None));
    let table = Table::with_default(Box::new(ToTheProduction {
        arborec: arborec.clone(),
        home: home.to_string(),
        random: SeededRandom::new(seed),
        answered: answered.clone(),
        offer: offer.clone(),
    }));
    let mut game = Game::with_table(state.clone(), ContentStore::embedded(), table)
        .with_galaxy(galaxy.clone());
    let mut previous = None;
    for _ in 0..2_000 {
        previous = StepSnapshot::capture(&mut game, 0);
        if game.step().error.is_some() {
            break;
        }
    }
    let offer = offer
        .lock()
        .unwrap()
        .clone()
        .expect("the Arborec seat reached its flagship production");
    let records = answered.lock().unwrap().records.clone();
    assert!(
        game.table.log.records.len() < records.len(),
        "the production question is asked inside the activation step, whose log the engine rolled back"
    );
    let config = SessionConfig::new("arborec", state.clone()).with_galaxy(galaxy, Vec::new());
    Stopped {
        config,
        records,
        copy: previous.expect("a copy at the step boundary"),
        offer,
        state,
    }
}

#[test]
fn a_replay_that_ends_inside_the_flagship_production_still_matches_its_records() {
    let stopped = play_to_the_flagship_production(7);
    let galaxy = stopped.config.galaxy.as_ref();
    let report = replay_session_forced(
        &stopped.state,
        galaxy,
        &stopped.records,
        &Default::default(),
    )
    .expect("replay");
    assert_eq!(report.decision_count, stopped.records.len());
    assert!(report.hashes_match, "{report:?}");
}

#[test]
fn batch_checks_inside_the_flagship_production_match_between_the_copy_and_the_full_replay() {
    let stopped = play_to_the_flagship_production(7);
    let produce: Vec<MovementStep> = stopped
        .offer
        .options
        .iter()
        .filter(|o| o.kind == "produce")
        .filter_map(|o| {
            Some(MovementStep::Produce {
                unit: o.payload.get("unit")?.as_str()?.to_owned(),
                count: u32::try_from(o.payload.get("count")?.as_u64()?).ok()?,
                exchange: o
                    .payload
                    .get("exchange")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    assert!(!produce.is_empty(), "{:?}", stopped.offer.options);
    let actor = stopped.offer.player.clone();
    let destination = stopped
        .offer
        .options
        .iter()
        .find_map(|o| o.payload.get("system")?.as_str().map(str::to_owned))
        .unwrap();
    let plans = [
        // Stops at the next production question: the step is interrupted mid-activation.
        vec![produce[0].clone()],
        vec![produce[0].clone(), MovementStep::DoneProducing],
        vec![MovementStep::DoneProducing],
    ];
    for steps in plans {
        let plan = MovementPlan {
            kind: BatchKind::Production,
            destination: destination.clone(),
            steps,
        };
        let full = simulate_with(
            &stopped.config,
            &stopped.records,
            &actor,
            &plan,
            None,
            SimulationMode::FullReplay,
        );
        let forked = simulate_with(
            &stopped.config,
            &stopped.records,
            &actor,
            &plan,
            Some(&stopped.copy),
            SimulationMode::Auto,
        );
        // A plan that names a step the engine will not take next (a payment comes first) is
        // refused; the copy and the full replay must refuse it identically.
        assert_eq!(simulation_difference(&forked, &full), None, "{plan:?}");
        if plan.steps.len() == 1 {
            let simulation = full
                .as_ref()
                .unwrap_or_else(|failure| panic!("{plan:?} was rejected: {failure:?}"));
            assert!(!simulation.selected.is_empty(), "{plan:?}");
            assert_eq!(
                forked.as_ref().unwrap().forked_at,
                Some(stopped.copy.log_len)
            );
        }
    }
}
