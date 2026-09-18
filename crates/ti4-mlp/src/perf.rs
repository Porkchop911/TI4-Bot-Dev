//! Opt-in training-performance diagnostics (`plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md`).
//!
//! Nothing here runs unless a driver calls [`enable`]. Disabled, each hook costs one relaxed atomic
//! load and never reads a clock. No hook changes what is computed: clocks are read between pieces
//! of work, never inside them, and the optional CUDA synchronisation only waits for work already
//! queued, so the kernels, their order and their results are unchanged.
//!
//! Also here: a binary capture of a frozen PPO batch, so one real rollout's decisions can be replayed
//! through the optimiser any number of times, and a SHA-256 digest over the same encoding so two
//! rollouts' training data can be compared exactly.

use std::cell::RefCell;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use sha2::Digest;
use ti4_tensor::Device;

use crate::ppo::Step;
use crate::{CriticInput, FACTION_ROSTER, FactionRow, SparseOption};

static ENABLED: AtomicBool = AtomicBool::new(false);
static SYNC: AtomicBool = AtomicBool::new(false);

/// Turn the timers on for the rest of the process.
///
/// `sync` adds a CUDA synchronisation at each PPO phase boundary. That attributes device time to the
/// phase that queued it, at the cost of the host/device overlap it removes, so a synchronised run
/// is an attribution run and never a throughput measurement.
pub fn enable(sync: bool) {
    SYNC.store(sync, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
}

/// Whether [`enable`] has been called.
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The stages of one decider call, in the order they run.
#[derive(Debug, Clone, Copy)]
pub enum Stage {
    /// Projecting the observation and legal options into named feature vectors.
    Features = 0,
    /// Resolving feature names to vocabulary columns.
    Vocabulary,
    /// The actor forward pass and softmax.
    Forward,
    /// Drawing the sampled option.
    Sampling,
    /// Building and resolving the critic's feature vector.
    CriticFeatures,
    /// The critic forward pass.
    CriticForward,
    /// Measuring progress and pushing the PPO record.
    Record,
}

/// [`Stage`] names, by discriminant.
pub const STAGES: [&str; 7] = [
    "features",
    "vocabulary",
    "forward",
    "sampling",
    "critic_features",
    "critic_forward",
    "record",
];

/// One thread's decider totals: nanoseconds by stage, and decisions timed.
#[derive(Debug, Clone, Copy, Default)]
pub struct StageTotals {
    /// Nanoseconds charged to each [`Stage`], by discriminant.
    pub nanos: [u128; 7],
    /// Decider calls timed.
    pub decisions: u64,
}

thread_local! {
    static STAGE_TOTALS: RefCell<StageTotals> = const {
        RefCell::new(StageTotals { nanos: [0; 7], decisions: 0 })
    };
}

/// A stopwatch through one decider call. Inert when diagnostics are off.
pub struct Lap(Option<Instant>);

impl Lap {
    /// Start timing one decision.
    #[must_use]
    pub fn start() -> Self {
        if !enabled() {
            return Self(None);
        }
        STAGE_TOTALS.with(|totals| totals.borrow_mut().decisions += 1);
        Self(Some(Instant::now()))
    }

    /// Charge the time since the previous mark to `stage`.
    pub fn mark(&mut self, stage: Stage) {
        if let Some(since) = self.0 {
            let now = Instant::now();
            STAGE_TOTALS.with(|totals| {
                totals.borrow_mut().nanos[stage as usize] += (now - since).as_nanos();
            });
            self.0 = Some(now);
        }
    }
}

/// This thread's decider totals since the last call, reset to zero.
#[must_use]
pub fn take_stages() -> StageTotals {
    STAGE_TOTALS.with(|totals| std::mem::take(&mut *totals.borrow_mut()))
}

/// The phases of one PPO minibatch.
#[derive(Debug, Clone, Copy)]
pub enum Phase {
    /// Host-side packing inside scoring: the option walk and the slot walk. Also inside `Score`.
    Pack = 0,
    /// Building the whole minibatch graph, host packing and uploads included.
    Score,
    /// Queueing (or, synchronised, running) the backward pass.
    Backward,
    /// The Adam step, including its one global-norm host read.
    Step,
    /// Draining an epoch's telemetry to the host.
    Drain,
    /// Host time of the actor's scoring call inside `Score`: gather, trunk and readout queued.
    Actor,
    /// Host time of the critic's scoring call inside `Score`.
    Critic,
    /// Building the update's device-resident batch, once per update.
    Cache,
}

/// [`Phase`] names, by discriminant.
pub const PHASES: [&str; 8] = [
    "pack", "score", "backward", "step", "drain", "actor", "critic", "cache",
];

/// Process-wide PPO totals: nanoseconds by phase, and minibatches stepped.
#[derive(Debug, Clone, Copy, Default)]
pub struct PhaseTotals {
    /// Nanoseconds charged to each [`Phase`], by discriminant.
    pub nanos: [u128; 8],
    /// Adam steps taken.
    pub minibatches: u64,
    /// Real options scored, summed over minibatches.
    pub options: u64,
    /// Cells in the padded `[decisions, widest]` rectangles, summed over minibatches.
    pub cells: u64,
    /// The widest decision in any minibatch.
    pub widest_max: u64,
}

static PHASE_TOTALS: Mutex<PhaseTotals> = Mutex::new(PhaseTotals {
    nanos: [0; 8],
    minibatches: 0,
    options: 0,
    cells: 0,
    widest_max: 0,
});

fn charge(phase: Phase, nanos: u128) {
    if let Ok(mut totals) = PHASE_TOTALS.lock() {
        totals.nanos[phase as usize] += nanos;
        if matches!(phase, Phase::Step) {
            totals.minibatches += 1;
        }
    }
}

/// A clock through one minibatch's phases. Inert when diagnostics are off.
pub struct PhaseClock(Option<Instant>);

impl PhaseClock {
    /// Start timing.
    #[must_use]
    pub fn start() -> Self {
        Self(enabled().then(Instant::now))
    }

    /// Charge the time since the previous mark to `phase`, first waiting for `device`'s queued work
    /// when synchronised attribution is on.
    pub fn mark(&mut self, phase: Phase, device: Device) {
        if let Some(since) = self.0 {
            if SYNC.load(Ordering::Relaxed)
                && let Device::Cuda(index) = device
            {
                tch::Cuda::synchronize(i64::try_from(index).unwrap_or(0));
            }
            let now = Instant::now();
            charge(phase, (now - since).as_nanos());
            self.0 = Some(now);
        }
    }
}

/// Charge the host time since `start` to [`Phase::Pack`]. `start` is `None` when diagnostics are off.
pub fn pack_since(start: Option<Instant>) {
    if let Some(start) = start {
        charge(Phase::Pack, start.elapsed().as_nanos());
    }
}

/// Charge the host time since `start` to `phase`. `start` is `None` when diagnostics are off.
pub fn charge_since(phase: Phase, start: Option<Instant>) {
    if let Some(start) = start {
        charge(phase, start.elapsed().as_nanos());
    }
}

/// A pack timer's start, or `None` when diagnostics are off.
#[must_use]
pub fn pack_start() -> Option<Instant> {
    enabled().then(Instant::now)
}

/// Record one minibatch's layout: `decisions` rows padded to `widest`, holding `options` real
/// cells. Does nothing when diagnostics are off.
pub fn padding(decisions: usize, options: usize, widest: usize) {
    if !enabled() {
        return;
    }
    let wide = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
    if let Ok(mut totals) = PHASE_TOTALS.lock() {
        totals.options += wide(options);
        totals.cells += wide(decisions.saturating_mul(widest));
        totals.widest_max = totals.widest_max.max(wide(widest));
    }
}

/// Process-wide PPO totals since the last call, reset to zero.
#[must_use]
pub fn take_phases() -> PhaseTotals {
    PHASE_TOTALS
        .lock()
        .map(|mut totals| std::mem::take(&mut *totals))
        .unwrap_or_default()
}

// ---- batch capture -----------------------------------------------------------------------------

const MAGIC: &[u8; 8] = b"TI4PPO01";

fn too_large(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{what} does not fit u32"),
    )
}

