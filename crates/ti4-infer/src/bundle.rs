//! The reader of a training bundle directory (`ti4-mlp/src/bundle.rs`), for the pack tool and the
//! tests.
//!
//! ```text
//! checkpoint-<update>/
//!   trunk.safetensors      W1 b1 W2 b2, and R{i}_W_in R{i}_b_in R{i}_W_out R{i}_b_out
//!   readout.safetensors    w_shared b_shared delta b_delta
//!   value.safetensors      the critic; absent for critic_mode batch_mean
//!   embedding.safetensors  embedding [33][16]
//!   slots.json             the vocabulary
//!   battle_predictor.json  schemas 11 and 12 only
//!   manifest.json          schema, shapes, provenance, checksums
//! ```
//!
//! The checks are those of `ti4_mlp::bundle::read` that say what the weights mean: the closed set
//! of files, the manifest's fields, schema 7 to 12 with its projection ABI and head layout, the
//! faction roster, the SHA-256 of every file, `slots_sha256`, the slot counts, f32 only, and every
//! tensor's shape. It does not compare the manifest's `tch`, `libtorch`, `compiler` and `threads`
//! with a training build: those say how the bundle was trained, not what it computes. The
//! vocabulary itself is checked where it is parsed (`ti4_policy::vocabulary`).
//!
//! The `safetensors` files are parsed by hand: a u64 little-endian header length, the header JSON,
//! then the raw little-endian data.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::format::{BlockParts, Parts, Source};
use crate::{
    DIPLOMACY_HEADS, EMBED_DIM, FACTION_ROSTER, InferError, LEGACY_HEADS, MAX_CAPACITY,
    MAX_RESIDUAL_BLOCKS, is_sha256_hex, sha256_hex,
};

/// The bound on the whole directory.
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// The bound on `manifest.json`.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// The bound on `slots.json`.
const MAX_SLOTS_BYTES: u64 = 32 * 1024 * 1024;
/// The bound on `battle_predictor.json`.
const MAX_BATTLE_BYTES: u64 = 32 * 1024 * 1024;
/// The bound on the header of a `safetensors` file.
const MAX_SAFETENSORS_HEADER: u64 = 1024 * 1024;

/// The predictor file of schemas 11 and 12.
pub const BATTLE_FILE: &str = "battle_predictor.json";

/// Every file a bundle may hold.
const FILES: [&str; 7] = [
    "manifest.json",
    "trunk.safetensors",
    "readout.safetensors",
    "value.safetensors",
    "embedding.safetensors",
    "slots.json",
    BATTLE_FILE,
];

/// The fields of a manifest, exactly.
const MANIFEST_KEYS: [&str; 20] = [
    "schema",
    "projection_abi",
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
    "checksums",
];

fn invalid(detail: impl Into<String>) -> InferError {
    InferError::Bundle(detail.into())
}

/// One tensor of a `safetensors` file: its shape and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    /// The dimensions.
    pub shape: Vec<usize>,
    /// Row-major values.
    pub values: Vec<f32>,
}

