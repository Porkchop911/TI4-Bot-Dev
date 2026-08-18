//! Algorithm arena at Stage 1.
//!
//! The same arms as the Stage-2 arena, on the opening task instead of the four-round one. Three
//! reasons it belongs here rather than there, all measured rather than assumed:
//!
//! * **Faster.** 281.9 games/s against 66.5, and 26,834 decisions per 96 games against 112,940 --
//!   a 4.2x discount on the resource the whole programme is short of.
//! * **Better defined.** Clearance is bounded in `[0, 1]` and asks one question per seat: did the
//!   opening bar get cleared. Victory points at four rounds are a noisy proxy that Stage 1 exists
//!   precisely to avoid selecting on.
//! * **Prerequisite.** Stage 2 resumes from Stage-1 weights, so a Stage-1 policy that clears 42%
//!   of its openings is the floor Stage 2 is built on.
//!
//! **One thing cannot be tested here, and it is the largest effect measured so far.** Stage 1 is a
//! one-round horizon, so bucketing the baseline by round leaves every decision in the same bucket
//! and `--round-baseline` is a no-op. The Stage-2 arena's +1.29 VP result is not reproducible at
//! Stage 1 and must not be re-litigated here. What that buys is a cleaner read on the optimiser
//! family, uncontaminated by the baseline question.
//!
//! There is deliberately **no early stop on a clearance threshold**. The Stage-1 trainer it is
//! adapted from halts when every faction passes 0.900, which would end arms after different
//! numbers of games and make them incomparable -- the one thing an arena cannot allow.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::gradient::Step;
use ti4_training::ppo::PpoStep;
use ti4_training::reward::Stage;
use ti4_training::stage1::{
    FactionPlan, FactionStart, OpeningMetrics, evaluate_factions_on_pool, train_factions,
};

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn number(name: &str, fallback: usize) -> usize {
    argument(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn decimal(name: &str, fallback: f64) -> f64 {
    argument(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn flag(name: &str) -> bool {
    std::env::args().any(|value| value == name)
}

fn load(path: &Path, factions: &[FactionId]) -> Result<BTreeMap<FactionId, Profile>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse checkpoint: {error}"))?;
    let table = document
        .get("profiles")
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(table.clone()).map_err(|error| format!("profile table: {error}"))?;
    let mut profiles = BTreeMap::new();
    for faction in factions {
        let profile = loaded
            .get(faction.as_str())
            .cloned()
            .unwrap_or_else(|| blank_explicit_profile(faction.as_str()));
        profile
            .validate(Some(faction.as_str()))
            .map_err(|error| format!("{faction}: {error}"))?;
        profiles.insert(faction.clone(), profile);
    }
    Ok(profiles)
}

fn report(update: usize, metrics: &BTreeMap<FactionId, OpeningMetrics>) {
    println!("\nupdate {update}");
    println!("faction       games  clearance  shortfall  planets  systems   units");
    println!("------------  -----  ---------  ---------  -------  -------  ------");
    for (faction, row) in metrics {
        println!(
            "{faction:<12}  {:>5}  {:>9.4}  {:>9.4}  {:>7.2}  {:>7.2}  {:>6.2}",
            row.seat_games,
            row.clearance,
            row.shortfall,
            row.planets_gained,
            row.systems,
            row.units_gained
        );
    }
    let count = f64::from(u32::try_from(metrics.len().max(1)).unwrap_or(1));
    // The arena's single comparison number: mean clearance over the six factions. Reported to four
    // places because the differences being looked for are in the third.
    println!(
        "  MEAN CLEARANCE {:.4}   mean shortfall {:.4}",
        metrics.values().map(|row| row.clearance).sum::<f64>() / count,
        metrics.values().map(|row| row.shortfall).sum::<f64>() / count
    );
}

#[expect(clippy::too_many_lines, reason = "one block per phase, read as a report")]
fn main() -> Result<(), String> {
    let updates = number("--updates", 800);
    let every = number("--every", 100).max(1);
    let train_seeds = u64::try_from(number("--train-seeds", 16)).unwrap_or(16);
    let train_seed_base = u64::try_from(number("--train-seed-base", 93_000_000)).unwrap_or(0);
    let eval_seeds = u64::try_from(number("--eval-seeds", 200)).unwrap_or(200);
    let eval_first = u64::try_from(number("--eval-first-seed", 98_000_000)).unwrap_or(0);
    let learning_rate = decimal("--learning-rate", 0.03);
    let entropy = decimal("--entropy", 0.01);
    let discount = decimal("--discount", 1.0);
    let round_baseline = flag("--round-baseline");
    let ppo_epochs = number("--ppo-epochs", 1);
    let ppo_clip = decimal("--ppo-clip", 0.2);
    let checkpoint = argument("--checkpoint").map(PathBuf::from);
    let map_pool_path = argument("--map-pool").map(PathBuf::from);
    let output = argument("--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("out/stage1_arena.json"));

    if round_baseline {
        // Refused rather than silently ignored. Stage 1 is one round, so every decision lands in
        // the same bucket and the flag changes nothing -- accepting it would produce an arm that
        // looks like the Stage-2 winner and is actually a duplicate of the reference.
        return Err(
            "--round-baseline is a no-op at Stage 1: the horizon is one round, so every decision \
             shares a bucket and the centring is identical to the reference. Refusing rather than \
             running a duplicate arm under a different name."
                .to_owned(),
        );
    }
    if ppo_epochs == 0 {
        return Err("--ppo-epochs must be at least 1".to_owned());
    }

    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .into_iter()
        .map(FactionId::new)
        .collect();

    let mut plan = FactionPlan {
        stage: Stage::One,
        rounds: 1,
        factions: factions.clone(),
        generations: 0,
        train_seeds,
        train_seed_stride: train_seeds,
        step: Step {
            learning_rate,
            entropy,
            gradient_clip: 1.0,
        },
        seed: train_seed_base,
        sources: FULL,
        map_pool: None,
        tile_seed_offset: 20_000_000,
        start: None,
        high_vp_bonus: 0.0,
        clearance_weight: 0.0,
        discount,
        round_baseline: false,
        pipeline: false,
        rollout_depth: 1,
        ppo: (ppo_epochs > 1).then_some(PpoStep {
            learning_rate,
            entropy,
            gradient_clip: 1.0,
            clip: ppo_clip,
            epochs: ppo_epochs,
        }),
    };

    let pool = match &map_pool_path {
        Some(path) => {
            let loaded = ti4_sim::MapPool::load(path)
                .map_err(|error| format!("load {}: {error}", path.display()))?;
            loaded
                .validate_systems(ContentStore::embedded(), plan.sources)
                .map_err(|error| format!("validate {}: {error}", path.display()))?;
            if loaded.home_slots() != plan.factions.len() {
                return Err(format!(
                    "{} has {} home slots; this arena seats {}",
                    path.display(),
                    loaded.home_slots(),
                    plan.factions.len()
                ));
            }
            Arc::new(loaded)
        }
        None => return Err("--map-pool is required so every arm plays the same maps".to_owned()),
    };
    plan.map_pool = Some(Arc::clone(&pool));

    let mut profiles = match &checkpoint {
        Some(path) => load(path, &factions)?,
        None => factions
            .iter()
            .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
            .collect(),
    };

    println!("Stage-1 algorithm arena");
    println!("  factions: sol,letnev,xxcha,hacan,jolnar,l1z1x");
    println!("  objective: opening clearance (bounded [0,1]), one-round horizon");
    println!(
        "  batch: {train_seeds} seeds x 6 rotations = {} games/update, seed base {train_seed_base}",
        train_seeds * 6
    );
    println!("  maps: Python-compatible pool {}",
        map_pool_path.as_ref().map_or_else(String::new, |path| path.display().to_string()));
    println!("  step: learning rate {learning_rate:.4} (entropy {entropy}, clip 1)");
    if discount < 1.0 {
        println!("  reward: returns discounted at gamma {discount:.3}");
    }
    if ppo_epochs > 1 {
        println!(
            "  algorithm: PPO -- {ppo_epochs} clipped-surrogate epochs per retained batch, clip {ppo_clip:.2}"
        );
    } else {
        println!("  algorithm: REINFORCE -- one step per batch");
    }
    println!(
        "  held-out panel: {eval_seeds} seeds x 6 rotations from {eval_first} (disjoint from training)"
    );
    println!(
        "  start: {}",
        checkpoint
            .as_ref()
            .map_or("blank".to_owned(), |path| path.display().to_string())
    );
    println!("  early stop: none -- every arm runs the full {updates} updates");

    let initial = evaluate_factions_on_pool(
        ContentStore::embedded(),
        &plan.factions,
        &profiles,
        plan.sources,
        eval_first,
        eval_seeds,
        Arc::clone(&pool),
        plan.tile_seed_offset,
    );
    report(0, &initial);

    let started = std::time::Instant::now();
    let mut done = 0usize;
    while done < updates {
        let count = every.min(updates - done);
        plan.generations = count;
        plan.start = Some(FactionStart {
            profiles,
            generation: done,
        });
        let run = train_factions(ContentStore::embedded(), &plan);
        if run
            .generations
            .iter()
            .any(|generation| generation.errors > 0)
        {
            return Err("a Stage-1 rollout failed; refusing to continue".to_owned());
        }
        if let Some(clip) = run.generations.iter().filter_map(|g| g.clip).last() {
            println!(
                "  trust region: clip fraction {:.4}, approximate KL {:.5}",
                clip.clip_fraction, clip.kl_mean
            );
        }
        profiles = run.profiles;
        done += count;

        let metrics = evaluate_factions_on_pool(
            ContentStore::embedded(),
            &plan.factions,
            &profiles,
            plan.sources,
            eval_first,
            eval_seeds,
            Arc::clone(&pool),
            plan.tile_seed_offset,
        );
        report(done, &metrics);

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let document = serde_json::json!({
            "trainer": "rust_stage1_algorithm_arena",
            "stage": 1,
            "final_update": done,
            "run_complete": done == updates,
            "arguments": {
                "learning_rate": learning_rate,
                "entropy": entropy,
                "discount": discount,
                "ppo_epochs": ppo_epochs,
                "ppo_clip": ppo_clip,
                "train_seeds": train_seeds,
                "train_seed_base": train_seed_base,
            },
            "profiles": profiles,
            "metrics": metrics,
        });
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(&document)
                .map_err(|error| format!("serialize checkpoint: {error}"))?,
        )
        .map_err(|error| format!("write {}: {error}", output.display()))?;
        println!("checkpointed {} at update {done}", output.display());
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "\n{done} updates in {elapsed:.1}s ({:.3}s/update)",
        elapsed / f64::from(u32::try_from(done.max(1)).unwrap_or(1))
    );
    Ok(())
}
