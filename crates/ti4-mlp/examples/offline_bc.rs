//! Post-process a completed self-play corpus and fine-tune only the MLP actor on CUDA.
//!
//! `pack` refuses a corpus without `manifest.json`, preventing contention with a live capture.
//! It validates feature columns against the starting checkpoint, removes forced decisions, splits
//! by game, and writes compact ragged tensors. `train` takes a bounded, deterministic reservoir
//! sample (60% standout, 30% productive strong-table, 10% control) and uses one-hot BC targets.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use ti4_mlp::bundle::{Provenance, read, write};
use ti4_mlp::distill::{Sample, Settings, train as fit};
use ti4_mlp::{FactionRow, SparseOption};
use ti4_policy::progress::Progress;

const MAGIC: &[u8; 8] = b"TI4BC001";
/// The zstd magic number. Capture corpora are either plain JSONL (current capture) or a stream of
/// zstd frames (older corpora); both must stay readable.
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
const SCHEMA: &str = "ti4-offline-bc-v1";
const CAPTURE_SCHEMA: &str = "ti4-offline-selfplay-v1";
const OBSERVATION_SCHEMA: &str = "seat-authorized-canonical-mlp-v1";
const FACTIONS: [&str; 6] = ["jolnar", "letnev", "sol", "xxcha", "hacan", "l1z1x"];

#[derive(Deserialize)]
struct Game {
    game_id: String,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    error: Option<String>,
    seats: Vec<Seat>,
}
#[derive(Deserialize)]
struct Seat {
    seat: String,
    faction: String,
    policy: Policy,
    final_progress: Progress,
}
#[derive(Deserialize)]
struct Policy {
    policy_id: String,
}
#[derive(Deserialize)]
struct Decision {
    game_id: String,
    seat: String,
    faction: String,
    policy_id: String,
    seat_decision_index: u64,
    head: String,
    legal_actions: Vec<Action>,
    chosen_action_index: usize,
}
#[derive(Deserialize)]
struct Action {
    actor_features: Vec<Feature>,
}
#[derive(Deserialize)]
struct Feature {
    name: String,
    column: usize,
    value: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Bucket {
    Standout = 0,
    Strong = 1,
    Control = 2,
    Weak = 3,
}
impl Bucket {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            0 => Ok(Self::Standout),
            1 => Ok(Self::Strong),
            2 => Ok(Self::Control),
            3 => Ok(Self::Weak),
            _ => Err(format!("bad bucket {value}")),
        }
    }
    const fn name(self) -> &'static str {
        match self {
            Self::Standout => "standout",
            Self::Strong => "strong_productive",
            Self::Control => "control",
            Self::Weak => "weak",
        }
    }
}

struct Outcome {
    faction: String,
    policy: String,
    bucket: Bucket,
}
struct Packed {
    bucket: Bucket,
    row: FactionRow,
    head: usize,
    policy: u64,
    game: u64,
    key: u64,
    chosen: usize,
    options: Vec<SparseOption>,
}

#[derive(Serialize, Deserialize)]
struct FileInfo {
    file: String,
    decisions: u64,
    bytes: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    schema: String,
    source: String,
    source_manifest_sha256: String,
    slots_sha256: String,
    strong_table_min_vp: i64,
    weak_table_max_vp: i64,
    validation_percent: u8,
    policies: BTreeMap<String, String>,
    buckets: BTreeMap<String, u64>,
    train: FileInfo,
    validation: FileInfo,
}

#[derive(Deserialize)]
struct CaptureManifest {
    schema: String,
    observation_schema: String,
    games: usize,
    vocabulary_slots_sha256: String,
    records: BTreeMap<String, usize>,
    shards: BTreeMap<String, String>,
}

struct RawCorpus {
    root: PathBuf,
    games: PathBuf,
    decisions: PathBuf,
    frames: usize,
    games_sha256: String,
    decisions_sha256: String,
}

type CuratedSplit = (Vec<Packed>, Vec<Packed>);

fn fail(message: &str) -> ! {
    eprintln!("offline_bc: {message}");
    std::process::exit(2)
}
fn arg(name: &str) -> Option<String> {
    let mut a = std::env::args();
    while let Some(v) = a.next() {
        if v == name {
            return a.next();
        }
    }
    None
}
fn args(name: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut values = std::env::args();
    while let Some(value) = values.next() {
        if value == name
            && let Some(value) = values.next()
        {
            found.push(value);
        }
    }
    found
}
fn need(name: &str) -> String {
    arg(name).unwrap_or_else(|| fail(&format!("missing {name}")))
}
fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name)
        .map_or(Ok(default), |v| v.parse())
        .unwrap_or_else(|_| fail(&format!("invalid {name}")))
}

