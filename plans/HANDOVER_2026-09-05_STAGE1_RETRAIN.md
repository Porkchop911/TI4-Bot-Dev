# Handover — Stage-1 retrain after the observation-contract rework (2026-09-05)

## Objective

Retrain a Stage-1 MLP champion to **≥95% clearance, ≤5% waste** (both at the rigorous greedy
convention — see `plans/STAGE1_TRAINING_TECHNIQUES.md`), because the OBS-002–012 observation-
contract rework (this session, ending in `OBS-011`'s vocabulary migration to generation
`fa3d6f9...`, `oov_registry_version: 10`) retired every previously-trained checkpoint. All 12
checkpoints under `out/champions/` are schema 6; `ti4_mlp::bundle::read` hard-requires schema 7
with no migration path (`crates/ti4-mlp/src/bundle.rs:50`). Read
`plans/STAGE1_TRAINING_TECHNIQUES.md` first — it has the full experimental history and the reasons
behind every choice below; this file is the resume point, not the rationale.

## Current branch and HEAD

`wp/tier-c-review-remediation-obs008c2b-003e1` at `c7c4e23` (`OBS-012: decision completeness
qualification`). No source changes were made for this training work — everything below lives under
`out/`, which is gitignored.

## Working-tree state — read before touching anything

```
 M crates/ti4-policy/src/critic.rs
?? sample.html
?? sample.ti4review.json
```

`critic.rs`'s modification is **not mine and not `ti4-engine-rs-70`'s** (the peer session sharing
this working tree) — we each independently confirmed it and neither claims it. It adds per-
exploration-card identity facts (`exploration:{card}`) to `actor_inventory_facts` plus a matching
test/fixture update. It mirrors an earlier, similarly unclaimed edit to
`crates/ti4-policy/src/features.rs` that turned out to be an accepted Codex pass (later committed as
`7315f97`). This one has been flagged to the user directly by the peer session but was not
confirmed accepted as of this handover — **do not commit or discard it without checking with the
user first.** `sample.html`/`sample.ti4review.json` are pre-existing, never staged.

**This is a shared working tree, not separate worktrees.** At least one other Claude session
(`ti4-engine-rs-70`) and possibly a third editor (Codex, or another local session) have been active
in this exact tree concurrently this session. Before starting GPU work, check `tasklist` for
`ppo_update.exe`/`corpus_train.exe` and coordinate — a single GPU (3090) is shared, and
`out/checkpoints/*`, `out/corpus/*` directories are not versioned or lock-protected; some scripts
(`clone_round.ps1`) delete-and-rewrite their target directories.

## What exists right now, and where

- **Shared blank starting point**: `out/checkpoints/blank-fa3d6f9-shared` — schema 7, vocabulary
  `fa3d6f9...`, width 256, capacity 20480, shared critic, update 0. Reuse this rather than
  regenerating (deterministic anyway, but avoids redundant compute).
