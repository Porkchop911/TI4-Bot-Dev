//! Experimental CUDA inference qualification, not a production rollout backend.
//!
//! One CPU four-round game supplies real, seat-authorized actor/critic inputs. Replay compares
//! ordinary CPU inference with mixed-head CPU/CUDA batches, including packing and transfers.
//! One unbatched CUDA game then checks the actual engine/decider integration. No weights change.
//! Usage: --bundle DIR --map-pool FILE [--seed N] [--samples 512] [--repeats 3]
//!        [--temperature 2.5] [--diplomacy]
//! JSONL goes to stdout. Rounds are fixed at four; errors, truncations and CUDA absence refuse.

#![deny(warnings)]

use std::{collections::BTreeMap, error::Error, path::Path, rc::Rc, sync::Arc, time::Instant};

use serde_json::json;
use sha2::{Digest, Sha256};
use ti4_engine::choice::Decider;
use ti4_mlp::{Actor, FactionRow, bundle::Loaded, ppo::Step};
use ti4_model::{
    content_types::DEFAULT,
    id::{FactionId, PlayerId},
};
use ti4_tensor::Device;
use ti4_training::rollout::{GameDigest, Horizon, OpeningMap, SimulationCapabilities};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const BATCHES: [usize; 3] = [1, 8, 32];
const PROBABILITY_TOLERANCE: f64 = 1e-3;
const VALUE_ABS_TOLERANCE: f64 = 1e-2;
const VALUE_REL_TOLERANCE: f64 = 1e-4;

struct Config {
    bundle: String,
    pool: String,
    seed: u64,
    samples: usize,
    repeats: usize,
    temperature: f64,
    diplomacy: bool,
}

impl Config {
    fn parse() -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut args = std::env::args().skip(1);
        let mut diplomacy = false;
        while let Some(key) = args.next() {
            if key == "--diplomacy" && !diplomacy {
                diplomacy = true;
                continue;
            }
            if ![
                "--bundle",
                "--map-pool",
                "--seed",
                "--samples",
                "--repeats",
                "--temperature",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unknown or repeated argument {key}").into());
            }
            let value = args.next().ok_or_else(|| format!("{key} needs a value"))?;
            if value.starts_with("--") || values.insert(key.clone(), value).is_some() {
                return Err(format!("missing value or repeated argument {key}").into());
            }
        }
        let required = |key: &str| {
            values
                .get(key)
                .cloned()
                .ok_or_else(|| format!("{key} required"))
        };
        let value =
            |key: &str, default: &str| values.get(key).cloned().unwrap_or_else(|| default.into());
        let config = Self {
            bundle: required("--bundle")?,
            pool: required("--map-pool")?,
            seed: value("--seed", "1261600101").parse()?,
            samples: value("--samples", "512").parse()?,
            repeats: value("--repeats", "3").parse()?,
            temperature: value("--temperature", "2.5").parse()?,
            diplomacy,
        };
        if !(32..=2048).contains(&config.samples)
            || !(1..=5).contains(&config.repeats)
            || !config.temperature.is_finite()
            || config.temperature <= 0.0
        {
            return Err(
                "samples must be 32..2048, repeats 1..5, temperature finite and positive".into(),
            );
        }
        Ok(config)
    }
}

fn synchronize(device: Device) {
    if let Device::Cuda(index) = device {
        tch::Cuda::synchronize(i64::try_from(index).expect("CUDA index fits"));
    }
}