fn put_u32(out: &mut impl Write, value: usize, what: &str) -> std::io::Result<()> {
    out.write_all(
        &u32::try_from(value)
            .map_err(|_| too_large(what))?
            .to_le_bytes(),
    )
}

fn put_sparse(out: &mut impl Write, sparse: &SparseOption) -> std::io::Result<()> {
    put_u32(out, sparse.columns.len(), "a sparse length")?;
    for column in &sparse.columns {
        out.write_all(&column.to_le_bytes())?;
    }
    for value in &sparse.values {
        out.write_all(&value.to_bits().to_le_bytes())?;
    }
    Ok(())
}

/// Encode one step: every field the optimiser reads, in a fixed order and exact bit patterns.
///
/// # Errors
/// A write failure, or a count that does not fit `u32`.
pub fn encode_step(out: &mut impl Write, step: &Step) -> std::io::Result<()> {
    put_u32(out, step.row.index(), "a faction row")?;
    put_u32(out, step.head, "a head index")?;
    put_u32(out, step.chosen, "a chosen index")?;
    out.write_all(&step.behaviour_log_prob.to_bits().to_le_bytes())?;
    out.write_all(&step.temperature.to_bits().to_le_bytes())?;
    match step.behaviour_value {
        Some(value) => {
            out.write_all(&[1])?;
            out.write_all(&value.to_bits().to_le_bytes())?;
        }
        None => out.write_all(&[0])?,
    }
    out.write_all(&step.return_to_go.to_bits().to_le_bytes())?;
    put_u32(out, step.options.len(), "an option count")?;
    for option in &step.options {
        put_sparse(out, option)?;
    }
    match &step.critic {
        Some(critic) => {
            out.write_all(&[1])?;
            put_sparse(out, critic.sparse())?;
        }
        None => out.write_all(&[0])?,
    }
    Ok(())
}

