//! Replay one captured PPO batch through the optimiser, for timing and repeatability.
//!
//! `plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md`, packages A0 and A1. Reads a bundle and a batch
//! captured by `ppo_update --capture-batch`, then for each repeat: a fresh actor from the bundle and
//! a fresh Adam (bundles carry no optimiser state, so both sides of any comparison start cold and
//! identical), `--warm` untimed updates on the batch, then one timed update. Reports wall time to
//! completion, the phase breakdown, and SHA-256 over the parameter and Adam fingerprints and the
//! last epoch's losses, so repeats can be compared bit for bit.
//!
//! usage: `ppo_replay --bundle <dir> --capture <file> --shuffle-seed <n> [--repeats 5] [--warm 0]
//! [--device cuda] [--learning-rate 3e-4] [--movement-entropy 0.05] [--sync] [--out <jsonl>]`
//!
//! Unknown flags are refused.

use std::io::Write;
use std::time::Instant;

use sha2::Digest;
use ti4_mlp::ppo::{Adam, Batch, Settings};

const VALUE_FLAGS: [&str; 9] = [
    "--bundle",
    "--capture",
    "--shuffle-seed",
    "--repeats",
    "--warm",
    "--device",
    "--learning-rate",
    "--movement-entropy",
    "--out",
];
const BOOLEAN_FLAGS: [&str; 1] = ["--sync"];

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn check_flags() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if BOOLEAN_FLAGS.contains(&arg) {
            index += 1;
        } else if VALUE_FLAGS.contains(&arg) {
            if index + 1 >= args.len() {
                refuse(&format!("{arg} expects a value"));
            }
            index += 2;
        } else {
            refuse(&format!("unknown argument {arg:?}"));
        }
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn parsed<T: std::str::FromStr>(name: &str, default: Option<T>) -> T {
    match argument(name) {
        Some(value) => value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name}: cannot parse {value:?}"))),
        None => default.unwrap_or_else(|| refuse(&format!("{name} is required"))),
    }
}

fn sha(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes.into_iter().collect::<Vec<u8>>());
    format!("{:x}", hasher.finalize())
}

fn words_digest(words: &[u32]) -> String {
    sha(words.iter().flat_map(|word| word.to_le_bytes()))
}

fn synchronize(device: ti4_tensor::Device) {
    if let ti4_tensor::Device::Cuda(index) = device {
        tch::Cuda::synchronize(i64::try_from(index).unwrap_or(0));
    }
}

