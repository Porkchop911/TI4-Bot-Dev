//! The `.ti4m` model file: what the server and the page read.
//!
//! ```text
//! "TI4M" | u32 version | u32 header length | header JSON | zero pad to 64
//! sections, little-endian, each at a multiple of 64 from the end of the pad, zero-padded to 64:
//!   w1         f32 [capacity][width]          rows, as gathered
//!   embedding  f32 [33][16]
//!   b1, b2     f32 [width]
//!   w2         f32 packed panels
//!   r{i}_in, r{i}_b_in, r{i}_out, r{i}_b_out   panels, f32 [width], panels, f32 [width]
//!   readout    f32 [33][heads][width]         w_shared[h] + delta[f][h]
//!   readout_b  f32 [33][heads]                b_shared[h] + b_delta[f][h]
//!   slots      the vocabulary JSON of the bundle, verbatim
//!   battle     battle_predictor.json, verbatim (schemas 11 and 12 only)
//! trailer: SHA-256 of every byte before it
//! ```
//!
//! The header is a JSON object: `name`, `label`, `temperature`, `width`, `capacity`, `heads`,
//! `factions`, `residual_blocks`, `embed_dim`, `source` (`manifest_sha256`, `schema`,
//! `projection_abi` of the bundle), `data_bytes` (the length of the sections with their padding),
//! and `sections`: for each section in the order above, its `name`, `offset` (from the start of the
//! sections), `bytes` (without padding) and `layout` (`rows`, `panels8` or `json`).
//!
//! A dense matrix `W [out][in]` is stored as `out / 8` panels: for each `k` in `0..in`, the eight
//! weights `W[p·8 .. p·8+8][k]` side by side. The critic and the value head are not in the file.

use serde_json::{Value, json};

use crate::kernel::{NR, pack_panels};
use crate::model::{BlockRanges, Model};
use crate::simd::AlignedF32;
use crate::{
    EMBED_DIM, FACTION_ROSTER, InferError, MAX_CAPACITY, MAX_RESIDUAL_BLOCKS, hex, is_head_layout,
    is_sha256_hex, sha256_hex,
};

/// The version of the file this crate writes and reads.
pub const VERSION: u32 = 1;

/// The first four bytes of a model file.
pub const MAGIC: [u8; 4] = *b"TI4M";

/// The alignment of the sections.
const ALIGN: usize = 64;

/// The largest header a reader accepts.
const MAX_HEADER_BYTES: usize = 1024 * 1024;

/// The bytes of the trailer.
const TRAILER: usize = 32;

/// The temperature of a model whose pack names none (`plans/MLP_TEMPERATURE_REGIME.md`).
pub const DEFAULT_TEMPERATURE: f32 = 0.25;

/// What a pack gives a model beside its weights.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    /// The `<name>` of the bot kind `mlp:<name>`: 1 to 64 of `a-z`, `0-9`, `-`, `_`, `.`.
    pub name: String,
    /// The label shown in the lobby.
    pub label: String,
    /// The softmax temperature of the bot; finite and positive.
    pub temperature: f32,
}

/// The bundle a model was packed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The SHA-256 of the bundle's `manifest.json`.
    pub manifest_sha256: String,
    /// The bundle schema, 7 to 12.
    pub schema: u32,
    /// The projection ABI the bundle's features were extracted with.
    pub projection_abi: u32,
}

/// One residual block, as a bundle stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockParts {
    /// `[width][width]`, `[out][in]`.
    pub w_in: Vec<f32>,
    /// `[width]`.
    pub b_in: Vec<f32>,
    /// `[width][width]`, `[out][in]`.
    pub w_out: Vec<f32>,
    /// `[width]`.
    pub b_out: Vec<f32>,
}

