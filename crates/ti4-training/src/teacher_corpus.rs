//! The fixed teacher corpus (M10-031), per MLP plan §6.1.
//!
//! # What is captured
//!
//! The r6 champions play the training map pool for seeds `202_608_210..202_608_338` × six
//! rotations — 768 games — and every **non-forced** decision is recorded with:
//!
//! - its deterministic order, faction and head;
//! - every legal option id;
//! - the sparse factual actor vector per option;
//! - the option-free factual critic vector;
//! - the complete teacher probability vector;
//! - the accepted four-round return the critic warm-up reads.
//!
//! # Forced choices are filtered, and that is not an optimisation
//!
//! A decision with one legal option carries no information about preference: the teacher's
//! probability for it is 1.0 whatever the teacher believes, so a KL term over it is identically
//! zero and it contributes nothing but weight. Worse, forced decisions are not uniformly
//! distributed across heads — they cluster in the mechanical ones — so keeping them would tilt the
//! per-faction mean §6.1 asks for toward whichever head happens to be most often forced.
//!
//! # Hidden information
//!
//! Capture runs inside `choose_seeing`, from the engine-bound `SeatObservation`. §6.1: "Capture is
//! built only from the acting seat's authorized view; it does not retain a raw omniscient state as
//! a shortcut." That is enforced by the same types M09-021 and M09-027 established rather than by
//! this module's care — the critic extractor takes the capability and nothing else.
//!
//! # The split is by seed, never by decision
//!
//! `202_608_210..202_608_306` trains, `202_608_306..202_608_338` validates. Splitting by decision
//! would put decisions from the *same game* on both sides, and a model that memorised a position
//! would score as one that generalised. Any overlap between the clusters is a hard error, checked
//! rather than assumed.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::Digest;
use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::learned::Profile;

use crate::reward::{Reward, Stage, returns};
use crate::rollout::{Horizon, OpeningMap, play_capturing};

/// §6.1's fixed capture seeds.
pub const TRAIN_SEEDS: std::ops::Range<u64> = 202_608_210..202_608_306;
/// §6.1's fixed validation seeds. Disjoint from [`TRAIN_SEEDS`], and checked to be.
pub const VALIDATION_SEEDS: std::ops::Range<u64> = 202_608_306..202_608_338;
/// §6.1's horizon.
pub const ROUNDS: u32 = 4;
/// The cap a shard may not exceed, per §6.1.
pub const SHARD_CAP_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Which side of the split a game belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cluster {
    /// Seeds the student learns from.
    Train,
    /// Seeds it is measured on, and never trains on.
    Validation,
}

impl Cluster {
    /// The name this cluster's shard carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Validation => "validation",
        }
    }

    /// Which cluster a seed belongs to, or `None` if it is outside the fixed corpus.
    #[must_use]
    pub fn of(seed: u64) -> Option<Self> {
        if TRAIN_SEEDS.contains(&seed) {
            Some(Self::Train)
        } else if VALIDATION_SEEDS.contains(&seed) {
            Some(Self::Validation)
        } else {
            None
        }
    }
}

/// One captured non-forced decision.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Decision {
    /// The game seed.
    pub seed: u64,
    /// Which rotation of the faction order.
    pub rotation: usize,
    /// The decision's index within its seat's trajectory. With `seed`, `rotation` and `seat` this
    /// is a total order, so a shard can be checked for completeness rather than trusted.
    pub order: usize,
    /// Which seat took it.
    pub seat: String,
    /// The faction that seat played. The conditioning key — never the seat index, which changes
    /// with rotation (F-M09-026-2).
    pub faction: String,
    /// The schema-4 head.
    pub head: String,
    /// Every legal option id, in the order the engine offered them.
    pub options: Vec<String>,
    /// The sparse factual actor vector per option, positionally matched to `options`.
    pub actor: Vec<Vec<(String, f64)>>,
    /// The option-free factual critic vector.
    pub critic: Vec<(String, f64)>,
    /// The teacher's complete probability vector, positionally matched to `options`.
    pub teacher: Vec<f64>,
    /// The accepted four-round return at this decision.
    pub value_target: f64,
}

