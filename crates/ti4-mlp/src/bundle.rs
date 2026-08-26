//! The schema-6 inference bundle (M09-028), per MLP plan §§4.4–4.6.
//!
//! # What a bundle is
//!
//! A directory, not a file, so tensors are not base64 inside JSON:
//!
//! ```text
//! checkpoint-<update>/
//!   trunk.safetensors      W1 b1 W2 b2
//!   readout.safetensors    shared heads + per-faction residuals and biases
//!   value.safetensors      omitted for batch_mean
//!   embedding.safetensors  the 33x16 identity table
//!   slots.json             feature name -> column, ordered
//!   manifest.json          written last: schema, shapes, provenance, checksums
//! ```
//!
//! `safetensors` because it is flat, checksummable, zero-copy, and executes no code on load.
//!
//! # The manifest is written last, and that is the recovery rule
//!
//! §4.6: a directory without a readable `manifest.json` is **incomplete by construction**. Nothing
//! else needs to be inspected to know it, and nothing needs to be cleaned up for it to be ignored.
//! The write order is: staging sibling directory -> every tensor file, each fsynced -> `slots.json`
//! -> `manifest.json` fsynced last -> one directory rename. A crash at any point leaves either the
//! previous state or a `.tmp` sibling that no reader will accept.
//!
//! This is the same protocol M09-024b2 arrived at for vocabulary generations, for the same reason:
//! two renames are not crash-recoverable, and an in-memory rollback does not run when the process
//! dies.
//!
//! # Everything is validated before a live model exists
//!
//! §4.4's load bounds are not advisory. Byte lengths are checked against manifest shapes *before*
//! allocation, checksums before parsing, the slot digest before the weights are believed to mean
//! anything. `slots_sha256` is called out in the plan as "the single most likely silent
//! corruption — same weights, different feature meaning", and it is the one check that cannot be
//! inferred from the tensors themselves.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::Digest;
use ti4_tensor::Tensor;

use crate::{Actor, EMBED_DIM, FACTION_ROSTER, SeparateCritic, Width, heads};

/// The schema this module reads and writes. Distinct from the linear schemas 2–5 so a wrong loader
/// fails loudly rather than misreading a field it recognises.
pub const SCHEMA: u32 = 6;

/// §4.4's total size bound for a bundle directory.
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// §4.4's bound on `manifest.json`.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// §4.4's bound on `slots.json`.
const MAX_SLOTS_BYTES: u64 = 32 * 1024 * 1024;

/// The file names a schema-6 inference bundle may contain, and no others.
///
/// A closed list rather than a filter: an unrecognised name is a hard error, so a bundle cannot
/// carry an extra file that some other reader would honour.
const INFERENCE_FILES: [&str; 5] = [
    "trunk.safetensors",
    "readout.safetensors",
    "value.safetensors",
    "embedding.safetensors",
    "slots.json",
];

/// Which value tensors a bundle carries, per §4.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticMode {
    /// The value head shares the actor's trunk. `value.safetensors` holds the readout.
    Shared,
    /// A separate critic trunk. Not produced by M09-028; reserved for M10-033.
    Separate,
    /// No value tensors at all — the fallback §6.2 chooses before PPO.
    BatchMean,
}

impl CriticMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Separate => "separate",
            Self::BatchMean => "batch_mean",
        }
    }

    fn parse(text: &str) -> Result<Self, BundleError> {
        match text {
            "shared" => Ok(Self::Shared),
            "separate" => Ok(Self::Separate),
            "batch_mean" => Ok(Self::BatchMean),
            other => Err(BundleError::Invalid(format!("unknown critic_mode {other}"))),
        }
    }

    /// Whether `value.safetensors` must be present.
    const fn needs_value_file(self) -> bool {
        !matches!(self, Self::BatchMean)
    }
}

/// Provenance the manifest carries so any number quoted from a bundle can be traced.
#[derive(Debug, Clone)]
pub struct Provenance {
    /// What produced the bundle.
    pub source: String,
    /// The commit it was produced at.
    pub git_commit: String,
    /// The training update number, or 0 for an untrained bundle.
    pub update: u64,
}

/// Anything that stopped a bundle being written or read.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// The bundle is structurally wrong. Every load failure is this, before any state is built.
    #[error("invalid bundle: {0}")]
    Invalid(String),
    /// The filesystem refused.
    #[error("{context}: {source}")]
    Io {
        /// What was being attempted.
        context: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A tensor file did not load.
    #[error("tensor {file}: {reason}")]
    Tensor {
        /// Which file.
        file: String,
        /// Why.
        reason: String,
    },
}

fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> BundleError {
    let context = context.into();
    |source| BundleError::Io { context, source }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), BundleError> {
    let mut file =
        std::fs::File::create(path).map_err(io(format!("creating {}", path.display())))?;
    file.write_all(bytes)
        .map_err(io(format!("writing {}", path.display())))?;
    file.sync_all()
        .map_err(io(format!("syncing {}", path.display())))?;
    Ok(())
}

/// What a written bundle is, and where.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The bundle directory.
    pub directory: PathBuf,
    /// `manifest.json`'s digest, which identifies the bundle.
    pub manifest_sha256: String,
}