/// The weights of a model as a bundle stores them (row-major f32), and what the file keeps of the
/// bundle beside them. [`crate::bundle::read`] makes one; [`write`] packs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Parts {
    /// The trunk width.
    pub width: usize,
    /// The rows of the input table.
    pub capacity: usize,
    /// The readout heads, in order: [`crate::LEGACY_HEADS`] or [`crate::DIPLOMACY_HEADS`].
    pub heads: Vec<String>,
    /// `W1 [capacity][width]`.
    pub w1: Vec<f32>,
    /// `[33][16]`.
    pub embedding: Vec<f32>,
    /// `[width]`.
    pub b1: Vec<f32>,
    /// `W2 [width][width]`, `[out][in]`.
    pub w2: Vec<f32>,
    /// `[width]`.
    pub b2: Vec<f32>,
    /// The residual blocks, in order.
    pub blocks: Vec<BlockParts>,
    /// `[heads][width]`.
    pub w_shared: Vec<f32>,
    /// `[heads]`.
    pub b_shared: Vec<f32>,
    /// `[33][heads][width]`.
    pub delta: Vec<f32>,
    /// `[33][heads]`.
    pub b_delta: Vec<f32>,
    /// The bundle's `slots.json`, verbatim.
    pub slots: String,
    /// The bundle's `battle_predictor.json`, verbatim, for schemas 11 and 12.
    pub battle: Option<String>,
    /// The bundle.
    pub source: Source,
}

fn parts_error(detail: impl Into<String>) -> InferError {
    InferError::Parts(detail.into())
}

fn format_error(detail: impl Into<String>) -> InferError {
    InferError::Format(detail.into())
}

const fn width_ok(width: usize) -> bool {
    width > 0 && width % NR == 0 && width >= EMBED_DIM
}

fn name_ok(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
}

fn temperature_ok(temperature: f32) -> bool {
    temperature.is_finite() && temperature > 0.0
}

fn schema_ok(schema: u32) -> bool {
    (7..=12).contains(&schema)
}

fn is_battle_schema(schema: u32) -> bool {
    schema == 11 || schema == 12
}

/// What kind of bytes a section holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Rows,
    Panels,
    Json,
}

impl Layout {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Panels => "panels8",
            Self::Json => "json",
        }
    }
}

/// The sections a model of this shape has, in order, with their layout and length in bytes (for
/// the JSON sections, `None`: their length is whatever the text is).
fn expected_sections(
    width: usize,
    capacity: usize,
    heads: usize,
    blocks: usize,
    battle: bool,
) -> Vec<(String, Layout, Option<usize>)> {
    let f = |count: usize| Some(count * 4);
    let factions = FACTION_ROSTER.len();
    let mut sections = vec![
        ("w1".to_owned(), Layout::Rows, f(capacity * width)),
        (
            "embedding".to_owned(),
            Layout::Rows,
            f(factions * EMBED_DIM),
        ),
        ("b1".to_owned(), Layout::Rows, f(width)),
        ("b2".to_owned(), Layout::Rows, f(width)),
        ("w2".to_owned(), Layout::Panels, f(width * width)),
    ];
    for index in 0..blocks {
        sections.push((format!("r{index}_in"), Layout::Panels, f(width * width)));
        sections.push((format!("r{index}_b_in"), Layout::Rows, f(width)));
        sections.push((format!("r{index}_out"), Layout::Panels, f(width * width)));
        sections.push((format!("r{index}_b_out"), Layout::Rows, f(width)));
    }
    sections.push((
        "readout".to_owned(),
        Layout::Rows,
        f(factions * heads * width),
    ));
    sections.push(("readout_b".to_owned(), Layout::Rows, f(factions * heads)));
    sections.push(("slots".to_owned(), Layout::Json, None));
    if battle {
        sections.push(("battle".to_owned(), Layout::Json, None));
    }
    sections
}

