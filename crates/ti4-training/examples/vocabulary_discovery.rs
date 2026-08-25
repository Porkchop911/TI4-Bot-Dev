//! M09-024b2: build the dense feature vocabulary from §4.5's three sources.
//!
//! One bounded discovery pass over the §6.1 teacher seed schedule, publishing exactly one artifact.
//!
//! # Everything here fails closed
//!
//! An earlier version verified nothing and published anyway. It opened the checkpoint twice without
//! checking either read against the durable accepted identity, opened the pool without its role
//! gate, discarded every rollout error while counting the game as a success, enforced only the
//! global 65,536 limit rather than this branch's reviewed 24,576 ceiling, accepted any `--rounds`
//! value while recording no schedule identity, and wrote the artifact with a bare `fs::write` whose
//! digest was computed from memory rather than from the bytes on disk.
//!
//! Each of those is now a gate, and the artifact is published only if every one of them passes.
//!
//! ```text
//! cargo run --release -p ti4-training --example vocabulary_discovery -- \
//!     --checkpoint out/stage2_r6/final10000.json \
//!     --map-pool out/pools/full_np8_12_train.json \
//!     --out out/vocabulary/slots.json
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::Digest;
use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_policy::vocabulary::{CAPACITY_LIMIT, Vocabulary};
use ti4_sim::artifacts::ArtifactRole;
use ti4_training::rollout::Horizon;
use ti4_training::vocabulary_corpus::{
    Contribution, champion_names, champion_profiles, content_names, replay_names,
};

/// MLP plan §6.1's fixed teacher seed schedule. Not configurable: a one-round or half-schedule pass
/// would publish under the same evidence labels as the approved run (F-M09-024b2-4).
const SEEDS: std::ops::Range<u64> = 202_608_210..202_608_338;
/// The six r6 champion factions, in the rotation order the schedule uses.
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// §6.1's horizon. Fixed for the same reason as the seed range.
const ROUNDS: u32 = 4;
/// The reviewed ceiling for this branch, below the architecture's global limit.
const REVIEWED_CAPACITY_CEILING: usize = 24_576;
/// Declared artifact cap.
const ARTIFACT_CAP_BYTES: usize = 16 * 1024 * 1024;
/// §4.5's three sources, and exactly three.
const EXPECTED_SOURCES: usize = 3;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    eprintln!("No artifact was written.");
    std::process::exit(2);
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

