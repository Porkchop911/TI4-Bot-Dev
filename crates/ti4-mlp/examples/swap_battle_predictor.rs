//! Write a copy of an arena checkpoint that carries a different battle predictor.
//!
//! For a retrained predictor (BF-22: battle feature version 8, the wide roster). The policy's
//! weights and vocabulary are copied unchanged; only `battle_predictor.json` differs. Every battle
//! fact name the new predictor emits must already be placed by the vocabulary, so no fact falls
//! to an out-of-vocabulary column it never did before. The source is never modified and the
//! destination must not exist; the written bundle is read back through the ordinary loader.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example swap_battle_predictor -- \
//!   --bundle <arena checkpoint> --predictor <battle_predictor.json> --out <new checkpoint>
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
        vocabulary,
        critic_mode,
        update,
    } = loaded;
    let Some(old) = actor.battle_predictor().cloned() else {
        refuse("the source carries no battle predictor; use migrate_bundle_to_arena");
    };
    let names = ti4_policy::battle::fact_names(predictor.feature_version);
    if let Some(missing) = names.iter().find(|name| !vocabulary.is_assigned(name)) {
        refuse(&format!(
            "the vocabulary does not place {missing}, which predictor v{} emits",
            predictor.feature_version
        ));
    }
    let old_version = old.feature_version;
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
                "battle predictor v{old_version} replaced by {predictor_path} in {source}"
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
    let version = reread
        .actor
        .battle_predictor()
        .map_or(0, |predictor| predictor.feature_version);
    println!("source   {source}  (predictor v{old_version}, update {update})");
    println!("written  {}  (predictor v{version})", destination.display());
}