/// Checks every shape of `parts` and that every weight is finite.
fn check_parts(meta: &Meta, parts: &Parts) -> Result<(), InferError> {
    if !name_ok(&meta.name) {
        return Err(parts_error(format!(
            "name {:?} is not 1 to 64 of a-z, 0-9, '-', '_', '.'",
            meta.name
        )));
    }
    if meta.label.trim().is_empty() {
        return Err(parts_error("the label is empty"));
    }
    if !temperature_ok(meta.temperature) {
        return Err(parts_error(format!(
            "temperature {} is not finite and positive",
            meta.temperature
        )));
    }
    let width = parts.width;
    if !width_ok(width) {
        return Err(InferError::Width(width));
    }
    if parts.capacity == 0 || parts.capacity > MAX_CAPACITY {
        return Err(parts_error(format!(
            "capacity {} is outside 1..={MAX_CAPACITY}",
            parts.capacity
        )));
    }
    if !is_head_layout(&parts.heads) {
        return Err(parts_error(format!(
            "heads {:?} are not a known layout",
            parts.heads
        )));
    }
    if parts.blocks.len() > MAX_RESIDUAL_BLOCKS {
        return Err(parts_error(format!(
            "{} residual blocks, at most {MAX_RESIDUAL_BLOCKS}",
            parts.blocks.len()
        )));
    }
    let source = &parts.source;
    if !schema_ok(source.schema) || !is_sha256_hex(&source.manifest_sha256) {
        return Err(parts_error(format!(
            "source {source:?} is not a bundle of schema 7 to 12"
        )));
    }
    if parts.battle.is_some() != is_battle_schema(source.schema) {
        return Err(parts_error(
            "a battle predictor goes with schemas 11 and 12, and only with them",
        ));
    }
    let heads = parts.heads.len();
    let factions = FACTION_ROSTER.len();
    let mut tensors: Vec<(String, &[f32], usize)> = vec![
        ("W1".to_owned(), &parts.w1, parts.capacity * width),
        (
            "embedding".to_owned(),
            &parts.embedding,
            factions * EMBED_DIM,
        ),
        ("b1".to_owned(), &parts.b1, width),
        ("W2".to_owned(), &parts.w2, width * width),
        ("b2".to_owned(), &parts.b2, width),
        ("w_shared".to_owned(), &parts.w_shared, heads * width),
        ("b_shared".to_owned(), &parts.b_shared, heads),
        ("delta".to_owned(), &parts.delta, factions * heads * width),
        ("b_delta".to_owned(), &parts.b_delta, factions * heads),
    ];
    for (index, block) in parts.blocks.iter().enumerate() {
        tensors.push((format!("R{index}_W_in"), &block.w_in, width * width));
        tensors.push((format!("R{index}_b_in"), &block.b_in, width));
        tensors.push((format!("R{index}_W_out"), &block.w_out, width * width));
        tensors.push((format!("R{index}_b_out"), &block.b_out, width));
    }
    for (name, values, expected) in tensors {
        if values.len() != expected {
            return Err(parts_error(format!(
                "{name} has {} floats, the shape needs {expected}",
                values.len()
            )));
        }
        if let Some(value) = values.iter().find(|value| !value.is_finite()) {
            return Err(parts_error(format!("{name} holds {value}")));
        }
    }
    Ok(())
}

fn push_f32(data: &mut Vec<u8>, values: &[f32]) {
    data.reserve(values.len() * 4);
    for value in values {
        data.extend_from_slice(&value.to_le_bytes());
    }
}

fn pad(data: &mut Vec<u8>) {
    data.resize(data.len().next_multiple_of(ALIGN), 0);
}

