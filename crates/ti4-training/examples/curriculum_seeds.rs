//! Classify candidate training seeds by their two initially-revealed public objectives, and emit
//! a stratified per-update seed file for `ppo_update --curriculum-seeds`.
//!
//! NEGLECTED_SCORING_PLAN_2026-09-09 Stage 2, screen scale, isolated worktree only.
//!
//! Setup reveals its two Stage-I objectives before any play happens (`setup.rs`, 61.13), from
//! `deck_seed` alone -- `ti4_training::rollout::seated` passes the trainer's raw per-game `seed`
//! straight through as `deck_seed`, untransformed. So classification needs one `start_game_seeded`
//! call per candidate seed, no self-play, and is exact rather than a proxy.
use std::io::Write;

use ti4_content::ContentStore;
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn main() {
    let content = ContentStore::embedded();
    let target = argument("--target").unwrap_or_else(|| "infrastructure".to_owned());
    // Exclusion mode: a seed is eligible only if NONE of its initially revealed objectives is in
    // this set. Replaces the single-alias `--target` predicate when given. Setup reveals exactly
    // two Stage-I objectives, so "reject if any is economic" means "both openers are non-economic".
    let reject_if_any: Vec<String> = argument("--reject-if-any").map_or_else(Vec::new, |list| {
        list.split(',')
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    });
    let updates: usize = argument("--updates")
        .unwrap_or_else(|| refuse("--updates is required"))
        .parse()
        .unwrap_or_else(|_| refuse("--updates expects an unsigned integer"));
    let seeds_per_update: usize = argument("--seeds-per-update")
        .map_or(16, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse("--seeds-per-update expects an unsigned integer"))
        });
    let target_fraction: f64 = argument("--target-fraction").map_or(0.25, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--target-fraction expects a float"))
    });
    let scan_from: u64 = argument("--scan-from")
        .unwrap_or_else(|| refuse("--scan-from is required"))
        .parse()
        .unwrap_or_else(|_| refuse("--scan-from expects a u64"));
    let scan_count: u64 = argument("--scan-count")
        .unwrap_or_else(|| refuse("--scan-count is required"))
        .parse()
        .unwrap_or_else(|_| refuse("--scan-count expects a u64"));
    let out_path = argument("--out").unwrap_or_else(|| refuse("--out is required"));

    // Six seats, standard training roster -- must match ppo_update's FACTIONS.len() player count,
    // since setup's per-player card count and deck composition depend on player count (61.13,
    // cards_per_player). Faction identity does not affect which cards are revealed.
    let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();

    let target_want = ((updates * seeds_per_update) as f64 * target_fraction).ceil() as usize;
    let ordinary_want = updates * seeds_per_update - target_want;

    let mut target_seeds: Vec<u64> = Vec::new();
    let mut ordinary_seeds: Vec<u64> = Vec::new();
    let mut scanned = 0u64;
    let mut errors = 0u64;
    for seed in scan_from..scan_from + scan_count {
        scanned += 1;
        match start_game_seeded(content, &players, DEFAULT, None, seed) {
            Ok(state) => {
                let has_target = if reject_if_any.is_empty() {
                    state
                        .revealed_objectives
                        .iter()
                        .any(|id| id.as_str() == target)
                } else {
                    !state
                        .revealed_objectives
                        .iter()
                        .any(|id| reject_if_any.iter().any(|bad| bad == id.as_str()))
                };
                // Every unrejected seed is eligible for the ordinary pool -- "ordinary" means
                // unconditioned, not "excluding the target" (NEGLECTED_SCORING_PLAN_2026-09-09
                // Stage 2: "let normal gameplay generate all later states"). The target pool is
                // the conditioned subset.
                if ordinary_seeds.len() < ordinary_want {
                    ordinary_seeds.push(seed);
                }
                if has_target && target_seeds.len() < target_want {
                    target_seeds.push(seed);
                }
            }
            Err(_) => errors += 1,
        }
        if ordinary_seeds.len() >= ordinary_want && target_seeds.len() >= target_want {
            break;
        }
    }

    if target_seeds.len() < target_want || ordinary_seeds.len() < ordinary_want {
        refuse(&format!(
            "scanned {scanned} seeds from {scan_from}, found {}/{target_want} target and \
             {}/{ordinary_want} ordinary ({errors} setup errors) -- widen --scan-count",
            target_seeds.len(),
            ordinary_seeds.len()
        ));
    }

    // Interleave 75/25 within each update rather than block-ordering the whole file: an update's
    // 16 games should each independently draw its ratio, not have update 0 be all-ordinary and a
    // later update all-target because of where the file happened to be sliced.
    let mut file = std::fs::File::create(&out_path)
        .unwrap_or_else(|error| refuse(&format!("creating {out_path}: {error}")));
    let mut oi = 0usize;
    let mut ti = 0usize;
    let target_per_update = ((seeds_per_update as f64) * target_fraction).round() as usize;
    for _update in 0..updates {
        for slot in 0..seeds_per_update {
            let use_target = slot < target_per_update;
            let seed = if use_target {
                let s = target_seeds[ti];
                ti += 1;
                s
            } else {
                let s = ordinary_seeds[oi];
                oi += 1;
                s
            };
            writeln!(file, "{seed}").unwrap_or_else(|error| refuse(&format!("{error}")));
        }
    }

    eprintln!(
        "wrote {out_path}: {updates} updates x {seeds_per_update} seeds, {target_per_update}/{seeds_per_update} \
         per update from the '{target}' pool ({} target, {} ordinary seeds consumed; scanned {scanned}, \
         base rate {:.3})",
        ti,
        oi,
        target_seeds.len() as f64 / scanned.max(1) as f64
    );
}
