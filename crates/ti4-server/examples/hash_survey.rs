//! Replay saved games and print their decision hashes and final-state hash, for comparing two
//! builds of the engine over the same saved histories.
//!
//! ```text
//! cargo run -p ti4-server --example hash_survey -- <server-data dir>
//! ```
//!
//! The directory should be a COPY of a store root (a folder of `game_*` directories); nothing is
//! written. Prints one JSON object per game: whether the replay reproduced the recorded decision
//! hashes, a hash of all of them, a hash of the final state, and the dice rolled.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use ti4_content::ContentStore;
use ti4_server::storage::FileGameStore;

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("store root"));
    let store = FileGameStore::new(&root).expect("open store");
    let content = ContentStore::embedded();
    let mut ids: Vec<String> = std::fs::read_dir(&root)
        .expect("read store")
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("game_"))
        .collect();
    ids.sort();
    for id in ids {
        let (initial_state, player_ids, seed, map_template) = match store
            .load_player_init(&id)
            .map(|p| (p.initial_state, p.player_ids, Some(p.seed), p.map_template))
            .or_else(|_| {
                store
                    .load_init(&id)
                    .map(|i| (i.initial_state, i.player_ids, i.seed, i.map_template))
            }) {
            Ok(init) => init,
            Err(error) => {
                println!(
                    "{}",
                    serde_json::json!({"game": id, "error": error.to_string()})
                );
                continue;
            }
        };
        let history = store.load_history(&id).ok().flatten();
        let decisions = history.as_ref().map_or_else(
            || store.load_decisions(&id).unwrap_or_default(),
            |h| h.decisions.clone(),
        );
        let marks = history
            .as_ref()
            .map(|h| h.rng_marks.clone())
            .unwrap_or_default();
        let galaxy = seed.and_then(|seed| {
            ti4_server::map::rebuild_galaxy(
                content,
                &player_ids,
                seed,
                map_template.as_deref(),
                &initial_state,
            )
            .ok()
        });
        match ti4_server::session::replay::replay_session_forced(
            &initial_state,
            galaxy.as_ref(),
            &decisions,
            &marks,
        ) {
            Ok(report) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                format!("{:?}", report.replayed_hashes).hash(&mut hasher);
                let state = serde_json::to_vec(&report.final_state).unwrap();
                let rolls = report_rolls(&report);
                println!(
                    "{}",
                    serde_json::json!({
                        "game": id,
                        "decisions": report.decision_count,
                        "hashes_match": report.hashes_match,
                        "decision_hashes": format!("{:016x}", hasher.finish()),
                        "state_hash": format!("{:016x}", fnv(&state)),
                        "rolls": rolls,
                    })
                );
            }
            Err(error) => println!(
                "{}",
                serde_json::json!({"game": id, "error": error.to_string()})
            ),
        }
    }
}

fn report_rolls(report: &ti4_server::session::replay::ReplayReport) -> serde_json::Value {
    let mut by_reason = std::collections::BTreeMap::<String, usize>::new();
    for roll in &report.rolls {
        *by_reason.entry(roll.reason.clone()).or_default() += 1;
    }
    let faces: Vec<u32> = report.rolls.iter().flat_map(|r| r.faces.clone()).collect();
    serde_json::json!({
        "total": report.rolls.len(),
        "faces_hash": format!("{:016x}", fnv(&serde_json::to_vec(&faces).unwrap())),
        "by_reason": by_reason,
    })
}