/// What a capture produced.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// Where the shards and manifest live.
    pub directory: PathBuf,
    /// Decisions captured per cluster.
    pub decisions: BTreeMap<Cluster, usize>,
    /// Games completed.
    pub games: usize,
    /// Non-forced decisions kept, and forced ones dropped.
    pub forced_dropped: usize,
    /// The manifest digest, which identifies this corpus.
    pub manifest_sha256: String,
}

/// Anything that stopped a capture.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// A game did not complete. Counted, never hidden.
    #[error("{completed} of {expected} games completed; first failure: {first}")]
    Campaign {
        /// How many finished.
        completed: usize,
        /// How many were asked for.
        expected: usize,
        /// The first failure's reason.
        first: String,
    },
    /// The corpus is structurally wrong.
    #[error("invalid corpus: {0}")]
    Invalid(String),
    /// The filesystem refused.
    #[error("{context}: {source}")]
    Io {
        /// What was attempted.
        context: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
}

fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> CorpusError {
    let context = context.into();
    |source| CorpusError::Io { context, source }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// The two seed clusters do not overlap.
///
/// §6.1: "Any overlap of seed clusters is a hard error." Checked here rather than trusted to the
/// constants staying disjoint, because the constants are exactly the sort of thing a later edit
/// widens by one without noticing what it now includes.
fn clusters_are_disjoint() -> Result<(), CorpusError> {
    let train: BTreeSet<u64> = TRAIN_SEEDS.collect();
    let validation: BTreeSet<u64> = VALIDATION_SEEDS.collect();
    let overlap: Vec<u64> = train.intersection(&validation).copied().collect();
    if !overlap.is_empty() {
        return Err(CorpusError::Invalid(format!(
            "the seed clusters overlap on {} seeds, e.g. {}",
            overlap.len(),
            overlap[0]
        )));
    }
    if train.is_empty() || validation.is_empty() {
        return Err(CorpusError::Invalid(
            "a seed cluster is empty, so the split would be vacuous".to_owned(),
        ));
    }
    Ok(())
}

fn named(vector: &ti4_policy::features::FeatureVector) -> Vec<(String, f64)> {
    vector
        .iter()
        .map(|(key, value)| (ti4_policy::intern::name_of(*key), *value))
        .collect()
}

/// Capture the fixed corpus and write it to `directory`.
///
/// # Errors
/// [`CorpusError::Campaign`] if any game fails — a corpus with a hole in it is not the fixed corpus
/// §6.1 names, and silently shipping 767 of 768 games would make every later number
/// unreproducible. [`CorpusError::Invalid`] for a structural problem, [`CorpusError::Io`] for the
/// filesystem.
#[expect(
    clippy::too_many_arguments,
    reason = "every input the corpus identity depends on is named here, once"
)]
#[expect(
    clippy::too_many_lines,
    reason = "a linear capture: the campaign is visible in the order it runs"
)]
pub fn capture(
    directory: &Path,
    content: &'static ContentStore,
    sources: SourceSet,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, Profile>,
    factions: &[&str],
    tile_seed_offset: u64,
    teacher_sha256: &str,
    pool_sha256: &str,
    slots_sha256: &str,
) -> Result<Corpus, CorpusError> {
    clusters_are_disjoint()?;

    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let shared: BTreeMap<String, Arc<Profile>> = champions
        .iter()
        .map(|(faction, profile)| (faction.clone(), Arc::new(profile.clone())))
        .collect();

    let seeds: Vec<u64> = TRAIN_SEEDS.chain(VALIDATION_SEEDS).collect();
    let expected = seeds.len() * factions.len();
    let mut completed = 0usize;
    let mut first_failure: Option<String> = None;
    let mut forced_dropped = 0usize;

    // Streamed, not buffered. At roughly 258,000 decisions each carrying every option's feature
    // vector, holding the corpus in memory before writing it needs several gigabytes; the shard
    // itself is far smaller compressed.
    //
    // This is sound only because **the iteration order already is the deterministic order** the
    // shard requires: seeds ascending, then rotation, then seat, then the decision's index within
    // that seat. The ordering is re-checked on read rather than assumed.
    std::fs::create_dir_all(directory).map_err(io("creating the corpus directory"))?;
    let mut shards = Shards::open(directory);
    let mut counts: BTreeMap<Cluster, usize> = BTreeMap::new();

    // Stage 2 is the reward the four-round horizon is defined against; the critic warm-up reads
    // exactly this return, so it is computed here rather than left for the trainer to re-derive
    // from a different reward and quietly disagree.
    let reward = Reward::for_stage(Stage::Two);

    for &seed in &seeds {
        let cluster = Cluster::of(seed)
            .ok_or_else(|| CorpusError::Invalid(format!("seed {seed} belongs to no cluster")))?;
        for rotation in 0..factions.len() {
            let seated: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    (
                        player.clone(),
                        FactionId::new(factions[(index + rotation) % factions.len()]),
                    )
                })
                .collect();
            let profiles: BTreeMap<PlayerId, Arc<Profile>> = players
                .iter()
                .filter_map(|player| {
                    shared
                        .get(seated[player].as_str())
                        .map(|profile| (player.clone(), Arc::clone(profile)))
                })
                .collect();
            if profiles.len() != players.len() {
                return Err(CorpusError::Invalid(format!(
                    "a seated faction has no champion profile at seed {seed} rotation {rotation}"
                )));
            }

            let (rollout, critics) = play_capturing(
                content,
                &players,
                &seated,
                &profiles,
                sources,
                seed,
                Horizon {
                    rounds: ROUNDS,
                    steps: 10_000,
                },
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &OpeningMap::PythonPool {
                    pool: Arc::clone(pool),
                    tile_seed_offset,
                },
                ti4_policy::critic::CriticFeatures::factual(),
            );
            if let Some(error) = rollout.error {
                first_failure.get_or_insert(format!("seed {seed} rotation {rotation}: {error}"));
                continue;
            }
            completed += 1;

            for seat in &rollout.seats {
                let value_targets = returns(&seat.episode, &reward);
                let seat_critics = critics.get(&seat.player);
                for (order, step) in seat.trajectory.iter().enumerate() {
                    // Forced: one legal option. Its KL term is identically zero.
                    if step.legal.len() < 2 {
                        forced_dropped += 1;
                        continue;
                    }
                    // Positional agreement between options, vectors and probabilities is what a
                    // consumer will rely on, so it is built once here and checked on load.
                    let options: Vec<String> = step.legal.keys().cloned().collect();
                    let actor: Vec<Vec<(String, f64)>> = step.legal.values().map(named).collect();
                    let teacher: Vec<f64> = options
                        .iter()
                        .map(|id| step.probabilities.get(id).copied().unwrap_or(0.0))
                        .collect();
                    let critic = seat_critics
                        .and_then(|vectors| vectors.get(order))
                        .map(named)
                        .ok_or_else(|| {
                            CorpusError::Invalid(format!(
                                "no critic vector for decision {order} of {} at seed {seed}",
                                seat.player
                            ))
                        })?;
                    let decision = Decision {
                        seed,
                        rotation,
                        order,
                        seat: seat.player.as_str().to_owned(),
                        faction: seat.faction.as_str().to_owned(),
                        head: step.head.clone(),
                        options,
                        actor,
                        critic,
                        teacher,
                        value_target: value_targets.get(order).copied().unwrap_or(0.0),
                    };
                    shards.write(cluster, &decision)?;
                    *counts.entry(cluster).or_default() += 1;
                }
            }
        }
    }

    if completed != expected {
        return Err(CorpusError::Campaign {
            completed,
            expected,
            first: first_failure.unwrap_or_else(|| "unreported".to_owned()),
        });
    }

    let digests = shards.finish(directory)?;
    write_manifest(
        directory,
        &counts,
        &digests,
        completed,
        forced_dropped,
        teacher_sha256,
        pool_sha256,
        slots_sha256,
    )
}