/// Packs `parts` into the bytes of a `.ti4m` file.
///
/// # Errors
/// [`InferError::Width`] for a width that is not a positive multiple of 8 of at least 16, and
/// [`InferError::Parts`] for a bad name, label or temperature, shapes that disagree, a head list
/// that is not a known layout, or a weight that is not finite.
pub fn write(meta: &Meta, parts: &Parts) -> Result<Vec<u8>, InferError> {
    check_parts(meta, parts)?;
    let width = parts.width;
    let heads = parts.heads.len();
    let factions = FACTION_ROSTER.len();

    // w_shared[h] + delta[f][h], and the same for the biases, added once in f32.
    let mut readout = Vec::with_capacity(factions * heads * width);
    let mut readout_b = Vec::with_capacity(factions * heads);
    for f in 0..factions {
        for h in 0..heads {
            let shared = &parts.w_shared[h * width..][..width];
            let delta = &parts.delta[(f * heads + h) * width..][..width];
            readout.extend(shared.iter().zip(delta).map(|(s, d)| s + d));
            readout_b.push(parts.b_shared[h] + parts.b_delta[f * heads + h]);
        }
    }

    let mut contents: Vec<Vec<u8>> = Vec::new();
    let mut f32_section = |values: &[f32]| {
        let mut bytes = Vec::new();
        push_f32(&mut bytes, values);
        contents.push(bytes);
    };
    f32_section(&parts.w1);
    f32_section(&parts.embedding);
    f32_section(&parts.b1);
    f32_section(&parts.b2);
    f32_section(&pack_panels(&parts.w2, width, width));
    for block in &parts.blocks {
        f32_section(&pack_panels(&block.w_in, width, width));
        f32_section(&block.b_in);
        f32_section(&pack_panels(&block.w_out, width, width));
        f32_section(&block.b_out);
    }
    f32_section(&readout);
    f32_section(&readout_b);
    contents.push(parts.slots.as_bytes().to_vec());
    if let Some(battle) = &parts.battle {
        contents.push(battle.as_bytes().to_vec());
    }

    let layout = expected_sections(
        width,
        parts.capacity,
        heads,
        parts.blocks.len(),
        parts.battle.is_some(),
    );
    let mut data = Vec::new();
    let mut table = Vec::new();
    for ((name, layout, _), bytes) in layout.iter().zip(&contents) {
        table.push(json!({
            "name": name,
            "offset": data.len(),
            "bytes": bytes.len(),
            "layout": layout.as_str(),
        }));
        data.extend_from_slice(bytes);
        pad(&mut data);
    }

    let header = json!({
        "name": meta.name,
        "label": meta.label,
        "temperature": meta.temperature,
        "width": width,
        "capacity": parts.capacity,
        "heads": parts.heads,
        "factions": FACTION_ROSTER.as_slice(),
        "residual_blocks": parts.blocks.len(),
        "embed_dim": EMBED_DIM,
        "source": {
            "manifest_sha256": parts.source.manifest_sha256,
            "schema": parts.source.schema,
            "projection_abi": parts.source.projection_abi,
        },
        "data_bytes": data.len(),
        "sections": table,
    });
    let header = serde_json::to_vec(&header)
        .map_err(|error| parts_error(format!("the header cannot be encoded: {error}")))?;
    Ok(assemble(&header, &data))
}

/// The bytes of a file: the preamble, the header and its pad, the sections, and the trailer.
#[must_use]
pub fn assemble(header: &[u8], data: &[u8]) -> Vec<u8> {
    let mut file = Vec::with_capacity(12 + header.len() + ALIGN + data.len() + TRAILER);
    file.extend_from_slice(&MAGIC);
    file.extend_from_slice(&VERSION.to_le_bytes());
    file.extend_from_slice(
        &u32::try_from(header.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    file.extend_from_slice(header);
    pad(&mut file);
    file.extend_from_slice(data);
    let digest = {
        use sha2::Digest as _;
        sha2::Sha256::digest(&file)
    };
    file.extend_from_slice(&digest);
    file
}

/// The header JSON and the sections of a file, after only the preamble is checked: for a tool or a
/// test that rewrites a header. [`read`] checks the rest.
///
/// # Errors
/// [`InferError::Magic`], [`InferError::Version`], or [`InferError::Truncated`] when the bytes end
/// before the header does.
pub fn split(bytes: &[u8]) -> Result<(&[u8], &[u8]), InferError> {
    if bytes.len() < 12 || bytes[..4] != MAGIC {
        return Err(InferError::Magic);
    }
    let word =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let version = word(4);
    if version != VERSION {
        return Err(InferError::Version(version));
    }
    let header_len = usize::try_from(word(8)).unwrap_or(usize::MAX);
    if header_len > MAX_HEADER_BYTES {
        return Err(format_error(format!(
            "the header has {header_len} bytes, at most {MAX_HEADER_BYTES}"
        )));
    }
    let base = (12 + header_len).next_multiple_of(ALIGN);
    if bytes.len() < base + TRAILER {
        return Err(InferError::Truncated {
            expected: base + TRAILER,
            found: bytes.len(),
        });
    }
    Ok((
        &bytes[12..12 + header_len],
        &bytes[base..bytes.len() - TRAILER],
    ))
}

fn usize_field(header: &Value, field: &str) -> Result<usize, InferError> {
    header
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format_error(format!("the header has no {field}")))
}

fn str_field<'a>(header: &'a Value, field: &str) -> Result<&'a str, InferError> {
    header
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format_error(format!("the header has no {field}")))
}

