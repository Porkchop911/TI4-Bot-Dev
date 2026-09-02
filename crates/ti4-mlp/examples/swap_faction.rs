//! Give one faction another's conditioning, and see what it does.
//!
//! Three tensors carry faction identity: the identity embedding added to the first-layer
//! preactivation, and the `delta`/`b_delta` readout adjustments. Everything else in the trunk is
//! shared. Copying those three rows makes the model *treat* the target faction exactly as it treats
//! the source, while the features it is shown remain the target's own — its real fleet, its real
//! planets, its real abilities.
//!
//! That splits a question the per-faction clearance table cannot. Hacan sits at 90.38% and Letnev at
//! 95.07%; either Hacan's learned conditioning is worse, or Hacan's opening is genuinely harder. If
//! Hacan improves wearing Letnev's rows, it is the conditioning. If it does not, the difficulty is
//! in the position and no amount of per-faction training will move it.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example swap_faction -- \
//!   --bundle out/champions/best-94.97_r2-epoch22 --from letnev --to hacan \
//!   --out out/checkpoints/swap-letnev-into-hacan
//! ```

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

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let from = argument("--from").unwrap_or_else(|| refuse("--from is required"));
    let to = argument("--to").unwrap_or_else(|| refuse("--to is required"));
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let git_commit = std::env::var("GIT_COMMIT")
        .unwrap_or_else(|_| refuse("GIT_COMMIT is required so the bundle can be traced"));

    let source = ti4_mlp::FactionRow::of(&from)
        .unwrap_or_else(|error| refuse(&format!("--from {from}: {error}")));
    let target =
        ti4_mlp::FactionRow::of(&to).unwrap_or_else(|error| refuse(&format!("--to {to}: {error}")));

    let mut actor = loaded.actor;
    actor.copy_faction_identity(source, target);

    ti4_mlp::bundle::write(
        std::path::Path::new(&out),
        &actor,
        &slots_text,
        loaded.critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!("swap_faction {from} -> {to}"),
            git_commit,
            update: loaded.update,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing {out}: {error}")));

    println!("{to} now wears {from}'s conditioning; everything else is unchanged.");
    println!("wrote {out}");
}