fn hash(parts: &[&[u8]]) -> u64 {
    let mut h = sha2::Sha256::new();
    for part in parts {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part);
    }
    u64::from_le_bytes(h.finalize()[..8].try_into().expect("8 bytes"))
}
fn digest(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut h = sha2::Sha256::new();
    let mut b = vec![0; 1024 * 1024];
    loop {
        let n = f
            .read(&mut b)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn bytes_digest(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn read_checked(path: &Path, expected_sha256: &str) -> Result<Vec<u8>, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
    let actual = bytes_digest(&bytes);
    if actual != expected_sha256 {
        return Err(format!(
            "checksum mismatch for {}: manifest {expected_sha256}, actual {actual}",
            path.display()
        ));
    }
    Ok(bytes)
}

fn validate_capture_manifest(
    manifest: &CaptureManifest,
    slots_sha256: &str,
    root: &Path,
) -> Result<(), String> {
    if manifest.schema != CAPTURE_SCHEMA {
        return Err(format!(
            "{} has capture schema {}, expected {CAPTURE_SCHEMA}",
            root.display(),
            manifest.schema
        ));
    }
    if manifest.observation_schema != OBSERVATION_SCHEMA {
        return Err(format!(
            "{} has observation schema {}, expected {OBSERVATION_SCHEMA}",
            root.display(),
            manifest.observation_schema
        ));
    }
    if manifest.vocabulary_slots_sha256 != slots_sha256 {
        return Err(format!(
            "{} vocabulary digest does not match the starting checkpoint",
            root.display()
        ));
    }
    if manifest.records.get("games") != Some(&manifest.games) {
        return Err(format!(
            "{} manifest games/records.games disagree",
            root.display()
        ));
    }
    Ok(())
}

fn manifest_hash(manifest: &CaptureManifest, path: &Path) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid shard name {}", path.display()))?;
    manifest
        .shards
        .get(name)
        .cloned()
        .ok_or_else(|| format!("manifest does not authenticate shard {name}"))
}

fn raw_corpus(root: PathBuf, slots_sha256: &str) -> Result<RawCorpus, String> {
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(format!(
            "{} has no manifest.json; refusing an active/incomplete corpus",
            root.display()
        ));
    }
    let manifest: CaptureManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|error| format!("reading {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("parsing {}: {error}", manifest_path.display()))?;
    validate_capture_manifest(&manifest, slots_sha256, &root)?;
    let games = shard(&root, "games")?;
    let decisions = shard(&root, "decisions")?;
    let games_sha256 = manifest_hash(&manifest, &games)?;
    let decisions_sha256 = manifest_hash(&manifest, &decisions)?;
    Ok(RawCorpus {
        root,
        games,
        decisions,
        frames: manifest.games,
        games_sha256,
        decisions_sha256,
    })
}
/// Capture writes `.jsonl` (current) or `.jsonl.zst` (older corpora). Both suffixes are exact
/// strings this pipeline produces, so the case-sensitive comparison is intentional.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn jsonl_shard_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".jsonl") || lower.ends_with(".jsonl.zst")
}

fn files(root: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| format!("read {}: {e}", root.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(|n| n.starts_with(prefix) && jsonl_shard_name(n))
        {
            out.push(path);
        }
    }
    out.sort();
    if out.is_empty() {
        Err(format!("no {prefix}*.jsonl[.zst] in {}", root.display()))
    } else {
        Ok(out)
    }
}
fn is_zstd(path: &Path) -> Result<bool, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut head = [0u8; 4];
    f.read_exact(&mut head)
        .map_err(|e| format!("reading {} header: {e}", path.display()))?;
    Ok(head == ZSTD_MAGIC)
}

/// A published corpus stores each shard as plain JSONL (current capture) or a zstd frame stream
/// (older corpora); resolve whichever exists.
fn shard(corpus: &Path, name: &str) -> Result<PathBuf, String> {
    let zst = corpus.join(format!("{name}.jsonl.zst"));
    if zst.is_file() {
        return Ok(zst);
    }
    let plain = corpus.join(format!("{name}.jsonl"));
    if plain.is_file() {
        return Ok(plain);
    }
    Err(format!(
        "no {name}.jsonl or {name}.jsonl.zst in {}",
        corpus.display()
    ))
}

/// Split a JSONL byte buffer into at most `workers` chunks that never cut through a line, so the
/// parse/compile step keeps one worker per logical processor on plain corpora too. Newline bytes
/// cannot occur inside a multi-byte UTF-8 sequence, so chunk boundaries stay valid UTF-8.
fn line_chunks(bytes: &[u8], workers: usize) -> Vec<std::ops::Range<usize>> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let target = bytes.len().div_ceil(workers.max(1)).max(1);
    let mut starts = vec![0usize];
    let mut next = 0usize;
    while next < bytes.len() {
        let from = (next + target).min(bytes.len());
        if from >= bytes.len() {
            break;
        }
        match bytes[from..].iter().position(|&b| b == b'\n') {
            Some(offset) => {
                next = from + offset + 1;
                if next < bytes.len() {
                    starts.push(next);
                }
            }
            None => break,
        }
    }
    let mut ranges = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(bytes.len());
        if end > *start {
            ranges.push(*start..end);
        }
    }
    ranges
}

fn lines<T: for<'a> Deserialize<'a>>(
    path: &Path,
    mut use_value: impl FnMut(T) -> Result<(), String>,
) -> Result<(), String> {
    let f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    // Plain JSONL and zstd-compressed JSONL are both valid capture output; sniff the magic.
    let reader: Box<dyn BufRead> = if is_zstd(path)? {
        let z = zstd::Decoder::new(BufReader::new(f))
            .map_err(|e| format!("decode {}: {e}", path.display()))?;
        Box::new(BufReader::new(z))
    } else {
        Box::new(BufReader::new(f))
    };
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?;
        if !line.trim().is_empty() {
            use_value(
                serde_json::from_str(&line)
                    .map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?,
            )?;
        }
    }
    Ok(())
}

