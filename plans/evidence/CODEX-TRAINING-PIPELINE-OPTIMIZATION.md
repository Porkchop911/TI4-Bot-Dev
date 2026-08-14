# CODEX training-pipeline optimization and Stage-2 wiring

| Field | Value |
|---|---|
| Branch | `codex/stage1-parity-fixes` |
| Date | 2026-08-14 |
| Oracle | `D:\Projects\ti4-engine`, read-only |
| External writes | none |

## Implemented

- schema-4 profiles are shared per batch through `Arc<Profile>`;
- Rayon provides one persistent work-stealing pool for ordinary and rotated rollout panels;
- training workers reduce trajectories to sufficient statistics before returning;
- deterministic statistics merge is parallelized by `(policy identity, decision head)`;
- Stage 1 and Stage 2 use the same reduced training path;
- `FactionPlan` selects Stage-1/one-round or Stage-2/four-round reward and horizon;
- six-faction Stage 2 uses all six physical-seat rotations and the Python Save-52 map pool;
- `stage2_training` supports blank or checkpoint/bootstrap starts, held-out faction metrics,
  periodic reporting, atomic checkpoints, SHA-256 companions, and continued seed schedules.

Stage 1 remains teacher-free. Stage-2 bootstrap profiles affect the starting policy only; all new
credit comes from the Stage-2 VP/objective reward. No action labels or authored inference utility
were added.

## Verification

- one-worker versus 32-worker ordered Save-54 rollout equality: passed;
- worker-side versus parent-side Stage-1 statistic equality: passed;
- worker-side versus parent-side four-round Stage-2 statistic equality: passed;
- faction Stage-2 interrupted/resumed versus uninterrupted in-memory profile equality: passed;
- real Save-52 six-faction Stage-2 blank update and checksummed checkpoint smoke: passed;
- `cargo test -p ti4-training`: 98 passed before final documentation;
- `cargo test -p ti4-sim`: 27 passed;
- strict focused Clippy checks: passed before final workspace verification.

## Performance

The exact Stage-1 training configuration produced:

| Path | Seconds/update | Average equivalent cores |
|---|---:|---:|
| original sequential rotated path | ~1.02 | ~1 |
| first static threaded path | ~0.41 | ~4.8 |
| shared/Rayon/worker-reduced path | ~0.091 | ~16.8 |
| optimized Python historical control | 0.556 | ~60.7% machine CPU |

The final 200-update sustained Rust benchmark completed training in 18.1 seconds, used 306.5 CPU-
seconds over 18.2 wall-seconds, peaked at 36 threads, preserved the established update-200 learning
result, and emitted no stderr.

## Remaining performance ceiling

Named features still use `BTreeMap<String, f64>`. Numeric feature interning may be worthwhile, but
it changes checkpoint representation and therefore requires a versioned mapping plus a solved-
profile transfer gate. It was not folded into this semantics-preserving execution package.