/// Write one inference bundle, manifest last.
///
/// `slots_text` is the accepted vocabulary's `slots.json`, verbatim — the same bytes whose digest
/// the manifest records, so the two cannot drift.
///
/// # Errors
/// [`BundleError`] if the destination exists, a write fails, or the actor's shapes do not match the
/// roster and head list the manifest would claim.
pub fn write(
    destination: &Path,
    actor: &Actor,
    slots_text: &str,
    critic_mode: CriticMode,
    provenance: &Provenance,
) -> Result<Bundle, BundleError> {
    // §4.6: "require the destination not to exist". A bundle is immutable; replacing one in place
    // is how a resumed run silently changes the checkpoint it claims to extend.
    if destination.exists() {
        return Err(BundleError::Invalid(format!(
            "{} already exists; bundles are never written in place",
            destination.display()
        )));
    }
    let parent = destination.parent().ok_or_else(|| {
        BundleError::Invalid(format!("{} has no parent directory", destination.display()))
    })?;
    std::fs::create_dir_all(parent).map_err(io("creating the bundle parent"))?;

    // The staging directory is a sibling, so the rename is within one filesystem.
    let staging = staging_for(destination)?;
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(io("creating the staging directory"))?;

    let mut digests: BTreeMap<String, String> = BTreeMap::new();

    write_tensors(
        &staging,
        "trunk.safetensors",
        &[
            ("W1", actor.input()),
            ("b1", actor.b1()),
            ("W2", actor.hidden()),
            ("b2", actor.b2()),
        ],
        &mut digests,
    )?;
    write_tensors(
        &staging,
        "readout.safetensors",
        &[
            ("w_shared", actor.shared_readout()),
            ("b_shared", actor.b_shared()),
            ("delta", actor.delta()),
            ("b_delta", actor.b_delta()),
        ],
        &mut digests,
    )?;
    write_tensors(
        &staging,
        "embedding.safetensors",
        &[("embedding", actor.embedding())],
        &mut digests,
    )?;
    match critic_mode {
        CriticMode::Shared => write_tensors(
            &staging,
            "value.safetensors",
            &[
                ("w_value", actor.value_readout()),
                ("b_value", actor.b_value()),
            ],
            &mut digests,
        )?,
        CriticMode::Separate => {
            let critic = actor.separate_critic().ok_or_else(|| {
                BundleError::Invalid("separate mode has no separate critic tensors".to_owned())
            })?;
            write_tensors(
                &staging,
                "value.safetensors",
                &critic.tensors(),
                &mut digests,
            )?;
        }
        CriticMode::BatchMean => {}
    }

    let slots_path = staging.join("slots.json");
    write_synced(&slots_path, slots_text.as_bytes())?;
    digests.insert("slots.json".to_owned(), sha256(slots_text.as_bytes()));

    let slot_count = slot_count_of(slots_text)?;
    let manifest = manifest_document(
        actor,
        slots_text,
        slot_count,
        critic_mode,
        provenance,
        &digests,
    )?;
    let manifest_path = staging.join("manifest.json");
    write_synced(&manifest_path, manifest.as_bytes())?;

    // The commit. One rename of a directory whose manifest is already durable.
    std::fs::rename(&staging, destination).map_err(|error| {
        let _ = std::fs::remove_dir_all(&staging);
        BundleError::Io {
            context: format!("committing {}", destination.display()),
            source: error,
        }
    })?;

    Ok(Bundle {
        directory: destination.to_owned(),
        manifest_sha256: sha256(manifest.as_bytes()),
    })
}

/// The staging sibling for a destination, validated to stay inside the parent.
fn staging_for(destination: &Path) -> Result<PathBuf, BundleError> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            BundleError::Invalid(format!("{} has no file name", destination.display()))
        })?;
    let parent = destination
        .parent()
        .ok_or_else(|| BundleError::Invalid("no parent".to_owned()))?;
    Ok(parent.join(format!("{name}.tmp")))
}

fn write_tensors(
    directory: &Path,
    file: &str,
    named: &[(&str, &Tensor)],
    digests: &mut BTreeMap<String, String>,
) -> Result<(), BundleError> {
    let path = directory.join(file);
    // Always written from CPU (§4.4's device behaviour): a bundle produced on CUDA must load on a
    // CPU-only machine.
    let owned: Vec<(String, Tensor)> = named
        .iter()
        .map(|(name, tensor)| {
            (
                (*name).to_owned(),
                tensor.to_device(ti4_tensor::Device::Cpu),
            )
        })
        .collect();
    Tensor::write_safetensors(&owned, &path).map_err(|error| BundleError::Tensor {
        file: file.to_owned(),
        reason: error.to_string(),
    })?;
    let bytes = std::fs::read(&path).map_err(io(format!("re-reading {file}")))?;
    // Re-read rather than hashing what was intended to be written.
    digests.insert(file.to_owned(), sha256(&bytes));
    // Opened for **write**: `FlushFileBuffers` needs `GENERIC_WRITE`, so syncing through a
    // read-only handle fails with access denied on Windows. `write(true)` without `truncate`
    // reopens the existing bytes rather than replacing them.
    let handle = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(io(format!("opening {file} to sync")))?;
    handle.sync_all().map_err(io(format!("syncing {file}")))?;
    Ok(())
}

fn slot_count_of(slots_text: &str) -> Result<usize, BundleError> {
    let vocabulary = ti4_policy::vocabulary::Vocabulary::from_json(slots_text)
        .map_err(|error| BundleError::Invalid(format!("slots.json does not load: {error}")))?;
    Ok(vocabulary.slot_count())
}