/// Write a frozen batch's steps to `path`, which must not already exist.
///
/// # Errors
/// The file exists, or a write fails.
pub fn write_capture(steps: &[Step], path: &Path) -> std::io::Result<()> {
    let file = std::fs::File::create_new(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(MAGIC)?;
    out.write_all(&(steps.len() as u64).to_le_bytes())?;
    for step in steps {
        encode_step(&mut out, step)?;
    }
    out.flush()?;
    out.into_inner()
        .map_err(std::io::IntoInnerError::into_error)?
        .sync_all()
}

fn take<const N: usize>(input: &mut impl Read) -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    input
        .read_exact(&mut bytes)
        .map_err(|error| format!("truncated capture: {error}"))?;
    Ok(bytes)
}

fn take_u32(input: &mut impl Read) -> Result<usize, String> {
    usize::try_from(u32::from_le_bytes(take::<4>(input)?))
        .map_err(|_| "u32 does not fit".to_owned())
}

fn take_f64(input: &mut impl Read) -> Result<f64, String> {
    Ok(f64::from_bits(u64::from_le_bytes(take::<8>(input)?)))
}

fn take_sparse(input: &mut impl Read) -> Result<SparseOption, String> {
    let length = take_u32(input)?;
    let mut columns = Vec::with_capacity(length);
    for _ in 0..length {
        columns.push(i64::from_le_bytes(take::<8>(input)?));
    }
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        values.push(f32::from_bits(u32::from_le_bytes(take::<4>(input)?)));
    }
    Ok(SparseOption { columns, values })
}

fn take_flag(input: &mut impl Read) -> Result<bool, String> {
    match take::<1>(input)?[0] {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(format!("capture flag byte {other} is neither 0 nor 1")),
    }
}