/// Parses a `safetensors` file of f32 tensors.
///
/// The header must be an object of tensors (and an optional `__metadata__` of strings), each with
/// `dtype` `F32`, a `shape` and `data_offsets` that cover the data exactly, in some order, without
/// a gap or an overlap.
///
/// # Errors
/// [`InferError::Bundle`] naming `file` for anything else.
pub fn parse_safetensors(file: &str, bytes: &[u8]) -> Result<BTreeMap<String, Tensor>, InferError> {
    let bad = |detail: String| invalid(format!("{file}: {detail}"));
    if bytes.len() < 8 {
        return Err(bad("shorter than its header length".to_owned()));
    }
    let header_len = u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
    if header_len > MAX_SAFETENSORS_HEADER || 8 + header_len > bytes.len() as u64 {
        return Err(bad(format!(
            "header length {header_len} does not fit the file"
        )));
    }
    let header_len = usize::try_from(header_len).unwrap_or(usize::MAX);
    let header: Value = serde_json::from_slice(&bytes[8..8 + header_len])
        .map_err(|error| bad(format!("the header is not JSON: {error}")))?;
    let header = header
        .as_object()
        .ok_or_else(|| bad("the header is not an object".to_owned()))?;
    let data = &bytes[8 + header_len..];

    let mut spans: Vec<(usize, usize, String, Vec<usize>)> = Vec::new();
    for (name, entry) in header {
        if name == "__metadata__" {
            let strings = entry
                .as_object()
                .is_some_and(|metadata| metadata.values().all(Value::is_string));
            if !strings {
                return Err(bad("__metadata__ is not an object of strings".to_owned()));
            }
            continue;
        }
        let dtype = entry.get("dtype").and_then(Value::as_str);
        if dtype != Some("F32") {
            return Err(bad(format!("tensor {name} is {dtype:?}, not F32")));
        }
        let shape = entry
            .get("shape")
            .and_then(Value::as_array)
            .and_then(|dims| {
                dims.iter()
                    .map(|dim| dim.as_u64().and_then(|dim| usize::try_from(dim).ok()))
                    .collect::<Option<Vec<usize>>>()
            })
            .ok_or_else(|| bad(format!("tensor {name} has no shape")))?;
        let offsets = entry
            .get("data_offsets")
            .and_then(Value::as_array)
            .and_then(|pair| {
                pair.iter()
                    .map(|offset| {
                        offset
                            .as_u64()
                            .and_then(|offset| usize::try_from(offset).ok())
                    })
                    .collect::<Option<Vec<usize>>>()
            })
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| bad(format!("tensor {name} has no data_offsets")))?;
        let (begin, end) = (offsets[0], offsets[1]);
        let count = shape
            .iter()
            .try_fold(1usize, |count, dim| count.checked_mul(*dim))
            .and_then(|count| count.checked_mul(4));
        if begin > end || end > data.len() || count != Some(end - begin) {
            return Err(bad(format!(
                "tensor {name} of shape {shape:?} has data_offsets [{begin}, {end}] in {} bytes",
                data.len()
            )));
        }
        spans.push((begin, end, name.clone(), shape));
    }
    spans.sort();
    let mut cursor = 0;
    for (begin, end, name, _) in &spans {
        if *begin != cursor {
            return Err(bad(format!(
                "tensor {name} does not start where the one before ends"
            )));
        }
        cursor = *end;
    }
    if cursor != data.len() {
        return Err(bad(format!(
            "the tensors cover {cursor} of {} data bytes",
            data.len()
        )));
    }
    Ok(spans
        .into_iter()
        .map(|(begin, end, name, shape)| {
            let values = data[begin..end]
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            (name, Tensor { shape, values })
        })
        .collect())
}

/// Reads one bundle directory into the [`Parts`] of a model.
///
/// The directory may hold only the files of a bundle, regular and not linked, within the size
/// bounds; then [`from_files`] checks the rest.
///
/// # Errors
/// [`InferError::Io`] when the file system refuses, [`InferError::Bundle`] for anything the bundle
/// gets wrong.
pub fn read(directory: &Path) -> Result<Parts, InferError> {
    let io = |context: String| move |source| InferError::Io { context, source };
    let manifest_path = directory.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(invalid(format!(
            "{} has no manifest.json, so it is incomplete",
            directory.display()
        )));
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total: u64 = 0;
    let entries =
        std::fs::read_dir(directory).map_err(io(format!("listing {}", directory.display())))?;
    for entry in entries {
        let entry = entry.map_err(io("reading a directory entry".to_owned()))?;
        let path = entry.path();
        let metadata = path
            .symlink_metadata()
            .map_err(io(format!("stat-ing {}", path.display())))?;
        if !metadata.file_type().is_file() {
            return Err(invalid(format!(
                "{} is not a regular file; a bundle holds only regular files",
                path.display()
            )));
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| invalid("a file name is not UTF-8"))?
            .to_owned();
        if !FILES.contains(&name.as_str()) {
            return Err(invalid(format!(
                "unrecognised file {name}; a bundle is a closed set"
            )));
        }
        let cap = match name.as_str() {
            "manifest.json" => MAX_MANIFEST_BYTES,
            "slots.json" => MAX_SLOTS_BYTES,
            BATTLE_FILE => MAX_BATTLE_BYTES,
            _ => MAX_TOTAL_BYTES,
        };
        total = total.saturating_add(metadata.len());
        if metadata.len() > cap || total > MAX_TOTAL_BYTES {
            return Err(invalid(format!("{name} is over its size bound")));
        }
        let bytes = std::fs::read(&path).map_err(io(format!("reading {}", path.display())))?;
        files.insert(name, bytes);
    }
    let manifest = files.remove("manifest.json").unwrap_or_default();
    from_files(&manifest, &files)
}