/// One streaming zstd encoder per cluster, opened on first use.
struct Shards {
    open: BTreeMap<Cluster, zstd::Encoder<'static, std::fs::File>>,
    directory: PathBuf,
}

impl Shards {
    fn open(directory: &Path) -> Self {
        Self {
            open: BTreeMap::new(),
            directory: directory.to_owned(),
        }
    }

    fn write(&mut self, cluster: Cluster, decision: &Decision) -> Result<(), CorpusError> {
        let directory = self.directory.clone();
        let encoder = match self.open.entry(cluster) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let name = format!("{}.jsonl.zst", cluster.as_str());
                let file = std::fs::File::create(directory.join(&name))
                    .map_err(io(format!("creating {name}")))?;
                entry.insert(zstd::Encoder::new(file, 3).map_err(io(format!("opening {name}")))?)
            }
        };
        serde_json::to_writer(&mut *encoder, decision)
            .map_err(|error| CorpusError::Invalid(format!("serialising a decision: {error}")))?;
        encoder
            .write_all(NEWLINE)
            .map_err(io("writing a decision"))?;
        Ok(())
    }

    /// Close every encoder, then hash what actually landed on disk.
    fn finish(self, directory: &Path) -> Result<BTreeMap<String, String>, CorpusError> {
        let mut digests = BTreeMap::new();
        for (cluster, encoder) in self.open {
            let name = format!("{}.jsonl.zst", cluster.as_str());
            let file = encoder.finish().map_err(io(format!("closing {name}")))?;
            file.sync_all().map_err(io(format!("syncing {name}")))?;
            drop(file);
            let written =
                std::fs::read(directory.join(&name)).map_err(io(format!("re-reading {name}")))?;
            if written.len() as u64 > SHARD_CAP_BYTES {
                return Err(CorpusError::Invalid(format!(
                    "the {} shard is {} bytes, above the {SHARD_CAP_BYTES} cap",
                    cluster.as_str(),
                    written.len()
                )));
            }
            digests.insert(name, sha256(&written));
        }
        Ok(digests)
    }
}

