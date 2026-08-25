//! M09-024b: build the dense feature vocabulary from §4.5's three sources.
//!
//! One bounded discovery pass. Reads the r6 checkpoint and the training map pool, replays the
//! §6.1 teacher seed schedule, and writes exactly one artifact — the `slots.json` every trained
//! weight will be addressed by.
//!
//! Run:
//! ```text
//! cargo run --release -p ti4-training --example vocabulary_discovery -- \
//!     --checkpoint out/stage2_r6/final10000.json \
//!     --map-pool out/pools/full_np8_12_train.json \
//!     --out out/vocabulary/slots.json
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_policy::vocabulary::{CAPACITY_LIMIT, Vocabulary};
use ti4_training::rollout::Horizon;
use ti4_training::vocabulary_corpus::{champion_names, content_names, replay_names};

/// MLP plan §6.1's fixed teacher seed schedule.
const SEEDS: std::ops::Range<u64> = 202_608_210..202_608_338;
/// The six r6 champion factions, in the rotation order the schedule uses.
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
/// Matches the diagnostic drivers already in this crate.
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// Declared artifact cap for this package.
const ARTIFACT_CAP_BYTES: usize = 16 * 1024 * 1024;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn report(label: &str, vocabulary: &Vocabulary) {
    println!(
        "  {label:<28} slot_count {:>6}   V_cap {:>6}   free {:>6}",
        vocabulary.slot_count(),
        vocabulary.capacity(),
        vocabulary.free_rows()
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "a linear discovery script: it reads in the order the pass runs, and splitting it               would hide that order behind call sites"
)]
fn main() {
    let content = ContentStore::embedded();
    let checkpoint =
        argument("--checkpoint").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let out =
        PathBuf::from(argument("--out").unwrap_or_else(|| "out/vocabulary/slots.json".to_owned()));
    let rounds: u32 = argument("--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);

    let started = std::time::Instant::now();

    // Source (a) — the champions' existing names.
    let champions = champion_names(Path::new(&checkpoint)).expect("r6 checkpoint");
    println!("source (a) r6 champions : {} names", champions.names.len());

    // Source (c) — everything a content record determines on its own.
    let content_source = content_names(content, DEFAULT);
    println!(
        "source (c) content      : {} names",
        content_source.names.len()
    );

    // Source (b) — the replay. The expensive one, and the only reason this package is P2.
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let profiles = serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool = Arc::new(ti4_sim::MapPool::load(Path::new(&pool_path)).expect("pool"));
    let horizon = Horizon {
        rounds,
        steps: 2_000_000,
    };
    let (replay, games) = replay_names(
        content,
        DEFAULT,
        &pool,
        &profiles,
        &FACTIONS,
        SEEDS,
        TILE_SEED_OFFSET,
        horizon,
    );
    println!(
        "source (b) replay       : {} names over {games} games",
        replay.names.len()
    );

    // What each source alone contributed. A source that produced nothing looks exactly like a
    // source that was redundant unless this is measured.
    println!("\nunique contributions:");
    for (source, others) in [
        (&champions, vec![&content_source, &replay]),
        (&content_source, vec![&champions, &replay]),
        (&replay, vec![&champions, &content_source]),
    ] {
        println!(
            "  {:<24} {:>6} names no other source produced",
            source.source,
            source.unique_against(&others)
        );
    }

    // Where the names actually come from. If the union overruns the architecture limit, the
    // review that follows needs the distribution, not the total: a few unbounded families are a
    // different problem from uniform growth across all of them.
    let mut union: BTreeSet<String> = BTreeSet::new();
    for source in [&champions, &content_source, &replay] {
        union.extend(source.names.iter().cloned());
    }
    let mut by_family: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for name in &union {
        *by_family
            .entry(ti4_policy::vocabulary::family_of(name).to_owned())
            .or_default() += 1;
    }
    let mut ranked: Vec<(&String, &usize)> = by_family.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!(
        "
union by family ({} names, {} families):",
        union.len(),
        by_family.len()
    );
    for (family, count) in ranked.iter().take(15) {
        #[expect(clippy::cast_precision_loss, reason = "reporting only")]
        let share = 100.0 * (**count as f64) / (union.len() as f64);
        println!("  {family:<24} {count:>7}  {share:>5.1}%");
    }

    // Growth, one source at a time, so the replay's contribution to V_cap is readable rather than
    // folded into a single final number.
    println!("\ngrowth:");
    let mut cumulative: BTreeSet<String> = BTreeSet::new();
    let mut built: Option<Vocabulary> = None;
    for source in [&champions, &content_source, &replay] {
        cumulative.extend(source.names.iter().cloned());
        match Vocabulary::build(cumulative.iter()) {
            Ok(vocabulary) => {
                report(&format!("+ {}", source.source), &vocabulary);
                built = Some(vocabulary);
            }
            Err(error) => {
                eprintln!("\nSTOPPED after + {}: {error}", source.source);
                eprintln!(
                    "MLP plan section 4.5: above {CAPACITY_LIMIT} this package stops for an \
                     explicit architecture review rather than allocating a larger model."
                );
                std::process::exit(2);
            }
        }
    }
    let vocabulary = built.expect("at least one source");

    // The double-build requirement, over the real corpus rather than a fixture.
    let reversed: Vec<&String> = cumulative.iter().rev().collect();
    let second = Vocabulary::build(reversed).expect("second build");
    let first_json = vocabulary.to_json().expect("json");
    let second_json = second.to_json().expect("json");
    assert_eq!(
        first_json, second_json,
        "the double build over reversed input differed"
    );
    println!("\ndouble build over reversed input: byte-identical");

    assert!(
        first_json.len() <= ARTIFACT_CAP_BYTES,
        "slots.json is {} bytes, above the declared {ARTIFACT_CAP_BYTES} cap",
        first_json.len()
    );

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).expect("output directory");
    }
    std::fs::write(&out, &first_json).expect("write slots.json");

    let digest = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(first_json.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    println!("\nmanifest:");
    println!("  slots_sha256          {digest}");
    println!("  slot_count            {}", vocabulary.slot_count());
    println!("  V_cap                 {}", vocabulary.capacity());
    println!("  allocated_for         {}", vocabulary.allocated_for());
    println!(
        "  oov_registry_version  {}",
        vocabulary.oov_registry_version()
    );
    println!("  oov_count             {}", vocabulary.oov_count());
    println!("  artifact bytes        {}", first_json.len());
    println!("  wrote                 {}", out.display());
    println!("  wall time             {:.1?}", started.elapsed());
}