fn string_list(header: &Value, field: &str) -> Result<Vec<String>, InferError> {
    header
        .get(field)
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| format_error(format!("the header has no list of strings {field}")))
}

/// Reads and checks a `.ti4m` file, and copies its sections once into an aligned buffer.
///
/// The checks, in order: the magic and the version; the length the header implies
/// ([`InferError::Truncated`]); the SHA-256 trailer ([`InferError::Digest`]); the width
/// ([`InferError::Width`]); then every other field and every section ([`InferError::Format`]).
///
/// # Errors
/// As above.
pub fn read(bytes: &[u8]) -> Result<Model, InferError> {
    let (header_bytes, data) = split(bytes)?;
    let header: Value = serde_json::from_slice(header_bytes)
        .map_err(|error| format_error(format!("the header is not JSON: {error}")))?;
    let data_bytes = usize_field(&header, "data_bytes")?;
    let base = bytes.len() - TRAILER - data.len();
    let expected = base.saturating_add(data_bytes).saturating_add(TRAILER);
    if bytes.len() != expected {
        return Err(InferError::Truncated {
            expected,
            found: bytes.len(),
        });
    }
    let (body, trailer) = bytes.split_at(bytes.len() - TRAILER);
    let found = sha256_hex(body);
    if found != hex(trailer) {
        return Err(InferError::Digest {
            expected: hex(trailer),
            found,
        });
    }

    const KEYS: [&str; 12] = [
        "name",
        "label",
        "temperature",
        "width",
        "capacity",
        "heads",
        "factions",
        "residual_blocks",
        "embed_dim",
        "source",
        "data_bytes",
        "sections",
    ];
    let object = header
        .as_object()
        .ok_or_else(|| format_error("the header is not an object"))?;
    if object.len() != KEYS.len() || !object.keys().all(|key| KEYS.contains(&key.as_str())) {
        return Err(format_error(format!(
            "the header keys {:?} are not those of version {VERSION}",
            object.keys().collect::<Vec<_>>()
        )));
    }

    let width = usize_field(&header, "width")?;
    if !width_ok(width) {
        return Err(InferError::Width(width));
    }
    let name = str_field(&header, "name")?.to_owned();
    if !name_ok(&name) {
        return Err(format_error(format!("name {name:?} is not a model name")));
    }
    let label = str_field(&header, "label")?.to_owned();
    if label.trim().is_empty() {
        return Err(format_error("the label is empty"));
    }
    #[expect(clippy::cast_possible_truncation, reason = "written from an f32")]
    let temperature = header
        .get("temperature")
        .and_then(Value::as_f64)
        .ok_or_else(|| format_error("the header has no temperature"))? as f32;
    if !temperature_ok(temperature) {
        return Err(format_error(format!(
            "temperature {temperature} is not finite and positive"
        )));
    }
    let capacity = usize_field(&header, "capacity")?;
    if capacity == 0 || capacity > MAX_CAPACITY {
        return Err(format_error(format!(
            "capacity {capacity} is outside 1..={MAX_CAPACITY}"
        )));
    }
    let heads = string_list(&header, "heads")?;
    if !is_head_layout(&heads) {
        return Err(format_error(format!(
            "heads {heads:?} are not a known layout"
        )));
    }
    if string_list(&header, "factions")? != FACTION_ROSTER {
        return Err(format_error(
            "the faction roster is not the one of this build",
        ));
    }
    let blocks = usize_field(&header, "residual_blocks")?;
    if blocks > MAX_RESIDUAL_BLOCKS {
        return Err(format_error(format!(
            "{blocks} residual blocks, at most {MAX_RESIDUAL_BLOCKS}"
        )));
    }
    if usize_field(&header, "embed_dim")? != EMBED_DIM {
        return Err(format_error("embed_dim is not the one of this build"));
    }
    let source = header
        .get("source")
        .ok_or_else(|| format_error("the header has no source"))?;
    let source = Source {
        manifest_sha256: str_field(source, "manifest_sha256")?.to_owned(),
        schema: u32::try_from(usize_field(source, "schema")?).unwrap_or(0),
        projection_abi: u32::try_from(usize_field(source, "projection_abi")?).unwrap_or(0),
    };
    if !schema_ok(source.schema) || !is_sha256_hex(&source.manifest_sha256) {
        return Err(format_error(format!(
            "source {source:?} is not a bundle of schema 7 to 12"
        )));
    }
    let battle = is_battle_schema(source.schema);

    // The sections: exactly the expected ones, in order, each at the next multiple of 64.
    let expected = expected_sections(width, capacity, heads.len(), blocks, battle);
    let table = header
        .get("sections")
        .and_then(Value::as_array)
        .ok_or_else(|| format_error("the header has no sections"))?;
    if table.len() != expected.len() {
        return Err(format_error(format!(
            "{} sections, a model of this shape has {}",
            table.len(),
            expected.len()
        )));
    }
    let mut cursor = 0;
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(table.len());
    for (entry, (name, layout, length)) in table.iter().zip(&expected) {
        let found_name = str_field(entry, "name")?;
        let found_layout = str_field(entry, "layout")?;
        let offset = usize_field(entry, "offset")?;
        let section_bytes = usize_field(entry, "bytes")?;
        if found_name != name || found_layout != layout.as_str() {
            return Err(format_error(format!(
                "section {found_name} ({found_layout}) where {name} ({}) belongs",
                layout.as_str()
            )));
        }
        if offset != cursor || length.is_some_and(|length| length != section_bytes) {
            return Err(format_error(format!(
                "section {name} at {offset} with {section_bytes} bytes, expected at {cursor}{}",
                length.map_or(String::new(), |length| format!(" with {length} bytes"))
            )));
        }
        cursor = (offset + section_bytes).next_multiple_of(ALIGN);
        if cursor > data.len() {
            return Err(format_error(format!("section {name} runs past the data")));
        }
        ranges.push(offset..offset + section_bytes);
    }
    if cursor != data.len() {
        return Err(format_error(format!(
            "the sections end at {cursor}, the data at {}",
            data.len()
        )));
    }

    let text = |range: &std::ops::Range<usize>, what: &str| {
        String::from_utf8(data[range.clone()].to_vec())
            .map_err(|_| format_error(format!("the {what} section is not UTF-8")))
    };
    let json_at = 5 + 4 * blocks + 2;
    let slots = text(&ranges[json_at], "slots")?;
    let battle = if battle {
        Some(text(&ranges[json_at + 1], "battle")?)
    } else {
        None
    };

    // Every offset is a multiple of 64 bytes from the aligned start, so a float range of a section
    // starts on a cache line of the buffer too.
    let floats = |range: &std::ops::Range<usize>| range.start / 4..range.end / 4;
    let mut next = ranges.iter().map(floats);
    let mut take = || next.next().unwrap_or_default();
    let (w1, embedding, b1, b2, w2) = (take(), take(), take(), take(), take());
    let blocks = (0..blocks)
        .map(|_| BlockRanges {
            w_in: take(),
            b_in: take(),
            w_out: take(),
            b_out: take(),
        })
        .collect();
    let (readout, readout_b) = (take(), take());

    Ok(Model {
        name,
        label,
        temperature,
        width,
        capacity,
        heads,
        source,
        digest: found,
        buffer: AlignedF32::from_le_bytes(data),
        w1,
        embedding,
        b1,
        b2,
        w2,
        blocks,
        readout,
        readout_b,
        slots,
        battle,
        backend: crate::Backend::best(),
    })
}