fn put(out: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    out.write_all(bytes)
        .map_err(|e| format!("write packed data: {e}"))
}
fn write_record(out: &mut impl Write, s: &Packed) -> Result<(), String> {
    put(
        out,
        &[
            s.bucket as u8,
            u8::try_from(s.row.index()).map_err(|_| "row > u8")?,
            u8::try_from(s.head).map_err(|_| "head > u8")?,
            0,
        ],
    )?;
    for value in [s.policy, s.game, s.key] {
        put(out, &value.to_le_bytes())?;
    }
    put(
        out,
        &u32::try_from(s.chosen)
            .map_err(|_| "choice > u32")?
            .to_le_bytes(),
    )?;
    put(
        out,
        &u32::try_from(s.options.len())
            .map_err(|_| "options > u32")?
            .to_le_bytes(),
    )?;
    for option in &s.options {
        put(
            out,
            &u32::try_from(option.columns.len())
                .map_err(|_| "features > u32")?
                .to_le_bytes(),
        )?;
        for (&column, &value) in option.columns.iter().zip(&option.values) {
            put(
                out,
                &u32::try_from(column)
                    .map_err(|_| "column > u32")?
                    .to_le_bytes(),
            )?;
            put(out, &value.to_le_bytes())?;
        }
    }
    Ok(())
}
fn exact<const N: usize>(input: &mut impl Read) -> Result<[u8; N], String> {
    let mut b = [0; N];
    input
        .read_exact(&mut b)
        .map_err(|e| format!("truncated packed data: {e}"))?;
    Ok(b)
}
fn byte(input: &mut impl Read) -> Result<Option<u8>, String> {
    let mut b = [0];
    match input.read(&mut b).map_err(|e| e.to_string())? {
        0 => Ok(None),
        1 => Ok(Some(b[0])),
        _ => unreachable!(),
    }
}
fn read_record(input: &mut impl Read) -> Result<Option<Packed>, String> {
    let Some(bucket) = byte(input)? else {
        return Ok(None);
    };
    let row = byte(input)?.ok_or("missing row")?;
    let head = byte(input)?.ok_or("missing head")?;
    byte(input)?.ok_or("missing reserved")?;
    let policy = u64::from_le_bytes(exact(input)?);
    let game = u64::from_le_bytes(exact(input)?);
    let key = u64::from_le_bytes(exact(input)?);
    let chosen = u32::from_le_bytes(exact(input)?) as usize;
    let n = u32::from_le_bytes(exact(input)?) as usize;
    if !(2..=65_536).contains(&n) {
        return Err(format!("bad option count {n}"));
    }
    let mut options = Vec::with_capacity(n);
    for _ in 0..n {
        let m = u32::from_le_bytes(exact(input)?) as usize;
        if m > 65_536 {
            return Err("too many features".into());
        }
        let mut o = SparseOption::default();
        for _ in 0..m {
            o.columns.push(i64::from(u32::from_le_bytes(exact(input)?)));
            o.values.push(f32::from_le_bytes(exact(input)?));
        }
        options.push(o);
    }
    if chosen >= n {
        return Err("choice outside legal set".into());
    }
    let faction = *ti4_mlp::FACTION_ROSTER
        .get(row as usize)
        .ok_or("bad faction row")?;
    Ok(Some(Packed {
        bucket: Bucket::parse(bucket)?,
        row: FactionRow::of(faction).map_err(|e| e.to_string())?,
        head: head as usize,
        policy,
        game,
        key,
        chosen,
        options,
    }))
}

fn finish_file(
    encoder: zstd::Encoder<'static, BufWriter<std::fs::File>>,
    path: &Path,
    decisions: u64,
) -> Result<FileInfo, String> {
    let mut w = encoder.finish().map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(FileInfo {
        file: path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or("bad output name")?
            .to_owned(),
        decisions,
        bytes: std::fs::metadata(path).map_err(|e| e.to_string())?.len(),
        sha256: digest(path)?,
    })
}

fn compile_decision(
    d: Decision,
    outcomes: &HashMap<(String, String), Outcome>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
) -> Result<Option<(Packed, String)>, String> {
    if d.legal_actions.len() <= 1 {
        return Ok(None);
    }
    let o = outcomes
        .get(&(d.game_id.clone(), d.seat.clone()))
        .ok_or_else(|| format!("missing outcome for {}/{}", d.game_id, d.seat))?;
    if o.faction != d.faction
        || o.policy != d.policy_id
        || d.chosen_action_index >= d.legal_actions.len()
    {
        return Err(format!("metadata/choice mismatch in {}", d.game_id));
    }
    let row = FactionRow::of(&d.faction).map_err(|e| e.to_string())?;
    let head = ti4_mlp::heads()
        .iter()
        .position(|x| *x == d.head)
        .ok_or_else(|| format!("unknown head {}", d.head))?;
    let mut options = Vec::with_capacity(d.legal_actions.len());
    for action in d.legal_actions {
        let mut sparse = SparseOption::default();
        for f in action.actor_features {
            let expected = vocabulary.column_of(&f.name);
            if expected != f.column || !f.value.is_finite() {
                return Err(format!("invalid feature {} column/value", f.name));
            }
            sparse.columns.push(f.column as i64);
            sparse.values.push(f.value as f32);
        }
        options.push(sparse);
    }
    let policy = hash(&[d.policy_id.as_bytes()]);
    let game = hash(&[d.game_id.as_bytes()]);
    let key = hash(&[
        d.game_id.as_bytes(),
        d.seat.as_bytes(),
        &d.seat_decision_index.to_le_bytes(),
    ]);
    Ok(Some((
        Packed {
            bucket: o.bucket,
            row,
            head,
            policy,
            game,
            key,
            chosen: d.chosen_action_index,
            options,
        },
        d.policy_id,
    )))
}

