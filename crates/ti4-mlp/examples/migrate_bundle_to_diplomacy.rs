//! Write the diplomacy-capable copy of a legacy checkpoint.
//!
//! Reads a schema-7/8 bundle whose vocabulary is OOV registry v10, and writes a new, immutable
//! bundle beside it:
//!
//! 1. the vocabulary gains v11's reserved `oov:diplomacy` column and every ordinary slot moves up
//!    by one (`Vocabulary::migrate_v10_to_v11`), with the input rows it addresses moved to match;
//! 2. the actor gains the `diplomacy` readout row (`Actor::with_diplomacy_head`), so it becomes
//!    schema 9 or 10.
//!
//! The source is never modified, and the destination must not exist. The written bundle is read
//! back through the ordinary loader before the tool reports success. That diplomacy-off play is
//! unchanged by the migration is checked separately by `diplomacy_migration_check`.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example migrate_bundle_to_diplomacy -- \
//!   --bundle out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280 \
//!   --out out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280-diplomacy-v11
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
    let destination = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let destination = Path::new(&destination);
    if destination.exists() {
        refuse(&format!(
            "{} already exists; bundles are never written in place",
            destination.display()
        ));
    }
    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));

    let loaded = ti4_mlp::bundle::read(Path::new(&source))
        .unwrap_or_else(|error| refuse(&format!("{source}: {error}")));
    let legacy_heads = loaded.actor.head_names().len();
    let legacy_capacity = loaded.actor.capacity();
    let migrated = ti4_mlp::bundle::migrate_v10_to_v11(loaded)
        .unwrap_or_else(|error| refuse(&format!("migration: {error}")));
    let ti4_mlp::bundle::Loaded {
        actor,
        vocabulary,
        critic_mode,
        update,
    } = migrated;
    let actor = actor.with_diplomacy_head();
    let slots = vocabulary
        .to_json()
        .unwrap_or_else(|error| refuse(&format!("encoding slots.json: {error}")));

    ti4_mlp::bundle::write(
        destination,
        &actor,
        &slots,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!("diplomacy migration (OOV v10->v11, diplomacy head) of {source}"),
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
    println!("source      {source}");
    println!(
        "  heads {legacy_heads}, capacity {legacy_capacity}, update {update}, critic {critic_mode:?}"
    );
    println!("written     {}", destination.display());
    println!(
        "  heads {}, capacity {}, update {}, critic {:?} (read back through bundle::read)",
        reread.actor.head_names().len(),
        reread.actor.capacity(),
        reread.update,
        reread.critic_mode
    );
}
