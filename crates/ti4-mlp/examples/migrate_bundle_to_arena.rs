//! Write the arena-capable copy of a diplomacy checkpoint (schema 9/10 -> 11/12).
//!
//! 1. The vocabulary appends the closed `action-plan:battle-*` names into preallocated rows. Those
//!    rows are zero and are checked to be zero, so the migrated policy scores every option exactly
//!    as the source did until PPO trains them.
//! 2. The actor carries the frozen battle predictor (`battle_predictor.json`), which live play
//!    uses to emit those facts on movement options.
//!
//! The source is never modified and the destination must not exist. The written bundle is read
//! back through the ordinary loader before the tool reports success.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example migrate_bundle_to_arena -- \
//!   --bundle <diplomacy checkpoint> --predictor <battle_predictor.json> --out <new checkpoint>
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
    let predictor_path =
        argument("--predictor").unwrap_or_else(|| refuse("--predictor is required"));
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

    let predictor_text = std::fs::read_to_string(&predictor_path)
        .unwrap_or_else(|error| refuse(&format!("{predictor_path}: {error}")));
    let predictor = ti4_policy::battle::BattlePredictor::from_json(&predictor_text)
        .unwrap_or_else(|error| refuse(&format!("{predictor_path}: {error}")));

    let loaded = ti4_mlp::bundle::read(Path::new(&source))
        .unwrap_or_else(|error| refuse(&format!("{source}: {error}")));
    let ti4_mlp::bundle::Loaded {
        mut actor,
        mut vocabulary,
        critic_mode,
        update,
    } = loaded;
    if actor.head_layout() != ti4_mlp::HeadLayout::Diplomacy {
        refuse("the source must be a diplomacy-layout bundle (schema 9 or 10)");
    }
    if actor.battle_predictor().is_some() {
        refuse("the source already carries a battle predictor");
    }

    let names = ti4_policy::battle::fact_names(predictor.feature_version);
    let slots_before = vocabulary.slot_count();
    let added = vocabulary
        .append(names)
        .unwrap_or_else(|error| refuse(&format!("appending battle facts: {error}")));
    if added != names.len() {
        refuse(&format!(
            "{} of the battle facts were already assigned; the source is not a plain diplomacy bundle",
            names.len() - added
        ));
    }
    // The appended rows must be zero, or the migration changes play before any training.
    for name in names {
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
            source: format!(
                "arena migration (battle facts, predictor {predictor_path}) of {source}"
            ),
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
    println!("source      {source}  (slots {slots_before}, update {update})");
    println!(
        "written     {}  (slots {}, capacity {}, predictor '{}')",
        destination.display(),
        reread.vocabulary.slot_count(),
        reread.vocabulary.capacity(),
        carried.continuation
    );
}
