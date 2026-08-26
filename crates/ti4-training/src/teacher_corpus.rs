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
/// The only manifest schema this reader understands.
pub const CORPUS_SCHEMA: &str = "ti4-teacher-corpus-v1";
/// The accepted fixed training-pool bytes used by M10-031.
pub const FIXED_POOL_SHA256: &str =
    "106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df";
/// The six fixed training factions, in rotation order.
pub const FIXED_FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TRAIN_SHARD: &str = "train.jsonl.zst";
const VALIDATION_SHARD: &str = "validation.jsonl.zst";
const MANIFEST_FILE: &str = "manifest.json";

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

/// The external artifacts a consumer expects this fixed corpus to name.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedCorpus<'a> {
    /// Accepted teacher checkpoint bytes.
    pub teacher_sha256: &'a str,
    /// Accepted training-pool bytes.
    pub pool_sha256: &'a str,
    /// Accepted feature-vocabulary bytes.
    pub slots_sha256: &'a str,
}

/// The closed, typed corpus manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Manifest schema.
    pub schema: String,
    /// Completed games.
    pub games: usize,
    /// Records in the training shard.
    pub train_decisions: usize,
    /// Records in the validation shard.
    pub validation_decisions: usize,
    /// Forced decisions deliberately excluded.
    pub forced_dropped: usize,
    /// Rollout horizon.
    pub rounds: u32,
    /// Exact training seed range.
    pub train_seeds: String,
    /// Exact validation seed range.
    pub validation_seeds: String,
    /// Teacher checkpoint identity.
    pub teacher_sha256: String,
    /// Training-pool identity.
    pub pool_sha256: String,
    /// Feature-vocabulary identity.
    pub slots_sha256: String,
    /// Fixed student temperature.
    pub student_temperature: f64,
    /// Closed shard-name to digest map.
    pub shards: BTreeMap<String, String>,
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

fn digest_is_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn expected_seed_range(range: &std::ops::Range<u64>) -> String {
    format!("{}..{}", range.start, range.end)
}

fn manifest(directory: &Path, expected: &ExpectedCorpus<'_>) -> Result<Manifest, CorpusError> {
    let bytes = std::fs::read(directory.join(MANIFEST_FILE)).map_err(io(
        "reading manifest.json; a corpus without one is incomplete",
    ))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| CorpusError::Invalid(format!("manifest.json is invalid: {error}")))?;

    let expected_games = (TRAIN_SEEDS.count() + VALIDATION_SEEDS.count()) * 6;
    let expected_shards = BTreeSet::from([TRAIN_SHARD.to_owned(), VALIDATION_SHARD.to_owned()]);
    let found_shards: BTreeSet<String> = manifest.shards.keys().cloned().collect();
    let expected_files = BTreeSet::from([
        MANIFEST_FILE.to_owned(),
        TRAIN_SHARD.to_owned(),
        VALIDATION_SHARD.to_owned(),
    ]);
    let found_files: BTreeSet<String> = std::fs::read_dir(directory)
        .map_err(io("listing the corpus directory"))?
        .map(|entry| {
            let entry = entry.map_err(io("reading a corpus directory entry"))?;
            entry.file_name().into_string().map_err(|_| {
                CorpusError::Invalid("the corpus contains a non-UTF-8 file name".to_owned())
            })
        })
        .collect::<Result<_, _>>()?;

    let invalid = |message: String| Err(CorpusError::Invalid(message));
    if manifest.schema != CORPUS_SCHEMA {
        return invalid(format!("unsupported schema {}", manifest.schema));
    }
    if manifest.games != expected_games || manifest.rounds != ROUNDS {
        return invalid(format!(
            "manifest describes {} games/{} rounds, expected {expected_games}/{ROUNDS}",
            manifest.games, manifest.rounds
        ));
    }
    if manifest.train_seeds != expected_seed_range(&TRAIN_SEEDS)
        || manifest.validation_seeds != expected_seed_range(&VALIDATION_SEEDS)
    {
        return invalid("manifest seed ranges do not match the fixed split".to_owned());
    }
    if manifest.student_temperature.to_bits() != 1.0f64.to_bits() {
        return invalid("student temperature is not exactly 1.0".to_owned());
    }
    if manifest.teacher_sha256 != expected.teacher_sha256
        || manifest.pool_sha256 != expected.pool_sha256
        || manifest.slots_sha256 != expected.slots_sha256
    {
        return invalid("manifest input identities do not match the accepted inputs".to_owned());
    }
    if manifest.train_decisions == 0 || manifest.validation_decisions == 0 {
        return invalid("a manifest decision count is zero".to_owned());
    }
    if found_shards != expected_shards || found_files != expected_files {
        return invalid(format!(
            "corpus file set is not closed: manifest {found_shards:?}, directory {found_files:?}"
        ));
    }
    if manifest
        .shards
        .values()
        .any(|digest| !digest_is_valid(digest))
    {
        return invalid(
            "a shard checksum is not a lowercase/uppercase hexadecimal SHA-256".to_owned(),
        );
    }
    Ok(manifest)
}

