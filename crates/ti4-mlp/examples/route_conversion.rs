//! Candidate-vs-frozen-panel conversion for one named public objective, matching
//! `VP_SOURCES_2026-09-09.md`'s own methodology (scored / revealed, counted over the candidate's
//! own seat across rotations) so a result here is directly comparable to its 25.0% Infrastructure
//! baseline. NEGLECTED_SCORING_PLAN_2026-09-09 Stage 3 screen, isolated worktree only.
//!
//! One candidate bundle holds each of six seats in turn (six rotations, `seated_faction` keeps
//! "seat{i}" as a fixed physical position across them); the other five seats are frozen copies of
//! the opponent bundle. `revealed` counts games where the target objective ever entered
//! `state.revealed_objectives`; `scored` counts the subset where the candidate seat's own
//! `scored_by` includes it. Mean VP is reported alongside as the secondary, higher-power metric.

use rayon::prelude::*;

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, ObjectiveId, PlayerId};
use ti4_training::rollout::{Horizon, OpeningMap};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn main() {
    let candidate_path =
        argument("--candidate").unwrap_or_else(|| refuse("--candidate is required"));
    let opponent_path = argument("--opponent").unwrap_or_else(|| refuse("--opponent is required"));
    let target = argument("--target").unwrap_or_else(|| "infrastructure".to_owned());
    // Either a contiguous range (--seeds/--seed-base) or an explicit list (--seed-list, one u64
    // per line) -- the latter for evaluating only the seeds already known to reveal `target`,
    // which "revealed" is seed-determined enough to make worth skipping the ~85-90% that don't.
    let explicit_seeds: Option<Vec<u64>> = argument("--seed-list").map(|path| {
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| refuse(&format!("reading {path}: {error}")))
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.parse()
                    .unwrap_or_else(|_| refuse(&format!("{path}: {line:?} is not a u64")))
            })
            .collect()
    });
    let seed_list: Vec<u64> = explicit_seeds.unwrap_or_else(|| {
        let seeds: u64 = argument("--seeds")
            .unwrap_or_else(|| refuse("--seeds or --seed-list is required"))
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a u64"));
        let seed_base: u64 = argument("--seed-base")
            .unwrap_or_else(|| refuse("--seed-base is required"))
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects a u64"));
        (seed_base..seed_base + seeds).collect()
    });
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });
    let rounds: u32 = argument("--rounds").map_or(4, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds expects a u32"))
    });
    let pool_path = argument("--map-pool").unwrap_or_else(|| refuse("--map-pool is required"));

    ti4_tensor::configure_deterministic(20_260_909)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let candidate = ti4_mlp::bundle::read(std::path::Path::new(&candidate_path))
        .unwrap_or_else(|error| refuse(&format!("reading {candidate_path}: {error}")));
    let candidate_actor = std::rc::Rc::new(
        candidate
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );
    let candidate_vocab = candidate.vocabulary;

    let opponent = ti4_mlp::bundle::read(std::path::Path::new(&opponent_path))
        .unwrap_or_else(|error| refuse(&format!("reading {opponent_path}: {error}")));
    let opponent_actor = std::rc::Rc::new(
        opponent
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );
    let opponent_vocab = opponent.vocabulary;

    let content = ContentStore::embedded();
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[
            ti4_sim::artifacts::ArtifactRole::Train,
            ti4_sim::artifacts::ArtifactRole::Validation,
        ],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();
    let target_id = ObjectiveId::new(target.clone());

    println!("route conversion: {target}");
    println!("  candidate   {candidate_path}");
    println!("  opponent    {opponent_path} (x5, frozen)");
    println!(
        "  seeds       {} explicit seeds x {} rotations",
        seed_list.len(),
        FACTIONS.len()
    );
    println!("  temperature {temperature} | rounds {rounds}");

    let mut revealed_games = 0u64;
    let mut scored_games = 0u64;
    let mut per_objective: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut vp_values: Vec<f64> = Vec::new();
    let mut total_games = 0u64;
    let mut vp_sum = 0f64;
    let mut errors = 0u64;

    // One chunk per rayon worker, each carrying its own owned actor copies: `tch::Tensor` is
    // `Send` but not `Sync`, so an actor cannot be borrowed across threads. The copies are made
    // here, on this thread, and moved in -- the same shape `ppo_update` uses for its rollouts.
    // Results are collected in chunk order and flattened in job order, so the tallies do not
    // depend on which worker finished first.
    let jobs: Vec<(u64, usize)> = seed_list
        .iter()
        .copied()
        .flat_map(|seed| (0..FACTIONS.len()).map(move |seat| (seed, seat)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    eprintln!(
        "  workers     {workers} ({} games, {per_worker} per worker)",
        jobs.len()
    );
    let chunks: Vec<(ti4_mlp::Actor, ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| {
            (
                candidate_actor.inference_copy(),
                opponent_actor.inference_copy(),
                chunk.to_vec(),
            )
        })
        .collect();

    type Tally = (u64, u64, u64, u64, Vec<f64>, BTreeMap<String, (u64, u64)>);
    let harvest: Vec<Tally> = chunks
        .into_par_iter()
        .map(|(cand, opp, chunk)| {
            let candidate_actor = std::rc::Rc::new(cand);
            let opponent_actor = std::rc::Rc::new(opp);
            let mut total_games = 0u64;
            let mut revealed_games = 0u64;
            let mut scored_games = 0u64;
            let mut errors = 0u64;
            let mut vp_values: Vec<f64> = Vec::new();
            let mut per_objective: BTreeMap<String, (u64, u64)> = BTreeMap::new();
            for (seed, candidate_seat) in chunk {
            let candidate_player = PlayerId::new(format!("seat{candidate_seat}"));
            let outcome = ti4_training::rollout::audit_game_with_deciders(
                content,
                &factions,
                DEFAULT,
                seed,
                candidate_seat,
                Horizon {
                    rounds,
                    steps: 200_000,
                },
                &OpeningMap::PythonPool {
                    pool: Arc::clone(&pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                |assignments, baselines| {
                    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                    for index in 0..FACTIONS.len() {
                        let player = PlayerId::new(format!("seat{index}"));
                        let faction = assignments
                            .get(&player)
                            .ok_or_else(|| format!("{player} has no faction"))?;
                        let is_candidate = player == candidate_player;
                        let row = ti4_mlp::FactionRow::of(faction.as_str())
                            .map_err(|error| format!("{player}: {error}"))?;
                        let stream = seed
                            .wrapping_mul(1_000_003)
                            .wrapping_add(u64::try_from(index).unwrap_or(0));
                        let baseline = baselines
                            .get(&player)
                            .copied()
                            .ok_or_else(|| format!("{player} has no setup baseline"))?;
                        let (actor, vocab) = if is_candidate {
                            (&candidate_actor, &candidate_vocab)
                        } else {
                            (&opponent_actor, &opponent_vocab)
                        };
                        let (decider, _status) =
                            ti4_mlp::bot::MlpBot::sharing(actor, vocab.clone(), row, stream)
                                .from_setup(baseline)
                                .at_temperature(temperature)
                                .seat();
                        deciders.insert(player, decider);
                    }
                    Ok(deciders)
                },
            );
            let (_events, _picks, _assignments, _openings, final_state) = match outcome {
                Ok(audited) => audited,
                Err(error) => {
                    errors += 1;
                    eprintln!("seed {seed} rotation {candidate_seat}: {error}");
                    continue;
                }
            };
            total_games += 1;
            let was_revealed = final_state.revealed_objectives.contains(&target_id);
            if was_revealed {
                revealed_games += 1;
                if final_state.scored_by(&candidate_player).contains(&target_id) {
                    scored_games += 1;
                }
            }
            // Every revealed objective, not just the named target: the non-economy curriculum is
            // meant to lift several routes at once, and its main risk is giving up economy
            // conversion to do it. One target cannot show either.
            let scored_here = final_state.scored_by(&candidate_player);
            for id in &final_state.revealed_objectives {
                let entry = per_objective
                    .entry(id.as_str().to_owned())
                    .or_insert((0u64, 0u64));
                entry.0 += 1;
                if scored_here.contains(id) {
                    entry.1 += 1;
                }
            }
            let vp = final_state
                .player(&candidate_player)
                .map_or(0, |seat| seat.victory_points);
            vp_values.push(f64::from(vp));
            }
            (
                total_games,
                revealed_games,
                scored_games,
                errors,
                vp_values,
                per_objective,
            )
        })
        .collect();

    for (games, revealed, scored, errs, vps, objectives) in harvest {
        total_games += games;
        revealed_games += revealed;
        scored_games += scored;
        errors += errs;
        for v in &vps {
            vp_sum += v;
        }
        vp_values.extend(vps);
        for (alias, (revealed, scored)) in objectives {
            let entry = per_objective.entry(alias).or_insert((0, 0));
            entry.0 += revealed;
            entry.1 += scored;
        }
    }

    let conversion = if revealed_games > 0 {
        scored_games as f64 / revealed_games as f64 * 100.0
    } else {
        f64::NAN
    };
    let mean_vp = vp_sum / total_games.max(1) as f64;
    // Standard error over candidate-games. Rotations of one seed share a map and deck, so the
    // independent unit is the seed, not the game: this is optimistic by roughly sqrt(rotations)
    // and is reported as a floor, not a confidence claim.
    let variance = vp_values
        .iter()
        .map(|v| (v - mean_vp).powi(2))
        .sum::<f64>()
        / (vp_values.len().max(2) - 1) as f64;
    let se_games = (variance / vp_values.len().max(1) as f64).sqrt();
    let se_clustered = se_games * (FACTIONS.len() as f64).sqrt();

    println!(
        "\n{scored_games}/{revealed_games} scored when revealed ({conversion:.1}%) over {total_games} candidate-games ({errors} errors)"
    );
    println!(
        "mean VP {mean_vp:.4}  sd {:.4}  se(game-level) {se_games:.4}  se(seed-clustered) {se_clustered:.4}",
        variance.sqrt()
    );
    println!("\nper-objective conversion (scored/revealed):");
    let mut rows: Vec<(&String, &(u64, u64))> = per_objective.iter().collect();
    rows.sort_by(|a, b| (b.1).0.cmp(&(a.1).0));
    for (alias, (revealed, scored)) in rows {
        let rate = *scored as f64 / (*revealed).max(1) as f64 * 100.0;
        println!("  {alias:<22} {scored:>5}/{revealed:<5} {rate:>5.1}%");
    }
}
