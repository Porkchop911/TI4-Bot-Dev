//! Reachable Sol research/expansion counterfactuals. No training or authored inference policy.
//! --bundle <checkpoint> --out <new report.json> [--seeds 4] [--workers 16]
//! [--seed-base 610170001] [--map-pool <Train-role pool>] [--rounds 4]

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, io::Write, path::Path, rc::Rc, sync::Arc};
use ti4_content::ContentStore;
use ti4_engine::{
    choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation},
    game::Game,
};
use ti4_mlp::{
    Actor, FactionRow,
    bot::{DecisionTrace, InferenceStatus, MlpBot},
};
use ti4_model::{
    content_types::DEFAULT as SCOPE,
    id::{FactionId, PlayerId},
    state::Phase,
};
use ti4_policy::{
    progress::{self, Baseline},
    vocabulary::Vocabulary,
};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const CATALOG: &str = include_str!("../../../docs/strategy-wiki/MLP_CASE_CATALOG.json");

fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn json_hash(value: &impl Serialize) -> String {
    hash(serde_json::to_vec(value).expect("serializable diagnostic"))
}

fn gate(choice: &Choice) -> Option<&'static str> {
    if choice.details.get("kind").and_then(Value::as_str) == Some("strategy_secondary")
        && choice
            .details
            .get("card")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                ti4_engine::strategy_cards::card_name(ContentStore::embedded(), id).as_deref()
                    == Some("Technology")
            })
        && choice.option("yes").is_some()
        && choice.option("no").is_some()
    {
        return Some("technology-secondary");
    }
    if choice
        .context
        .as_ref()
        .is_some_and(|c| c.subtype == "research_technology")
        && choice.options.iter().any(ChoiceOption::is_decline)
        && choice.options.iter().any(|o| {
            o.kind == "research"
                && o.payload
                    .get("cost")
                    .and_then(Value::as_i64)
                    .is_some_and(|c| c > 0)
        })
    {
        return Some("paid-research");
    }
    None
}

fn card_progress(cards: Vec<ti4_engine::objectives::CardProgress>) -> Value {
    json!(cards.into_iter().map(|p| json!({"alias":p.alias,"family":p.family_token,"have":p.have,"threshold":p.threshold,"satisfied":p.satisfied})).collect::<Vec<_>>())
}

fn budget_gate(choice: &Choice, seen: &SeatObservation<'_>) -> bool {
    let p = seen.bound_seat();
    seen.round() == 1
        && seen.seat(p).is_some_and(|s| {
            s.strategic_tokens > 0
                && s.strategic_tokens <= 2
                && !s.technologies.iter().any(|t| t.as_str() == "gd")
        })
        && seen.available_spend(p, ti4_engine::production::Spend::Resources) >= 4
        && choice.details.get("kind").and_then(Value::as_str) == Some("strategy_secondary")
        && choice.details.get("costs_token").and_then(Value::as_bool) == Some(true)
        && choice
            .details
            .get("card")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                ti4_engine::strategy_cards::card_name(ContentStore::embedded(), id).as_deref()
                    != Some("Technology")
            })
        && choice.option("yes").is_some()
        && choice.option("no").is_some()
}

fn visible(seen: &SeatObservation<'_>, baseline: Baseline) -> Value {
    let p = seen.bound_seat();
    let seat = seen.seat(p).expect("bound seat exists");
    let planets: Vec<_> = seen
        .controlled_planets(p)
        .into_iter()
        .map(|(s, p)| json!({"system":s,"planet":p,"ready":seen.planet_is_ready(p)}))
        .collect();
    json!({"round":seen.round(),"phase":format!("{:?}",seen.phase()),"progress":progress::measure(seen.observed(),p,baseline),
        "resources":seen.available_spend(p,ti4_engine::production::Spend::Resources),"influence":seen.available_spend(p,ti4_engine::production::Spend::Influence),
        "technologies":seat.technologies,"has_gravity_drive":seat.technologies.iter().any(|t|t.as_str()=="gd"),
        "tactic_tokens":seat.tactic_tokens,"strategic_tokens":seat.strategic_tokens,"fleet_tokens":seat.fleet_tokens,
        "controlled_planets":planets,"public_progress":card_progress(seen.revealed_objective_progress(p)),"own_secret_progress":card_progress(seen.held_secret_progress())})
}

