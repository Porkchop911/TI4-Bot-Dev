//! Source-linked production subcases, not training labels or full-game strength evaluation.
//! Uses synthetic six-seat faction deployments; no validation/confirmation map is consumed.
//! --bundle <unchanged checkpoint> --out <new JSON file in an existing directory>
//! [--workers 16] [--streams 4]. Every branch is repeated and must reproduce exactly.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;

use rayon::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ti4_content::ContentStore;
use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation, Table, Window,
};
use ti4_engine::production::{self, ProductionWindow};
use ti4_mlp::bot::{DecisionTrace, MlpBot};
use ti4_model::content_types::POK;
use ti4_model::id::{FactionId, PlayerId, SystemId};
use ti4_model::state::{GameState, Phase};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const CATALOG: &str = include_str!("../../../docs/strategy-wiki/MLP_CASE_CATALOG.json");
const CASES: [(&str, &str, &str); 6] = [
    ("hacan-production-bottleneck", "hacan", "funded"),
    ("hacan-production-bottleneck", "hacan", "more-money"),
    ("hacan-production-bottleneck", "hacan", "extra-dock"),
    ("jolnar-opening-infantry", "jolnar", "shortage"),
    ("jolnar-opening-infantry", "jolnar", "stocked"),
    ("jolnar-opening-infantry", "jolnar", "no-cash"),
];

fn fixture(faction: &str, variant: &str) -> Result<(GameState, PlayerId, SystemId), String> {
    let store = ContentStore::embedded();
    let players: Vec<_> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let mut state =
        ti4_engine::setup::start_game(store, &players, POK, None).map_err(|e| e.to_string())?;
    for (player, faction) in players.iter().zip(FACTIONS) {
        ti4_engine::seating::deploy(&mut state, store, player, &FactionId::new(faction), POK)
            .map_err(|e| e.to_string())?;
    }
    let player = players[FACTIONS
        .iter()
        .position(|f| *f == faction)
        .ok_or("unknown faction")?]
    .clone();
    let seat = state.player(&player).ok_or("no seat")?;
    let home = seat.home_system.clone().ok_or("no home")?;
    let planets = seat.home_planets.clone();
    state.phase = Phase::Action;
    state.active = Some(player.clone());
    state.active_system = Some(home.clone());
    // A synthetic already-open production step. Reaching this window through Warfare or
    // activation is explicitly outside this subcase. No hero/action-card reaction is installed.
    state.player_mut(&player).ok_or("no seat")?.trade_goods = if variant == "no-cash" {
        0
    } else if variant == "more-money" {
        40
    } else {
        20
    };
    if variant == "no-cash" {
        state.exhausted_planets.extend(planets.clone());
    }
    if variant == "stocked" {
        ti4_engine::fixtures::put_on_planet(&mut state, &home, &planets[0], "infantry", &player, 6);
    }
    if variant == "extra-dock" {
        let planet = planets
            .iter()
            .find(|planet| {
                state
                    .system_state(&home)
                    .planet_units
                    .get(*planet)
                    .is_none_or(|units| units.iter().all(|u| u.type_id.as_str() != "spacedock"))
            })
            .ok_or("no free dock planet")?
            .clone();
        ti4_engine::fixtures::put_on_planet(&mut state, &home, &planet, "spacedock", &player, 1);
    }
    Ok((state, player, home))
}

struct Inspect {
    inner: Box<dyn Decider>,
    branch: String,
    forced: bool,
    observations: Rc<RefCell<Vec<Value>>>,
}