fn compile_batch(
    lines: Vec<String>,
    outcomes: &HashMap<(String, String), Outcome>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
) -> Result<Vec<Option<(Packed, String)>>, String> {
    lines
        .into_par_iter()
        .map(|line| {
            let decision: Decision = serde_json::from_str(&line)
                .map_err(|error| format!("parsing decision JSON: {error}"))?;
            compile_decision(decision, outcomes, vocabulary)
        })
        .collect()
}

/// Parse and compile one chunk of decision lines with the same curation as the zstd path: keep
/// standout/strong decisions, subsample control by `control_per_million` per million, and split
/// train/validation by game id.
fn compile_chunk(
    text: &str,
    outcomes: &HashMap<(String, String), Outcome>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    control_per_million: u64,
) -> Result<(Vec<Packed>, Vec<Packed>), String> {
    let mut train = Vec::new();
    let mut validation = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let decision: Decision = serde_json::from_str(line).map_err(|error| error.to_string())?;
        let Some(packed) = compile_decision(decision, outcomes, vocabulary)?.map(|pair| pair.0)
        else {
            continue;
        };
        let useful = matches!(packed.bucket, Bucket::Standout | Bucket::Strong)
            || (packed.bucket == Bucket::Control && packed.key % 1_000_000 < control_per_million);
        if !useful {
            continue;
        }
        if packed.game % 100 < 10 {
            validation.push(packed);
        } else {
            train.push(packed);
        }
    }
    Ok((train, validation))
}

fn frame_ranges(bytes: &[u8], expected: usize) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut starts: Vec<usize> = (0..bytes.len().saturating_sub(3))
        .into_par_iter()
        .filter(|&index| bytes[index..index + 4] == ZSTD_MAGIC)
        .collect();
    starts.sort_unstable();
    let mut ranges: Vec<std::ops::Range<usize>> = starts
        .par_iter()
        .filter_map(|&start| {
            let size = zstd::zstd_safe::find_frame_compressed_size(&bytes[start..]).ok()?;
            let end = start.checked_add(size)?;
            (end == bytes.len() || starts.binary_search(&end).is_ok()).then_some(start..end)
        })
        .collect();
    ranges.sort_by_key(|range| range.start);
    if ranges.first().map(|range| range.start) != Some(0) || ranges.len() != expected {
        return Err(format!(
            "expected {expected} valid independent zstd frames, found {} from {} magic candidates",
            ranges.len(),
            starts.len()
        ));
    }
    Ok(ranges)
}

fn decode_frames<T: for<'a> Deserialize<'a> + Send>(
    bytes: &[u8],
    label: &Path,
    expected: usize,
) -> Result<Vec<T>, String> {
    let ranges = frame_ranges(bytes, expected)?;
    let decoded: Result<Vec<Vec<T>>, String> = ranges
        .par_iter()
        .map(|range| {
            let plain = zstd::decode_all(std::io::Cursor::new(&bytes[range.clone()]))
                .map_err(|error| format!("decoding {} frame: {error}", label.display()))?;
            String::from_utf8(plain)
                .map_err(|error| format!("{} frame is not UTF-8: {error}", label.display()))?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    serde_json::from_str(line)
                        .map_err(|error| format!("parsing {} frame: {error}", label.display()))
                })
                .collect()
        })
        .collect();
    Ok(decoded?.into_iter().flatten().collect())
}

fn classify_games(
    games: Vec<Game>,
    strong: i64,
    weak: i64,
) -> Result<HashMap<(String, String), Outcome>, String> {
    let mut outcomes = HashMap::new();
    for game in games {
        if !game.completed || game.error.is_some() {
            continue;
        }
        if game.seats.len() != 6 {
            return Err(format!("{} is not six-player", game.game_id));
        }
        let total: i64 = game
            .seats
            .iter()
            .map(|seat| seat.final_progress.victory_points)
            .sum();
        let mut ranked: Vec<usize> = (0..6).collect();
        ranked.sort_by_key(|&index| {
            let p = game.seats[index].final_progress;
            std::cmp::Reverse((
                p.victory_points,
                p.scoreable_public + p.scoreable_secret,
                p.planets_gained + p.systems,
                p.units_gained,
            ))
        });
        for (index, seat) in game.seats.into_iter().enumerate() {
            if !FACTIONS.contains(&seat.faction.as_str()) {
                return Err(format!("out-of-scope faction {}", seat.faction));
            }
            let bucket = if seat.final_progress.victory_points >= 6 {
                Bucket::Standout
            } else if total >= strong && ranked[..3].contains(&index) {
                Bucket::Strong
            } else if total <= weak {
                Bucket::Weak
            } else {
                Bucket::Control
            };
            let key = (game.game_id.clone(), seat.seat);
            if outcomes
                .insert(
                    key,
                    Outcome {
                        faction: seat.faction,
                        policy: seat.policy.policy_id,
                        bucket,
                    },
                )
                .is_some()
            {
                return Err(format!("duplicate game/seat outcome in {}", game.game_id));
            }
        }
    }
    Ok(outcomes)
}

fn to_sample(packed: Packed) -> Sample {
    let mut teacher = vec![0.0; packed.options.len()];
    teacher[packed.chosen] = 1.0;
    Sample {
        row: packed.row,
        head: packed.head,
        options: packed.options,
        teacher,
    }
}

fn plain_games(bytes: &[u8], path: &Path, expected: usize) -> Result<Vec<Game>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
    let games: Result<Vec<Game>, _> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect();
    let games = games.map_err(|error| format!("parsing {}: {error}", path.display()))?;
    if games.len() != expected {
        return Err(format!(
            "expected {expected} game records in {}, found {}",
            path.display(),
            games.len()
        ));
    }
    Ok(games)
}