fn trace_value(
    trace: &DecisionTrace,
    actor: &Actor,
    vocabulary: &Vocabulary,
) -> Result<Value, String> {
    let logits = ti4_tensor::to_vec(
        &actor
            .logits(
                &trace.options,
                &trace.head,
                FactionRow::of("sol").map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let options: Vec<_> = trace.choice.options.iter().enumerate().map(|(i,o)| {
        let facts:Vec<_> = trace.vectors[i].iter().map(|(k,v)|json!({"name":ti4_policy::intern::name_of(*k),"value":v,"column":vocabulary.column_of_key(*k),"assigned":vocabulary.is_assigned_key(*k)})).collect();
        json!({"option":o,"facts":facts,"columns":trace.options[i].columns,"values":trace.options[i].values,"logit":logits[i],"probability":trace.probabilities[i]})
    }).collect();
    Ok(json!({"head":trace.head,"options":options,"chosen":trace.choice.options[trace.chosen].id}))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Target {
    index: usize,
    round: u32,
    kind: String,
    cost: i64,
    prefix: String,
    root_hash: String,
    natural: String,
    take: String,
    skip: String,
}

#[derive(Default)]
struct Journal {
    prefix: Sha256,
    choices: usize,
    roots: Vec<Value>,
    timeline: Vec<Value>,
    forced: bool,
}

struct Watch {
    inner: Box<dyn Decider>,
    sol: bool,
    reserve: bool,
    baseline: Baseline,
    actor: Rc<Actor>,
    vocabulary: Vocabulary,
    trace: Option<Rc<RefCell<Vec<DecisionTrace>>>>,
    journal: Rc<RefCell<Journal>>,
    intervention: Option<(Target, String)>,
}

impl Watch {
    fn failed(choice: &Choice, reason: String) -> IllegalChoice {
        IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason,
        }
    }
}

impl Decider for Watch {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let proposed = self.inner.choose_seeing(choice, seen)?;
        let traces = self
            .trace
            .as_ref()
            .map(|t| std::mem::take(&mut *t.borrow_mut()))
            .unwrap_or_default();
        let mut j = self.journal.borrow_mut();
        let index = j.choices;
        let prefix = format!("{:x}", j.prefix.clone().finalize());
        let mut executed = proposed.clone();
        let kind = if !self.sol {
            None
        } else if self.reserve {
            budget_gate(choice, seen).then_some("pre-gd-secondary")
        } else {
            gate(choice)
        };
        let view = self.sol.then(|| visible(seen, self.baseline));
        let mut root = None;
        if let Some(kind) = kind {
            if traces.len() != 1 {
                return Err(Self::failed(
                    choice,
                    format!("research gate has {} actual actor traces", traces.len()),
                ));
            }
            let actor_input = trace_value(&traces[0], &self.actor, &self.vocabulary)
                .map_err(|e| Self::failed(choice, e))?;
            let root_hash = json_hash(&json!({"choice":choice,"view":view,"actor":actor_input}));
            let skip = choice
                .options
                .iter()
                .find(|o| o.is_decline() || o.id == "no")
                .ok_or_else(|| Self::failed(choice, "no skip option".into()))?;
            let take = if kind == "technology-secondary" || kind == "pre-gd-secondary" {
                choice.option("yes").expect("gate requires yes")
            } else if proposed.kind == "research" {
                &proposed
            } else {
                // When the baseline declines, test the model's highest-logit legal research.
                let best = actor_input["options"]
                    .as_array()
                    .expect("array")
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| choice.options[*i].kind == "research")
                    .max_by(|(ia, a), (ib, b)| {
                        a["logit"]
                            .as_f64()
                            .unwrap()
                            .total_cmp(&b["logit"].as_f64().unwrap())
                            .then_with(|| ib.cmp(ia))
                    })
                    .map(|(i, _)| i)
                    .ok_or_else(|| Self::failed(choice, "no take option".into()))?;
                &choice.options[best]
            };
            let cost = choice
                .options
                .iter()
                .filter_map(|o| o.payload.get("cost").and_then(Value::as_i64))
                .max()
                .unwrap_or(if kind == "pre-gd-secondary" { 0 } else { 4 });
            let target = Target {
                index,
                round: seen.round(),
                kind: kind.into(),
                cost,
                prefix: prefix.clone(),
                root_hash: root_hash.clone(),
                natural: proposed.id.clone(),
                take: take.id.clone(),
                skip: skip.id.clone(),
            };
            root = Some(json!({"target":target,"choice":choice,"view":view,"actor":actor_input}));
        }
        if let Some((target, answer)) = &self.intervention {
            if index == target.index {
                let actual: Target = serde_json::from_value(
                    root.as_ref().ok_or_else(|| {
                        Self::failed(choice, "target index is not a Sol research gate".into())
                    })?["target"]
                        .clone(),
                )
                .map_err(|e| Self::failed(choice, e.to_string()))?;
                if actual.prefix != target.prefix
                    || actual.root_hash != target.root_hash
                    || actual.natural != target.natural
                {
                    return Err(Self::failed(
                        choice,
                        "counterfactual prefix/root/model choice differs from baseline".into(),
                    ));
                }
                executed = choice
                    .option(answer)
                    .cloned()
                    .ok_or_else(|| Self::failed(choice, "target answer not offered".into()))?;
                j.forced = true;
            }
        }
        if let Some(mut root) = root {
            root["executed"] = json!(executed.id);
            j.roots.push(root);
        }
        if self.sol {
            j.timeline.push(json!({"index":index,"round":seen.round(),"phase":format!("{:?}",seen.phase()),"head":ti4_policy::learned::decision_head(choice),"context":choice.context,"proposed":proposed.id,"executed":executed,"view":view}));
        }
        let bytes = serde_json::to_vec(&json!({"choice":choice,"executed":executed.id}))
            .expect("choice serializes");
        j.prefix.update((bytes.len() as u64).to_le_bytes());
        j.prefix.update(bytes);
        j.choices += 1;
        Ok(executed)
    }
    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

fn checkpoint(game: &Game<'_>, baseline: Baseline, label: &str) -> Value {
    let p = PlayerId::new("seat0");
    let seen = Observed::new(&game.state, ContentStore::embedded(), SCOPE, game.galaxy());
    let seat = game.state.player(&p).expect("Sol seated");
    let mut units = Vec::new();
    for (system, s) in &game.state.board {
        for u in s.units.iter().filter(|u| u.owner == p) {
            units.push(json!({"system":system,"planet":null,"type":u.type_id}));
        }
        for (planet, us) in &s.planet_units {
            for u in us.iter().filter(|u| u.owner == p) {
                units.push(json!({"system":system,"planet":planet,"type":u.type_id}));
            }
        }
    }
    json!({"label":label,"round":game.state.round,"phase":format!("{:?}",game.state.phase),"progress":progress::measure(&seen,&p,baseline),
        "technologies":seat.technologies,"scored_public":game.state.scored_by(&p),"controlled_planets":seen.controlled_planets(&p),"units":units,
        "public_progress":card_progress(seen.revealed_objective_progress(&p))})
}

#[derive(Clone)]
struct Job {
    seed: u64,
    diplomacy: bool,
    reserve: bool,
    intervention: Option<(Target, String)>,
}

fn play(
    actor: &Rc<Actor>,
    vocabulary: &Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    rounds: u32,
    job: &Job,
) -> Value {
    let store = ContentStore::embedded();
    let players: Vec<_> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let factions = players
        .iter()
        .cloned()
        .zip(FACTIONS.map(FactionId::new))
        .collect();
    let journal = Rc::new(RefCell::new(Journal::default()));
    let mut statuses: Vec<InferenceStatus> = Vec::new();
    let mut sol_baseline = Baseline::default();
    let game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
        store,
        &players,
        &factions,
        SCOPE,
        job.seed,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: 0,
        },
        ti4_training::rollout::SimulationCapabilities {
            diplomacy: job.diplomacy,
        },
        |baselines| {
            sol_baseline = baselines[&players[0]];
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (i, p) in players.iter().enumerate() {
                let row = FactionRow::of(FACTIONS[i]).map_err(|e| e.to_string())?;
                let mut bot = MlpBot::sharing(
                    actor,
                    vocabulary.clone(),
                    row,
                    job.seed.wrapping_mul(1_000_003).wrapping_add(i as u64),
                )
                .at_temperature(0.001)
                .from_setup(baselines[p]);
                let trace = (i == 0).then(|| bot.decision_trace());
                let (inner, status) = bot.seat();
                statuses.push(status);
                deciders.insert(
                    p.clone(),
                    Box::new(Watch {
                        inner,
                        sol: i == 0,
                        reserve: job.reserve,
                        baseline: baselines[p],
                        actor: Rc::clone(actor),
                        vocabulary: vocabulary.clone(),
                        trace,
                        journal: Rc::clone(&journal),
                        intervention: job.intervention.clone(),
                    }),
                );
            }
            Ok(deciders)
        },
    );
    let mut game = match game {
        Ok(g) => g,
        Err(e) => {
            return json!({"seed":job.seed,"diplomacy":job.diplomacy,"intervention":job.intervention,"error":e,"stage":"setup"});
        }
    };
    let initial_hash = json_hash(&game.state);
    let mut snapshots = vec![checkpoint(&game, sol_baseline, "initial")];
    let mut error = None;
    let started = std::time::Instant::now();
    let mut steps = 0;
    while game.state.round <= rounds && !game.state.finished {
        if steps >= 40_000 || started.elapsed().as_secs() >= 120 {
            error = Some("step/time bound reached".to_owned());
            break;
        }
        let phase = game.state.phase;
        let round = game.state.round;
        let result = game.step();
        steps += 1;
        if let Some(e) = result.error {
            error = Some(e.to_string());
            break;
        }
        if phase == Phase::Action && game.state.phase == Phase::Status {
            snapshots.push(checkpoint(&game, sol_baseline, "end-action/pre-status"));
        }
        if game.state.round != round {
            snapshots.push(checkpoint(&game, sol_baseline, "next-round-start"));
        }
    }
    let inference: Vec<_> = statuses
        .into_iter()
        .map(|s| s.into_result().map_err(|e| e.to_string()))
        .collect();
    if inference.iter().any(Result::is_err) {
        error = Some(format!("inference status {inference:?}"));
    }
    let j = journal.borrow();
    if job.intervention.is_some() && !j.forced {
        error = Some(format!(
            "{}; target never reached",
            error.unwrap_or_default()
        ));
    }
    let cutoff = game.state.round > rounds || game.state.finished;
    if !cutoff && error.is_none() {
        error = Some("horizon not reached".into());
    }
    snapshots.push(checkpoint(&game, sol_baseline, "final"));
    json!({"seed":job.seed,"diplomacy":job.diplomacy,"intervention":job.intervention,"initial_hash":initial_hash,"error":error,
        "outcome":{"state_hash":json_hash(&game.state),"action_digest":format!("{:x}",j.prefix.clone().finalize()),"choices":j.choices,"steps":steps,"finished":game.state.finished,"reached_cutoff":game.state.round>rounds,"snapshots":snapshots},
        "roots":j.roots,"timeline":j.timeline,"inference":inference,"target_applied":j.forced})
}