- **Plain-PPO baseline** (peer session's, no waste term): `out/checkpoints/stage1-fa3d6f9/
  checkpoint-64340` — 2050 updates, T=0.25, movement-entropy 0.05, seed-base 650000000. Measured
  89.43% ±0.41 greedy clearance. Waste never measured for this one (expected high; superseded by
  the run below).
- **BC-refinement attempts from that baseline** (mine, this session) — kept for the record, not
  recommended to build on further (plateaued/regressed, see the techniques doc):
  `out/checkpoints/clone-mine-r1/epoch-24` (90.04% clearance, 33.22% any-waste),
  `out/checkpoints/clone-mine-r2/epoch-24` (89.23% clearance, 28.36% any-waste).
  Corpora: `out/corpus/{positive,rescued}-mine-r{1,2}`.
- **Waste-penalty-on-mature-policy experiment** (mine, do not use):
  `out/checkpoints/waste-mine-p8/checkpoint-15100` — 72.25% clearance (letnev 15.53%, broken),
  3.15% any-waste. Kept only as the documented negative result.
- **The current best asset — training paused mid-run**:
  `out/checkpoints/blank-waste-mine-p5/checkpoint-59540` — update 1600 of a planned 2050, trained
  from `blank-fa3d6f9-shared` with `--waste-penalty 5` present from update 0 (same base recipe as
  `checkpoint-64340`: T=0.25, movement-entropy 0.05, seed-base 650000000). **93.40% ±0.33 greedy
  clearance, 2.31% any-waste (0.008 waste/tactical), every faction individually above 90%.** This is
  the checkpoint to build on. Full run log: `/tmp/blank_waste_p5.log` (this machine, `C:\Users\Niko`
  session temp — copy it out if you need the intermediate report history; it is not in the repo).

## Why training was stopped at update 1600, not 2050

The user asked to pause at the next checkpoint boundary while reviewing results — not a quality
plateau. `--report-every 100` means the stop landed exactly on a saved, reload-verified checkpoint
(`ppo_update` publishes every report window for exactly this reason: a stop or crash costs at most
one window). The trend through update 1600 was still positive on both clearance and waste with no
sign of the degradation seen in the mature-policy waste-penalty experiment. **450 updates remain
against the original 2050 budget and are very likely worth running** — resume is not implemented
(`M10-035` is not built per the tool's own doc comment), so continuing means starting a **new**
`ppo_update` run from `checkpoint-59540` for up to 450 more updates, not resuming the old process.

## Reproduce / continue

```powershell
$env:LIBTORCH = "$PWD\out\libtorch-2.9.1-cu128"
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"
$env:PATH = "$env:LIBTORCH\lib;$env:PATH"
$env:GIT_COMMIT = (git rev-parse HEAD)

# Continue the winning run to the original budget (recommended next step):
.\target\release\examples\ppo_update.exe `
  --bundle out\checkpoints\blank-waste-mine-p5\checkpoint-59540 `
  --stage 1 --rounds 1 `
  --temperature 0.25 --movement-entropy 0.05 `
  --waste-penalty 5 --updates 450 --report-every 100 `
  --device cuda --out out\checkpoints\blank-waste-mine-p5-continued
```

Rebuild the `ti4-mlp` examples first if source has changed since this session
(`cargo build --release -p ti4-mlp --example ppo_update --example clearance_eval --example
build_positive_corpus --example rescue_search --example corpus_train --example behaviour_report`
with the same `LIBTORCH`/`cu128` env — binaries are dynamically linked against whichever libtorch
was on `PATH` at build time; mixing the `cu128`-built binaries with the `cpu`-only libtorch on
`PATH` fails with a `c10.dll` load error, and vice versa. Use `libtorch-2.9.1-cu128` consistently
for anything you built against it.)

**After any run, always confirm with a real measurement — never trust the in-training table**
(it is self-play at the training temperature, not greedy; see the techniques doc for why the gap
can be large):

```powershell
.\target\release\examples\clearance_eval.exe --bundle <checkpoint> --temperature 0.001 --seeds 600
.\target\release\examples\build_positive_corpus.exe --bundle <checkpoint> --seeds 600 --temperatures "0.001" --out out\corpus\measure-scratch
.\target\release\examples\behaviour_report.exe --bundle <checkpoint> --seeds 600
```

## Decisions made and rationale

- **Chose to train fresh rather than attempt any schema-6→7 migration.** No migration path exists
  by design (`OBS-003i`); confirmed by reading `bundle::read`'s source, not assumed.
- **Reused the peer session's `checkpoint-64340`/`blank-fa3d6f9-shared` rather than regenerating.**
  Avoided ~1 hour of redundant baseline PPO compute on a shared single-GPU machine.
- **Two negative results (BC-round-2, waste-penalty-8) were kept and documented rather than
  discarded**, per this project's own standing practice of recording what did not work and why —
  see `plans/STAGE1_TRAINING_TECHNIQUES.md`.
- **The winning recipe (waste penalty from blank) was the user's explicit instruction**, given after
  the BC and retrofit-penalty approaches were shown to plateau/collapse respectively; it is not
  something I inferred independently, though the result strongly vindicates it.
- **Training was stopped by user request at a checkpoint boundary**, not because of a measured
  plateau — resuming is very likely still profitable.

## Open review findings or blockers

- `crates/ti4-policy/src/critic.rs`'s uncommitted diff (see "Working-tree state" above) — unclaimed
  by both sessions, flagged to the user, disposition unknown as of this handover.
- Independent Tier-C review of `OBS-004a` through `OBS-012` remains OUTSTANDING across the board
  (recorded per-package in each `plans/evidence/OBS-*.md`) — unrelated to this training work but
  still open on this branch.
- No checkpoint from this training work has been copied into `out/champions/` or given a permanent
  name/provenance entry in `plans/CHAMPIONS.md` yet. Do that once a final number is accepted, per
  that file's own convention ("nothing regenerates `out/champions/`; the sweep directories are
  deleted and rewritten by their scripts").

## Next exact action

1. Check `tasklist` for `ppo_update.exe`/`corpus_train.exe` and coordinate with `ti4-engine-rs-70`
   (or whoever else is active) before touching the GPU.
2. Run the 450-update continuation above from `checkpoint-59540`.
3. Measure with `clearance_eval` + `build_positive_corpus --temperatures 0.001` (never the
   in-training table).
4. If ≥95% clearance and ≤5% waste both hold, copy the checkpoint into `out/champions/` with a
   descriptive name and add a row to `plans/CHAMPIONS.md` recording its provenance (recipe,
   updates, measured numbers, seeds) — that file is the only durable record, since `out/` is
   gitignored.
5. If short on either axis, `plans/STAGE1_TRAINING_TECHNIQUES.md` §"Recipe, distilled" step 6
   (a gated waste-penalty finishing pass, clearance-floor gated) is the documented next lever —
   only after confirming this run's own extended trend first.

## Files to read first

1. `plans/STAGE1_TRAINING_TECHNIQUES.md` — full experimental history and rationale.
2. This file.
3. `plans/CHAMPIONS.md` — the pre-rework record, for provenance-recording conventions and why
   nothing in it is loadable anymore.
4. `plans/STAGE2_TRANSFERABLE_LESSONS.md` — the waste-penalty-collapse mechanism, first documented
   pre-rework, reproduced (worse) this session.