fn print_composition(label: &str, packed: &[Packed]) {
    let mut buckets = BTreeMap::new();
    let mut factions = BTreeMap::new();
    let mut heads = BTreeMap::new();
    let mut policies = BTreeMap::new();
    for item in packed {
        *buckets.entry(item.bucket.name()).or_insert(0_usize) += 1;
        *factions
            .entry(ti4_mlp::FACTION_ROSTER[item.row.index()])
            .or_insert(0_usize) += 1;
        *heads.entry(ti4_mlp::heads()[item.head]).or_insert(0_usize) += 1;
        *policies
            .entry(format!("{:016x}", item.policy))
            .or_insert(0_usize) += 1;
    }
    println!(
        "curation {label} buckets {}",
        serde_json::to_string(&buckets).expect("composition serializes")
    );
    println!(
        "curation {label} factions {}",
        serde_json::to_string(&factions).expect("composition serializes")
    );
    println!(
        "curation {label} heads {}",
        serde_json::to_string(&heads).expect("composition serializes")
    );
    println!(
        "curation {label} policy_hashes {}",
        serde_json::to_string(&policies).expect("composition serializes")
    );
}

fn load_games(corpus: &RawCorpus) -> Result<Vec<Game>, String> {
    println!("loading and authenticating {}", corpus.games.display());
    let bytes = read_checked(&corpus.games, &corpus.games_sha256)?;
    if bytes.starts_with(&ZSTD_MAGIC) {
        decode_frames(&bytes, &corpus.games, corpus.frames)
    } else {
        plain_games(&bytes, &corpus.games, corpus.frames)
    }
}

fn compile_corpus(
    corpus: &RawCorpus,
    outcomes: &HashMap<(String, String), Outcome>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    control_per_million: u64,
) -> Result<CuratedSplit, String> {
    println!("loading and authenticating {}", corpus.decisions.display());
    let bytes = read_checked(&corpus.decisions, &corpus.decisions_sha256)?;
    let finished = AtomicUsize::new(0);
    let parts: Result<Vec<CuratedSplit>, String> = if bytes.starts_with(&ZSTD_MAGIC) {
        let ranges = frame_ranges(&bytes, corpus.frames)?;
        println!(
            "parallel-decoding {} decision frames from {}",
            ranges.len(),
            corpus.root.display()
        );
        ranges
            .par_iter()
            .map(|range| {
                let plain = zstd::decode_all(std::io::Cursor::new(&bytes[range.clone()]))
                    .map_err(|error| error.to_string())?;
                let text = String::from_utf8(plain).map_err(|error| error.to_string())?;
                let part = compile_chunk(&text, outcomes, vocabulary, control_per_million)?;
                let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
                if done.is_multiple_of(256) || done == corpus.frames {
                    println!("decoded {done}/{} frames", corpus.frames);
                    let _ = std::io::stdout().flush();
                }
                Ok(part)
            })
            .collect()
    } else {
        let chunks = line_chunks(&bytes, rayon::current_num_threads());
        println!(
            "parallel-parsing {} decision chunks from {}",
            chunks.len(),
            corpus.root.display()
        );
        chunks
            .par_iter()
            .map(|range| {
                let text = std::str::from_utf8(&bytes[range.clone()]).map_err(|error| {
                    format!("{} is not UTF-8: {error}", corpus.decisions.display())
                })?;
                compile_chunk(text, outcomes, vocabulary, control_per_million)
            })
            .collect()
    };
    let mut train = Vec::new();
    let mut validation = Vec::new();
    for (part_train, part_validation) in parts? {
        train.extend(part_train);
        validation.extend(part_validation);
    }
    println!("finished {}", corpus.root.display());
    Ok((train, validation))
}