fn manifest_document(
    actor: &Actor,
    slots_text: &str,
    slot_count: usize,
    critic_mode: CriticMode,
    provenance: &Provenance,
    digests: &BTreeMap<String, String>,
) -> Result<String, BundleError> {
    let backend = ti4_tensor::backend();
    validate_provenance(provenance)?;
    let document = serde_json::json!({
        "schema": SCHEMA,
        "dtype": "f32",
        "trunk": { "width": actor.width(), "depth": 2, "activation": "relu" },
        "embed_dim": EMBED_DIM,
        "factions": FACTION_ROSTER.as_slice(),
        "heads": heads(),
        "slot_count": slot_count,
        "slot_capacity": actor.capacity(),
        "slots_sha256": sha256(slots_text.as_bytes()),
        "student_temperature": 1.0,
        "critic_mode": critic_mode.as_str(),
        "tch": ti4_tensor::TCH_VERSION,
        "libtorch": ti4_tensor::LIBTORCH_VERSION,
        "compiler": env!("TI4_RUSTC_VERSION"),
        "threads": {
            "intraop": backend.intra_op_threads,
            "interop": backend.inter_op_threads,
        },
        "source": provenance.source,
        "git_commit": provenance.git_commit,
        "update": provenance.update,
        "checksums": digests,
    });
    serde_json::to_string_pretty(&document)
        .map(|text| format!("{text}\n"))
        .map_err(|error| BundleError::Invalid(format!("manifest cannot be encoded: {error}")))
}

fn validate_provenance(provenance: &Provenance) -> Result<(), BundleError> {
    if provenance.source.trim().is_empty() {
        return Err(BundleError::Invalid(
            "provenance source is empty".to_owned(),
        ));
    }
    let commit = provenance.git_commit.as_bytes();
    if !(7..=64).contains(&commit.len()) || !commit.iter().all(u8::is_ascii_hexdigit) {
        return Err(BundleError::Invalid(
            "git_commit must be a recorded 7-64 digit hexadecimal commit".to_owned(),
        ));
    }
    Ok(())
}

/// A validated schema-6 bundle, and the model it describes.
#[derive(Debug)]
pub struct Loaded {
    /// The actor, with every tensor placed and verified.
    pub actor: Actor,
    /// The vocabulary the weights are addressed by.
    pub vocabulary: ti4_policy::vocabulary::Vocabulary,
    /// The mode the bundle was written in.
    pub critic_mode: CriticMode,
    /// The training update the bundle came from.
    pub update: u64,
}

/// Read and fully validate one bundle.
///
/// Every §4.4 bound is checked before a live model exists: the directory holds only recognised
/// names, sizes are within their caps, checksums match, shapes equal the manifest's, and the head
/// and faction lists are exactly the ones this build uses.
///
/// # Errors
/// [`BundleError::Invalid`] for any structural failure — and it is always raised before the actor
/// is constructed, so a rejected bundle never half-loads.
pub fn read(directory: &Path) -> Result<Loaded, BundleError> {
    let manifest_path = directory.join("manifest.json");
    // The recovery rule, stated as the first thing that happens: the manifest is written last, so a
    // directory without a readable one is incomplete and is not a candidate at all.
    if !manifest_path.is_file() {
        return Err(BundleError::Invalid(format!(
            "{} has no manifest.json, so it is incomplete by construction",
            directory.display()
        )));
    }
    let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| BundleError::Invalid(format!("manifest.json is not JSON: {error}")))?;
    validate_manifest_keys(&manifest)?;

    let schema = manifest
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BundleError::Invalid("manifest has no schema".to_owned()))?;
    if schema != u64::from(SCHEMA) {
        return Err(BundleError::Invalid(format!(
            "schema {schema} is not {SCHEMA}"
        )));
    }
    let dtype = string_field(&manifest, "dtype")?;
    if dtype != "f32" {
        return Err(BundleError::Invalid(format!("dtype {dtype} is not f32")));
    }
    let critic_mode = CriticMode::parse(&string_field(&manifest, "critic_mode")?)?;
    validate_fixed_manifest_fields(&manifest)?;

    inspect_directory(directory, critic_mode)?;

    // Heads and factions are positional: a reordered roster silently mislabels every faction, and
    // no checksum would notice, because the tensors are unchanged.
    expect_list(&manifest, "heads", heads())?;
    expect_list(&manifest, "factions", &FACTION_ROSTER)?;

    let checksums = manifest
        .get("checksums")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| BundleError::Invalid("manifest has no checksums".to_owned()))?;
    validate_checksum_inventory(checksums, critic_mode)?;

    // slots.json first: it is what makes the weights mean anything.
    let slots_bytes = read_bounded(&directory.join("slots.json"), MAX_SLOTS_BYTES)?;
    verify_digest("slots.json", &slots_bytes, checksums)?;
    let declared_slots = string_field(&manifest, "slots_sha256")?;
    if declared_slots != sha256(&slots_bytes) {
        return Err(BundleError::Invalid(
            "slots_sha256 does not match slots.json; the weights would mean something else"
                .to_owned(),
        ));
    }
    let slots_text = String::from_utf8(slots_bytes)
        .map_err(|error| BundleError::Invalid(format!("slots.json is not UTF-8: {error}")))?;
    let vocabulary = ti4_policy::vocabulary::Vocabulary::from_json(&slots_text)
        .map_err(|error| BundleError::Invalid(format!("slots.json does not load: {error}")))?;
    let slot_count = u64_field(&manifest, "slot_count")?;
    if slot_count != vocabulary.slot_count() as u64 {
        return Err(BundleError::Invalid(format!(
            "manifest says {slot_count} slots, slots.json has {}",
            vocabulary.slot_count()
        )));
    }

    let width = i64::try_from(
        manifest
            .get("trunk")
            .and_then(|trunk| trunk.get("width"))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| BundleError::Invalid("manifest has no trunk.width".to_owned()))?,
    )
    .map_err(|_| BundleError::Invalid("trunk.width does not fit".to_owned()))?;
    let width = Width::of(width)
        .ok_or_else(|| BundleError::Invalid(format!("width {width} is not 256 or 128")))?;
    let capacity = i64::try_from(u64_field(&manifest, "slot_capacity")?)
        .map_err(|_| BundleError::Invalid("slot_capacity does not fit".to_owned()))?;
    if capacity != i64::try_from(vocabulary.capacity()).unwrap_or(i64::MAX) {
        return Err(BundleError::Invalid(format!(
            "manifest says capacity {capacity}, slots.json says {}",
            vocabulary.capacity()
        )));
    }
    if slot_count > u64::try_from(capacity).unwrap_or(0) || capacity > 65_536 {
        return Err(BundleError::Invalid(format!(
            "slot bounds require slot_count <= slot_capacity <= 65536, got {slot_count} and {capacity}"
        )));
    }

    let named = load_tensors(directory, critic_mode, checksums)?;

    check_shapes(&named, width, capacity, critic_mode)?;

    // Only now does a live model exist.
    let mut actor = Actor::zeros(width, capacity);
    install(&mut actor, &named, critic_mode);

    Ok(Loaded {
        actor,
        vocabulary,
        critic_mode,
        update: u64_field(&manifest, "update")?,
    })
}