/// Read a capture written by [`write_capture`].
///
/// # Errors
/// A missing or truncated file, a wrong magic, trailing bytes, or a faction row outside the roster.
pub fn read_capture(path: &Path) -> Result<Vec<Step>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    let mut input = BufReader::new(file);
    if &take::<8>(&mut input)? != MAGIC {
        return Err(format!("{} is not a PPO batch capture", path.display()));
    }
    let count = usize::try_from(u64::from_le_bytes(take::<8>(&mut input)?))
        .map_err(|_| "step count does not fit".to_owned())?;
    let mut steps = Vec::with_capacity(count);
    for _ in 0..count {
        let row = take_u32(&mut input)?;
        if row >= FACTION_ROSTER.len() {
            return Err(format!("faction row {row} is outside the roster"));
        }
        let head = take_u32(&mut input)?;
        let chosen = take_u32(&mut input)?;
        let behaviour_log_prob = take_f64(&mut input)?;
        let temperature = take_f64(&mut input)?;
        let behaviour_value = if take_flag(&mut input)? {
            Some(take_f64(&mut input)?)
        } else {
            None
        };
        let return_to_go = take_f64(&mut input)?;
        let option_count = take_u32(&mut input)?;
        let mut options = Vec::with_capacity(option_count);
        for _ in 0..option_count {
            options.push(take_sparse(&mut input)?);
        }
        let critic = if take_flag(&mut input)? {
            Some(CriticInput::from_sparse(take_sparse(&mut input)?))
        } else {
            None
        };
        steps.push(Step {
            row: FactionRow(row),
            head,
            options,
            chosen,
            behaviour_log_prob,
            temperature,
            behaviour_value,
            return_to_go,
            critic,
        });
    }
    let mut rest = [0u8; 1];
    if input.read(&mut rest).map_err(|error| error.to_string())? != 0 {
        return Err(format!("{} has trailing bytes", path.display()));
    }
    Ok(steps)
}

/// Feeds written bytes into a hasher.
struct Hashing<'a>(&'a mut sha2::Sha256);

impl Write for Hashing<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// SHA-256 over [`encode_step`] of every step, in order: equal digests mean the optimiser would be
/// handed bit-identical data in the same order.
///
/// # Errors
/// A count that does not fit `u32`.
pub fn steps_digest(steps: &[Step]) -> std::io::Result<String> {
    let mut hasher = sha2::Sha256::new();
    for step in steps {
        encode_step(&mut Hashing(&mut hasher), step)?;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(row: usize, with_critic: bool) -> Step {
        Step {
            row: FactionRow(row),
            head: 3,
            options: vec![
                SparseOption {
                    columns: vec![1, 7, 9],
                    values: vec![0.5, -1.25, 3.0],
                },
                SparseOption {
                    columns: vec![2],
                    values: vec![f32::MIN_POSITIVE],
                },
            ],
            chosen: 1,
            behaviour_log_prob: -std::f64::consts::LN_2,
            temperature: 2.5,
            behaviour_value: with_critic.then_some(0.125),
            return_to_go: -3.75,
            critic: with_critic.then(|| {
                CriticInput::from_sparse(SparseOption {
                    columns: vec![4, 5],
                    values: vec![1.0, 2.0],
                })
            }),
        }
    }

    #[test]
    fn a_capture_reads_back_bit_for_bit_and_is_never_overwritten() {
        // A file, not a directory: nothing here creates a folder.
        let path = std::env::temp_dir().join(format!(
            "ti4-perf-capture-{}-{:?}.bin",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let steps = vec![sample(0, true), sample(FACTION_ROSTER.len() - 1, false)];
        write_capture(&steps, &path).expect("writes");
        let overwritten = write_capture(&steps, &path);
        let read = read_capture(&path).expect("reads");
        let _ = std::fs::remove_file(&path);

        assert!(overwritten.is_err(), "an existing capture was overwritten");
        assert_eq!(read.len(), 2);
        assert_eq!(
            steps_digest(&read).expect("digest"),
            steps_digest(&steps).expect("digest")
        );
        assert_eq!(read[1].row.index(), FACTION_ROSTER.len() - 1);
        assert_eq!(
            read[0].behaviour_log_prob.to_bits(),
            steps[0].behaviour_log_prob.to_bits()
        );
        assert_eq!(
            read[0].options[1].values[0].to_bits(),
            f32::MIN_POSITIVE.to_bits()
        );
        assert_eq!(
            read[0].critic.as_ref().map(|c| c.sparse().columns.clone()),
            Some(vec![4, 5])
        );
        assert!(read[1].critic.is_none() && read[1].behaviour_value.is_none());
    }

    #[test]
    fn the_digest_sees_one_changed_bit() {
        let steps = vec![sample(1, true)];
        let mut changed = vec![sample(1, true)];
        changed[0].return_to_go = f64::from_bits(changed[0].return_to_go.to_bits() ^ 1);
        assert_ne!(
            steps_digest(&steps).expect("digest"),
            steps_digest(&changed).expect("digest")
        );
    }
}