fn game(
    config: &Config,
    loaded: &Loaded,
    pool: &Arc<ti4_sim::MapPool>,
    device: Device,
) -> Result<(Vec<Step>, GameDigest)> {
    let actor = Rc::new(loaded.actor.inference_copy().to_device(device));
    synchronize(device);
    let started = Instant::now();
    let players: Vec<_> = (0..6).map(|n| PlayerId::new(format!("seat{n}"))).collect();
    let factions: BTreeMap<_, _> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                ti4_training::rollout::seated_faction(
                    &FACTIONS.map(FactionId::new),
                    config.seed,
                    0,
                    index,
                ),
            )
        })
        .collect();
    let mut handles = Vec::new();
    let mut statuses = Vec::new();
    let (rollout, digest) =
        ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
            ti4_content::ContentStore::embedded(),
            &players,
            &factions,
            DEFAULT,
            config.seed,
            Horizon {
                rounds: 4,
                steps: 10_000,
            },
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            &OpeningMap::PythonPool {
                pool: Arc::clone(pool),
                tile_seed_offset: 20_000_000,
            },
            SimulationCapabilities {
                diplomacy: config.diplomacy,
            },
            true,
            |baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, player) in players.iter().enumerate() {
                    let row =
                        FactionRow::of(factions[player].as_str()).map_err(|e| e.to_string())?;
                    let baseline = *baselines.get(player).ok_or("missing setup baseline")?;
                    let stream = config
                        .seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(index as u64);
                    let bot = ti4_mlp::bot::MlpBot::sharing(
                        &actor,
                        loaded.vocabulary.clone(),
                        row,
                        stream,
                    )
                    .at_temperature(config.temperature)
                    .recording_ppo(loaded.critic_mode)
                    .from_setup(baseline);
                    handles.push(bot.ppo_records());
                    let (decider, status) = bot.seat();
                    statuses.push(status);
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        );
    if let Some(error) = rollout.error {
        return Err(error.into());
    }
    let decisions = statuses
        .into_iter()
        .map(ti4_mlp::bot::InferenceStatus::into_result)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<usize>();
    let mut records = Vec::new();
    for handle in handles {
        records.extend(handle.borrow_mut().drain(..).map(|record| record.step));
    }
    if records.is_empty() {
        return Err("game recorded no decisions".into());
    }
    synchronize(device);
    let digest = digest.ok_or("missing game digest")?;
    let vp: Vec<_> = rollout
        .seats
        .iter()
        .map(|seat| (&seat.faction, seat.episode.final_progress.victory_points))
        .collect();
    println!(
        "{}",
        json!({"kind":"live_game", "device":format!("{device:?}"), "rounds":4,
        "wall_s":started.elapsed().as_secs_f64(), "decisions":decisions, "recorded":records.len(),
        "vp":vp, "state_sha256":digest.state_sha256, "events_sha256":digest.log_sha256,
        "events":digest.events, "batching":"none", "errors":0})
    );
    Ok((records, digest))
}

struct Prediction {
    probabilities: Vec<f64>,
    value: Option<f64>,
}

// This deliberately calls the existing production entry points for the reference.
fn reference(actor: &Actor, steps: &[Step]) -> Result<Vec<Prediction>> {
    steps
        .iter()
        .map(|step| {
            Ok(Prediction {
                probabilities: actor.probabilities(
                    &step.options,
                    ti4_mlp::all_heads()[step.head],
                    step.row,
                    step.temperature,
                )?,
                value: step
                    .critic
                    .as_ref()
                    .map(|critic| actor.value(critic, step.row))
                    .transpose()?,
            })
        })
        .collect()
}

fn batched(
    actor: &Actor,
    steps: &[Step],
    width: usize,
    temperature: f64,
) -> Result<Vec<Prediction>> {
    let mut output = Vec::with_capacity(steps.len());
    for chunk in steps.chunks(width) {
        let mut options = Vec::new();
        let mut heads = Vec::new();
        let mut rows = Vec::new();
        for step in chunk {
            options.extend_from_slice(&step.options);
            heads.extend(std::iter::repeat_n(
                i64::try_from(step.head)?,
                step.options.len(),
            ));
            rows.extend(std::iter::repeat_n(
                i64::try_from(step.row.index())?,
                step.options.len(),
            ));
        }
        // One actor output transfer per batch. Sparse input packing and upload happen inside logits_mixed.
        let logits =
            (actor.logits_mixed(&options, &heads, &rows)? / temperature).to_device(Device::Cpu);
        let critics: Vec<_> = chunk
            .iter()
            .filter_map(|step| step.critic.as_ref())
            .collect();
        let values = if critics.is_empty() {
            None
        } else {
            if critics.len() != chunk.len() {
                return Err("mixed critic presence".into());
            }
            let rows: Vec<_> = chunk
                .iter()
                .map(|s| i64::try_from(s.row.index()))
                .collect::<std::result::Result<_, _>>()?;
            Some(ti4_tensor::to_vec(
                &actor.value_batch(&critics, &rows)?.to_device(Device::Cpu),
            )?)
        };
        let mut offset = 0;
        for (index, step) in chunk.iter().enumerate() {
            let length = i64::try_from(step.options.len())?;
            output.push(Prediction {
                probabilities: ti4_mlp::stable_softmax(&logits.narrow(0, offset, length))?,
                value: values.as_ref().map(|values| f64::from(values[index])),
            });
            offset += length;
        }
    }
    Ok(output)
}

