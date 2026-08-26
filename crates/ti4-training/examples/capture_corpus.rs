//! M10-031: capture the fixed teacher corpus.
//!
//! ```text
//! cargo run --release -p ti4-training --example capture_corpus
//! ```
//!
//! Every input the corpus identity depends on is verified before a single game is played, and its
//! digest goes into the manifest: the teacher checkpoint, the map pool, and the accepted feature
//! vocabulary. A corpus that cannot say which teacher produced it is not reproducible, and every
//! number the distillation later quotes rests on it.

use std::collections::BTreeMap;
use std::sync::Arc;

use sha2::Digest;
use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_training::teacher_corpus::{Cluster, ExpectedCorpus, capture};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    eprintln!("No corpus was written.");
    std::process::exit(2);
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn main() {
    let content = ContentStore::embedded();
    let out = argument("--out").unwrap_or_else(|| "out/corpus/teacher-v1".to_owned());
    let directory = std::path::Path::new(&out);
    if directory.join("manifest.json").exists() {
        refuse(&format!(
            "{out} already holds a complete corpus; the fixed corpus is never refreshed implicitly \
             (MLP plan §6.1)"
        ));
    }

    let checkpoint_path =
        argument("--checkpoint").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let checkpoint = std::fs::read(&checkpoint_path)
        .unwrap_or_else(|error| refuse(&format!("reading {checkpoint_path}: {error}")));
    let teacher_sha256 = sha256(&checkpoint);
    if teacher_sha256 != ti4_sim::baseline::R6_CHECKPOINT_SHA256 {
        refuse(&format!(
            "{checkpoint_path} is {teacher_sha256}, not the accepted r6 checkpoint {}",
            ti4_sim::baseline::R6_CHECKPOINT_SHA256
        ));
    }
    let champions =
        ti4_training::vocabulary_corpus::champion_profiles(&checkpoint, &checkpoint_path)
            .unwrap_or_else(|error| refuse(&format!("checkpoint profiles: {error}")));
    for faction in FACTIONS {
        if !champions.contains_key(faction) {
            refuse(&format!("the checkpoint has no champion for {faction}"));
        }
    }

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| {
        refuse(&format!(
            "{pool_path} is not an allowed training pool: {error}"
        ))
    });
    let pool_sha256 = sha256(&pool_bytes);
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the verified pool: {error}"))),
    );

    // The accepted vocabulary. Captured names must be resolvable against the same generation the
    // student will be built on, so its digest is part of the corpus identity.
    let generation = ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new(
        "out/vocabulary",
    ))
    .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary generation: {error}")));
    let slots_text = std::fs::read_to_string(&generation.slots)
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let slots_sha256 = sha256(slots_text.as_bytes());

    println!("M10-031 teacher corpus capture");
    println!("  teacher     {teacher_sha256}");
    println!("  pool        {pool_path}  {pool_sha256}");
    println!("  vocabulary  {slots_sha256}");
    println!("  out         {out}");
    println!("  seeds       train 202608210..202608306, validation 202608306..202608338");
    println!("  factions    {}\n", FACTIONS.join(", "));

    let started = std::time::Instant::now();
    let corpus = capture(
        directory,
        content,
        DEFAULT,
        &pool,
        &champions,
        &FACTIONS,
        TILE_SEED_OFFSET,
        &teacher_sha256,
        &pool_sha256,
        &slots_sha256,
    )
    .unwrap_or_else(|error| refuse(&error.to_string()));

    let train = corpus.decisions.get(&Cluster::Train).copied().unwrap_or(0);
    let validation = corpus
        .decisions
        .get(&Cluster::Validation)
        .copied()
        .unwrap_or(0);
    println!("  games                 {}", corpus.games);
    println!("  train decisions       {train}");
    println!("  validation decisions  {validation}");
    println!("  forced dropped        {}", corpus.forced_dropped);
    println!("  manifest              {}", corpus.manifest_sha256);
    println!("  wall time             {:.1?}", started.elapsed());

    // Read both shards back through the verifying loader before claiming success. A corpus that
    // cannot be read is not a corpus, and finding that out here costs one pass rather than a
    // distillation run.
    let expected_corpus = ExpectedCorpus {
        teacher_sha256: &teacher_sha256,
        pool_sha256: &pool_sha256,
        slots_sha256: &slots_sha256,
    };
    for cluster in [Cluster::Train, Cluster::Validation] {
        let decisions =
            ti4_training::teacher_corpus::read_shard(directory, cluster, &expected_corpus)
                .unwrap_or_else(|error| {
                    refuse(&format!(
                        "re-reading the {} shard: {error}",
                        cluster.as_str()
                    ))
                });
        let by_faction: BTreeMap<&str, usize> =
            decisions.iter().fold(BTreeMap::new(), |mut counts, d| {
                *counts.entry(d.faction.as_str()).or_default() += 1;
                counts
            });
        println!(
            "\n  {} shard verified: {} decisions",
            cluster.as_str(),
            decisions.len()
        );
        for (faction, count) in &by_faction {
            println!("    {faction:<10} {count}");
        }
        if by_faction.len() != FACTIONS.len() {
            refuse(&format!(
                "the {} shard covers {} factions, not {}",
                cluster.as_str(),
                by_faction.len(),
                FACTIONS.len()
            ));
        }
    }
}