#[expect(
    clippy::too_many_lines,
    reason = "a linear discovery script: every gate is visible in the order it runs rather than \
              hidden behind call sites"
)]
fn main() {
    let content = ContentStore::embedded();
    let checkpoint_path =
        argument("--checkpoint").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let out =
        PathBuf::from(argument("--out").unwrap_or_else(|| "out/vocabulary/slots.json".to_owned()));

    let started = std::time::Instant::now();

    // --- Inputs: read once, verify those exact bytes, parse every consumer from them. ---
    let checkpoint_bytes = match std::fs::read(&checkpoint_path) {
        Ok(bytes) => bytes,
        Err(error) => refuse(&format!("reading {checkpoint_path}: {error}")),
    };
    let checkpoint_sha = sha256(&checkpoint_bytes);
    // The **exact** digest. A 16-hex prefix is 64 bits, and a gate that accepts any envelope
    // sharing it is not enforcing a durable identity (F-M09-024b2-6/-9).
    if checkpoint_sha != ti4_sim::baseline::R6_CHECKPOINT_SHA256 {
        refuse(&format!(
            "{checkpoint_path} is {checkpoint_sha}, not the accepted r6 checkpoint {}",
            ti4_sim::baseline::R6_CHECKPOINT_SHA256
        ));
    }
    let pool_bytes = match ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(&pool_path),
        &[ArtifactRole::Train],
    ) {
        Ok(bytes) => bytes,
        Err(error) => refuse(&format!(
            "{pool_path} is not an approved Train pool: {error}"
        )),
    };
    let pool_sha = sha256(&pool_bytes);
    let pool = match ti4_sim::MapPool::from_reader(Cursor::new(&pool_bytes)) {
        Ok(pool) => Arc::new(pool),
        Err(error) => refuse(&format!("parsing the verified pool bytes: {error}")),
    };
    println!("checkpoint {checkpoint_sha} (accepted r6)");
    println!("pool       {pool_sha} (role Train)");

    // --- The three sources. ---
    let champions = match champion_names(&checkpoint_bytes, &checkpoint_path) {
        Ok(contribution) => contribution,
        Err(error) => refuse(&format!("source (a): {error}")),
    };
    let content_source = content_names(content, DEFAULT);
    let profiles = match champion_profiles(&checkpoint_bytes, &checkpoint_path) {
        Ok(profiles) => profiles,
        Err(error) => refuse(&format!("checkpoint profiles: {error}")),
    };
    let campaign = match replay_names(
        content,
        DEFAULT,
        &pool,
        &profiles,
        &FACTIONS,
        SEEDS,
        TILE_SEED_OFFSET,
        Horizon {
            rounds: ROUNDS,
            steps: 2_000_000,
        },
    ) {
        Ok(campaign) => campaign,
        Err(error) => refuse(&format!("source (b): {error}")),
    };
    let replay = Contribution {
        source: "replay",
        names: campaign.names.clone(),
    };
    println!(
        "source (a) r6 champions : {} names\nsource (c) content      : {} names\nsource (b) \
         replay       : {} names over {} completed games",
        champions.names.len(),
        content_source.names.len(),
        replay.names.len(),
        campaign.completed
    );

    // --- Gate: three sources, each non-empty and each contributing something no other did. ---
    let sources = [&champions, &content_source, &replay];
    assert_eq!(sources.len(), EXPECTED_SOURCES, "§4.5 names three sources");
    println!("\nunique contributions:");
    for (index, source) in sources.iter().enumerate() {
        let others: Vec<&Contribution> = sources
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, s)| *s)
            .collect();
        let unique = source.unique_against(&others);
        println!(
            "  {:<16} {unique:>6} names no other source produced",
            source.source
        );
        if source.names.is_empty() {
            refuse(&format!("source {} is empty", source.source));
        }
        if unique == 0 {
            refuse(&format!(
                "source {} contributed nothing no other source did; it is not load-bearing",
                source.source
            ));
        }
    }

    // --- Build. ---
    let mut union: BTreeSet<String> = BTreeSet::new();
    for source in sources {
        union.extend(source.names.iter().cloned());
    }
    let mut by_family: BTreeMap<String, usize> = BTreeMap::new();
    for name in &union {
        *by_family
            .entry(ti4_policy::vocabulary::family_of(name).to_owned())
            .or_default() += 1;
    }
    let mut ranked: Vec<(&String, &usize)> = by_family.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!(
        "\nunion by family ({} names, {} families):",
        union.len(),
        by_family.len()
    );
    for (family, count) in ranked.iter().take(12) {
        #[expect(clippy::cast_precision_loss, reason = "reporting only")]
        let share = 100.0 * (**count as f64) / (union.len() as f64);
        println!("  {family:<24} {count:>7}  {share:>5.1}%");
    }

    let vocabulary = match Vocabulary::build(union.iter()) {
        Ok(vocabulary) => vocabulary,
        Err(error) => refuse(&format!("building the vocabulary: {error}")),
    };

    // --- Gate: the reviewed ceiling, not merely the architecture's global limit. ---
    if vocabulary.capacity() > REVIEWED_CAPACITY_CEILING {
        refuse(&format!(
            "V_cap {} is above this branch's reviewed ceiling of {REVIEWED_CAPACITY_CEILING} (the \
             global limit is {CAPACITY_LIMIT}); this needs a fresh architecture review",
            vocabulary.capacity()
        ));
    }

    // --- Gate: the double build over reversed input. ---
    let reversed: Vec<&String> = union.iter().rev().collect();
    let second = match Vocabulary::build(reversed) {
        Ok(second) => second,
        Err(error) => refuse(&format!("the second build failed: {error}")),
    };
    let text = match vocabulary.to_json() {
        Ok(text) => text,
        Err(error) => refuse(&format!("serialising: {error}")),
    };
    let second_text = second.to_json().unwrap_or_default();
    if text != second_text {
        refuse("the double build over reversed input differed");
    }
    if text.len() > ARTIFACT_CAP_BYTES {
        refuse(&format!(
            "slots.json is {} bytes, above the {ARTIFACT_CAP_BYTES} cap",
            text.len()
        ));
    }
    println!("\ndouble build over reversed input: byte-identical");

    // --- Provenance, tied to the artifact, published with it as one generation. ---
    let digest = sha256(text.as_bytes());
    let provenance = format!(
        "{{\n \"artifact\": \"{}\",\n \"slots_sha256\": \"{digest}\",\n \"slot_count\": {},\n \
         \"v_cap\": {},\n \"allocated_for\": {},\n \"oov_registry_version\": {},\n \
         \"oov_count\": {},\n \"checkpoint_sha256\": \"{checkpoint_sha}\",\n \
         \"pool_sha256\": \"{pool_sha}\",\n \"pool_role\": \"train\",\n \
         \"seed_range\": \"{}..{}\",\n \"rotations\": {},\n \"faction_order\": \"{}\",\n \
         \"horizon_rounds\": {ROUNDS},\n \"games_completed\": {},\n \
         \"tile_seed_offset\": {TILE_SEED_OFFSET},\n \"content_scope\": \"DEFAULT\"\n}}\n",
        out.display(),
        vocabulary.slot_count(),
        vocabulary.capacity(),
        vocabulary.allocated_for(),
        vocabulary.oov_registry_version(),
        vocabulary.oov_count(),
        SEEDS.start,
        SEEDS.end,
        FACTIONS.len(),
        FACTIONS.join(","),
        campaign.completed,
    );
    // One generation, committed by a single pointer update. A publication failure reports its own
    // state rather than going through `refuse`, whose "No artifact was written" is only true
    // before publication begins.
    let root = out.parent().unwrap_or(Path::new(".")).to_path_buf();
    let published =
        match ti4_training::vocabulary_corpus::publish_generation(&root, &text, &provenance) {
            Ok(published) => published,
            Err(error) => {
                eprintln!(
                    "
PUBLICATION FAILED: {error}"
                );
                std::process::exit(3);
            }
        };
    assert_eq!(published.digest, digest, "the published digest moved");

    println!("\nmanifest:");
    print!("{provenance}");
    println!("  artifact bytes        {}", text.len());
    println!("  generation            {}", published.slots.display());
    println!("  provenance            {}", published.provenance.display());
    println!(
        "  accepted pointer      {}",
        root.join("current.json").display()
    );
    println!("  wall time             {:.1?}", started.elapsed());
}
