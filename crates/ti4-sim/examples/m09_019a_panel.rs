//! M09-019a — post-rules r6 validation re-baseline panel (real artifacts).
//!
//! Plays the surviving r6 champions (`out/stage2_r6/final10000.json`, the `accepted` map of the
//! completed run) against the **validation-role** pool (`out/pools/full_np8_12_holdout.json`) on
//! the current post-rules engine, and writes the raw per-game results to
//! `out/m09-019a/panel.json`. The summary printed here is what evidence quotes.
//!
//! Fixed parameters (recorded in `plans/evidence/M09-019.md`):
//! - seeds `919_001..=919_030` (distinct from the M08-021 behavioral suite's `812_xxx` range);
//! - horizon: 4 rounds / 160,000 steps — r6's stage-2 training horizon and the per-round step
//!   budget of [`ti4_sim::run::Horizon`]'s default;
//! - DEFAULT (= FULL) content scope, seats p1..p6 on the six in-scope factions.
//!
//! The checkpoints are read-only inputs: their sha256 is recorded before and after the panel and
//! must be unchanged — the post-rules baseline is a measurement, never an overwrite of pre-rules
//! weights.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_sim::baseline::{R6_CHECKPOINT_SHA_PREFIX, VALIDATION_POOL_SHA_PREFIX, run_panel};
use ti4_sim::run::Horizon;

const POOL: &str = "out/pools/full_np8_12_holdout.json";
const CHECKPOINT: &str = "out/stage2_r6/final10000.json";
const OUT_DIR: &str = "out/m09-019a";

fn main() -> Result<(), String> {
    let pool_path = Path::new(POOL);
    let checkpoint_path = Path::new(CHECKPOINT);
    for path in [pool_path, checkpoint_path] {
        if !path.exists() {
            return Err(format!(
                "missing local artifact {} — the panel measures gitignored out/ data",
                path.display()
            ));
        }
    }

    // Non-overwrite proof: hash both inputs before playing anything.
    let pool_before = sha256_file(pool_path)?;
    let checkpoint_before = sha256_file(checkpoint_path)?;

    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (1..=6)
        .map(|index| PlayerId::new(format!("p{index}")))
        .collect();
    let seeds: Vec<u64> = (919_001..=919_030).collect();
    let horizon = Horizon {
        rounds: 4,
        steps: 160_000,
    };

    let report = run_panel(
        content,
        &players,
        DEFAULT,
        &seeds,
        pool_path,
        checkpoint_path,
        horizon,
        VALIDATION_POOL_SHA_PREFIX,
        R6_CHECKPOINT_SHA_PREFIX,
    )
    .map_err(|error| error.to_string())?;

    // The panel must not have touched its inputs.
    let pool_after = sha256_file(pool_path)?;
    let checkpoint_after = sha256_file(checkpoint_path)?;
    if pool_before != pool_after || checkpoint_before != checkpoint_after {
        return Err("input checksum changed during the panel — refusing to report".into());
    }

    let summary = report.summary();
    // GameResult is not Serialize; the panel record keeps exactly what evidence quotes.
    let games: Vec<serde_json::Value> = report
        .games
        .iter()
        .map(|game| {
            serde_json::json!({
                "seed": game.seed,
                "finished": game.finished,
                "winner": game.winner,
                "rounds": game.rounds,
                "victory_points": game.victory_points,
                "decisions": game.decisions,
                "ended_because": game.ended_because.label(),
                "error": game.error,
            })
        })
        .collect();
    let output = serde_json::json!({
        "package": "M09-019a",
        "pool": POOL,
        "checkpoint": CHECKPOINT,
        "pool_sha256_before": pool_before,
        "pool_sha256_after": pool_after,
        "checkpoint_sha256_before": checkpoint_before,
        "checkpoint_sha256_after": checkpoint_after,
        "seeds": [919_001u64, 919_030],
        "horizon": { "rounds": horizon.rounds, "steps": horizon.steps },
        "sources": "DEFAULT (= FULL)",
        "summary": summary,
        "games": games,
    });

    let out_dir = PathBuf::from(OUT_DIR);
    fs::create_dir_all(&out_dir).map_err(|error| error.to_string())?;
    fs::write(
        out_dir.join("panel.json"),
        serde_json::to_string_pretty(&output).unwrap(),
    )
    .map_err(|error| error.to_string())?;

    println!(
        "M09-019a panel: {} games, {} failed, {} completed, {} decisions",
        summary.games_played, summary.games_failed, summary.completed, summary.total_decisions
    );
    for (seat, mean) in &summary.mean_vp_per_seat {
        println!("  seat {seat}: mean VP {mean:.3}");
    }
    println!(
        "pool sha256 {} | checkpoint sha256 {}",
        report.pool_sha256, report.champion_sha256
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use std::fmt::Write as _;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let digest = Sha256::digest(&bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    Ok(out)
}