fn validate_manifest_keys(manifest: &serde_json::Value) -> Result<(), BundleError> {
    const KEYS: [&str; 18] = [
        "schema",
        "dtype",
        "trunk",
        "embed_dim",
        "factions",
        "heads",
        "slot_count",
        "slot_capacity",
        "slots_sha256",
        "student_temperature",
        "critic_mode",
        "tch",
        "libtorch",
        "compiler",
        "threads",
        "source",
        "git_commit",
        "update",
    ];
    let object = manifest
        .as_object()
        .ok_or_else(|| BundleError::Invalid("manifest is not an object".to_owned()))?;
    let expected: BTreeSet<&str> = KEYS.into_iter().chain(["checksums"]).collect();
    let found: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    if found != expected {
        return Err(BundleError::Invalid(format!(
            "manifest fields do not match schema 6: {found:?}"
        )));
    }
    Ok(())
}

fn validate_fixed_manifest_fields(manifest: &serde_json::Value) -> Result<(), BundleError> {
    let trunk = manifest
        .get("trunk")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| BundleError::Invalid("manifest has no trunk object".to_owned()))?;
    if trunk.len() != 3
        || trunk.get("depth").and_then(serde_json::Value::as_u64) != Some(2)
        || trunk.get("activation").and_then(serde_json::Value::as_str) != Some("relu")
    {
        return Err(BundleError::Invalid(
            "trunk must be exactly {width, depth: 2, activation: relu}".to_owned(),
        ));
    }
    if u64_field(manifest, "embed_dim")? != u64::try_from(EMBED_DIM).unwrap_or(u64::MAX) {
        return Err(BundleError::Invalid(
            "embed_dim does not match this build".to_owned(),
        ));
    }
    if manifest
        .get("student_temperature")
        .and_then(serde_json::Value::as_f64)
        != Some(1.0)
    {
        return Err(BundleError::Invalid(
            "student_temperature must be 1.0".to_owned(),
        ));
    }
    for (field, expected) in [
        ("tch", ti4_tensor::TCH_VERSION),
        ("libtorch", ti4_tensor::LIBTORCH_VERSION),
        ("compiler", env!("TI4_RUSTC_VERSION")),
    ] {
        if string_field(manifest, field)? != expected {
            return Err(BundleError::Invalid(format!(
                "{field} does not match this build"
            )));
        }
    }
    let backend = ti4_tensor::backend();
    let threads = manifest
        .get("threads")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| BundleError::Invalid("manifest has no threads object".to_owned()))?;
    if threads.len() != 2
        || threads.get("intraop").and_then(serde_json::Value::as_i64)
            != Some(i64::from(backend.intra_op_threads))
        || threads.get("interop").and_then(serde_json::Value::as_i64)
            != Some(i64::from(backend.inter_op_threads))
    {
        return Err(BundleError::Invalid(
            "thread settings do not match the deterministic backend".to_owned(),
        ));
    }
    let provenance = Provenance {
        source: string_field(manifest, "source")?,
        git_commit: string_field(manifest, "git_commit")?,
        update: u64_field(manifest, "update")?,
    };
    validate_provenance(&provenance)
}

fn validate_checksum_inventory(
    checksums: &serde_json::Map<String, serde_json::Value>,
    critic_mode: CriticMode,
) -> Result<(), BundleError> {
    let mut expected: BTreeSet<&str> = [
        "trunk.safetensors",
        "readout.safetensors",
        "embedding.safetensors",
        "slots.json",
    ]
    .into_iter()
    .collect();
    if critic_mode.needs_value_file() {
        expected.insert("value.safetensors");
    }
    let found: BTreeSet<&str> = checksums.keys().map(String::as_str).collect();
    if found != expected
        || checksums.values().any(|value| {
            value.as_str().is_none_or(|digest| {
                digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit)
            })
        })
    {
        return Err(BundleError::Invalid(
            "checksum inventory does not exactly match the bundle files".to_owned(),
        ));
    }
    Ok(())
}

