//! Probe: reproduce one Stage-2 boundary's evaluation sequence (incumbent + candidate panels and
//! the isolated fallback) with per-call timing, to diagnose low-utilization stretches.
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut checkpoint = None;
    let mut pool_path = None;
    while i < args.len() {
        match args[i].as_str() {
            "--checkpoint" => {
                i += 1;
                checkpoint = Some(args[i].clone());
            }
            "--map-pool" => {
                i += 1;
                pool_path = Some(args[i].clone());
            }
            _ => {}
        }
        i += 1;
    }
    let (checkpoint, pool_path) = match (checkpoint, pool_path) {
        (Some(c), Some(p)) => (c, p),
        _ => {
            eprintln!("usage: panel_probe --checkpoint <json> --map-pool <gz>");
            return;
        }
    };

    let load_started = Instant::now();
    let bytes = std::fs::read(&checkpoint).expect("read checkpoint");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("parse checkpoint");
    let accepted: std::collections::BTreeMap<
        ti4_model::id::FactionId,
        ti4_policy::learned::Profile,
    > = serde_json::from_value(document["accepted"].clone()).expect("parse accepted table");
    let candidate: std::collections::BTreeMap<
        ti4_model::id::FactionId,
        ti4_policy::learned::Profile,
    > = serde_json::from_value(document["profiles"].clone()).expect("parse learner table");
    let pool = ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("load map pool");
    println!(
        "setup done in {:.1}s (checkpoint + profiles + pool)",
        load_started.elapsed().as_secs_f64()
    );

    let factions: Vec<ti4_model::id::FactionId> =
        ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
            .iter()
            .map(|f| ti4_model::id::FactionId::new(*f))
            .collect();
    let content = ti4_content::ContentStore::embedded();

    fn panel(
        label: &str,
        profiles: &std::collections::BTreeMap<
            ti4_model::id::FactionId,
            ti4_policy::learned::Profile,
        >,
        factions: &[ti4_model::id::FactionId],
        pool: &std::sync::Arc<ti4_sim::MapPool>,
        first_seed: u64,
    ) {
        let seeds: Vec<u64> = (first_seed..first_seed + 32).collect();
        let started = Instant::now();
        let rollouts = ti4_training::rollout::play_rotated_save54_pool_batch(
            ti4_content::ContentStore::embedded(),
            factions,
            profiles,
            ti4_model::content_types::FULL,
            &seeds,
            ti4_training::rollout::Horizon::rounds(4),
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            std::sync::Arc::clone(pool),
            20_000_000,
        );
        let errors = rollouts.iter().filter(|r| r.error.is_some()).count();
        println!(
            "{label}: {} games in {:.1}s ({} errors)",
            rollouts.len(),
            started.elapsed().as_secs_f64(),
            errors
        );
    }

    let pool = std::sync::Arc::new(pool);
    // The trainer's rejected-boundary sequence: incumbent validation + confirmation, candidate
    // validation, then the isolated fallback (one primary panel per faction).
    panel(
        "incumbent-validation",
        &accepted,
        &factions,
        &pool,
        75_000_000,
    );
    panel(
        "incumbent-confirmation",
        &accepted,
        &factions,
        &pool,
        76_000_000,
    );
    panel(
        "candidate-validation",
        &candidate,
        &factions,
        &pool,
        77_000_000,
    );
    for (index, faction) in factions.iter().enumerate() {
        let mut isolated = accepted.clone();
        isolated.insert(faction.clone(), candidate[faction].clone());
        panel(
            &format!("isolated-{faction}-primary"),
            &isolated,
            &factions,
            &pool,
            78_000_000 + index as u64 * 1000,
        );
    }
}