/// Validate the complete fixed-corpus manifest and closed file set.
///
/// # Errors
/// Returns [`CorpusError::Invalid`] for any unsupported layout or identity mismatch.
pub fn validate_manifest(
    directory: &Path,
    expected: &ExpectedCorpus<'_>,
) -> Result<Manifest, CorpusError> {
    manifest(directory, expected)
}

fn staging_directory(directory: &Path) -> Result<PathBuf, CorpusError> {
    if directory.exists() {
        return Err(CorpusError::Invalid(format!(
            "{} already exists; corpus generations are immutable",
            directory.display()
        )));
    }
    let parent = directory.parent().ok_or_else(|| {
        CorpusError::Invalid(format!("{} has no parent directory", directory.display()))
    })?;
    std::fs::create_dir_all(parent).map_err(io("creating the corpus parent directory"))?;
    let name = directory
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| CorpusError::Invalid("the corpus directory name is not UTF-8".to_owned()))?;
    let staging = parent.join(format!(".{name}.staging-{}", std::process::id()));
    std::fs::create_dir(&staging).map_err(io(format!(
        "creating fresh staging directory {}",
        staging.display()
    )))?;
    Ok(staging)
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
    if factions != FIXED_FACTIONS {
        return Err(CorpusError::Invalid(format!(
            "capture factions {factions:?} do not match the fixed roster {FIXED_FACTIONS:?}"
        )));
    }
    let staging = staging_directory(directory)?;

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
    let mut shards = Shards::open(&staging);
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
                    //
                    // The vectors are the **projected** ones the MLP consumes, not
                    // `TrajectoryStep::legal`. `legal` holds the raw schema-4 features the linear
                    // teacher scores with, and the two are different feature sets: the projection
                    // drops the unbounded `state-option:`/`prompt-option:` crosses and adds the bare
                    // `seat-state:` facts. The first capture used `legal` and so trained the student
                    // on inputs it never sees at inference — measured at 131,353 `prompt-option` and
                    // 40,339 `state-option` features present that should not have been, and zero
                    // `seat-state` features that should have been.
                    let projected = seat_critics
                        .and_then(|capture| capture.projected.get(order))
                        .ok_or_else(|| {
                            CorpusError::Invalid(format!(
                                "no projected vectors for decision {order} of {} at seed {seed}",
                                seat.player
                            ))
                        })?;
                    let options: Vec<String> = projected.iter().map(|(id, _)| id.clone()).collect();
                    let actor: Vec<Vec<(String, f64)>> =
                        projected.iter().map(|(_, vector)| named(vector)).collect();
                    let teacher: Vec<f64> = options
                        .iter()
                        .map(|id| step.probabilities.get(id).copied().unwrap_or(0.0))
                        .collect();
                    let critic = seat_critics
                        .and_then(|capture| capture.critic.get(order))
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

    let digests = shards.finish(&staging)?;
    let mut corpus = write_manifest(
        &staging,
        &counts,
        &digests,
        completed,
        forced_dropped,
        teacher_sha256,
        pool_sha256,
        slots_sha256,
    )?;
    let expected_identity = ExpectedCorpus {
        teacher_sha256,
        pool_sha256,
        slots_sha256,
    };
    let _ = manifest(&staging, &expected_identity)?;
    std::fs::rename(&staging, directory).map_err(io(format!(
        "publishing corpus generation {}",
        directory.display()
    )))?;
    directory.clone_into(&mut corpus.directory);
    Ok(corpus)
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

