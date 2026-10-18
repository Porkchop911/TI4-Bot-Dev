//! Fine-tune a trained bundle on one faction's recorded best games (`ppo_update --record-best`).
//!
//! Each record is a seat-game's exact training records. Every decision with a critic input becomes
//! an advantage-weighted demonstration (`positive_corpus::advantage_weighted_clone_loss`): imitate
//! the recorded choice by how much its return beat the model's own value estimate, nothing when it
//! did not. Each record (game) carries one unit of weight, so long games do not dominate. A tenth
//! of the records, chosen by seed, is held out and its loss reported before and after.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example faction_finetune -- --bundle <final checkpoint> \
//!   --records <run>/best/sol[,<run2>/best/sol] --out <new bundle> [--epochs 2] \
//!   [--learning-rate 3e-5] [--minibatch 256] [--device cuda] [--seed 1]
//! ```

use ti4_mlp::positive_corpus::{AdvantageDemo, Demo};

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// Weighted mean loss over `demos` in minibatches, without training.
fn held_out_loss(actor: &ti4_mlp::Actor, demos: &[AdvantageDemo], minibatch: usize) -> f64 {
    let (mut total, mut weight) = (0.0, 0.0);
    for chunk in demos.chunks(minibatch) {
        let mass: f64 = chunk.iter().map(|demo| demo.demo.weight).sum();
        if let Some(loss) = ti4_mlp::positive_corpus::advantage_weighted_clone_loss(actor, chunk)
            .unwrap_or_else(|error| refuse(&error))
        {
            total += loss.double_value(&[]) * mass;
            weight += mass;
        }
    }
    if weight > 0.0 { total / weight } else { 0.0 }
}

#[expect(
    clippy::too_many_lines,
    reason = "one fine-tune: load, split, train, check, write"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let records = argument("--records").unwrap_or_else(|| refuse("--records is required"));
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let epochs: usize = number("--epochs", 2);
    let learning_rate: f64 = number("--learning-rate", 3e-5);
    let minibatch: usize = number("--minibatch", 256);
    let seed: u64 = number("--seed", 1);
    let device = match argument("--device").as_deref() {
        None | Some("cuda") => ti4_tensor::Device::Cuda(0),
        Some("cpu") => ti4_tensor::Device::Cpu,
        Some(other) => refuse(&format!("--device {other}: expected cuda or cpu")),
    };
    ti4_tensor::configure_deterministic(20_261_018)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    // Every record file, in name order so the split is reproducible.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for directory in records.split(',') {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| refuse(&format!("{directory}: {error}")));
        files.extend(
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.to_string_lossy().ends_with(".ti4ppo.zst")),
        );
    }
    files.sort();
    if files.len() < 10 {
        refuse(&format!(
            "{} records: too few to fine-tune and hold some out",
            files.len()
        ));
    }

    // Records -> demonstrations, one unit of weight per record; every tenth record (after a seeded
    // rotation) is held out.
    let (mut train, mut held) = (Vec::new(), Vec::new());
    let mut rows = std::collections::BTreeSet::new();
    let offset = usize::try_from(seed).unwrap_or(0) % 10;
    for (index, path) in files.iter().enumerate() {
        let steps = ti4_mlp::perf::read_capture(path).unwrap_or_else(|error| refuse(&error));
        let usable: Vec<_> = steps
            .into_iter()
            .filter(|step| step.critic.is_some())
            .collect();
        if usable.is_empty() {
            continue;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a game has a few thousand decisions"
        )]
        let weight = 1.0 / usable.len() as f64;
        let target = if (index + offset) % 10 == 0 {
            &mut held
        } else {
            &mut train
        };
        for step in usable {
            rows.insert(step.row.index());
            target.push(AdvantageDemo {
                demo: Demo {
                    row: step.row,
                    head: step.head,
                    options: step.options,
                    chosen: step.chosen,
                    weight,
                },
                return_to_go: step.return_to_go,
                critic: step.critic.expect("filtered"),
            });
        }
    }
    if rows.len() != 1 {
        refuse(&format!(
            "the records span {} faction rows; fine-tune one faction at a time",
            rows.len()
        ));
    }
    println!("faction fine-tune");
    println!("  bundle      {bundle_path}");
    println!(
        "  records     {} files: {} training decisions, {} held-out decisions",
        files.len(),
        train.len(),
        held.len()
    );
    println!(
        "  schedule    {epochs} epoch(s), minibatch {minibatch}, learning rate {learning_rate}"
    );

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let critic_mode = loaded.critic_mode;
    let mut actor = loaded.actor.to_device(device);
    let settings = ti4_mlp::ppo::Settings {
        learning_rate,
        ..ti4_mlp::ppo::Settings::default()
    };
    let mut optimizer = ti4_mlp::ppo::Adam::new(&mut actor, critic_mode, settings)
        .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));

    let before = held_out_loss(&actor, &held, minibatch);
    println!("  held out    loss {before:.5} before");

    // A seeded shuffle per epoch (xorshift; no new dependency for a permutation).
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut order: Vec<usize> = (0..train.len()).collect();
    for epoch in 0..epochs {
        for index in (1..order.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let other = usize::try_from(state % (index as u64 + 1)).unwrap_or(0);
            order.swap(index, other);
        }
        let (mut total, mut steps) = (0.0, 0usize);
        for chunk in order.chunks(minibatch) {
            let batch: Vec<AdvantageDemo> =
                chunk.iter().map(|&index| train[index].clone()).collect();
            let Some(loss) =
                ti4_mlp::positive_corpus::advantage_weighted_clone_loss(&actor, &batch)
                    .unwrap_or_else(|error| refuse(&error))
            else {
                continue;
            };
            optimizer
                .zero_grad(&actor)
                .unwrap_or_else(|error| refuse(&error));
            total += loss.double_value(&[]);
            loss.backward();
            optimizer
                .step(&actor)
                .unwrap_or_else(|error| refuse(&error));
            steps += 1;
        }
        #[expect(clippy::cast_precision_loss, reason = "step counts are small")]
        let mean = if steps > 0 { total / steps as f64 } else { 0.0 };
        let held_now = held_out_loss(&actor, &held, minibatch);
        println!(
            "  epoch {epoch}    {steps} steps, train loss {mean:.5}, held-out loss {held_now:.5}"
        );
    }
    let after = held_out_loss(&actor, &held, minibatch);
    println!(
        "  held out    loss {after:.5} after ({:+.5})",
        after - before
    );

    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let cpu = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
    let bundle = ti4_mlp::bundle::write(
        std::path::Path::new(&out),
        &cpu,
        &slots_text,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!(
                "faction fine-tune of {bundle_path} on {} records ({records}), {epochs} epoch(s), learning rate {learning_rate}",
                files.len()
            ),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: loaded.update,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the bundle: {error}")));
    ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the written bundle does not load: {error}")));
    println!("  written     {} (reloaded)", bundle.directory.display());
}