fn u64_field(document: &Value, field: &str) -> Result<u64, InferError> {
    document
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(format!("the manifest has no {field}")))
}

fn str_field<'a>(document: &'a Value, field: &str) -> Result<&'a str, InferError> {
    document
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("the manifest has no {field}")))
}

fn expect_list(document: &Value, field: &str, expected: &[&str]) -> Result<(), InferError> {
    let list = document.get(field).and_then(Value::as_array);
    let same = list.is_some_and(|list| {
        list.len() == expected.len()
            && list
                .iter()
                .zip(expected)
                .all(|(item, expected)| item.as_str() == Some(*expected))
    });
    if same {
        Ok(())
    } else {
        Err(invalid(format!("{field} is not {expected:?}")))
    }
}

/// Checks a bundle given as its manifest and its other files by name, and returns its parts.
///
/// # Errors
/// [`InferError::Bundle`] for any check that fails; the schema is checked first.
pub fn from_files(
    manifest_bytes: &[u8],
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Parts, InferError> {
    let manifest: Value = serde_json::from_slice(manifest_bytes)
        .map_err(|error| invalid(format!("manifest.json is not JSON: {error}")))?;
    let schema = u64_field(&manifest, "schema")?;
    if !(7..=12).contains(&schema) {
        return Err(invalid(format!("unsupported MLP bundle schema {schema}")));
    }
    let schema = u32::try_from(schema).unwrap_or(0);
    let object = manifest
        .as_object()
        .ok_or_else(|| invalid("the manifest is not an object"))?;
    let found: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    if found != MANIFEST_KEYS.into_iter().collect() {
        return Err(invalid(format!(
            "the manifest fields {found:?} are not those of a bundle"
        )));
    }

    let diplomacy = schema >= 9;
    let battle = schema >= 11;
    let residual = schema % 2 == 0;
    let heads: &[&str] = if diplomacy {
        &DIPLOMACY_HEADS
    } else {
        &LEGACY_HEADS
    };
    let expected_abi = match (diplomacy, battle) {
        (false, _) => 2,
        (true, false) => 3,
        (true, true) => 4,
    };
    let projection_abi = u64_field(&manifest, "projection_abi")?;
    if projection_abi != expected_abi {
        return Err(invalid(format!(
            "projection ABI {projection_abi} is not {expected_abi} for schema {schema}"
        )));
    }
    if str_field(&manifest, "dtype")? != "f32" {
        return Err(invalid("dtype is not f32"));
    }
    let critic_mode = str_field(&manifest, "critic_mode")?;
    if !["shared", "separate", "batch_mean"].contains(&critic_mode) {
        return Err(invalid(format!("unknown critic_mode {critic_mode}")));
    }

    let trunk = manifest
        .get("trunk")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("the manifest has no trunk object"))?;
    let blocks = if residual {
        let declared = trunk
            .get("residual_blocks")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                invalid(format!(
                    "residual schema {schema} has no trunk.residual_blocks"
                ))
            })?;
        if declared == 0 || declared > MAX_RESIDUAL_BLOCKS as u64 {
            return Err(invalid(format!(
                "trunk.residual_blocks {declared} is outside 1..={MAX_RESIDUAL_BLOCKS}"
            )));
        }
        usize::try_from(declared).unwrap_or(0)
    } else {
        0
    };
    if trunk.len() != if residual { 4 } else { 3 }
        || trunk.get("depth").and_then(Value::as_u64) != Some(2)
        || trunk.get("activation").and_then(Value::as_str) != Some("relu")
    {
        return Err(invalid(
            "trunk must be exactly {width, depth: 2, activation: relu}",
        ));
    }
    let width = trunk
        .get("width")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("the manifest has no trunk.width"))?;
    if width != 128 && width != 256 {
        return Err(invalid(format!("width {width} is not 256 or 128")));
    }
    let width = usize::try_from(width).unwrap_or(0);
    if u64_field(&manifest, "embed_dim")? != EMBED_DIM as u64 {
        return Err(invalid("embed_dim is not 16"));
    }
    if manifest.get("student_temperature").and_then(Value::as_f64) != Some(1.0) {
        return Err(invalid("student_temperature must be 1.0"));
    }
    if str_field(&manifest, "source")?.trim().is_empty() {
        return Err(invalid("provenance source is empty"));
    }
    let commit = str_field(&manifest, "git_commit")?;
    if !(7..=64).contains(&commit.len()) || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "git_commit must be a 7-64 digit hexadecimal commit",
        ));
    }
    u64_field(&manifest, "update")?;
    expect_list(&manifest, "heads", heads)?;
    expect_list(&manifest, "factions", &FACTION_ROSTER)?;

    // The files: exactly the ones the schema and the critic mode name, each with its checksum.
    let mut expected_files: BTreeSet<&str> = [
        "trunk.safetensors",
        "readout.safetensors",
        "embedding.safetensors",
        "slots.json",
    ]
    .into_iter()
    .collect();
    if critic_mode != "batch_mean" {
        expected_files.insert("value.safetensors");
    }
    if battle {
        expected_files.insert(BATTLE_FILE);
    }
    let present: BTreeSet<&str> = files.keys().map(String::as_str).collect();
    if present != expected_files {
        return Err(invalid(format!(
            "the bundle holds {present:?}, schema {schema} with critic_mode {critic_mode} holds {expected_files:?}"
        )));
    }
    let checksums = manifest
        .get("checksums")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("the manifest has no checksums"))?;
    let listed: BTreeSet<&str> = checksums.keys().map(String::as_str).collect();
    if listed != expected_files
        || checksums
            .values()
            .any(|digest| !digest.as_str().is_some_and(is_sha256_hex))
    {
        return Err(invalid(
            "the checksums do not list exactly the bundle's files",
        ));
    }
    for (name, bytes) in files {
        let found = sha256_hex(bytes);
        let declared = checksums
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default();
        if found != declared {
            return Err(invalid(format!(
                "{name} hashes {found}, the manifest says {declared}"
            )));
        }
    }

    // The vocabulary: its digest, its counts.
    let slots_bytes = &files["slots.json"];
    if str_field(&manifest, "slots_sha256")? != sha256_hex(slots_bytes) {
        return Err(invalid("slots_sha256 does not match slots.json"));
    }
    let slots =
        String::from_utf8(slots_bytes.clone()).map_err(|_| invalid("slots.json is not UTF-8"))?;
    let vocabulary: Value = serde_json::from_str(&slots)
        .map_err(|error| invalid(format!("slots.json is not JSON: {error}")))?;
    let slot_count = u64_field(&manifest, "slot_count")?;
    let capacity = u64_field(&manifest, "slot_capacity")?;
    let listed_slots = vocabulary
        .get("slots")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| invalid("slots.json has no slots"))?;
    if slot_count != listed_slots as u64
        || vocabulary.get("capacity").and_then(Value::as_u64) != Some(capacity)
    {
        return Err(invalid(format!(
            "the manifest says {slot_count} slots of capacity {capacity}; slots.json disagrees"
        )));
    }
    if slot_count > capacity || capacity > MAX_CAPACITY as u64 {
        return Err(invalid(format!(
            "slot bounds require slot_count <= slot_capacity <= {MAX_CAPACITY}, got {slot_count} and {capacity}"
        )));
    }
    let capacity = usize::try_from(capacity).unwrap_or(0);

    let battle_text = if battle {
        Some(
            String::from_utf8(files[BATTLE_FILE].clone())
                .map_err(|_| invalid(format!("{BATTLE_FILE} is not UTF-8")))?,
        )
    } else {
        None
    };

    // The tensors: every one the manifest implies, of its shape, and no other.
    let mut named: BTreeMap<String, Tensor> = BTreeMap::new();
    let mut tensor_files = vec![
        "trunk.safetensors",
        "readout.safetensors",
        "embedding.safetensors",
    ];
    if critic_mode != "batch_mean" {
        tensor_files.push("value.safetensors");
    }
    for file in tensor_files {
        for (name, tensor) in parse_safetensors(file, &files[file])? {
            if named.contains_key(&name) {
                return Err(invalid(format!(
                    "tensor {name} appears in more than one file"
                )));
            }
            named.insert(name, tensor);
        }
    }
    let (h, f, w) = (heads.len(), FACTION_ROSTER.len(), width);
    let mut shapes: Vec<(String, Vec<usize>)> = [
        ("W1", vec![capacity, w]),
        ("b1", vec![w]),
        ("W2", vec![w, w]),
        ("b2", vec![w]),
        ("w_shared", vec![h, w]),
        ("b_shared", vec![h]),
        ("delta", vec![f, h, w]),
        ("b_delta", vec![f, h]),
        ("embedding", vec![f, EMBED_DIM]),
    ]
    .into_iter()
    .map(|(name, shape)| (name.to_owned(), shape))
    .collect();
    match critic_mode {
        "shared" => {
            shapes.push(("w_value".to_owned(), vec![w]));
            shapes.push(("b_value".to_owned(), vec![1]));
        }
        "separate" => {
            for (name, shape) in [
                ("critic_W1", vec![capacity, 128]),
                ("critic_b1", vec![128]),
                ("critic_W2", vec![128, 128]),
                ("critic_b2", vec![128]),
                ("critic_readout", vec![128]),
                ("critic_bias", vec![1]),
            ] {
                shapes.push((name.to_owned(), shape));
            }
        }
        _ => {}
    }
    for index in 0..blocks {
        for part in ["W_in", "b_in", "W_out", "b_out"] {
            let shape = if part.starts_with('W') {
                vec![w, w]
            } else {
                vec![w]
            };
            shapes.push((format!("R{index}_{part}"), shape));
        }
    }
    for (name, shape) in &shapes {
        let tensor = named
            .get(name)
            .ok_or_else(|| invalid(format!("the bundle has no tensor {name}")))?;
        if tensor.shape != *shape {
            return Err(invalid(format!(
                "tensor {name} has shape {:?}, the manifest implies {shape:?}",
                tensor.shape
            )));
        }
    }
    if let Some(name) = named
        .keys()
        .find(|name| !shapes.iter().any(|(known, _)| known == *name))
    {
        return Err(invalid(format!(
            "the bundle carries an unrecognised tensor {name}"
        )));
    }

    let mut take = |name: &str| {
        named
            .remove(name)
            .map(|tensor| tensor.values)
            .unwrap_or_default()
    };
    let blocks = (0..blocks)
        .map(|index| BlockParts {
            w_in: take(&format!("R{index}_W_in")),
            b_in: take(&format!("R{index}_b_in")),
            w_out: take(&format!("R{index}_W_out")),
            b_out: take(&format!("R{index}_b_out")),
        })
        .collect();
    Ok(Parts {
        width,
        capacity,
        heads: heads.iter().map(|head| (*head).to_owned()).collect(),
        w1: take("W1"),
        embedding: take("embedding"),
        b1: take("b1"),
        w2: take("W2"),
        b2: take("b2"),
        blocks,
        w_shared: take("w_shared"),
        b_shared: take("b_shared"),
        delta: take("delta"),
        b_delta: take("b_delta"),
        slots,
        battle: battle_text,
        source: Source {
            manifest_sha256: sha256_hex(manifest_bytes),
            schema,
            projection_abi: u32::try_from(projection_abi).unwrap_or(0),
        },
    })
}