/// Checksum, then load, then collect — in that order, so a corrupt file is rejected before it is
/// parsed at all.
fn load_tensors(
    directory: &Path,
    critic_mode: CriticMode,
    checksums: &serde_json::Map<String, serde_json::Value>,
) -> Result<BTreeMap<String, Tensor>, BundleError> {
    let mut named: BTreeMap<String, Tensor> = BTreeMap::new();
    let mut files = vec![
        "trunk.safetensors",
        "readout.safetensors",
        "embedding.safetensors",
    ];
    if critic_mode.needs_value_file() {
        files.push("value.safetensors");
    }
    for file in files {
        let path = directory.join(file);
        let bytes = read_bounded(&path, MAX_TOTAL_BYTES)?;
        verify_digest(file, &bytes, checksums)?;
        let tensors = Tensor::read_safetensors(&path).map_err(|error| BundleError::Tensor {
            file: file.to_owned(),
            reason: error.to_string(),
        })?;
        for (name, tensor) in tensors {
            if named.insert(name.clone(), tensor).is_some() {
                return Err(BundleError::Invalid(format!(
                    "tensor {name} appears in more than one file"
                )));
            }
        }
    }
    Ok(named)
}

/// Every tensor's shape and dtype against what the manifest implies, before anything is installed.
fn check_shapes(
    named: &BTreeMap<String, Tensor>,
    width: Width,
    capacity: i64,
    critic_mode: CriticMode,
) -> Result<(), BundleError> {
    let heads_count = i64::try_from(heads().len()).unwrap_or(0);
    let factions = i64::try_from(FACTION_ROSTER.len()).unwrap_or(0);
    let w = width.units();
    let expected: Vec<(&str, Vec<i64>)> = {
        let mut expected = vec![
            ("W1", vec![capacity, w]),
            ("b1", vec![w]),
            ("W2", vec![w, w]),
            ("b2", vec![w]),
            ("w_shared", vec![heads_count, w]),
            ("b_shared", vec![heads_count]),
            ("delta", vec![factions, heads_count, w]),
            ("b_delta", vec![factions, heads_count]),
            ("embedding", vec![factions, EMBED_DIM]),
        ];
        match critic_mode {
            CriticMode::Shared => {
                expected.push(("w_value", vec![w]));
                expected.push(("b_value", vec![1]));
            }
            CriticMode::Separate => {
                expected.push(("critic_W1", vec![capacity, 128]));
                expected.push(("critic_b1", vec![128]));
                expected.push(("critic_W2", vec![128, 128]));
                expected.push(("critic_b2", vec![128]));
                expected.push(("critic_readout", vec![128]));
                expected.push(("critic_bias", vec![1]));
            }
            CriticMode::BatchMean => {}
        }
        expected
    };
    for (name, shape) in &expected {
        let tensor = named.get(*name).ok_or_else(|| {
            BundleError::Invalid(format!("the bundle has no tensor named {name}"))
        })?;
        if tensor.size() != *shape {
            return Err(BundleError::Invalid(format!(
                "tensor {name} has shape {:?}, the manifest implies {shape:?}",
                tensor.size()
            )));
        }
        if tensor.kind() != ti4_tensor::Kind::Float {
            return Err(BundleError::Invalid(format!("tensor {name} is not f32")));
        }
    }
    let unexpected: Vec<&String> = named
        .keys()
        .filter(|name| !expected.iter().any(|(known, _)| *known == name.as_str()))
        .collect();
    if let Some(name) = unexpected.first() {
        return Err(BundleError::Invalid(format!(
            "the bundle carries an unrecognised tensor {name}"
        )));
    }

    Ok(())
}

/// Move the verified tensors into a fresh actor.
fn install(actor: &mut Actor, named: &BTreeMap<String, Tensor>, critic_mode: CriticMode) {
    *actor.input_mut() = named["W1"].shallow_clone();
    *actor.b1_mut() = named["b1"].shallow_clone();
    *actor.hidden_mut() = named["W2"].shallow_clone();
    *actor.b2_mut() = named["b2"].shallow_clone();
    *actor.shared_readout_mut() = named["w_shared"].shallow_clone();
    *actor.b_shared_mut() = named["b_shared"].shallow_clone();
    *actor.delta_mut() = named["delta"].shallow_clone();
    *actor.b_delta_mut() = named["b_delta"].shallow_clone();
    *actor.embedding_mut() = named["embedding"].shallow_clone();
    match critic_mode {
        CriticMode::Shared => {
            *actor.value_readout_mut() = named["w_value"].shallow_clone();
            *actor.b_value_mut() = named["b_value"].shallow_clone();
        }
        CriticMode::Separate => actor.set_separate_critic(Some(SeparateCritic::new(
            named["critic_W1"].shallow_clone(),
            named["critic_b1"].shallow_clone(),
            named["critic_W2"].shallow_clone(),
            named["critic_b2"].shallow_clone(),
            named["critic_readout"].shallow_clone(),
            named["critic_bias"].shallow_clone(),
        ))),
        CriticMode::BatchMean => {}
    }
}