/// A record separator, as bytes. Named so the writer reads as data rather than an escape.
const NEWLINE: &[u8] = b"\n";

#[expect(
    clippy::too_many_arguments,
    reason = "the manifest's identity fields are named once, where the manifest is produced"
)]
fn write_manifest(
    directory: &Path,
    counts: &BTreeMap<Cluster, usize>,
    digests: &BTreeMap<String, String>,
    games: usize,
    forced_dropped: usize,
    teacher_sha256: &str,
    pool_sha256: &str,
    slots_sha256: &str,
) -> Result<Corpus, CorpusError> {
    let shard_lines = digests
        .iter()
        .map(|(name, digest)| format!("  \"{name}\": \"{digest}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let train = counts.get(&Cluster::Train).copied().unwrap_or(0);
    let validation = counts.get(&Cluster::Validation).copied().unwrap_or(0);
    // Built as one literal rather than a dozen appends: every field the corpus identity depends on
    // is visible together, which is how a reader checks that none is missing.
    let manifest = format!(
        "{{\n \"schema\": \"ti4-teacher-corpus-v1\",\n \"games\": {games},\n \
         \"train_decisions\": {train},\n \"validation_decisions\": {validation},\n \
         \"forced_dropped\": {forced_dropped},\n \"rounds\": {ROUNDS},\n \
         \"train_seeds\": \"{}..{}\",\n \"validation_seeds\": \"{}..{}\",\n \
         \"teacher_sha256\": \"{teacher_sha256}\",\n \"pool_sha256\": \"{pool_sha256}\",\n \
         \"slots_sha256\": \"{slots_sha256}\",\n \"student_temperature\": 1.0,\n \
         \"shards\": {{\n{shard_lines}\n }}\n}}\n",
        TRAIN_SEEDS.start, TRAIN_SEEDS.end, VALIDATION_SEEDS.start, VALIDATION_SEEDS.end,
    );

    // Written last, so a directory without it is incomplete by construction — the same rule the
    // vocabulary generations and the schema-6 bundle use.
    let mut file = std::fs::File::create(directory.join("manifest.json"))
        .map_err(io("creating manifest.json"))?;
    file.write_all(manifest.as_bytes())
        .map_err(io("writing manifest.json"))?;
    file.sync_all().map_err(io("syncing manifest.json"))?;

    Ok(Corpus {
        directory: directory.to_owned(),
        decisions: counts.clone(),
        games,
        forced_dropped,
        manifest_sha256: sha256(manifest.as_bytes()),
    })
}

/// Read one cluster's shard back, verifying its checksum first.
///
/// # Errors
/// [`CorpusError::Invalid`] if the manifest is missing, the checksum disagrees, a record does not
/// parse, or a record's option/vector/probability lengths do not agree.
pub fn read_shard(directory: &Path, cluster: Cluster) -> Result<Vec<Decision>, CorpusError> {
    let manifest_path = directory.join("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path).map_err(io(
        "reading manifest.json; a corpus without one is incomplete",
    ))?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| CorpusError::Invalid(format!("manifest.json is not JSON: {error}")))?;

    let name = format!("{}.jsonl.zst", cluster.as_str());
    let declared = manifest
        .get("shards")
        .and_then(|shards| shards.get(&name))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CorpusError::Invalid(format!("the manifest has no checksum for {name}")))?;

    let bytes = std::fs::read(directory.join(&name)).map_err(io(format!("reading {name}")))?;
    let found = sha256(&bytes);
    if found != declared {
        return Err(CorpusError::Invalid(format!(
            "{name} hashes {found}, the manifest says {declared}"
        )));
    }

    let plain = zstd::decode_all(bytes.as_slice()).map_err(io(format!("decompressing {name}")))?;
    let text = String::from_utf8(plain)
        .map_err(|error| CorpusError::Invalid(format!("{name} is not UTF-8: {error}")))?;

    let mut decisions = Vec::new();
    for (line, record) in text.lines().enumerate() {
        let decision: Decision = serde_json::from_str(record)
            .map_err(|error| CorpusError::Invalid(format!("{name} line {line}: {error}")))?;
        // The positional agreement a consumer relies on, checked on the way in rather than
        // discovered as a wrong gradient later.
        if decision.actor.len() != decision.options.len()
            || decision.teacher.len() != decision.options.len()
        {
            return Err(CorpusError::Invalid(format!(
                "{name} line {line}: {} options, {} vectors, {} probabilities",
                decision.options.len(),
                decision.actor.len(),
                decision.teacher.len()
            )));
        }
        if decision.options.len() < 2 {
            return Err(CorpusError::Invalid(format!(
                "{name} line {line}: a forced decision reached the corpus"
            )));
        }
        // Every captured seed must be on the side of the split its shard claims. A decision on the
        // wrong side is the leak the seed split exists to prevent, and it is cheap to check.
        if Cluster::of(decision.seed) != Some(cluster) {
            return Err(CorpusError::Invalid(format!(
                "{name} line {line}: seed {} is not in the {} cluster",
                decision.seed,
                cluster.as_str()
            )));
        }
        decisions.push(decision);
    }
    Ok(decisions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_clusters_are_disjoint_and_neither_is_empty() {
        clusters_are_disjoint().expect("the fixed clusters must be disjoint");
        assert_eq!(TRAIN_SEEDS.count(), 96);
        assert_eq!(VALIDATION_SEEDS.count(), 32);
        // 128 seeds x six rotations is §6.1's 768 games.
        assert_eq!((TRAIN_SEEDS.count() + VALIDATION_SEEDS.count()) * 6, 768);
    }

    #[test]
    fn a_seed_belongs_to_exactly_one_cluster() {
        assert_eq!(Cluster::of(202_608_210), Some(Cluster::Train));
        assert_eq!(Cluster::of(202_608_305), Some(Cluster::Train));
        assert_eq!(Cluster::of(202_608_306), Some(Cluster::Validation));
        assert_eq!(Cluster::of(202_608_337), Some(Cluster::Validation));
        // Outside the fixed corpus entirely — not silently folded into either side.
        assert_eq!(Cluster::of(202_608_209), None);
        assert_eq!(Cluster::of(202_608_338), None);
    }

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "ti4-corpus-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn decision(seed: u64, order: usize) -> Decision {
        Decision {
            seed,
            rotation: 0,
            order,
            seat: "seat0".to_owned(),
            faction: "sol".to_owned(),
            head: "production".to_owned(),
            options: vec!["a".to_owned(), "b".to_owned()],
            actor: vec![
                vec![("option:a".to_owned(), 1.0)],
                vec![("option:b".to_owned(), 1.0)],
            ],
            critic: vec![("critic-state:round".to_owned(), 3.0)],
            teacher: vec![0.7, 0.3],
            value_target: 1.5,
        }
    }

    /// Write a shard the way `capture` does: streamed, in the order handed over.
    fn written(scratch: &Scratch, decisions: Vec<Decision>, cluster: Cluster) -> Corpus {
        let mut shards = Shards::open(&scratch.0);
        let mut counts: BTreeMap<Cluster, usize> = BTreeMap::new();
        // Sorted here because `capture` gets this order from its own iteration. A test handing
        // records over out of order would otherwise assert something the writer never promised.
        let mut ordered = decisions;
        ordered.sort_by(|left, right| {
            (left.seed, left.rotation, &left.seat, left.order).cmp(&(
                right.seed,
                right.rotation,
                &right.seat,
                right.order,
            ))
        });
        for decision in &ordered {
            shards.write(cluster, decision).expect("writes");
            *counts.entry(cluster).or_default() += 1;
        }
        let digests = shards.finish(&scratch.0).expect("finishes");
        write_manifest(
            &scratch.0, &counts, &digests, 768, 12, "teacher", "pool", "slots",
        )
        .expect("manifest")
    }

    #[test]
    fn a_shard_round_trips_in_a_deterministic_order() {
        let scratch = Scratch::new("roundtrip");
        // Deliberately out of order on the way in.
        let corpus = written(
            &scratch,
            vec![
                decision(202_608_211, 1),
                decision(202_608_210, 5),
                decision(202_608_210, 2),
            ],
            Cluster::Train,
        );
        assert_eq!(corpus.decisions.get(&Cluster::Train), Some(&3));

        let read = read_shard(&scratch.0, Cluster::Train).expect("reads");
        let keys: Vec<(u64, usize)> = read.iter().map(|d| (d.seed, d.order)).collect();
        assert_eq!(
            keys,
            vec![(202_608_210, 2), (202_608_210, 5), (202_608_211, 1)],
            "the shard is not in the deterministic order"
        );
        // Exact equality is the right assertion here: JSON round-tripping an f64 must return the
        // same bits, and a tolerance would hide the case where it does not.
        assert!((read[0].teacher[0] - 0.7).abs() < f64::EPSILON);
        assert!((read[0].teacher[1] - 0.3).abs() < f64::EPSILON);
        assert!((read[0].value_target - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_tampered_shard_is_refused() {
        let scratch = Scratch::new("tamper");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        let path = scratch.0.join("train.jsonl.zst");
        let mut bytes = std::fs::read(&path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).expect("write");

        let error = read_shard(&scratch.0, Cluster::Train).expect_err("must refuse");
        assert!(error.to_string().contains("hashes"), "{error}");
    }

    #[test]
    fn a_validation_seed_in_the_training_shard_is_refused() {
        // The leak the seed split exists to prevent. Nothing about the record is malformed — the
        // lengths agree and it parses — so only the cluster check catches it.
        let scratch = Scratch::new("leak");
        let _ = written(
            &scratch,
            vec![decision(202_608_210, 0), decision(202_608_310, 1)],
            Cluster::Train,
        );
        let error = read_shard(&scratch.0, Cluster::Train).expect_err("must refuse");
        assert!(
            error.to_string().contains("not in the train cluster"),
            "{error}"
        );
    }

    #[test]
    fn a_forced_decision_never_survives_a_read() {
        let scratch = Scratch::new("forced");
        let mut forced = decision(202_608_210, 0);
        forced.options = vec!["only".to_owned()];
        forced.actor = vec![vec![("option:only".to_owned(), 1.0)]];
        forced.teacher = vec![1.0];
        let _ = written(&scratch, vec![forced], Cluster::Train);

        let error = read_shard(&scratch.0, Cluster::Train).expect_err("must refuse");
        assert!(error.to_string().contains("forced"), "{error}");
    }

    #[test]
    fn a_record_whose_lengths_disagree_is_refused() {
        let scratch = Scratch::new("ragged");
        let mut ragged = decision(202_608_210, 0);
        ragged.teacher = vec![1.0];
        let _ = written(&scratch, vec![ragged], Cluster::Train);

        let error = read_shard(&scratch.0, Cluster::Train).expect_err("must refuse");
        assert!(error.to_string().contains("probabilities"), "{error}");
    }

    #[test]
    fn a_corpus_without_a_manifest_is_incomplete() {
        let scratch = Scratch::new("nomanifest");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        std::fs::remove_file(scratch.0.join("manifest.json")).expect("remove");
        let error = read_shard(&scratch.0, Cluster::Train).expect_err("must refuse");
        assert!(error.to_string().contains("incomplete"), "{error}");
    }
}