fn parallel(
    actor: &Actor,
    vocabulary: &Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    rounds: u32,
    jobs: Vec<Job>,
    workers: usize,
) -> Vec<Value> {
    if jobs.is_empty() {
        return Vec::new();
    }
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<_> = jobs
        .chunks(per_worker)
        .map(|j| (actor.inference_copy(), j.to_vec()))
        .collect();
    chunks
        .into_par_iter()
        .map(|(actor, jobs)| {
            let actor = Rc::new(actor);
            jobs.into_iter()
                .map(|j| {
                    let r = play(&actor, vocabulary, pool, rounds, &j);
                    eprintln!(
                        "Sol seed{} diplomacy{} target{:?}: {}",
                        j.seed,
                        j.diplomacy,
                        j.intervention.as_ref().map(|(t, a)| (t.round, t.index, a)),
                        if r["error"].is_null() {
                            "complete"
                        } else {
                            "FAILED"
                        }
                    );
                    r
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect()
}

fn select_targets(baseline: &Value) -> Result<Vec<Target>, String> {
    let mut by_round: BTreeMap<u32, Target> = BTreeMap::new();
    for root in baseline["roots"].as_array().ok_or("no roots array")? {
        let t: Target =
            serde_json::from_value(root["target"].clone()).map_err(|e| e.to_string())?;
        let rank = |t: &Target| {
            if t.kind == "paid-research" && t.cost == 6 {
                0
            } else if t.kind == "technology-secondary" {
                1
            } else {
                2
            }
        };
        if by_round
            .get(&t.round)
            .is_none_or(|old| rank(&t) < rank(old))
        {
            by_round.insert(t.round, t);
        }
    }
    Ok(by_round.into_values().collect())
}

fn execute() -> Result<(), String> {
    let mut args = BTreeMap::new();
    let mut input = std::env::args().skip(1);
    while let Some(flag) = input.next() {
        if ![
            "--bundle",
            "--out",
            "--seeds",
            "--seed-base",
            "--workers",
            "--map-pool",
            "--rounds",
            "--mode",
        ]
        .contains(&flag.as_str())
            || args.contains_key(&flag)
        {
            return Err(format!("unknown/duplicate {flag}"));
        }
        args.insert(
            flag.clone(),
            input.next().ok_or(format!("missing value {flag}"))?,
        );
    }
    let number = |k: &str, d: u64| -> Result<u64, String> {
        args.get(k)
            .map_or(Ok(d), |v| v.parse().map_err(|_| format!("invalid {k}")))
    };
    let seeds = number("--seeds", 4)?;
    let mode = args.get("--mode").map_or("research", String::as_str);
    if !["research", "reserve-gd"].contains(&mode) {
        return Err("mode must be research or reserve-gd".into());
    }
    let base = number("--seed-base", 610_170_001)?;
    let workers = usize::try_from(number("--workers", 16)?).map_err(|_| "workers overflow")?;
    let rounds = u32::try_from(number("--rounds", 4)?).map_err(|_| "rounds overflow")?;
    if !(1..=16).contains(&seeds) || !(1..=16).contains(&workers) || !(1..=4).contains(&rounds) {
        return Err("seeds/workers 1..16; rounds1..4".into());
    }
    let end = base.checked_add(seeds).ok_or("seed overflow")?;
    let bundle = Path::new(args.get("--bundle").ok_or("--bundle required")?);
    let output = Path::new(args.get("--out").ok_or("--out required")?);
    if output.exists() || !output.parent().is_some_and(Path::is_dir) {
        return Err("output must be new within an existing directory".into());
    }
    let pool_path = args.get("--map-pool").map_or(
        "D:/Projects/ti4-engine-rs/out/pools/full_np8_12_train.json",
        String::as_str,
    );
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .map_err(|e| e.to_string())?;
    let pool_hash = hash(&pool_bytes);
    let map_pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(pool_bytes))
            .map_err(|e| e.to_string())?,
    );
    ti4_tensor::configure_deterministic(20_261_016).map_err(|e| e.to_string())?;
    let loaded = ti4_mlp::bundle::read(bundle).map_err(|e| e.to_string())?;
    let files: BTreeMap<_, _> = std::fs::read_dir(bundle)
        .map_err(|e| e.to_string())?
        .map(|e| {
            let e = e.map_err(|e| e.to_string())?;
            Ok((
                e.file_name().to_string_lossy().into_owned(),
                hash(std::fs::read(e.path()).map_err(|e| e.to_string())?),
            ))
        })
        .collect::<Result<_, String>>()?;
    let jobs: Vec<_> = (base..end)
        .flat_map(|seed| {
            [false, true].map(|diplomacy| Job {
                seed,
                diplomacy,
                reserve: mode == "reserve-gd",
                intervention: None,
            })
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .map_err(|e| e.to_string())?;
    let baseline = parallel(
        &loaded.actor,
        &loaded.vocabulary,
        &map_pool,
        rounds,
        jobs,
        workers,
    );
    let mut paired_jobs = Vec::new();
    let mut selections = Vec::new();
    for b in &baseline {
        if !b["error"].is_null() {
            continue;
        }
        let targets = select_targets(b)?;
        selections.push(json!({"seed":b["seed"],"diplomacy":b["diplomacy"],"targets":targets}));
        for t in targets {
            for answer in [t.take.clone(), t.skip.clone()] {
                paired_jobs.push(Job {
                    seed: b["seed"].as_u64().unwrap(),
                    diplomacy: b["diplomacy"].as_bool().unwrap(),
                    reserve: mode == "reserve-gd",
                    intervention: Some((t.clone(), answer)),
                });
            }
        }
    }
    let branches = parallel(
        &loaded.actor,
        &loaded.vocabulary,
        &map_pool,
        rounds,
        paired_jobs,
        workers,
    );
    let mut controls = Vec::new();
    for b in &branches {
        let (target, answer): (Target, String) =
            serde_json::from_value(b["intervention"].clone()).map_err(|e| e.to_string())?;
        if answer == target.natural {
            let base = baseline
                .iter()
                .find(|r| r["seed"] == b["seed"] && r["diplomacy"] == b["diplomacy"])
                .ok_or("baseline missing")?;
            controls.push(json!({"seed":b["seed"],"diplomacy":b["diplomacy"],"target":target.index,"equal":b["error"].is_null() && b["initial_hash"]==base["initial_hash"] && b["outcome"]==base["outcome"] && b["timeline"]==base["timeline"]}));
        }
    }
    for (name, digest) in &files {
        if hash(std::fs::read(bundle.join(name)).map_err(|e| e.to_string())?) != *digest {
            return Err("checkpoint changed".into());
        }
    }
    let failed = baseline
        .iter()
        .chain(&branches)
        .any(|r| !r["error"].is_null())
        || controls.iter().any(|c| c["equal"] != true);
    let report = json!({"schema":1,"status":"sol-reachable-diagnostic-not-training-labels","mode":mode,"seed_base":base,"seeds":seeds,"workers":workers,"rounds":rounds,"temperature":0.001,"sources":"DEFAULT/FULL (includes TE)","lineup":FACTIONS,"pool":pool_path,"pool_role":"train","pool_sha256":pool_hash,"checkpoint":bundle,"checkpoint_hashes":files,"source_sha256":hash(include_bytes!("sol_strategy_probe.rs")),"catalog_sha256":hash(CATALOG),"source_claims":["C17","C19"],"source_case":serde_json::from_str::<Value>(CATALOG).map_err(|e|e.to_string())?["families"].as_array().unwrap().iter().find(|f|f["id"]=="sol-research-versus-expansion"),"baseline":baseline,"selections":selections,"branches":branches,"natural_controls":controls});
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut file = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut file, &report).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.get_ref().sync_all().map_err(|e| e.to_string())?;
    if failed {
        return Err("incomplete/failed games or replay controls; retained report".into());
    }
    Ok(())
}

fn main() {
    if let Err(e) = execute() {
        eprintln!("REFUSED: {e}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserve_gate_checks_actual_seat_budget_and_first_gd_condition() {
        struct Check(Rc<RefCell<bool>>);
        impl Decider for Check {
            fn choose(&mut self, c: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                Ok(c.options[0].clone())
            }
            fn choose_seeing(
                &mut self,
                c: &Choice,
                s: &SeatObservation<'_>,
            ) -> Result<ChoiceOption, IllegalChoice> {
                *self.0.borrow_mut() = budget_gate(c, s);
                self.choose(c)
            }
        }
        let store = ContentStore::embedded();
        let p = PlayerId::new("seat0");
        let mut state = ti4_engine::fixtures::game(&["seat0"]);
        ti4_engine::seating::deploy(&mut state, store, &p, &FactionId::new("sol"), SCOPE).unwrap();
        let c = Choice::new(
            p.clone(),
            "budget",
            vec![
                ChoiceOption::new("no", "strategy"),
                ChoiceOption::new("yes", "strategy"),
            ],
        )
        .detailed("kind", "strategy_secondary")
        .detailed("card", "pok3politics")
        .detailed("costs_token", true);
        let observed = Rc::new(RefCell::new(false));
        let mut check = Check(Rc::clone(&observed));
        let mut ask = |state: &ti4_model::state::GameState| {
            ti4_engine::choice::ask_private(&c, state, store, SCOPE, None, &mut check).unwrap();
            *observed.borrow()
        };
        assert!(ask(&state));
        state.player_mut(&p).unwrap().strategic_tokens = 0;
        assert!(!ask(&state));
        state.player_mut(&p).unwrap().strategic_tokens = 1;
        state.round = 2;
        assert!(!ask(&state));
        state.round = 1;
        state
            .player_mut(&p)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("gd"));
        assert!(!ask(&state));
    }
    #[test]
    fn research_gate_requires_typed_or_display_source_and_real_options() {
        let p = PlayerId::new("seat0");
        let c = Choice::new(
            p.clone(),
            "arbitrary wording",
            vec![
                ChoiceOption::new("yes", "strategy"),
                ChoiceOption::new("no", "strategy"),
            ],
        )
        .detailed("kind", "strategy_secondary")
        .detailed("card", "pok7technology");
        assert_eq!(gate(&c), Some("technology-secondary"));
        let c = Choice::new(
            p,
            "research a technology",
            vec![
                ChoiceOption::new("gd", "research").with("cost", 4),
                ChoiceOption::decline(),
            ],
        );
        assert_eq!(
            gate(&c),
            None,
            "prompt text alone is not a typed paid offer"
        );
        let c = c.contextualized(ti4_engine::decision_context::DecisionContext::new(
            PlayerId::new("seat0"),
            ti4_engine::decision_context::DecisionSource::StrategyCard {
                card: "Technology".into(),
                secondary: true,
            },
            "research_technology",
            Phase::Action,
            1,
        ));
        assert_eq!(gate(&c), Some("paid-research"));
    }
    #[test]
    fn picks_one_gate_per_round_without_outcome_selection() {
        let target = |round, kind: &str, cost| Target {
            index: cost as usize,
            round,
            kind: kind.into(),
            cost,
            prefix: String::new(),
            root_hash: String::new(),
            natural: "no".into(),
            take: "yes".into(),
            skip: "no".into(),
        };
        let b = json!({"roots":[{"target":target(1,"technology-secondary",4)},{"target":target(1,"paid-research",6)},{"target":target(2,"technology-secondary",4)}]});
        let selected = select_targets(&b).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].cost, 6);
        assert_eq!(selected[1].round, 2);
    }
}
