//! Post-process a completed self-play corpus and fine-tune only the MLP actor on CUDA.
//!
//! `pack` refuses a corpus without `manifest.json`, preventing contention with a live capture.
//! It validates feature columns against the starting checkpoint, removes forced decisions, splits
//! by game, and writes compact ragged tensors. `train` takes a bounded, deterministic reservoir
//! sample (60% standout, 30% productive strong-table, 10% control) and uses one-hot BC targets.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use ti4_mlp::bundle::{Provenance, read, write};
use ti4_mlp::distill::{Sample, Settings, train as fit};
use ti4_mlp::{FactionRow, SparseOption};
use ti4_policy::progress::Progress;

const MAGIC: &[u8; 8] = b"TI4BC001";
const SCHEMA: &str = "ti4-offline-bc-v1";
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
fn files(root: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| format!("read {}: {e}", root.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".jsonl.zst"))
        {
            out.push(path);
        }
    }
    out.sort();
    if out.is_empty() {
        Err(format!("no {prefix}*.jsonl.zst in {}", root.display()))
    } else {
        Ok(out)
    }
}
fn lines<T: for<'a> Deserialize<'a>>(
    path: &Path,
    mut use_value: impl FnMut(T) -> Result<(), String>,
) -> Result<(), String> {
    let f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let z = zstd::Decoder::new(BufReader::new(f))
        .map_err(|e| format!("decode {}: {e}", path.display()))?;
    for (i, line) in BufReader::new(z).lines().enumerate() {
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
    for path in files(&source, "decisions")? {
        lines::<Decision>(&path, |d| {
            if d.legal_actions.len() <= 1 {
                return Ok(());
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
            let mut options = Vec::new();
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
            let policy_key = format!("{policy:016x}");
            if policies
                .insert(policy_key, d.policy_id.clone())
                .is_some_and(|old| old != d.policy_id)
            {
                return Err("policy hash collision".into());
            }
            let game = hash(&[d.game_id.as_bytes()]);
            let key = hash(&[
                d.game_id.as_bytes(),
                d.seat.as_bytes(),
                &d.seat_decision_index.to_le_bytes(),
            ]);
            let p = Packed {
                bucket: o.bucket,
                row,
                head,
                policy,
                game,
                key,
                chosen: d.chosen_action_index,
                options,
            };
            *buckets.entry(o.bucket.name().to_owned()).or_insert(0) += 1;
            let split = usize::from(game % 100 < u64::from(validation_percent));
            if split == 0 {
                write_record(&mut train, &p)?;
            } else {
                write_record(&mut valid, &p)?;
            }
            counts[split] += 1;
            Ok(())
        })?;
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
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| fail("usage: offline_bc <pack|train>"));
    let result = match mode.as_str() {
        "pack" => pack(),
        "train" => train(),
        _ => Err(format!("unknown mode {mode}")),
    };
    if let Err(e) = result {
        fail(&e);
    }
}