/// The directory holds exactly the recognised names, nothing nested, nothing symlinked.
fn inspect_directory(directory: &Path, critic_mode: CriticMode) -> Result<(), BundleError> {
    let mut present: BTreeSet<String> = BTreeSet::new();
    let mut total: u64 = 0;
    let entries =
        std::fs::read_dir(directory).map_err(io(format!("listing {}", directory.display())))?;
    for entry in entries {
        let entry = entry.map_err(io("reading a directory entry"))?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(io("stat-ing a bundle entry"))?;
        // `symlink_metadata` does not follow, so a link is seen as a link and refused rather than
        // silently reading whatever it points at — including outside the bundle.
        if metadata.file_type().is_symlink() {
            return Err(BundleError::Invalid(format!(
                "{} is a symlink; bundles contain only regular files",
                entry.path().display()
            )));
        }
        if metadata.is_dir() {
            return Err(BundleError::Invalid(format!(
                "{} is a nested directory",
                entry.path().display()
            )));
        }
        total = total.saturating_add(metadata.len());
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| BundleError::Invalid("a file name is not UTF-8".to_owned()))?
            .to_owned();
        if name != "manifest.json" && !INFERENCE_FILES.contains(&name.as_str()) {
            return Err(BundleError::Invalid(format!(
                "unrecognised file {name}; schema-6 bundles are a closed set"
            )));
        }
        present.insert(name);
    }
    if total > MAX_TOTAL_BYTES {
        return Err(BundleError::Invalid(format!(
            "the bundle is {total} bytes, above the {MAX_TOTAL_BYTES} cap"
        )));
    }
    for required in [
        "trunk.safetensors",
        "readout.safetensors",
        "embedding.safetensors",
        "slots.json",
    ] {
        if !present.contains(required) {
            return Err(BundleError::Invalid(format!(
                "the bundle has no {required}"
            )));
        }
    }
    let has_value = present.contains("value.safetensors");
    if critic_mode.needs_value_file() && !has_value {
        return Err(BundleError::Invalid(format!(
            "critic_mode {} requires value.safetensors",
            critic_mode.as_str()
        )));
    }
    if !critic_mode.needs_value_file() && has_value {
        return Err(BundleError::Invalid(
            "critic_mode batch_mean carries no value tensors, but value.safetensors is present"
                .to_owned(),
        ));
    }
    Ok(())
}

fn read_bounded(path: &Path, cap: u64) -> Result<Vec<u8>, BundleError> {
    let metadata = std::fs::metadata(path).map_err(io(format!("stat-ing {}", path.display())))?;
    // Checked before the read, so an oversized file is refused rather than allocated.
    if metadata.len() > cap {
        return Err(BundleError::Invalid(format!(
            "{} is {} bytes, above its {cap} cap",
            path.display(),
            metadata.len()
        )));
    }
    std::fs::read(path).map_err(io(format!("reading {}", path.display())))
}

fn verify_digest(
    file: &str,
    bytes: &[u8],
    checksums: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), BundleError> {
    let declared = checksums
        .get(file)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Invalid(format!("the manifest has no checksum for {file}")))?;
    let found = sha256(bytes);
    if declared != found {
        return Err(BundleError::Invalid(format!(
            "{file} hashes {found}, the manifest says {declared}"
        )));
    }
    Ok(())
}

fn string_field(document: &serde_json::Value, field: &str) -> Result<String, BundleError> {
    document
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| BundleError::Invalid(format!("the manifest has no {field}")))
}

fn u64_field(document: &serde_json::Value, field: &str) -> Result<u64, BundleError> {
    document
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BundleError::Invalid(format!("the manifest has no {field}")))
}

fn expect_list(
    document: &serde_json::Value,
    field: &str,
    expected: &[&str],
) -> Result<(), BundleError> {
    let found: Vec<&str> = document
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BundleError::Invalid(format!("the manifest has no {field}")))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| BundleError::Invalid(format!("{field} contains a non-string entry")))
        })
        .collect::<Result<_, _>>()?;
    if found != expected {
        return Err(BundleError::Invalid(format!(
            "{field} does not match this build: {found:?}"
        )));
    }
    Ok(())
}