/// Stream one cluster's shard, handing each decision to `visit` and dropping it again.
///
/// # Why streaming is the default and `read_shard` is not
///
/// A decision carries every option's feature vector as `(String, f64)` pairs — around 529 of them.
/// Materialising the training shard's 803,449 decisions at once needs roughly 27 GB, almost all of
/// it feature *names* that the caller converts to column indices and throws away immediately.
/// Streaming keeps one decision alive at a time, so peak memory is whatever the caller retains.
///
/// The checksum is verified over the whole shard before any record is handed out, so a caller
/// cannot act on the first half of a corrupt file.
///
/// # Errors
/// As [`read_shard`].
pub fn stream_shard(
    directory: &Path,
    cluster: Cluster,
    expected: &ExpectedCorpus<'_>,
    mut visit: impl FnMut(Decision),
) -> Result<usize, CorpusError> {
    let (text, declared_count) = verified_text(directory, cluster, expected)?;
    let name = format!("{}.jsonl.zst", cluster.as_str());
    let mut seen = 0usize;
    let mut previous: Option<(u64, usize, String, usize)> = None;
    for (line, record) in text.lines().enumerate() {
        let decision: Decision = serde_json::from_str(record)
            .map_err(|error| CorpusError::Invalid(format!("{name} line {line}: {error}")))?;
        check_record(&name, line, &decision, cluster)?;
        let key = (
            decision.seed,
            decision.rotation,
            decision.seat.clone(),
            decision.order,
        );
        if previous.as_ref().is_some_and(|prior| &key <= prior) {
            return Err(CorpusError::Invalid(format!(
                "{name} line {line}: record key {key:?} is not strictly after {previous:?}"
            )));
        }
        previous = Some(key);
        seen += 1;
        visit(decision);
    }
    if seen != declared_count {
        return Err(CorpusError::Invalid(format!(
            "{name} contains {seen} decisions, manifest declares {declared_count}"
        )));
    }
    Ok(seen)
}

/// The shard's decompressed text, after its checksum agrees with the manifest.
fn verified_text(
    directory: &Path,
    cluster: Cluster,
    expected: &ExpectedCorpus<'_>,
) -> Result<(String, usize), CorpusError> {
    let manifest = manifest(directory, expected)?;
    let name = format!("{}.jsonl.zst", cluster.as_str());
    let declared = manifest
        .shards
        .get(&name)
        .ok_or_else(|| CorpusError::Invalid(format!("the manifest has no checksum for {name}")))?;
    let declared_count = match cluster {
        Cluster::Train => manifest.train_decisions,
        Cluster::Validation => manifest.validation_decisions,
    };

    let bytes = std::fs::read(directory.join(&name)).map_err(io(format!("reading {name}")))?;
    let found = sha256(&bytes);
    if &found != declared {
        return Err(CorpusError::Invalid(format!(
            "{name} hashes {found}, the manifest says {declared}"
        )));
    }
    let plain = zstd::decode_all(bytes.as_slice()).map_err(io(format!("decompressing {name}")))?;
    let text = String::from_utf8(plain)
        .map_err(|error| CorpusError::Invalid(format!("{name} is not UTF-8: {error}")))?;
    Ok((text, declared_count))
}