fn argmax(values: &[f64]) -> usize {
    values.iter().enumerate().fold(
        0,
        |best, (index, value)| if *value > values[best] { index } else { best },
    )
}

fn sampled(values: &[f64], draw: f64) -> usize {
    let mut sum = 0.0;
    for (index, probability) in values.iter().enumerate() {
        sum += probability;
        if draw < sum {
            return index;
        }
    }
    values.len() - 1
}

fn compare(expected: &[Prediction], actual: &[Prediction]) -> Result<serde_json::Value> {
    if expected.len() != actual.len() {
        return Err("prediction count changed".into());
    }
    let (mut max_probability, mut max_tv, mut max_value, mut max_log_probability) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let (mut greedy_changes, mut sample_changes, mut log_support_changes) = (0, 0, 0);
    for (decision, (left, right)) in expected.iter().zip(actual).enumerate() {
        if left.probabilities.len() != right.probabilities.len()
            || right.probabilities.is_empty()
            || right
                .probabilities
                .iter()
                .any(|p| !p.is_finite() || *p < 0.0)
            || (right.probabilities.iter().sum::<f64>() - 1.0).abs() > 1e-9
        {
            return Err("malformed distribution".into());
        }
        let mut tv = 0.0;
        for (p, q) in left.probabilities.iter().zip(&right.probabilities) {
            let difference = (p - q).abs();
            max_probability = max_probability.max(difference);
            tv += difference * 0.5;
            if *p > 0.0 && *q > 0.0 {
                max_log_probability = max_log_probability.max((p.ln() - q.ln()).abs());
            } else if (*p > 0.0) != (*q > 0.0) {
                log_support_changes += 1;
            }
        }
        max_tv = max_tv.max(tv);
        greedy_changes += usize::from(argmax(&left.probabilities) != argmax(&right.probabilities));
        // Fixed common draws: diagnostic sample agreement, not a claim of trajectory equivalence.
        let draw =
            f64::from(u32::try_from((decision * 7_919 + 104_729) % 1_000_003)?) / 1_000_003.0;
        sample_changes +=
            usize::from(sampled(&left.probabilities, draw) != sampled(&right.probabilities, draw));
        match (left.value, right.value) {
            (Some(a), Some(b)) => {
                let difference = (a - b).abs();
                if !b.is_finite()
                    || difference > VALUE_ABS_TOLERANCE + VALUE_REL_TOLERANCE * a.abs()
                {
                    return Err(format!("critic mismatch: CPU {a}, actual {b}").into());
                }
                max_value = max_value.max(difference);
            }
            (None, None) => {}
            _ => return Err("critic presence changed".into()),
        }
    }
    if max_probability > PROBABILITY_TOLERANCE
        || max_tv > PROBABILITY_TOLERANCE
        || log_support_changes != 0
    {
        return Err(format!("distribution drift: max p {max_probability}, TV {max_tv}, support changes {log_support_changes}").into());
    }
    Ok(
        json!({"max_probability_abs":max_probability, "max_total_variation":max_tv,
        "max_critic_abs":max_value, "max_log_probability_abs":max_log_probability,
        "greedy_changes":greedy_changes, "common_draw_changes":sample_changes,
        "support_changes":log_support_changes}),
    )
}

