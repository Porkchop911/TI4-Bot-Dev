//! Read-only VP attribution: fixed opponents; final VP reconciled with scored cards and ledger.
use rayon::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::{
    content_types::{ContentType, DEFAULT},
    id::{FactionId, PlayerId},
};
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
fn arg(k: &str) -> Option<String> {
    let a: Vec<_> = std::env::args().collect();
    a.windows(2).find(|p| p[0] == k).map(|p| p[1].clone())
}
#[derive(Default)]
struct Log {
    hash: Sha256,
    choices: Vec<Value>,
}
struct Watch {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<Log>>,
    capture: bool,
}
impl Watch {
    fn record(&self, c: &Choice, a: &ChoiceOption) {
        let mut l = self.log.borrow_mut();
        l.hash.update(format!("{c:?}{a:?}"));
        if self.capture {
            let subtype = c.context.as_ref().map(|x| x.subtype.as_str()).unwrap_or("");
            if subtype.contains("scor")
                || subtype.contains("secret")
                || ti4_policy::learned::decision_head(c) == "strategy"
            {
                l.choices.push(json!({"round":c.context.as_ref().map(|x|x.round),"subtype":subtype,"prompt":c.prompt,"options":c.options.iter().map(|o|o.id.clone()).collect::<Vec<_>>(),"chosen":a.id,"decline":a.is_decline()}));
            }
        }
    }
}
impl Decider for Watch {
    fn choose(&mut self, c: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let a = self.inner.choose(c)?;
        self.record(c, &a);
        Ok(a)
    }
    fn choose_seeing(
        &mut self,
        c: &Choice,
        s: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let a = self.inner.choose_seeing(c, s)?;
        self.record(c, &a);
        Ok(a)
    }
}
fn play(
    candidate: &Rc<ti4_mlp::Actor>,
    opponent: &Rc<ti4_mlp::Actor>,
    vocab: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    seed: u64,
    rotation: usize,
    seat: usize,
    capture: bool,
) -> Result<Value, String> {
    let players: Vec<_> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let me = &players[seat];
    let assignments: BTreeMap<_, _> = players
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), FactionId::new(FACTIONS[(i + rotation) % 6])))
        .collect();
    let log = Rc::new(RefCell::new(Log::default()));
    let mut statuses = Vec::new();
    let mut game = ti4_training::rollout::setup_game_with_decider_factory(
        ContentStore::embedded(),
        &players,
        &assignments,
        DEFAULT,
        seed,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: 0,
        },
        |baselines| {
            let mut bots: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (i, p) in players.iter().enumerate() {
                let actor = if i == seat { candidate } else { opponent };
                let (d, s) = ti4_mlp::bot::MlpBot::sharing(
                    actor,
                    vocab.clone(),
                    ti4_mlp::FactionRow::of(assignments[p].as_str()).unwrap(),
                    seed.wrapping_mul(1_000_003).wrapping_add(i as u64),
                )
                .at_temperature(0.001)
                .from_setup(baselines[p])
                .seat();
                statuses.push(s);
                bots.insert(
                    p.clone(),
                    Box::new(Watch {
                        inner: d,
                        log: Rc::clone(&log),
                        capture: capture && i == seat,
                    }),
                );
            }
            Ok(bots)
        },
    )?;
    let initial = game.state.player(me).unwrap().victory_points;
    let target = game.state.round + 4;
    let mut steps = 0;
    let mut awards = Vec::new();
    let mut ledger = Vec::new();
    let mut reveals: Vec<Value> = game
        .state
        .revealed_objectives
        .iter()
        .map(|id| json!({"id":id,"revealed_round":game.state.round,"available_round":game.state.round,"source":"initial"}))
        .collect();
    while !game.state.finished && game.state.round < target && steps < 400_000 {
        let round = game.state.round;
        let phase = game.state.phase;
        let old_revealed = game.state.revealed_objectives.clone();
        let old = game
            .state
            .scored_objectives
            .get(me)
            .cloned()
            .unwrap_or_default();
        let n = game.state.vp_ledger.len();
        let result = game.step();
        if let Some(e) = result.error {
            return Err(format!("{seed}/{rotation}/{seat}: {e:?}"));
        }
        steps += 1;
        if capture {
            for id in game
                .state
                .revealed_objectives
                .iter()
                .filter(|id| !old_revealed.contains(id))
            {
                // Status reveals happen after that round's objective-scoring step, so the card's
                // first normal scoring opportunity is the following round. Other effects can
                // reveal during another phase and leave a same-round opportunity.
                let available_round = if phase == ti4_model::Phase::Status {
                    round + 1
                } else {
                    round
                };
                reveals.push(json!({"id":id,"revealed_round":round,"available_round":available_round,"source":format!("{phase:?}")}));
            }
            if let Some(scored) = game.state.scored_objectives.get(me) {
                for id in scored.difference(&old) {
                    let content = ContentStore::embedded();
                    let public = content
                        .get(ContentType::PublicObjectives, id.as_str())
                        .is_some();
                    let points = ti4_engine::objectives::points_for(content, id)
                        .ok_or_else(|| format!("unknown score {id}"))?;
                    awards.push(json!({"round":round,"id":id,"kind":if public{"public"}else{"secret"},"points":points}));
                }
            }
            for (p, d, r) in &game.state.vp_ledger[n..] {
                if p == me {
                    ledger.push(json!({"round":round,"delta":d,"reason":r}));
                }
            }
        }
    }
    if !game.state.finished && game.state.round < target {
        return Err("step cap".into());
    }
    for s in statuses {
        s.into_result().map_err(|e| e.to_string())?;
    }
    let l = log.borrow();
    Ok(
        json!({"seed":seed,"rotation":rotation,"seat":seat,"faction":assignments[me],"initial":initial,"vp":game.state.player(me).unwrap().victory_points,"secret_hand":game.state.player(me).unwrap().secret_objectives,"reveals":reveals,"awards":awards,"ledger":ledger,"choices":l.choices,"hashes":[format!("{:x}",l.hash.clone().finalize()),format!("{:x}",Sha256::digest(serde_json::to_vec(&game.events).unwrap())),format!("{:x}",Sha256::digest(serde_json::to_vec(&game.state).unwrap()))]}),
    )
}
fn main() {
    ti4_tensor::configure_deterministic(20_260_821).unwrap();
    let bundle = arg("--bundle").expect("bundle");
    let opponent =
        arg("--opponent").unwrap_or("out/checkpoints/stage2-mlp-shaped/checkpoint-473312".into());
    let c = ti4_mlp::bundle::read(std::path::Path::new(&bundle)).unwrap();
    let o = ti4_mlp::bundle::read(std::path::Path::new(&opponent)).unwrap();
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_holdout.json"),
                &[ti4_sim::artifacts::ArtifactRole::Validation],
            )
            .unwrap(),
        ))
        .unwrap(),
    );
    let seeds: u64 = arg("--seeds").unwrap_or("4".into()).parse().unwrap();
    let base: u64 = arg("--seed-base")
        .unwrap_or("910001000".into())
        .parse()
        .unwrap();
    let threads: usize = arg("--threads").unwrap_or("24".into()).parse().unwrap();
    let verify = std::env::args().any(|a| a == "--verify");
    let jobs: Vec<_> = (base..base + seeds)
        .flat_map(|s| (0..6).flat_map(move |r| (0..6).map(move |p| (s, r, p))))
        .collect();
    let chunks: Vec<_> = jobs
        .chunks(jobs.len().div_ceil(threads))
        .map(|j| {
            (
                c.actor.inference_copy(),
                o.actor.inference_copy(),
                j.to_vec(),
            )
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .unwrap();
    let results: Vec<_> = chunks
        .into_par_iter()
        .flat_map_iter(|(ca, oa, j)| {
            let ca = Rc::new(ca);
            let oa = Rc::new(oa);
            j.into_iter()
                .map(|(s, r, p)| {
                    let a = play(&ca, &oa, &c.vocabulary, &pool, s, r, p, true).unwrap();
                    if verify {
                        let b = play(&ca, &oa, &c.vocabulary, &pool, s, r, p, false).unwrap();
                        assert_eq!(a["hashes"], b["hashes"]);
                    }
                    a
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for r in results {
        println!("{r}");
    }
}