/// Everything a record must satisfy to be usable, checked on the way in.
///
/// Shared by both readers so a streamed corpus and a materialised one cannot disagree about what
/// counts as valid.
fn check_record(
    name: &str,
    line: usize,
    decision: &Decision,
    cluster: Cluster,
) -> Result<(), CorpusError> {
    // The positional agreement a consumer relies on, checked here rather than discovered as a
    // wrong gradient later.
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
    // Every actor feature must be one the MLP projection admits.
    //
    // This is the check that would have caught the first capture, which stored
    // `TrajectoryStep::legal` — the raw schema-4 features the *linear* teacher scores with — instead
    // of the projected vectors the MLP consumes. Nothing else noticed: the records were well formed,
    // the lengths agreed, the checksums matched, and distillation produced a falling KL. What it was
    // actually training on carried 131,353 `prompt-option` and 40,339 `state-option` features per
    // 2,000 decisions — the unbounded memorisation crosses M09-024b1 excluded by design — and none
    // of the `seat-state` facts the projection adds.
    //
    // A feature set that differs from the one the model sees at inference is not a subtler kind of
    // corpus; it trains a different model.
    for (option, vector) in decision.actor.iter().enumerate() {
        for (feature, _) in vector {
            if !ti4_policy::projection::admits(feature) {
                return Err(CorpusError::Invalid(format!(
                    "{name} line {line}: option {option} carries {feature}, which the MLP \
                     projection suppresses — this corpus stores unprojected features"
                )));
            }
        }
    }
    if decision.rotation >= FIXED_FACTIONS.len()
        || !FIXED_FACTIONS.contains(&decision.faction.as_str())
        || !ti4_policy::learned::STAGE1_DECISION_HEADS.contains(&decision.head.as_str())
    {
        return Err(CorpusError::Invalid(format!(
            "{name} line {line}: invalid rotation/faction/head"
        )));
    }
    let unique_options: BTreeSet<&str> = decision.options.iter().map(String::as_str).collect();
    if unique_options.len() != decision.options.len()
        || unique_options.iter().any(|option| option.is_empty())
    {
        return Err(CorpusError::Invalid(format!(
            "{name} line {line}: option ids are empty or duplicated"
        )));
    }
    if !decision.value_target.is_finite()
        || decision
            .actor
            .iter()
            .flatten()
            .chain(&decision.critic)
            .any(|(feature, value)| feature.is_empty() || !value.is_finite())
        || decision
            .teacher
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(CorpusError::Invalid(format!(
            "{name} line {line}: a target, feature, or probability is non-finite/invalid"
        )));
    }
    let probability_sum: f64 = decision.teacher.iter().sum();
    if (probability_sum - 1.0).abs() > 1e-6 {
        return Err(CorpusError::Invalid(format!(
            "{name} line {line}: teacher probabilities sum to {probability_sum}, not 1"
        )));
    }
    if decision
        .critic
        .iter()
        .any(|(feature, _)| !feature.starts_with("critic-state:"))
    {
        return Err(CorpusError::Invalid(format!(
            "{name} line {line}: critic vector contains a non-critic feature"
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
    Ok(())
}

/// Read one cluster's shard back in full, verifying its checksum first.
///
/// Convenience over [`stream_shard`] for tests and small corpora. A production consumer should
/// stream: see that function for what materialising the training shard costs.
///
/// # Errors
/// [`CorpusError::Invalid`] if the manifest is missing, the checksum disagrees, a record does not
/// parse, or a record's option/vector/probability lengths do not agree.
pub fn read_shard(
    directory: &Path,
    cluster: Cluster,
    expected: &ExpectedCorpus<'_>,
) -> Result<Vec<Decision>, CorpusError> {
    let mut decisions = Vec::new();
    stream_shard(directory, cluster, expected, |decision| {
        decisions.push(decision);
    })?;
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

    fn expected() -> ExpectedCorpus<'static> {
        ExpectedCorpus {
            teacher_sha256: "teacher",
            pool_sha256: "pool",
            slots_sha256: "slots",
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
        let other = match cluster {
            Cluster::Train => (Cluster::Validation, decision(202_608_306, 0)),
            Cluster::Validation => (Cluster::Train, decision(202_608_210, 0)),
        };
        shards.write(other.0, &other.1).expect("writes other shard");
        *counts.entry(other.0).or_default() += 1;
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

        let read = read_shard(&scratch.0, Cluster::Train, &expected()).expect("reads");
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

        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("must refuse");
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
        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("must refuse");
        assert!(
            error.to_string().contains("not in the train cluster"),
            "{error}"
        );
    }

    #[test]
    fn an_unprojected_actor_vector_is_refused() {
        // The regression for the defect the first capture shipped: it stored
        // `TrajectoryStep::legal`, the raw schema-4 features the linear teacher scores with, rather
        // than the projected vectors the MLP consumes. The record is otherwise perfect — lengths
        // agree, seeds are in cluster, nothing is forced — so only this check catches it.
        let scratch = Scratch::new("unprojected");
        let mut raw = decision(202_608_210, 0);
        // `state-option` is `FamilyRole::UnboundedCross`: the option-identity cross the projection
        // suppresses before lookup, and precisely what leaked into the first corpus.
        raw.actor[0].push((
            "state-option:pok1leadership:faction-start-tech:nm".to_owned(),
            1.0,
        ));
        let _ = written(&scratch, vec![raw], Cluster::Train);

        let error = read_shard(&scratch.0, Cluster::Train, &expected())
            .expect_err("an unprojected feature must be refused");
        assert!(
            error.to_string().contains("suppresses"),
            "wrong refusal: {error}"
        );
    }

    #[test]
    fn a_projected_actor_vector_is_accepted() {
        // Non-vacuity for the check above: the same record without the suppressed feature must pass,
        // or the refusal could be coming from something else entirely.
        let scratch = Scratch::new("projected");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        let read =
            read_shard(&scratch.0, Cluster::Train, &expected()).expect("a projected record reads");
        assert_eq!(read.len(), 1);
        for (feature, _) in &read[0].actor[0] {
            assert!(
                ti4_policy::projection::admits(feature),
                "{feature} is not admitted"
            );
        }
    }

    #[test]
    fn a_forced_decision_never_survives_a_read() {
        let scratch = Scratch::new("forced");
        let mut forced = decision(202_608_210, 0);
        forced.options = vec!["only".to_owned()];
        forced.actor = vec![vec![("option:only".to_owned(), 1.0)]];
        forced.teacher = vec![1.0];
        let _ = written(&scratch, vec![forced], Cluster::Train);

        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("must refuse");
        assert!(error.to_string().contains("forced"), "{error}");
    }

    #[test]
    fn a_record_whose_lengths_disagree_is_refused() {
        let scratch = Scratch::new("ragged");
        let mut ragged = decision(202_608_210, 0);
        ragged.teacher = vec![1.0];
        let _ = written(&scratch, vec![ragged], Cluster::Train);

        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("must refuse");
        assert!(error.to_string().contains("probabilities"), "{error}");
    }

    #[test]
    fn a_corpus_without_a_manifest_is_incomplete() {
        let scratch = Scratch::new("nomanifest");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        std::fs::remove_file(scratch.0.join("manifest.json")).expect("remove");
        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("must refuse");
        assert!(error.to_string().contains("incomplete"), "{error}");
    }

    #[test]
    fn an_existing_generation_is_refused_before_any_payload_is_touched() {
        let scratch = Scratch::new("immutable");
        let destination = scratch.0.join("accepted");
        std::fs::create_dir(&destination).expect("destination");
        let marker = destination.join("train.jsonl.zst");
        std::fs::write(&marker, b"accepted bytes").expect("marker");

        let error = staging_directory(&destination).expect_err("must refuse existing destination");
        assert!(error.to_string().contains("immutable"), "{error}");
        assert_eq!(
            std::fs::read(marker).expect("marker remains"),
            b"accepted bytes"
        );
    }

    #[test]
    fn a_wrong_input_identity_and_an_extra_file_are_refused() {
        let scratch = Scratch::new("identity");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        let wrong = ExpectedCorpus {
            teacher_sha256: "some-other-teacher",
            ..expected()
        };
        let error = validate_manifest(&scratch.0, &wrong).expect_err("identity must be exact");
        assert!(error.to_string().contains("identities"), "{error}");

        std::fs::write(scratch.0.join("partial.tmp"), b"partial").expect("extra file");
        let error = validate_manifest(&scratch.0, &expected()).expect_err("closed set");
        assert!(error.to_string().contains("not closed"), "{error}");
    }

    #[test]
    fn a_manifest_count_that_does_not_match_the_shard_is_refused() {
        let scratch = Scratch::new("count");
        let _ = written(&scratch, vec![decision(202_608_210, 0)], Cluster::Train);
        let path = scratch.0.join(MANIFEST_FILE);
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("manifest")).expect("json");
        value["train_decisions"] = serde_json::json!(2);
        std::fs::write(&path, serde_json::to_vec_pretty(&value).expect("json")).expect("write");

        let error =
            read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("count mismatch");
        assert!(error.to_string().contains("manifest declares 2"), "{error}");
    }

    #[test]
    fn out_of_order_records_are_refused_instead_of_sorted_by_the_test_fixture() {
        let scratch = Scratch::new("order-refusal");
        let mut shards = Shards::open(&scratch.0);
        let later = decision(202_608_211, 0);
        let earlier = decision(202_608_210, 0);
        shards.write(Cluster::Train, &later).expect("later");
        shards.write(Cluster::Train, &earlier).expect("earlier");
        shards
            .write(Cluster::Validation, &decision(202_608_306, 0))
            .expect("validation");
        let digests = shards.finish(&scratch.0).expect("finish");
        let counts = BTreeMap::from([(Cluster::Train, 2), (Cluster::Validation, 1)]);
        write_manifest(
            &scratch.0, &counts, &digests, 768, 0, "teacher", "pool", "slots",
        )
        .expect("manifest");

        let error = read_shard(&scratch.0, Cluster::Train, &expected()).expect_err("order");
        assert!(error.to_string().contains("not strictly after"), "{error}");
    }
}
