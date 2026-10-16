//! Write a copy of a checkpoint with some factions' own rows set to zero.
//!
//! Each faction's rows are its readout residual (`delta`), readout bias (`b_delta`) and identity
//! embedding. Zeroed, that faction plays the shared network alone: what it knows comes only from
//! the features it is shown (its abilities, technologies, units, home), not from who it is. An
//! evaluation of the copy against the source measures how much the identity rows carry
//! (operator, 2026-10-08: the "zeroed-rows test").
//!
//! The source is never modified and the destination must not exist; the written bundle is read
//! back through the ordinary loader.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example zero_faction_rows -- \
//!   --bundle <checkpoint> --factions sol,letnev,xxcha,hacan,jolnar,l1z1x --out <new checkpoint>
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
    let factions = argument("--factions").unwrap_or_else(|| refuse("--factions is required"));
    let destination = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let destination = Path::new(&destination);
    if destination.exists() {
        refuse(&format!(
            "{} already exists; bundles are never written in place",
            destination.display()
        ));
    }
    ti4_tensor::configure_deterministic(20_261_008)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));
    let rows: Vec<ti4_mlp::FactionRow> = factions
        .split(',')
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(|alias| {
            ti4_mlp::FactionRow::of(alias).unwrap_or_else(|error| refuse(&error.to_string()))
        })
        .collect();
    if rows.is_empty() {
        refuse("--factions names no faction");
    }

    let loaded = ti4_mlp::bundle::read(Path::new(&source))
        .unwrap_or_else(|error| refuse(&format!("{source}: {error}")));
    let ti4_mlp::bundle::Loaded {
        mut actor,
        vocabulary,
        critic_mode,
        update,
    } = loaded;
    tch::no_grad(|| {
        for row in &rows {
            let at = i64::try_from(row.index()).expect("row fits");
            let _ = actor.delta_mut().get(at).fill_(0.0);
            let _ = actor.b_delta_mut().get(at).fill_(0.0);
            let _ = actor.embedding_mut().get(at).fill_(0.0);
        }
    });
    let slots = vocabulary
        .to_json()
        .unwrap_or_else(|error| refuse(&format!("encoding slots.json: {error}")));
    ti4_mlp::bundle::write(
        destination,
        &actor,
        &slots,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!("faction rows zeroed ({factions}) in {source}"),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing {}: {error}", destination.display())));
    ti4_mlp::bundle::read(destination).unwrap_or_else(|error| {
        refuse(&format!(
            "the written bundle does not load back: {}: {error}",
            destination.display()
        ))
    });
    println!(
        "written  {}  ({} factions' rows zeroed, from {source})",
        destination.display(),
        rows.len()
    );
}