impl Decider for Inspect {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let proposed = self.inner.choose_seeing(choice, seen)?;
        let root = !self.forced
            && choice
                .options
                .iter()
                .any(|o| o.kind == production::PRODUCE_KIND);
        let executed = if root {
            self.forced = true;
            let wanted = match self.branch.as_str() {
                "stop-first" => choice.options.iter().find(|o| o.is_decline()),
                "infantry-first" => choice.options.iter().find(|o| {
                    o.kind == production::PRODUCE_KIND
                        && o.payload
                            .get("unit")
                            .and_then(Value::as_str)
                            .is_some_and(|u| u.contains("infantry"))
                }),
                _ => Some(&proposed),
            };
            wanted
                .cloned()
                .ok_or_else(|| IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason: format!("controlled branch {} unavailable", self.branch),
                })?
        } else {
            proposed.clone()
        };
        let seat = seen.seat(&choice.player).expect("authenticated seat");
        self.observations.borrow_mut().push(json!({
            "round": seen.round(), "phase": format!("{:?}", seen.phase()),
            "trade_goods": seat.trade_goods,
            "available_resources": seen.available_spend(&choice.player, production::Spend::Resources),
            "tactic_tokens": seat.tactic_tokens, "strategic_tokens": seat.strategic_tokens,
            "fleet_tokens": seat.fleet_tokens,
            "proposed_id": proposed.id, "executed_id": executed.id,
            "root": root,
        }));
        Ok(executed)
    }
}