fn run() -> Result<()> {
    let config = Config::parse()?;
    ti4_tensor::configure_deterministic(20_260_916)?;
    let device = ti4_tensor::OptimizerDevice::Cuda.resolve()?;
    let _guard = tch::no_grad_guard();
    let loaded = ti4_mlp::bundle::read(Path::new(&config.bundle))?;
    if config.diplomacy && loaded.actor.layout_head_index("diplomacy").is_err() {
        return Err("diplomacy requires a diplomacy-head bundle".into());
    }
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(&config.pool),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )?;
    let pool = Arc::new(ti4_sim::MapPool::from_reader(std::io::Cursor::new(
        &pool_bytes,
    ))?);
    let manifest = std::fs::read(Path::new(&config.bundle).join("manifest.json"))?;
    println!(
        "{}",
        json!({"kind":"config", "bundle":config.bundle, "pool":config.pool,
        "manifest_sha256":format!("{:x}", Sha256::digest(&manifest)),
        "pool_sha256":format!("{:x}", Sha256::digest(&pool_bytes)), "seed":config.seed,
        "temperature":config.temperature, "diplomacy":config.diplomacy, "rounds":4,
        "samples":config.samples, "repeats":config.repeats, "batches":BATCHES,
        "probability_and_tv_tolerance":PROBABILITY_TOLERANCE,
        "critic_abs_tolerance":VALUE_ABS_TOLERANCE, "critic_rel_tolerance":VALUE_REL_TOLERANCE,
        "scope":"single-worker replay plus unbatched live CUDA; no training or central service"})
    );
    let (records, cpu_digest) = game(&config, &loaded, &pool, Device::Cpu)?;
    let count = config.samples.min(records.len());
    let steps: Vec<_> = (0..count)
        .map(|i| records[i * records.len() / count].clone())
        .collect();
    drop(records);
    let mut by_head = BTreeMap::new();
    for step in &steps {
        *by_head.entry(ti4_mlp::all_heads()[step.head]).or_insert(0) += 1;
    }
    println!(
        "{}",
        json!({"kind":"sample", "decisions":steps.len(), "by_head":by_head,
        "options":steps.iter().map(|step| step.options.len()).sum::<usize>(),
        "steps_sha256":ti4_mlp::perf::steps_digest(&steps)?})
    );
    let cpu = loaded.actor.inference_copy().to_device(Device::Cpu);
    let gpu = loaded.actor.inference_copy().to_device(device);
    let expected = reference(&cpu, &steps)?;
    for width in BATCHES {
        compare(
            &expected,
            &batched(&cpu, &steps, width, config.temperature)?,
        )?;
        compare(
            &expected,
            &batched(&gpu, &steps, width, config.temperature)?,
        )?;
    }
    // All shapes warmed; report every repeat, alternating order to limit a fixed-order bias.
    for repeat in 0..config.repeats {
        let mut cases = vec![("cpu_reference", Device::Cpu, 1)];
        for width in BATCHES {
            cases.push(("cpu_mixed", Device::Cpu, width));
            cases.push(("cuda_mixed", device, width));
        }
        if repeat % 2 == 1 {
            cases.reverse();
        }
        for (name, case_device, width) in cases {
            let actor = if case_device == Device::Cpu {
                &cpu
            } else {
                &gpu
            };
            synchronize(case_device);
            let started = Instant::now();
            let actual = if name == "cpu_reference" {
                reference(actor, &steps)?
            } else {
                batched(actor, &steps, width, config.temperature)?
            };
            synchronize(case_device);
            let seconds = started.elapsed().as_secs_f64();
            let drift = compare(&expected, &actual)?;
            println!(
                "{}",
                json!({"kind":"replay", "path":name, "batch":width, "repeat":repeat,
                "wall_s":seconds, "decisions_per_s":f64::from(u32::try_from(steps.len())?) / seconds, "drift":drift})
            );
        }
    }
    drop(gpu);
    let (gpu_records, gpu_digest) = game(&config, &loaded, &pool, device)?;
    drop(gpu_records);
    println!(
        "{}",
        json!({"kind":"result", "status":"passed", "live_game_digests_equal":cpu_digest == gpu_digest,
        "qualification":"smoke only; no production speedup or bitwise-equivalence claim"})
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("REFUSED: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_rejects_invalid_probabilities_and_critic_drift() {
        let expected = [Prediction {
            probabilities: vec![0.25, 0.75],
            value: Some(1.0),
        }];
        for (probabilities, value) in [
            (vec![0.25, 0.75], Some(f64::NAN)),
            (vec![0.25, 0.75], Some(2.0)),
            (vec![0.5, 0.5], Some(1.0)),
            (vec![0.25, 0.5], Some(1.0)),
            (vec![f64::NAN, 0.75], Some(1.0)),
        ] {
            assert!(
                compare(
                    &expected,
                    &[Prediction {
                        probabilities,
                        value
                    }]
                )
                .is_err()
            );
        }
        assert!(compare(&expected, &expected).is_ok());
    }

    #[test]
    fn distribution_agreement_does_not_hide_a_changed_greedy_choice() {
        let left = [Prediction {
            probabilities: vec![0.50001, 0.49999],
            value: None,
        }];
        let right = [Prediction {
            probabilities: vec![0.49999, 0.50001],
            value: None,
        }];
        let report = compare(&left, &right).unwrap();
        assert_eq!(report["greedy_changes"], 1);
    }
}