#[expect(
    clippy::too_many_lines,
    reason = "one orchestration path keeps manifest validation, curation, CUDA handoff and atomic save in execution order"
)]
fn train_raw_parallel() -> Result<(), String> {
    let corpus_roots: Vec<PathBuf> = args("--corpus").into_iter().map(PathBuf::from).collect();
    if corpus_roots.is_empty() {
        return Err("missing --corpus (may be repeated)".into());
    }
    let checkpoint = PathBuf::from(need("--checkpoint"));
    let output = PathBuf::from(need("--out"));
    if output.exists() {
        return Err("output exists".into());
    }
    let strong = num("--strong-table-min-vp", 24_i64);
    let weak = num("--weak-table-max-vp", 9_i64);
    let control_per_million = num("--control-per-million", 30_220_u64);
    let slots = std::fs::read_to_string(checkpoint.join("slots.json"))
        .map_err(|error| error.to_string())?;
    let slots_sha256 = bytes_digest(slots.as_bytes());
    let corpora: Result<Vec<RawCorpus>, String> = corpus_roots
        .into_iter()
        .map(|root| raw_corpus(root, &slots_sha256))
        .collect();
    let corpora = corpora?;
    let total_frames: usize = corpora.iter().map(|corpus| corpus.frames).sum();
    if total_frames == 0 {
        return Err("input corpora contain no games".into());
    }
    println!(
        "validated {} corpus manifests / {total_frames} game frames",
        corpora.len()
    );
    for corpus in &corpora {
        println!("  {}: {} frames", corpus.root.display(), corpus.frames);
    }
    let loaded = read(&checkpoint).map_err(|error| error.to_string())?;
    let mut games = Vec::with_capacity(total_frames);
    for corpus in &corpora {
        if corpus.frames == 0 {
            continue;
        }
        games.extend(load_games(corpus)?);
    }
    let outcomes = classify_games(games, strong, weak)?;

    let mut train_samples = Vec::new();
    let mut validation_samples = Vec::new();
    for corpus in &corpora {
        if corpus.frames == 0 {
            continue;
        }
        let (train, validation) =
            compile_corpus(corpus, &outcomes, &loaded.vocabulary, control_per_million)?;
        train_samples.extend(train);
        validation_samples.extend(validation);
    }
    println!(
        "curated {} train and {} validation decisions",
        train_samples.len(),
        validation_samples.len()
    );
    if train_samples.is_empty() || validation_samples.is_empty() {
        return Err("curation produced an empty train or validation split".into());
    }
    print_composition("train", &train_samples);
    print_composition("validation", &validation_samples);
    let train_samples: Vec<Sample> = train_samples.into_iter().map(to_sample).collect();
    let validation_samples: Vec<Sample> = validation_samples.into_iter().map(to_sample).collect();
    let device = ti4_tensor::OptimizerDevice::Cuda
        .resolve()
        .map_err(|error| format!("CUDA required: {error}"))?;
    let critic_mode = loaded.critic_mode;
    let mut actor = loaded.actor.to_device(device);
    let settings = Settings {
        learning_rate: num("--learning-rate", 3e-5),
        batch: num("--batch", 4_096),
        micro_batch: num("--micro-batch", 512),
        max_epochs: num("--epochs", 5),
        preserve_untrained_rows: true,
        ..Settings::default()
    };
    let result = fit(
        &mut actor,
        &train_samples,
        &validation_samples,
        settings,
        |epoch| {
            println!(
                "epoch {} train NLL {:.5} validation NLL {:.5} steps {}",
                epoch.number, epoch.train_kl, epoch.validation_kl, epoch.steps
            );
            let _ = std::io::stdout().flush();
        },
    )?;
    if result.parameter_movement <= 0.0 {
        return Err("parameters did not move".into());
    }
    let steps = result
        .epochs
        .iter()
        .find(|epoch| epoch.number == result.selected)
        .map_or(0, |epoch| epoch.steps);
    let actor = actor.to_device(ti4_tensor::Device::Cpu);
    let saved = write(
        &output,
        &actor,
        &slots,
        critic_mode,
        &Provenance {
            source: format!(
                "parallel raw offline BC {}",
                corpora
                    .iter()
                    .map(|corpus| corpus.root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" + ")
            ),
            git_commit: need("--git-commit"),
            update: steps as u64,
        },
    )
    .map_err(|error| error.to_string())?;
    println!(
        "selected epoch {}; wrote {}",
        result.selected,
        saved.directory.display()
    );
    Ok(())
}