fn trace_json(
    trace: &DecisionTrace,
    actor: &ti4_mlp::Actor,
    row: ti4_mlp::FactionRow,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
) -> Result<Value, String> {
    let logits = ti4_tensor::to_vec(
        &actor
            .logits(&trace.options, &trace.head, row)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let options: Vec<_> = trace.choice.options.iter().enumerate().map(|(i, option)| {
        let facts: Vec<_> = trace.vectors[i].iter().map(|(key, value)| {
            let name = ti4_policy::intern::name_of(*key);
            json!({"column": vocabulary.column_of_key(*key), "assigned": vocabulary.is_assigned(&name), "name": name, "value": value})
        }).collect();
        json!({"option": option, "facts": facts, "columns": trace.options[i].columns, "values": trace.options[i].values, "logit": logits[i], "probability": trace.probabilities[i]})
    }).collect();
    Ok(
        json!({"prompt": trace.choice.prompt, "details": trace.choice.details, "context": trace.choice.context, "head": trace.head, "proposed_id": trace.choice.options[trace.chosen].id, "options": options}),
    )
}

fn run(
    actor: &Rc<ti4_mlp::Actor>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    faction: &str,
    variant: &str,
    branch: &str,
    stream: u64,
) -> Result<Value, String> {
    let store = ContentStore::embedded();
    let (mut state, player, home) = fixture(faction, variant)?;
    let baseline =
        ti4_policy::progress::Baseline::taken(&Observed::new(&state, store, POK, None), &player);
    let row = ti4_mlp::FactionRow::of(faction).map_err(|e| e.to_string())?;
    let mut bot = MlpBot::sharing(actor, vocabulary.clone(), row, stream)
        .at_temperature(0.001)
        .from_setup(baseline);
    let trace = bot.decision_trace();
    let (decider, status) = bot.seat();
    let observations = Rc::new(RefCell::new(Vec::new()));
    let mut table = Table::with_default(Box::new(Inspect {
        inner: decider,
        branch: branch.to_owned(),
        forced: false,
        observations: Rc::clone(&observations),
    }));
    let capacity = production::capacity(&state, store, POK, &player, &home);
    let before =
        ti4_policy::progress::measure(&Observed::new(&state, store, POK, None), &player, baseline);
    let initial_home =
        serde_json::to_value(state.system_state(&home)).map_err(|e| e.to_string())?;
    let own_secrets = state
        .player(&player)
        .ok_or("no seat")?
        .secret_objectives
        .clone();
    let objectives = state.revealed_objectives.clone();
    let mut window = ProductionWindow::new(&state, store, POK, &player, &home);
    state.production_seq += 1;
    ti4_engine::technology::production_used(&mut state, store, POK, None, &mut table, &player)
        .map_err(|e| e.to_string())?;
    window.refresh(&state, store, POK);
    let discount = state.production_discount_remaining;
    let mut dice = ti4_engine::dice::Dice::new();
    let mut rng = ti4_engine::rng::GameRng::new(stream);
    let mut ctx = ti4_engine::choice::Resolving {
        content: store,
        sources: POK,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: None,
    };
    let mut decisions = Vec::new();
    while let Some(choice) = window.pending_choice(&state, store, POK) {
        if decisions.len() >= 256 {
            return Err("production exceeded 256 choices".into());
        }
        let answer = ctx
            .table
            .ask_seeing(&choice, &Observed::new(&state, store, POK, None))
            .map_err(|e| e.to_string())?;
        let captured = std::mem::take(&mut *trace.borrow_mut());
        if captured.len() != 1 {
            return Err(format!(
                "expected one actual actor trace, got {}",
                captured.len()
            ));
        }
        let mut entry = trace_json(&captured[0], actor, row, vocabulary)?;
        entry["observation"] = observations
            .borrow_mut()
            .pop()
            .ok_or("missing observation")?;
        entry["stage"] = json!("production");
        decisions.push(entry);
        window
            .resolve(&mut state, &mut ctx, answer)
            .map_err(|e| e.to_string())?;
    }
    let report = window.into_report();
    let before_enforcement =
        serde_json::to_value(state.system_state(&home)).map_err(|e| e.to_string())?;
    let removed =
        ti4_engine::fleet::enforce_seeing(&mut state, store, POK, None, ctx.table, &player, &home)
            .map_err(|e| e.to_string())?;
    let enforcement_trace = std::mem::take(&mut *trace.borrow_mut());
    let enforcement_observations = std::mem::take(&mut *observations.borrow_mut());
    if enforcement_trace.len() != enforcement_observations.len() {
        return Err("enforcement trace/observation mismatch".into());
    }
    for (captured, observation) in enforcement_trace.iter().zip(enforcement_observations) {
        let mut entry = trace_json(captured, actor, row, vocabulary)?;
        entry["observation"] = observation;
        entry["stage"] = json!("limit-enforcement");
        decisions.push(entry);
    }
    let standing = ti4_engine::fleet::standing(&state, store, POK, &player, &home, None);
    if standing.fleet_excess() != 0 || standing.capacity_excess != 0 {
        return Err("post-production fleet/capacity enforcement incomplete".into());
    }
    let final_home = serde_json::to_value(state.system_state(&home)).map_err(|e| e.to_string())?;
    let inference_decisions = status.into_result().map_err(|e| e.to_string())?;
    let after =
        ti4_policy::progress::measure(&Observed::new(&state, store, POK, None), &player, baseline);
    Ok(
        json!({"faction": faction, "variant": variant, "branch": branch, "stream": stream, "capacity": capacity, "discount": discount, "initial_home": initial_home, "before_enforcement_home": before_enforcement, "final_home": final_home, "units_removed_by_limits": removed, "final_fleet_excess": standing.fleet_excess(), "final_capacity_excess": standing.capacity_excess, "initial_public_objectives": objectives, "initial_own_secrets": own_secrets, "before": before, "after": after, "produced": report.produced, "unused_capacity": report.unused_capacity, "inference_decisions": inference_decisions, "decisions": decisions}),
    )
}

fn digest(path: &Path) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}

