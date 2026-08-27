//! Write an untrained bundle: §6.1's initialisation and nothing else.
//!
//! The control for every claim that training did something. A run started here has the same
//! architecture, the same vocabulary, the same slot assignment and the same critic mode as a
//! trained bundle, and none of the learning — so the difference between the two is the learning,
//! not the scaffolding.
//!
//! # Where the weights come from
//!
//! `distill::initialize` is the same function distillation starts from, with the same `INIT_SEED`,
//! so a blank bundle is exactly the point distillation departed from.
//!
//! Distillation initialises only the rows its corpus reaches, because §6.1's zero-extension
//! contract keeps random weights out of rows belonging to phases that are not enabled yet. This
//! initialises every assigned row instead, and the difference is behaviourally nil: a row no
//! feature ever names is never gathered, so whether it holds noise or zeros cannot reach a logit.
//! Doing it this way avoids loading an 800,000-sample corpus to discover a set that does not
//! matter.
//!
//! The value head is left at zero. In shared-critic mode that means the blank critic predicts zero
//! for every state, so the first advantages are the returns themselves — which is what an untrained
//! baseline should be, not an accident.

use ti4_mlp::bundle::{CriticMode, Provenance};

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn main() {
    let like = argument("--like").unwrap_or_else(|| {
        refuse("--like is required: a bundle to copy the vocabulary, slots and critic mode from")
    });
    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/blank".to_owned());

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let reference = ti4_mlp::bundle::read(std::path::Path::new(&like))
        .unwrap_or_else(|error| refuse(&format!("reading {like}: {error}")));
    // The slot map comes from the accepted vocabulary generation by default, not from `--like`.
    // A blank bundle exists to start a run, and a run should start on the vocabulary in force;
    // copying the reference bundle's slots would silently pin the new model to whatever generation
    // that bundle happened to be trained against, which is the mistake this argument prevents.
    let slots_path = argument("--slots").map_or_else(
        || {
            let accepted = ti4_training::vocabulary_corpus::accepted_generation(
                std::path::Path::new("out/vocabulary"),
            )
            .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary generation: {error}")));
            accepted.slots
        },
        std::path::PathBuf::from,
    );
    let slots_text = std::fs::read_to_string(&slots_path)
        .unwrap_or_else(|error| refuse(&format!("reading {}: {error}", slots_path.display())));
    println!("  slots       {}", slots_path.display());

    let capacity = reference.actor.capacity();
    let width = match reference.actor.width() {
        256 => ti4_mlp::Width::W256,
        128 => ti4_mlp::Width::W128,
        other => refuse(&format!(
            "{like} has trunk width {other}, which is not a pinned width"
        )),
    };
    if !matches!(
        reference.critic_mode,
        CriticMode::Shared | CriticMode::BatchMean
    ) {
        refuse(
            "a blank bundle cannot be written for a separate critic: its tensors are trained, \
                not initialised, so there is no untrained form of them to write",
        );
    }

    println!("blank bundle");
    println!("  like        {like}");
    println!(
        "  width       {} | capacity {capacity}",
        reference.actor.width()
    );
    println!("  critic mode {:?}", reference.critic_mode);

    let every_row: Vec<i64> = (0..capacity).collect();
    let actor = ti4_mlp::distill::initialize(width, capacity, &every_row);

    // Non-vacuity: an initialisation that produced a constant model would make every downstream
    // comparison against it meaningless, and would look exactly like a working one from outside.
    let sample = ti4_tensor::to_vec(actor.input())
        .unwrap_or_else(|error| refuse(&format!("reading the initialised table: {error}")));
    let spread = sample.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b))
        - sample.iter().fold(f32::INFINITY, |a, b| a.min(*b));
    if spread <= 0.0 {
        refuse("the initialised input table is constant, so it is not an initialisation");
    }
    println!(
        "  input       spread {spread:.6} over {} weights",
        sample.len()
    );

    let destination = std::path::Path::new(&out);
    let bundle = ti4_mlp::bundle::write(
        destination,
        &actor,
        &slots_text,
        reference.critic_mode,
        &Provenance {
            source: format!("untrained §6.1 initialisation, shaped like {like}"),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: 0,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the bundle: {error}")));

    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the bundle does not load: {error}")));
    if reloaded.update != 0 {
        refuse("a blank bundle must record update 0");
    }
    println!("  written     {}", bundle.directory.display());
    println!("  reloaded    ok, update {}", reloaded.update);
}
