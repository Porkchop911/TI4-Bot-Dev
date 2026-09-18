//! Move an arena checkpoint to a later fact version (5 on) without changing how it plays.
//!
//! The predictor keeps its networks and version 4's encoding; only its fact version changes. The
//! facts the new version adds are appended to the vocabulary into rows that must already be zero,
//! so the migrated policy scores every option exactly as the source did until PPO trains them.
//!
//! The source is never modified and the destination must not exist. The written bundle is read
//! back through the ordinary loader before the tool reports success.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example migrate_arena_facts -- \
//!   --bundle <arena checkpoint> --version 5 --out <new checkpoint>
//! ```

use std::path::Path;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(message: &str) -> ! {
    eprintln!("REFUSED: {message}");
    std::process::exit(2)
}

fn main() {
    let source = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let version: u32 = argument("--version")
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| refuse("--version <n> is required"));
    let destination = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let destination = Path::new(&destination);
    if destination.exists() {
        refuse(&format!(
            "{} already exists; bundles are never written in place",
            destination.display()
        ));
    }
    ti4_tensor::configure_deterministic(20_260_917)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));

    let loaded = ti4_mlp::bundle::read(Path::new(&source))
        .unwrap_or_else(|error| refuse(&format!("{source}: {error}")));
    let ti4_mlp::bundle::Loaded {
        mut actor,
        mut vocabulary,
        critic_mode,
        update,
    } = loaded;
    let predictor = actor
        .battle_predictor()
        .unwrap_or_else(|| refuse("the source carries no battle predictor"))
        .as_ref()
        .clone();
    let from = predictor.feature_version;
    let predictor = predictor
        .relabelled(version)
        .unwrap_or_else(|error| refuse(&format!("fact version {from} -> {version}: {error}")));

    let slots_before = vocabulary.slot_count();
    let old: std::collections::BTreeSet<&str> = ti4_policy::battle::fact_names(from)
        .iter()
        .copied()
        .collect();
    let new: Vec<&str> = ti4_policy::battle::fact_names(version)
        .iter()
        .copied()
        .filter(|name| !old.contains(name))
        .collect();
    let added = vocabulary
        .append(&new)
        .unwrap_or_else(|error| refuse(&format!("appending facts: {error}")));
    if added != new.len() {
        refuse(&format!(
            "{} of the new facts were already assigned",
            new.len() - added
        ));
    }
    for name in &new {
        let column = i64::try_from(vocabulary.column_of(name)).expect("column fits");
        let row_sum = actor
            .input()
            .narrow(0, column, 1)
            .abs()
            .sum(ti4_tensor::Kind::Float)
            .double_value(&[]);
        if row_sum != 0.0 {
            refuse(&format!(
                "input row {column} for {name} is not zero ({row_sum})"
            ));
        }
    }
    actor.set_battle_predictor(Some(std::sync::Arc::new(predictor)));

    let slots = vocabulary
        .to_json()
        .unwrap_or_else(|error| refuse(&format!("encoding slots.json: {error}")));
    ti4_mlp::bundle::write(
        destination,
        &actor,
        &slots,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!("arena fact migration {from} -> {version} of {source}"),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing {}: {error}", destination.display())));

    let reread = ti4_mlp::bundle::read(destination).unwrap_or_else(|error| {
        refuse(&format!(
            "the written bundle does not load back: {}: {error}",
            destination.display()
        ))
    });
    let carried = reread
        .actor
        .battle_predictor()
        .unwrap_or_else(|| refuse("the written bundle lost its battle predictor"));
    println!("source      {source}  (slots {slots_before}, update {update}, facts v{from})");
    println!(
        "written     {}  (slots {}, facts v{}, {} new: {})",
        destination.display(),
        reread.vocabulary.slot_count(),
        carried.feature_version,
        new.len(),
        new.join(", ")
    );
}