fn execute() -> Result<(), String> {
    let mut args = BTreeMap::new();
    let mut input = std::env::args().skip(1);
    while let Some(flag) = input.next() {
        if !["--bundle", "--out", "--workers", "--streams"].contains(&flag.as_str())
            || args.contains_key(&flag)
        {
            return Err(format!("unknown or duplicate flag {flag}"));
        }
        args.insert(
            flag.clone(),
            input.next().ok_or(format!("missing value for {flag}"))?,
        );
    }
    let bundle = Path::new(args.get("--bundle").ok_or("--bundle required")?);
    let output = Path::new(args.get("--out").ok_or("--out required")?);
    if output.exists() || !output.parent().is_some_and(Path::is_dir) {
        return Err("output must be new, in an existing directory".into());
    }
    let workers: usize = args
        .get("--workers")
        .map_or(Ok(16), |v| v.parse())
        .map_err(|_| "invalid workers")?;
    let streams: u64 = args
        .get("--streams")
        .map_or(Ok(4), |v| v.parse())
        .map_err(|_| "invalid streams")?;
    if !(1..=16).contains(&workers) || !(1..=16).contains(&streams) {
        return Err("workers/streams must be 1..16".into());
    }
    ti4_tensor::configure_deterministic(20_261_016).map_err(|e| e.to_string())?;
    let loaded = ti4_mlp::bundle::read(bundle).map_err(|e| e.to_string())?;
    let catalog: Value = serde_json::from_str(CATALOG).map_err(|e| e.to_string())?;
    let hashes: BTreeMap<_, _> = std::fs::read_dir(bundle)
        .map_err(|e| e.to_string())?
        .map(|entry| {
            let entry = entry.map_err(|e| e.to_string())?;
            Ok((
                entry.file_name().to_string_lossy().into_owned(),
                digest(&entry.path())?,
            ))
        })
        .collect::<Result<_, String>>()?;
    let jobs: Vec<_> = CASES
        .into_iter()
        .flat_map(|case| (0..streams).map(move |s| (case, 61_016_001 + s)))
        .collect();
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<_> = jobs
        .chunks(per_worker)
        .map(|c| (loaded.actor.inference_copy(), c.to_vec()))
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| e.to_string())?;
    let results: Vec<Vec<Value>> = pool.install(|| chunks.into_par_iter().map(|(actor, jobs)| {
        let actor = Rc::new(actor);
        jobs.into_iter().flat_map(|((family, faction, variant), stream)| {
            ["mlp", "infantry-first", "stop-first"].into_iter().map(|branch| {
                let first = run(&actor, &loaded.vocabulary, faction, variant, branch, stream);
                let second = run(&actor, &loaded.vocabulary, faction, variant, branch, stream);
                eprintln!("{family}/{variant}/{stream}/{branch}: {}", if first.is_ok() && first == second { "replayed" } else { "FAILED" });
                match (first, second) {
                    (Ok(mut result), Ok(replay)) if result == replay => { result["family"] = json!(family); result["replay_equal"] = json!(true); result },
                    (a,b) => json!({"family":family, "variant":variant, "stream":stream, "branch":branch, "error":"branch execution failed or replay differed", "first": a, "replay": b}),
                }
            }).collect::<Vec<_>>()
        }).collect()
    }).collect());
    let results: Vec<_> = results.into_iter().flatten().collect();
    let failed = results.iter().any(|v| v.get("error").is_some());
    for (name, hash) in &hashes {
        if digest(&bundle.join(name))? != *hash {
            return Err("checkpoint changed during probe".into());
        }
    }
    let report = json!({"schema":1, "status":"production-subcase-diagnostic-not-training-labels", "setup":"synthetic six-seat POK faction deployments; fixed setup deck seed 0 with public objectives and own secrets retained; no galaxy or reaction timing; production hooks+refresh", "temperature":0.001, "workers":workers, "checkpoint":bundle, "checkpoint_hashes":hashes, "source_sha256":format!("{:x}",Sha256::digest(include_bytes!("strategy_probe.rs"))), "catalog_sha256":format!("{:x}",Sha256::digest(CATALOG.as_bytes())), "catalog":catalog, "results":results});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, &report).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    if failed {
        return Err("some branches failed; errors retained in report".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("REFUSED: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_preserves_sampling_and_captures_the_scored_inputs() {
        let store = ContentStore::embedded();
        let (state, player, home) = fixture("jolnar", "shortage").unwrap();
        let choice = ProductionWindow::new(&state, store, POK, &player, &home)
            .pending_choice(&state, store, POK)
            .unwrap();
        let vocabulary =
            ti4_policy::vocabulary::Vocabulary::build(std::iter::empty::<&str>()).unwrap();
        let actor = Rc::new(ti4_mlp::Actor::zeros(ti4_mlp::Width::W128, 4096));
        let row = ti4_mlp::FactionRow::of("jolnar").unwrap();
        let mut traced = MlpBot::sharing(&actor, vocabulary.clone(), row, 42);
        let handle = traced.decision_trace();
        let (mut traced, traced_status) = traced.seat();
        let (mut plain, plain_status) = MlpBot::sharing(&actor, vocabulary, row, 42).seat();
        for _ in 0..20 {
            let ask = |d: &mut dyn Decider| {
                ti4_engine::choice::ask_private(&choice, &state, store, POK, None, d).unwrap()
            };
            assert_eq!(ask(traced.as_mut()), ask(plain.as_mut()));
            let entries = std::mem::take(&mut *handle.borrow_mut());
            assert_eq!(entries.len(), 1);
            let t = &entries[0];
            assert_eq!(t.choice.ids(), choice.ids());
            let expected = actor.probabilities(&t.options, &t.head, row, 1.0).unwrap();
            assert_eq!(t.probabilities, expected);
            assert_eq!(t.vectors.len(), choice.options.len());
        }
        assert_eq!(traced_status.into_result().unwrap(), 20);
        assert_eq!(plain_status.into_result().unwrap(), 20);
    }

    #[test]
    fn contrasts_change_the_intended_constraint() {
        let store = ContentStore::embedded();
        let (base, p, home) = fixture("hacan", "funded").unwrap();
        let (rich, _, _) = fixture("hacan", "more-money").unwrap();
        let (docks, _, _) = fixture("hacan", "extra-dock").unwrap();
        let cap = |s| production::capacity(s, store, POK, &p, &home);
        assert!(cap(&base) > 0);
        assert_eq!(cap(&base), cap(&rich));
        assert!(cap(&docks) > cap(&base));
        assert_eq!(
            rich.player(&p).unwrap().trade_goods - base.player(&p).unwrap().trade_goods,
            20
        );
        let (short, p, home) = fixture("jolnar", "shortage").unwrap();
        let (stocked, _, _) = fixture("jolnar", "stocked").unwrap();
        let (empty, _, _) = fixture("jolnar", "no-cash").unwrap();
        let infantry = |s: &GameState| {
            s.system_state(&home)
                .planet_units
                .values()
                .flatten()
                .filter(|u| u.owner == p && u.type_id.as_str() == "infantry")
                .count()
        };
        assert_eq!(infantry(&short), 2);
        assert_eq!(infantry(&stocked), 8);
        assert_eq!(
            production::available(&empty, store, POK, &p, production::Spend::Resources),
            0
        );
        assert!(
            short
                .player(&p)
                .unwrap()
                .technologies
                .contains(&ti4_model::id::TechnologyId::new("st"))
        );
    }

    #[test]
    fn zero_actor_trace_and_controlled_branches_are_reproducible() {
        // Empty vocabulary is valid for this fixture-only integration check: every fact
        // deliberately maps to OOV. The real run uses the unchanged checkpoint vocabulary.
        let vocabulary =
            ti4_policy::vocabulary::Vocabulary::build(std::iter::empty::<&str>()).unwrap();
        let actor = Rc::new(ti4_mlp::Actor::zeros(ti4_mlp::Width::W128, 4096));
        for branch in ["mlp", "infantry-first", "stop-first"] {
            let first = run(&actor, &vocabulary, "jolnar", "shortage", branch, 61016001).unwrap();
            assert_eq!(
                first,
                run(&actor, &vocabulary, "jolnar", "shortage", branch, 61016001).unwrap()
            );
            assert!(!first["decisions"].as_array().unwrap().is_empty());
            if branch == "stop-first" {
                assert!(first["produced"].as_array().unwrap().is_empty());
            }
        }
    }
}
