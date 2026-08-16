//! Stage-1 training for xxcha, l1z1x, sol (the three factions not in the original Python curriculum).
//!
//! Uses identical representation, reward, map pool, and optimizer settings as the Hacan/Jol-Nar/Letnev run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::archive::{Archive, Checkpoint};
use ti4_training::gradient::Step;
use ti4_training::reward::Stage;
use ti4_training::rollout::Horizon;
use ti4_training::stage1::{
    FactionPlan, FactionStart, OpeningMetrics, evaluate_factions_on_pool, train_factions,
};

fn number(name: &str, fallback: usize) -> usize {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn path_argument(name: &str) -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

fn blank(factions: &[FactionId]) -> BTreeMap<FactionId, Profile> {
    factions
        .iter()
        .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
        .collect()
}

fn load_start(
    path: &Path,
    factions: &[FactionId],
) -> Result<(BTreeMap<FactionId, Profile>, usize), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse checkpoint: {error}"))?;
    let update = document
        .get("final_update")
        .or_else(|| document.get("update"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let table = document
        .get("profiles")
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .map_err(|error| format!("read profile table: {error}"))?;
    let mut profiles = blank(factions);
    for faction in factions {
        if let Some(profile) = loaded.get(faction.as_str()) {
            profile
                .validate(Some(faction.as_str()))
                .map_err(|error| format!("{faction}: {error}"))?;
            if !profile.is_explicit() {
                return Err(format!(
                    "{faction}: schema {} is hashed; Stage 1 requires explicit profiles",
                    profile.schema
                ));
            }
            profiles.insert(faction.clone(), profile.clone());
        }
    }
    Ok((profiles, update))
}

fn report(update: usize, metrics: &BTreeMap<FactionId, OpeningMetrics>) {
    println!("\nupdate {update}");
    println!("faction       games  clearance  planets  systems   units  shortfall");
    println!("------------  -----  ---------  -------  -------  ------  ---------");
    for (faction, row) in metrics {
        println!(
            "{faction:<12}  {:>5}  {:>9.3}  {:>7.2}  {:>7.2}  {:>6.2}  {:>9.3}",
            row.seat_games,
            row.clearance,
            row.planets_gained,
            row.systems,
            row.units_gained,
            row.shortfall
        );
    }
}

fn save_checkpoint(
    path: &Path,
    update: usize,
    complete: bool,
    profiles: &BTreeMap<FactionId, Profile>,
    metrics: &BTreeMap<FactionId, OpeningMetrics>,
) -> Result<(), String> {
    let mut checkpoint = Checkpoint::new(
        "rust_stage1_policy_gradient_other".to_owned(),
        Stage::One,
        Horizon::opening(),
        BTreeMap::from([("factions".to_owned(), "xxcha,l1z1x,sol".to_owned())]),
    );
    checkpoint.final_update = update;
    checkpoint.run_complete = complete;
    checkpoint.profiles = profiles
        .iter()
        .map(|(faction, profile)| (faction.to_string(), profile.clone()))
        .collect();
    checkpoint.history.push(
        serde_json::to_value(metrics).map_err(|error| format!("serialize metrics: {error}"))?,
    );
    Archive::at(
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    )
    .save(&checkpoint, path)
    .map_err(|error| format!("save {}: {error}", path.display()))
}

#[expect(
    clippy::too_many_lines,
    reason = "one block per faction panel, read as a report"
)]
fn main() -> Result<(), String> {
    let updates = number("--updates", 25_000);
    let every = number("--every", 100).max(1);
    let eval_seeds = u64::try_from(number("--eval-seeds", 32)).unwrap_or(32);
    let checkpoint_path = path_argument("--checkpoint");
    let map_pool_path = path_argument("--map-pool");
    let output = path_argument("--out").unwrap_or_else(|| PathBuf::from("out/stage1_other.json"));

    // Build FactionPlan for xxcha, l1z1x, sol
    let factions: Vec<FactionId> = ["xxcha", "l1z1x", "sol"]
        .into_iter()
        .map(FactionId::new)
        .collect();

    let mut plan = FactionPlan {
        stage: Stage::One,
        rounds: 1,
        factions,
        generations: 0, // set per-batch below
        train_seeds: 16,
        train_seed_stride: 16,
        step: Step {
            learning_rate: 0.03,
            entropy: 0.01,
            gradient_clip: 1.0,
        },
        seed: 73_000_000,
        sources: FULL,
        map_pool: None,
        tile_seed_offset: 20_000_000,
        start: None,
        high_vp_bonus: 0.0,
    };

    // Load map pool if provided
    let map_pool = if let Some(path) = &map_pool_path {
        let pool = ti4_sim::MapPool::load(path)
            .map_err(|error| format!("load {}: {error}", path.display()))?;
        pool.validate_systems(ContentStore::embedded(), plan.sources)
            .map_err(|error| format!("validate {}: {error}", path.display()))?;
        if pool.home_slots() != plan.factions.len() {
            return Err(format!(
                "{} has {} home slots; Stage 1 has {} factions",
                path.display(),
                pool.home_slots(),
                plan.factions.len()
            ));
        }
        Some(Arc::new(pool))
    } else {
        None
    };

    if map_pool.is_some() {
        plan.map_pool.clone_from(&map_pool);
    }

    // Load or start blank profiles
    let (mut profiles, starting_update) = checkpoint_path.as_deref().map_or_else(
        || Ok((blank(&plan.factions), 0)),
        |path| load_start(path, &plan.factions),
    )?;

    println!("Stage-1 training for xxcha/l1z1x/sol");
    println!("  factions: xxcha,l1z1x,sol");
    println!("  representation: schema 4 explicit named heads");
    println!(
        "  batch: {} seeds x 3 rotations = {} games/update",
        plan.train_seeds,
        plan.train_seeds * 3
    );
    println!(
        "  maps: {}",
        map_pool_path.as_ref().map_or_else(
            || "Rust varied-map generator".to_owned(),
            |path| format!("Python-compatible pool {}", path.display())
        )
    );
    println!("  learning rate / entropy / clip: 0.03 / 0.01 / 1.0");
    println!(
        "  start: {}",
        checkpoint_path.as_ref().map_or("blank".to_owned(), |path| {
            format!("{} at update {starting_update}", path.display())
        })
    );

    // Initial evaluation
    let initial = evaluate_factions_on_pool(
        ContentStore::embedded(),
        &plan.factions,
        &profiles,
        plan.sources,
        96_000_000,
        eval_seeds,
        map_pool.clone().expect("map pool required for evaluation"),
        plan.tile_seed_offset,
    );
    report(starting_update, &initial);

    let started = std::time::Instant::now();
    let mut done = 0usize;

    while done < updates {
        let count = every.min(updates - done);
        plan.generations = count;
        plan.start = Some(FactionStart {
            profiles,
            generation: starting_update + done,
        });

        let run = train_factions(ContentStore::embedded(), &plan);
        if run
            .generations
            .iter()
            .any(|generation| generation.errors > 0)
        {
            return Err("a Stage-1 rollout failed; refusing to continue".to_owned());
        }

        profiles = run.profiles;
        done += count;
        let update = starting_update + done;

        // Evaluate on held-out seeds
        let metrics = evaluate_factions_on_pool(
            ContentStore::embedded(),
            &plan.factions,
            &profiles,
            plan.sources,
            96_000_000,
            eval_seeds,
            map_pool.clone().expect("map pool required for evaluation"),
            plan.tile_seed_offset,
        );
        report(update, &metrics);

        // Save checkpoint
        if let Some(ref parent) = output.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        save_checkpoint(&output, update, done == updates, &profiles, &metrics)?;
        println!("checkpointed {} at update {update}", output.display());

        // Check if all factions >= 0.900 clearance
        let all_above = metrics.values().all(|m| m.clearance >= 0.900);
        if all_above && done < updates {
            println!("\nAll factions reached >= 0.900 clearance at update {update}");
            break;
        }
    }

    let elapsed = started.elapsed();
    println!(
        "\n{} updates in {:.1}s ({:.3}s/update)",
        done,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / f64::from(u32::try_from(done.max(1)).unwrap_or(1))
    );
    Ok(())
}