fn main() {
    check_flags();
    let bundle_path = parsed::<String>("--bundle", None);
    let capture_path = parsed::<String>("--capture", None);
    let shuffle_seed = parsed::<u64>("--shuffle-seed", None);
    let repeats = parsed::<usize>("--repeats", Some(5));
    let warm = parsed::<usize>("--warm", Some(0));
    let sync = std::env::args().any(|a| a == "--sync");
    let optimizer_device = match argument("--device").as_deref() {
        None | Some("cuda") => ti4_tensor::OptimizerDevice::Cuda,
        Some("cpu") => ti4_tensor::OptimizerDevice::Cpu,
        Some(other) => refuse(&format!("--device {other}: expected cpu or cuda")),
    };
    let device = optimizer_device
        .resolve()
        .unwrap_or_else(|error| refuse(&format!("--device: {error}")));

    // The trainer's backend configuration and settings, so a replayed update is the same function.
    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let mut settings = Settings::default();
    settings.movement_entropy = parsed("--movement-entropy", Some(settings.entropy));
    settings.learning_rate = parsed("--learning-rate", Some(settings.learning_rate));

    let loaded_mode = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")))
        .critic_mode;
    let steps = ti4_mlp::perf::read_capture(std::path::Path::new(&capture_path))
        .unwrap_or_else(|error| refuse(&error));
    let steps_digest =
        ti4_mlp::perf::steps_digest(&steps).unwrap_or_else(|error| refuse(&error.to_string()));
    let batch = Batch::freeze(steps, loaded_mode)
        .unwrap_or_else(|error| refuse(&format!("freeze: {error}")));
    let options: usize = batch.steps().iter().map(|step| step.options.len()).sum();

    println!("ppo_replay");
    println!("  bundle      {bundle_path}");
    println!("  capture     {capture_path}  steps sha256 {steps_digest}");
    println!(
        "  batch       {} decisions, {options} options, minibatch {}, {} epochs",
        batch.len(),
        settings.minibatch,
        settings.epochs
    );
    println!(
        "  settings    lr {} | movement entropy {} | shuffle seed {shuffle_seed} | device {device:?} | warm {warm} | sync {sync}",
        settings.learning_rate, settings.movement_entropy
    );

    ti4_mlp::perf::enable(sync);
    ti4_tensor::gather_timing(true);
    let mut out = argument("--out").map(|path| {
        std::fs::File::options()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|error| refuse(&format!("opening {path}: {error}")))
    });

    for repeat in 0..repeats {
        let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
            .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
        let mut actor = loaded.actor.to_device(device);
        let mut optimizer = Adam::new(&mut actor, loaded.critic_mode, settings)
            .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));
        for _ in 0..warm {
            ti4_mlp::ppo::update(
                &mut actor,
                &batch,
                loaded.critic_mode,
                settings,
                shuffle_seed,
                &mut optimizer,
            )
            .unwrap_or_else(|error| refuse(&format!("warm update: {error}")));
        }
        synchronize(device);
        let _ = ti4_mlp::perf::take_phases();
        let _ = ti4_tensor::take_gather_nanos();
        let started = Instant::now();
        let stats = ti4_mlp::ppo::update(
            &mut actor,
            &batch,
            loaded.critic_mode,
            settings,
            shuffle_seed,
            &mut optimizer,
        )
        .unwrap_or_else(|error| refuse(&format!("update: {error}")));
        synchronize(device);
        let wall = started.elapsed();
        let phases = ti4_mlp::perf::take_phases();
        let gather = ti4_tensor::take_gather_nanos();

        let parameters = ti4_mlp::ppo::parameter_fingerprint(&actor, loaded.critic_mode)
            .unwrap_or_else(|error| refuse(&format!("fingerprint: {error}")));
        let adam = optimizer
            .state_fingerprint()
            .unwrap_or_else(|error| refuse(&format!("Adam fingerprint: {error}")));
        let last = stats.last().unwrap_or_else(|| refuse("no epoch ran"));
        let losses = sha([
            last.actor_loss,
            last.critic_loss,
            last.kl,
            last.clipped_fraction,
        ]
        .iter()
        .flat_map(|value| value.to_bits().to_le_bytes()));
        let parameters_digest = words_digest(&parameters);
        let adam_digest = words_digest(&adam);

        let phase_ms: Vec<String> = ti4_mlp::perf::PHASES
            .iter()
            .zip(phases.nanos)
            .map(|(name, nanos)| format!("{name} {:.1}", nanos as f64 / 1e6))
            .collect();
        println!(
            "  repeat {repeat}  wall {:.1} ms  minibatches {}  [{}] ms",
            wall.as_secs_f64() * 1e3,
            phases.minibatches,
            phase_ms.join(" | ")
        );
        println!(
            "            params {}  adam {}  losses {}",
            &parameters_digest[..16],
            &adam_digest[..16],
            &losses[..16]
        );
        println!(
            "            layout {} real options in {} padded cells ({:.1}% padding), widest decision {}",
            phases.options,
            phases.cells,
            100.0 * (1.0 - phases.options as f64 / phases.cells.max(1) as f64),
            phases.widest_max
        );
        println!(
            "            gather host ms: flatten+validate {:.0} | upload {:.0} | queue bag {:.0}",
            gather[0] as f64 / 1e6,
            gather[1] as f64 / 1e6,
            gather[2] as f64 / 1e6
        );
        if let Some(file) = out.as_mut() {
            let phases_json: serde_json::Map<String, serde_json::Value> = ti4_mlp::perf::PHASES
                .iter()
                .zip(phases.nanos)
                .map(|(name, nanos)| ((*name).to_owned(), serde_json::json!(nanos as f64 / 1e6)))
                .collect();
            let line = serde_json::json!({
                "kind": "replay",
                "bundle": bundle_path,
                "capture_steps_sha256": steps_digest,
                "shuffle_seed": shuffle_seed,
                "device": format!("{device:?}"),
                "sync": sync,
                "warm": warm,
                "repeat": repeat,
                "wall_ms": wall.as_secs_f64() * 1e3,
                "minibatches": phases.minibatches,
                "options": phases.options,
                "cells": phases.cells,
                "widest_max": phases.widest_max,
                "gather_ms": gather.map(|nanos| nanos as f64 / 1e6),
                "phase_ms": phases_json,
                "parameters_sha256": parameters_digest,
                "adam_sha256": adam_digest,
                "losses_sha256": losses,
            });
            writeln!(file, "{line}").unwrap_or_else(|error| refuse(&format!("writing: {error}")));
        }
    }
}
