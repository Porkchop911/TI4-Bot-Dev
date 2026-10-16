//! Write a copy of a checkpoint whose vocabulary knows more feature names.
//!
//! Names are appended into the bundle's preallocated rows (see `--init-from-oov` for how they
//! start) until PPO trains them. The list usually comes from `vocab_census`. Names the vocabulary
//! already places are skipped. The source is never modified and the destination must not exist; the written bundle is
//! read back through the ordinary loader before the tool reports success.
//!
//! ```text
//! GIT_COMMIT=$(git rev-parse HEAD) cargo run --release -p ti4-mlp --example migrate_bundle_append_names -- \
//!   --bundle <checkpoint> --names <file, one name per line> --out <new checkpoint> [--grow] [--init-from-oov]
//! ```
//!
//! `--grow` (BF-22, operator 2026-10-08) allows a list larger than the free rows: the vocabulary
//! is re-allocated for its new column count by the ordinary sizing rule, and the input tables are
//! padded with zero rows. Without it, a list that does not fit is refused as before.
//!
//! `--init-from-oov` starts each appended row as a copy of the out-of-vocabulary column the name
//! resolved to before, so the written policy scores every option exactly as the source did. The
//! default starts appended rows at zero, which equals the source only where that OOV column is
//! itself zero: a trained OOV column (checkpoint-41476's `state-kind` and `option` are not) means
//! zero rows change play.

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
    let names_path = argument("--names").unwrap_or_else(|| refuse("--names is required"));
    let destination = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let destination = Path::new(&destination);
    let grow = std::env::args().any(|arg| arg == "--grow");
    let init_from_oov = std::env::args().any(|arg| arg == "--init-from-oov");
    if destination.exists() {
        refuse(&format!(
            "{} already exists; bundles are never written in place",
            destination.display()
        ));
    }
    ti4_tensor::configure_deterministic(20_260_922)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));

    let text = std::fs::read_to_string(&names_path)
        .unwrap_or_else(|error| refuse(&format!("{names_path}: {error}")));
    let names: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    for name in &names {
        if !ti4_policy::projection::admits(name) {
            refuse(&format!(
                "{name} is not in an admitted family; the MLP would never read it"
            ));
        }
    }

    let loaded = ti4_mlp::bundle::read(Path::new(&source))
        .unwrap_or_else(|error| refuse(&format!("{source}: {error}")));
    let ti4_mlp::bundle::Loaded {
        actor,
        mut vocabulary,
        critic_mode,
        update,
    } = loaded;
    let mut actor = actor;
    let slots_before = vocabulary.slot_count();
    let capacity_before = vocabulary.capacity();
    let fresh: Vec<&String> = names
        .iter()
        .filter(|name| !vocabulary.is_assigned(name))
        .collect();
    // Where each new name resolved before the append: its family's out-of-vocabulary column.
    let fallback: Vec<usize> = fresh.iter().map(|name| vocabulary.column_of(name)).collect();
    let added = if grow {
        let growth = vocabulary
            .append_reallocating(fresh.iter().map(|name| name.as_str()))
            .unwrap_or_else(|error| refuse(&format!("appending names: {error}")));
        actor
            .grow_input_capacity(growth)
            .unwrap_or_else(|error| refuse(&format!("growing the input tables: {error}")));
        growth.added
    } else {
        vocabulary
            .append(fresh.iter().map(|name| name.as_str()))
            .unwrap_or_else(|error| refuse(&format!("appending names: {error}")))
    };
    let column = |index: usize| i64::try_from(index).expect("column fits");
    let pairs: Vec<(i64, i64)> = fresh
        .iter()
        .zip(&fallback)
        .map(|(name, from)| (column(*from), column(vocabulary.column_of(name))))
        .collect();
    if init_from_oov {
        actor
            .copy_input_rows(&pairs)
            .unwrap_or_else(|error| refuse(&format!("copying fallback rows: {error}")));
    }
    // Each appended row must be what it was declared to start as: its old fallback row under
    // --init-from-oov (so play is unchanged), zero otherwise.
    for ((from, to), name) in pairs.iter().zip(&fresh) {
        let row = actor.input().get(*to);
        let expected = if init_from_oov {
            actor.input().get(*from)
        } else {
            row.zeros_like()
        };
        let difference = (&row - &expected)
            .abs()
            .sum(ti4_tensor::Kind::Float)
            .double_value(&[]);
        if difference != 0.0 {
            refuse(&format!(
                "input row {to} for {name} is not its declared start ({difference})"
            ));
        }
    }

    let start = if init_from_oov {
        "rows copied from each name's former OOV column"
    } else {
        "zero rows"
    };
    let slots = vocabulary
        .to_json()
        .unwrap_or_else(|error| refuse(&format!("encoding slots.json: {error}")));
    ti4_mlp::bundle::write(
        destination,
        &actor,
        &slots,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: if vocabulary.capacity() == capacity_before {
                format!("{added} names appended from {names_path} to {source}; {start}")
            } else {
                format!(
                    "{added} names appended from {names_path} to {source}; {start}; capacity \
                     grown {capacity_before} -> {} with zero padding (BF-22 migrate)",
                    vocabulary.capacity()
                )
            },
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
    println!("source   {source}  (slots {slots_before}, update {update})");
    println!(
        "written  {}  (slots {}, capacity {}, {added} of {} names new)",
        destination.display(),
        reread.vocabulary.slot_count(),
        reread.vocabulary.capacity(),
        names.len()
    );
}