/// The highest complete bundle under `root`, per §4.6's recovery rule.
///
/// Incomplete directories — those with no readable `manifest.json` — are ignored rather than
/// repaired or deleted. Nothing is removed by this function at all: recovery never expands a glob
/// into an unverified deletion target.
///
/// # Errors
/// [`BundleError`] if the root cannot be listed. An empty root is `Ok(None)`, not an error.
pub fn latest_complete(root: &Path) -> Result<Option<PathBuf>, BundleError> {
    if !root.is_dir() {
        return Ok(None);
    }
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(root).map_err(io(format!("listing {}", root.display())))? {
        let entry = entry.map_err(io("reading a checkpoint entry"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // `checkpoint-<n>`, and never `checkpoint-<n>.tmp`: a staging sibling is by definition the
        // one thing that must not be resumed from.
        let Some(number) = name.strip_prefix("checkpoint-") else {
            continue;
        };
        let Ok(number) = number.parse::<u64>() else {
            continue;
        };
        let manifest_path = path.join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        let Ok(bytes) = read_bounded(&manifest_path, MAX_MANIFEST_BYTES) else {
            continue;
        };
        if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
            continue;
        }
        if best.as_ref().is_none_or(|(best, _)| number > *best) {
            best = Some((number, path));
        }
    }
    Ok(best.map(|(_, path)| path))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "ti4-bundle-{name}-{}-{:?}",
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

    fn slots() -> String {
        ti4_policy::vocabulary::Vocabulary::build(["option:a", "option:b", "option:c"])
            .expect("builds")
            .to_json()
            .expect("json")
    }

    fn actor(capacity: i64) -> Actor {
        let mut actor = Actor::zeros(Width::W128, capacity);
        // Non-zero, so a round trip that silently produced zeros would be visible.
        *actor.input_mut() = actor.input().f_add_scalar(0.25).expect("add");
        *actor.hidden_mut() = actor.hidden().f_add_scalar(-0.5).expect("add");
        actor
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "test".to_owned(),
            git_commit: "0000000".to_owned(),
            update: 7,
        }
    }

    fn write_bundle(scratch: &Scratch, mode: CriticMode) -> (PathBuf, String) {
        let text = slots();
        let capacity = ti4_policy::vocabulary::Vocabulary::from_json(&text)
            .expect("loads")
            .capacity()
            .try_into()
            .expect("capacity fits");
        let destination = scratch.0.join("checkpoint-7");
        write(&destination, &actor(capacity), &text, mode, &provenance()).expect("writes");
        (destination, text)
    }

    fn manifest(directory: &Path) -> serde_json::Value {
        serde_json::from_slice(
            &std::fs::read(directory.join("manifest.json")).expect("read manifest"),
        )
        .expect("parse manifest")
    }

    fn replace_manifest(directory: &Path, document: &serde_json::Value) {
        std::fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(document).expect("encode manifest"),
        )
        .expect("write manifest");
    }

    #[test]
    fn a_written_bundle_round_trips_through_the_loader() {
        let scratch = Scratch::new("roundtrip");
        let (directory, text) = write_bundle(&scratch, CriticMode::Shared);

        let loaded = read(&directory).expect("loads");
        assert_eq!(loaded.update, 7);
        assert_eq!(loaded.critic_mode, CriticMode::Shared);
        assert_eq!(
            loaded.vocabulary.slot_count(),
            3 + loaded.vocabulary.oov_count()
        );

        // The weights, not merely the shapes: a loader that allocated zeros would pass a shape
        // check and fail here.
        let original = actor(loaded.actor.capacity());
        let round_tripped = ti4_tensor::to_vec(loaded.actor.input()).expect("vec");
        let expected = ti4_tensor::to_vec(original.input()).expect("vec");
        assert_eq!(round_tripped, expected, "W1 did not survive the round trip");
        assert!(
            round_tripped.iter().any(|value| *value != 0.0),
            "the fixture is all zeros, so the comparison proves nothing"
        );
        let _ = text;
    }

    #[test]
    fn provenance_is_json_encoded_and_unrecorded_commits_are_refused() {
        let scratch = Scratch::new("provenance");
        let text = slots();
        let capacity = ti4_policy::vocabulary::Vocabulary::from_json(&text)
            .expect("loads")
            .capacity()
            .try_into()
            .expect("fits");
        let destination = scratch.0.join("checkpoint-9");
        let quoted = Provenance {
            source: "quoted \"source\"\nline".to_owned(),
            git_commit: "abcdef0".to_owned(),
            update: 9,
        };
        write(
            &destination,
            &actor(capacity),
            &text,
            CriticMode::Shared,
            &quoted,
        )
        .expect("quoted provenance writes valid JSON");
        assert_eq!(
            manifest(&destination)["source"].as_str(),
            Some(quoted.source.as_str())
        );
        read(&destination).expect("encoded provenance reloads");

        let invalid = Provenance {
            git_commit: "unrecorded".to_owned(),
            ..provenance()
        };
        let error = write(
            &scratch.0.join("checkpoint-10"),
            &actor(capacity),
            &text,
            CriticMode::Shared,
            &invalid,
        )
        .expect_err("an unrecorded commit must fail closed");
        assert!(error.to_string().contains("git_commit"), "{error}");
    }

    #[test]
    fn every_load_bearing_manifest_field_is_enforced() {
        let scratch = Scratch::new("strict-manifest");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);
        let original = manifest(&directory);

        let mut corruptions: Vec<(&str, serde_json::Value)> = Vec::new();
        let mut missing_update = original.clone();
        missing_update
            .as_object_mut()
            .expect("object")
            .remove("update");
        corruptions.push(("missing update", missing_update));
        let mut wrong_depth = original.clone();
        wrong_depth["trunk"]["depth"] = serde_json::json!(3);
        corruptions.push(("wrong depth", wrong_depth));
        let mut wrong_temperature = original.clone();
        wrong_temperature["student_temperature"] = serde_json::json!(0.5);
        corruptions.push(("wrong temperature", wrong_temperature));
        let mut mixed_heads = original.clone();
        mixed_heads["heads"]
            .as_array_mut()
            .expect("heads")
            .push(serde_json::json!(17));
        corruptions.push(("non-string head", mixed_heads));
        let mut extra_checksum = original.clone();
        extra_checksum["checksums"]["ignored.bin"] = serde_json::json!("0".repeat(64));
        corruptions.push(("extra checksum", extra_checksum));

        for (name, corruption) in corruptions {
            replace_manifest(&directory, &corruption);
            assert!(read(&directory).is_err(), "{name} was accepted");
        }
        replace_manifest(&directory, &original);
        read(&directory).expect("restored manifest loads");
    }

    #[test]
    fn a_separate_critic_round_trips_as_six_distinct_tensors() {
        let scratch = Scratch::new("separate");
        let text = slots();
        let capacity: i64 = ti4_policy::vocabulary::Vocabulary::from_json(&text)
            .expect("loads")
            .capacity()
            .try_into()
            .expect("fits");
        let mut model = actor(capacity);
        let opts = (ti4_tensor::Kind::Float, ti4_tensor::Device::Cpu);
        model.set_separate_critic(Some(SeparateCritic::new(
            Tensor::ones([capacity, 128], opts),
            Tensor::ones([128], opts) * 2.0,
            Tensor::ones([128, 128], opts) * 3.0,
            Tensor::ones([128], opts) * 4.0,
            Tensor::ones([128], opts) * 5.0,
            Tensor::ones([1], opts) * 6.0,
        )));
        let destination = scratch.0.join("checkpoint-8");
        write(
            &destination,
            &model,
            &text,
            CriticMode::Separate,
            &provenance(),
        )
        .expect("writes");
        let loaded = read(&destination).expect("loads");
        assert_eq!(loaded.critic_mode, CriticMode::Separate);
        let critic = loaded.actor.separate_critic().expect("separate tensors");
        let means: Vec<f64> = critic
            .tensors()
            .iter()
            .map(|(_, tensor)| tensor.mean(ti4_tensor::Kind::Float).double_value(&[]))
            .collect();
        assert_eq!(means, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn the_manifest_is_written_last_so_an_interrupted_write_is_not_a_candidate() {
        let scratch = Scratch::new("incomplete");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);

        // Simulate the crash the ordering exists for: every tensor present, no manifest.
        std::fs::remove_file(directory.join("manifest.json")).expect("remove");
        let error = read(&directory).expect_err("an incomplete bundle must be refused");
        assert!(
            error.to_string().contains("incomplete by construction"),
            "{error}"
        );
        assert!(
            latest_complete(&scratch.0).expect("scan").is_none(),
            "an incomplete directory became the resume candidate"
        );
    }

    #[test]
    fn an_unreadable_manifest_is_not_a_recovery_candidate() {
        let scratch = Scratch::new("unreadable-manifest");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);
        std::fs::write(directory.join("manifest.json"), b"not json").expect("corrupt");
        assert!(
            latest_complete(&scratch.0).expect("scan").is_none(),
            "an unreadable manifest was treated as a committed checkpoint"
        );
    }

    #[test]
    fn a_tampered_tensor_file_is_refused_before_the_model_is_built() {
        let scratch = Scratch::new("tamper");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);

        let trunk = directory.join("trunk.safetensors");
        let mut bytes = std::fs::read(&trunk).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&trunk, &bytes).expect("write");

        let error = read(&directory).expect_err("a tampered tensor must be refused");
        assert!(error.to_string().contains("hashes"), "{error}");
    }

    #[test]
    fn a_slots_digest_that_does_not_match_is_refused() {
        // §4.4 calls this "the single most likely silent corruption — same weights, different
        // feature meaning". Nothing about the tensors is wrong in this case, so only this check
        // catches it.
        let scratch = Scratch::new("slots");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);

        let other = ti4_policy::vocabulary::Vocabulary::build(["option:x", "option:y", "option:z"])
            .expect("builds")
            .to_json()
            .expect("json");
        std::fs::write(directory.join("slots.json"), &other).expect("write");

        let error = read(&directory).expect_err("a swapped vocabulary must be refused");
        assert!(error.to_string().contains("hashes"), "{error}");
    }

    #[test]
    fn an_unrecognised_file_is_refused() {
        let scratch = Scratch::new("extra");
        let (directory, _) = write_bundle(&scratch, CriticMode::Shared);
        std::fs::write(directory.join("notes.txt"), b"hello").expect("write");

        let error = read(&directory).expect_err("an extra file must be refused");
        assert!(error.to_string().contains("unrecognised file"), "{error}");
    }

    #[test]
    fn batch_mean_carries_no_value_tensors_and_must_not_find_any() {
        let scratch = Scratch::new("batchmean");
        let (directory, _) = write_bundle(&scratch, CriticMode::BatchMean);
        assert!(!directory.join("value.safetensors").exists());
        let loaded = read(&directory).expect("loads");
        assert_eq!(loaded.critic_mode, CriticMode::BatchMean);

        // And the other direction: a value file that should not be there is a refusal, not
        // something to ignore.
        std::fs::copy(
            directory.join("embedding.safetensors"),
            directory.join("value.safetensors"),
        )
        .expect("copy");
        let error = read(&directory).expect_err("a stray value file must be refused");
        assert!(error.to_string().contains("batch_mean"), "{error}");
    }

    #[test]
    fn a_bundle_is_never_written_in_place() {
        let scratch = Scratch::new("inplace");
        let (directory, text) = write_bundle(&scratch, CriticMode::Shared);
        let capacity = read(&directory).expect("loads").actor.capacity();

        let error = write(
            &directory,
            &actor(capacity),
            &text,
            CriticMode::Shared,
            &provenance(),
        )
        .expect_err("an existing destination must be refused");
        assert!(error.to_string().contains("already exists"), "{error}");
    }

    #[test]
    fn recovery_takes_the_highest_complete_checkpoint_and_ignores_staging_siblings() {
        let scratch = Scratch::new("recovery");
        let text = slots();
        let capacity = ti4_policy::vocabulary::Vocabulary::from_json(&text)
            .expect("loads")
            .capacity()
            .try_into()
            .expect("capacity fits");

        for update in [3_u64, 11, 7] {
            write(
                &scratch.0.join(format!("checkpoint-{update}")),
                &actor(capacity),
                &text,
                CriticMode::Shared,
                &provenance(),
            )
            .expect("writes");
        }
        // A staging sibling with a manifest — the one directory that must never be resumed from
        // even though it looks complete, because its name says it was mid-write.
        let staging = scratch.0.join("checkpoint-99.tmp");
        std::fs::create_dir_all(&staging).expect("dir");
        std::fs::write(staging.join("manifest.json"), b"{}").expect("write");

        let latest = latest_complete(&scratch.0).expect("scan").expect("some");
        assert!(
            latest.ends_with("checkpoint-11"),
            "recovery chose {}",
            latest.display()
        );
        assert!(staging.exists(), "recovery deleted a directory");
    }
}