fn pack() -> Result<(), String> {
    let source = PathBuf::from(need("--corpus"));
    let output = PathBuf::from(need("--out"));
    let checkpoint = PathBuf::from(need("--checkpoint"));
    let strong = num("--strong-table-min-vp", 24_i64);
    let weak = num("--weak-table-max-vp", 9_i64);
    let validation_percent = num("--validation-percent", 10_u8);
    if output.exists() || !(1..50).contains(&validation_percent) {
        return Err("output exists or split is invalid".into());
    }
    let source_manifest = source.join("manifest.json");
    if !source_manifest.is_file() {
        return Err("source has no manifest.json; refusing an active/incomplete corpus".into());
    }
    let slots =
        std::fs::read_to_string(checkpoint.join("slots.json")).map_err(|e| e.to_string())?;
    let slots_sha256 = format!("{:x}", sha2::Sha256::digest(slots.as_bytes()));
    let vocabulary = read(&checkpoint).map_err(|e| e.to_string())?.vocabulary;
    let mut outcomes = HashMap::new();
    for path in files(&source, "games")? {
        lines::<Game>(&path, |game| {
            if !game.completed || game.error.is_some() {
                return Ok(());
            }
            if game.seats.len() != 6 {
                return Err(format!("{} is not six-player", game.game_id));
            }
            let total: i64 = game
                .seats
                .iter()
                .map(|s| s.final_progress.victory_points)
                .sum();
            let mut ranked: Vec<usize> = (0..6).collect();
            ranked.sort_by_key(|&i| {
                let p = game.seats[i].final_progress;
                std::cmp::Reverse((
                    p.victory_points,
                    p.scoreable_public + p.scoreable_secret,
                    p.planets_gained + p.systems,
                    p.units_gained,
                ))
            });
            for (i, seat) in game.seats.into_iter().enumerate() {
                if !FACTIONS.contains(&seat.faction.as_str()) {
                    return Err(format!("out-of-scope faction {}", seat.faction));
                }
                let bucket = if seat.final_progress.victory_points >= 6 {
                    Bucket::Standout
                } else if total >= strong && ranked[..3].contains(&i) {
                    Bucket::Strong
                } else if total <= weak {
                    Bucket::Weak
                } else {
                    Bucket::Control
                };
                outcomes.insert(
                    (game.game_id.clone(), seat.seat),
                    Outcome {
                        faction: seat.faction,
                        policy: seat.policy.policy_id,
                        bucket,
                    },
                );
            }
            Ok(())
        })?;
    }
    let staging = output.with_extension(format!("staging-{}", std::process::id()));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let train_path = staging.join("train.ti4bc.zst");
    let valid_path = staging.join("validation.ti4bc.zst");
    let mut train = zstd::Encoder::new(
        BufWriter::new(std::fs::File::create(&train_path).map_err(|e| e.to_string())?),
        3,
    )
    .map_err(|e| e.to_string())?;
    let mut valid = zstd::Encoder::new(
        BufWriter::new(std::fs::File::create(&valid_path).map_err(|e| e.to_string())?),
        3,
    )
    .map_err(|e| e.to_string())?;
    put(&mut train, MAGIC)?;
    put(&mut valid, MAGIC)?;
    let mut counts = [0_u64; 2];
    let mut buckets = BTreeMap::new();
    let mut policies = BTreeMap::new();
    let batch_size = num("--parse-batch", 4_096_usize);
    for path in files(&source, "decisions")? {
        let file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
        // Plain JSONL (current capture) and zstd frame streams (older corpora) are both valid.
        let reader: Box<dyn BufRead> = if is_zstd(&path)? {
            let decoder =
                zstd::Decoder::new(BufReader::new(file)).map_err(|error| error.to_string())?;
            Box::new(BufReader::new(decoder))
        } else {
            Box::new(BufReader::new(file))
        };
        let mut batch = Vec::with_capacity(batch_size);
        let mut parsed = 0_u64;
        for line in reader.lines() {
            batch.push(line.map_err(|error| error.to_string())?);
            if batch.len() < batch_size {
                continue;
            }
            for item in compile_batch(std::mem::take(&mut batch), &outcomes, &vocabulary)? {
                let Some((p, policy_id)) = item else { continue };
                let policy_key = format!("{:016x}", p.policy);
                if policies
                    .insert(policy_key, policy_id.clone())
                    .is_some_and(|old| old != policy_id)
                {
                    return Err("policy hash collision".into());
                }
                *buckets.entry(p.bucket.name().to_owned()).or_insert(0) += 1;
                let split = usize::from(p.game % 100 < u64::from(validation_percent));
                if split == 0 {
                    write_record(&mut train, &p)?;
                } else {
                    write_record(&mut valid, &p)?;
                }
                counts[split] += 1;
            }
            batch = Vec::with_capacity(batch_size);
            parsed += batch_size as u64;
            if parsed % 262_144 == 0 {
                println!("packed {parsed} decisions");
                let _ = std::io::stdout().flush();
            }
        }
        for item in compile_batch(batch, &outcomes, &vocabulary)? {
            let Some((p, policy_id)) = item else { continue };
            policies.insert(format!("{:016x}", p.policy), policy_id);
            *buckets.entry(p.bucket.name().to_owned()).or_insert(0) += 1;
            let split = usize::from(p.game % 100 < u64::from(validation_percent));
            if split == 0 {
                write_record(&mut train, &p)?;
            } else {
                write_record(&mut valid, &p)?;
            }
            counts[split] += 1;
        }
    }
    let train = finish_file(train, &train_path, counts[0])?;
    let validation = finish_file(valid, &valid_path, counts[1])?;
    let manifest = Manifest {
        schema: SCHEMA.into(),
        source: source.display().to_string(),
        source_manifest_sha256: digest(&source_manifest)?,
        slots_sha256,
        strong_table_min_vp: strong,
        weak_table_max_vp: weak,
        validation_percent,
        policies,
        buckets,
        train,
        validation,
    };
    std::fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(staging, &output).map_err(|e| e.to_string())?;
    println!("published {}", output.display());
    Ok(())
}

struct Reservoir {
    target: usize,
    seen: u64,
    rng: rand_chacha::ChaCha8Rng,
    values: Vec<Packed>,
}
impl Reservoir {
    fn new(target: usize, seed: u64) -> Self {
        Self {
            target,
            seen: 0,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(seed),
            values: Vec::with_capacity(target),
        }
    }
    fn add(&mut self, value: Packed) {
        self.seen += 1;
        if self.values.len() < self.target {
            self.values.push(value);
        } else if self.target > 0 {
            let i = self.rng.random_range(0..self.seen);
            if i < self.target as u64 {
                self.values[i as usize] = value;
            }
        }
    }
}
fn load_samples(root: &Path, info: &FileInfo, max: usize) -> Result<Vec<Sample>, String> {
    let path = root.join(&info.file);
    if digest(&path)? != info.sha256 {
        return Err(format!("checksum mismatch: {}", path.display()));
    }
    let f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut input = zstd::Decoder::new(BufReader::new(f)).map_err(|e| e.to_string())?;
    if &exact::<8>(&mut input)? != MAGIC {
        return Err("bad packed magic".into());
    }
    let mut r = [
        Reservoir::new(max * 60 / 100, 0x51a0),
        Reservoir::new(max * 30 / 100, 0x57a0),
        Reservoir::new(max - max * 90 / 100, 0xc017),
    ];
    while let Some(p) = read_record(&mut input)? {
        match p.bucket {
            Bucket::Standout => r[0].add(p),
            Bucket::Strong => r[1].add(p),
            Bucket::Control => r[2].add(p),
            Bucket::Weak => {}
        }
    }
    let mut packed = Vec::new();
    for (i, mut x) in r.into_iter().enumerate() {
        println!("bucket {i}: selected {} / {}", x.values.len(), x.seen);
        packed.append(&mut x.values);
    }
    packed.sort_by_key(|x| x.key);
    Ok(packed
        .into_iter()
        .map(|p| {
            let mut teacher = vec![0.0; p.options.len()];
            teacher[p.chosen] = 1.0;
            Sample {
                row: p.row,
                head: p.head,
                options: p.options,
                teacher,
            }
        })
        .collect())
}

fn train() -> Result<(), String> {
    let corpus = PathBuf::from(need("--packed"));
    let checkpoint = PathBuf::from(need("--checkpoint"));
    let output = PathBuf::from(need("--out"));
    if output.exists() {
        return Err("output exists".into());
    }
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(corpus.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest.schema != SCHEMA {
        return Err("unsupported packed schema".into());
    }
    let slots =
        std::fs::read_to_string(checkpoint.join("slots.json")).map_err(|e| e.to_string())?;
    if format!("{:x}", sha2::Sha256::digest(slots.as_bytes())) != manifest.slots_sha256 {
        return Err("vocabulary digest mismatch".into());
    }
    let loaded = read(&checkpoint).map_err(|e| e.to_string())?;
    let critic_mode = loaded.critic_mode;
    let train_samples = load_samples(&corpus, &manifest.train, num("--max-train", 100_000))?;
    let validation = load_samples(
        &corpus,
        &manifest.validation,
        num("--max-validation", 20_000),
    )?;
    let device = ti4_tensor::OptimizerDevice::Cuda
        .resolve()
        .map_err(|e| format!("CUDA required: {e}"))?;
    let mut actor = loaded.actor.to_device(device);
    let settings = Settings {
        learning_rate: num("--learning-rate", 3e-5),
        batch: num("--batch", 4096),
        micro_batch: num("--micro-batch", 512),
        max_epochs: num("--epochs", 5),
        preserve_untrained_rows: true,
        ..Settings::default()
    };
    let result = fit(&mut actor, &train_samples, &validation, settings, |e| {
        println!(
            "epoch {} train NLL {:.5} validation NLL {:.5} steps {}",
            e.number, e.train_kl, e.validation_kl, e.steps
        );
        let _ = std::io::stdout().flush();
    })?;
    if result.parameter_movement <= 0.0 {
        return Err("parameters did not move".into());
    }
    let steps = result
        .epochs
        .iter()
        .find(|e| e.number == result.selected)
        .map_or(0, |e| e.steps);
    let actor = actor.to_device(ti4_tensor::Device::Cpu);
    let saved = write(
        &output,
        &actor,
        &slots,
        critic_mode,
        &Provenance {
            source: format!("offline BC {}", corpus.display()),
            git_commit: need("--git-commit"),
            update: steps as u64,
        },
    )
    .map_err(|e| e.to_string())?;
    println!(
        "selected epoch {}; wrote {}",
        result.selected,
        saved.directory.display()
    );
    Ok(())
}

fn main() {
    ti4_tensor::configure_deterministic(1_026_091_300)
        .unwrap_or_else(|error| fail(&format!("configuring tensor backend: {error}")));
    let workers = num(
        "--workers",
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    );
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .unwrap_or_else(|error| fail(&format!("configuring {workers} parse workers: {error}")));
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| fail("usage: offline_bc <pack|train>"));
    let result = match mode.as_str() {
        "pack" => pack(),
        "train" => train(),
        "train-raw-parallel" => train_raw_parallel(),
        _ => Err(format!("unknown mode {mode}")),
    };
    if let Err(e) = result {
        fail(&e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_manifest() -> CaptureManifest {
        CaptureManifest {
            schema: CAPTURE_SCHEMA.to_owned(),
            observation_schema: OBSERVATION_SCHEMA.to_owned(),
            games: 2,
            vocabulary_slots_sha256: "slots".to_owned(),
            records: BTreeMap::from([("games".to_owned(), 2)]),
            shards: BTreeMap::from([
                ("games.jsonl.zst".to_owned(), "games-sha".to_owned()),
                ("decisions.jsonl.zst".to_owned(), "decisions-sha".to_owned()),
            ]),
        }
    }

    #[test]
    fn capture_manifest_accepts_matching_training_contract() {
        assert!(
            validate_capture_manifest(&capture_manifest(), "slots", Path::new("corpus")).is_ok()
        );
    }

    #[test]
    fn capture_manifest_rejects_schema_drift() {
        let mut manifest = capture_manifest();
        manifest.schema = "future-capture".to_owned();
        assert!(
            validate_capture_manifest(&manifest, "slots", Path::new("corpus"))
                .unwrap_err()
                .contains("capture schema")
        );
    }

    #[test]
    fn capture_manifest_rejects_observation_or_vocabulary_drift() {
        let mut manifest = capture_manifest();
        manifest.observation_schema = "different-observations".to_owned();
        assert!(
            validate_capture_manifest(&manifest, "slots", Path::new("corpus"))
                .unwrap_err()
                .contains("observation schema")
        );
        let manifest = capture_manifest();
        assert!(
            validate_capture_manifest(&manifest, "different-slots", Path::new("corpus"))
                .unwrap_err()
                .contains("vocabulary digest")
        );
    }

    #[test]
    fn capture_manifest_rejects_game_count_disagreement() {
        let mut manifest = capture_manifest();
        manifest.records.insert("games".to_owned(), 1);
        assert!(
            validate_capture_manifest(&manifest, "slots", Path::new("corpus"))
                .unwrap_err()
                .contains("games/records.games disagree")
        );
    }

    #[test]
    fn manifest_only_authenticates_declared_shards() {
        let manifest = capture_manifest();
        assert_eq!(
            manifest_hash(&manifest, Path::new("games.jsonl.zst")).unwrap(),
            "games-sha"
        );
        assert!(
            manifest_hash(&manifest, Path::new("undeclared.jsonl.zst"))
                .unwrap_err()
                .contains("does not authenticate")
        );
    }
}
