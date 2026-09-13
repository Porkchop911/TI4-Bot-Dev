# Handover: training run and trainer performance (2026-09-11/12)

For the agent picking this up (codex). Written by the Claude Code session that did the work below.
Everything here is verifiable from the repository; where something is unverified it says so.

## 1. Repository state

- Branch `main`, HEAD `22266e1`. **Six commits ahead of `origin/main` (`e1ee387`), nothing pushed.**
  - `780a3d8`, `cb7c559` — pi's reviewer integration and Fracture ingress fix (engine part: ingress
    candidates enumerate the galaxy, not only `state.board`).
  - `2f3599d` — authored-bot behaviour bounds re-baselined to **v36** (cause: `cb7c559`; every metric
    stayed inside v35, only the exact-recomputation integrity check moved). `plans/evidence/M08-021.md`.
  - `a209b62` — new Stage-2 reward terms (fleet-pool hoard, empty fleet pool, trade-goods hoard,
    Styx at the horizon), all off by default.
  - `0bd2e75` — zero-initialised residual blocks after the actor trunk; bundle schema 8.
  - `22266e1` — trainer performance work (section 3).
- Working tree: **dirty on purpose**. An uncommitted diagnostic patch adds `--no-device-batch` and
  per-update parameter/Adam digests to `ppo_update` (gap-closing run in flight when this was
  written; see section 5). `plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md` is untracked and
  belongs to pi.
- `cargo test` per crate, 2026-09-12, against `22266e1`: model 75, content 130, engine 1273,
  policy 241, tensor 18, mlp 99 (+ 11 integration), training 141, sim 52, review 28, bridge lib 60 —
  all pass. **ti4-bridge's six `hexsummary_golden` tests fail**: `crates/ti4-bridge/tests/golden/`
  was never committed to main (it exists only in `37b236b` on
  `wp/tier-c-review-remediation-obs008c2b-003e1`). Pre-existing since 2026-09-10, unrelated to this
  work; restoring the folder needs the user's approval because it creates a directory.

## 2. Training

**Paused, deliberately**, at `out/blank-shaped-4layers/continue/checkpoint-45496` (update 300 of the
continuation; 550 updates from blank). Nothing is running.

- Lineage: blank bundle (schema 8, width 256, capacity 20480, shared critic, 2 residual blocks) ->
  250 updates (`out/blank-shaped-4layers/checkpoints/checkpoint-33616`) -> 300 more
  (`continue/checkpoint-45496`). Recipe and provenance in each folder's `provenance.txt`.
- Reward: vp 1, objective 0.35, secret 0.25, fleet 0.03, tech 0.1, strategy diversity 1, r1 bonus 2,
  r1 shaping 0.1, waste 1, fleet-pool>=7 at end 1, empty fleet pool 10, trade goods >7 0.1 per good
  per round, Styx at end 1.
- Greedy (T=0.01, 600 held-out maps x 6 rotations, same engine): `checkpoint-33616` 82.81% clearance
  / 2.940 VP; `checkpoint-7472` (the older VP-only line) 91.88% / 3.852. The new line is 250-550
  updates from blank, so it is a level, not a regression.
- To resume: seeds continue at `--seed-base 2400208800`; the optimiser starts cold (bundles carry no
  Adam state), so checkpoint numbering restarts and the run needs its **own output folder — ask the
  user before creating it**.
- Watch: greedy clearance, not the in-training tables (T=2.5 hid a 5 pp greedy drop once). Jol-Nar
  trails early; that has been true in every stage-2 run.

## 3. Trainer performance (packages A0-A4 of pi's handoff)

One update at production settings: **20.32 s -> 17.10 s (-16%)**; rollout 12.94 -> 12.06, optimise
7.38 -> 5.04. Reports: `out/perf-20260911/report-performance.md` (consolidated),
`report-a0-a1.md`, `report-a2.md`, `provenance.md` (hashes, commands, machine). A published page
carries the same content.

Three changes, all in `22266e1`:

- **A2** — each PPO minibatch writes the gather's arrays into buffers reused across the update
  instead of re-flattening and re-validating a frozen, canonical batch four times per update.
- **Scheduling** — rollout workers claim games from a shared cursor instead of a fixed three each;
  results are reassembled in job order.
- **A4** — the batch's sparse inputs are uploaded once per update (`DeviceBatch`, bound 6 GiB, else
  the A2 path) and each minibatch is assembled on the device.

Diagnostics (`crates/ti4-mlp/src/perf.rs`, hooks in `bot.rs`, `ppo.rs`, `ti4-tensor`, and a rollout
digest in `ti4-training`) are **off unless a driver calls `perf::enable`**. `ppo_update` gained
`--diag`, `--diag-sync`, `--hash-games`, `--capture-batch`, `--no-checkpoint`, and now **refuses
unknown flags**. New example `ppo_replay` replays a captured batch with a fresh Adam.

### The rule that governs any further optimisation here

**CUDA training is not bit-repeatable** (embedding-bag backward accumulates with atomics; 30 replays
of one batch gave 30 distinct parameter digests). Equivalence is therefore proven on **CPU**, where
one full update reproduces the baseline checkpoint bit for bit, and on the **kernel inputs**. Never
use a CUDA output as an equivalence gate.

## 4. Where the remaining time is

Per update: rollout ~12.1 s (71%), optimise ~5.0 s (29%).
Optimise: Adam step 2.24 s (waiting on the GPU's backward), host graph build 1.47 s, device batch
build and upload 0.87 s, backward queue 0.38 s, rest ~0.5 s.
Rollout worker time: feature extraction 39.9%, actor forward 24.0%, engine and other 14.7%, critic
features 7.6%, critic forward 7.5%, record 5.0%, vocabulary 1.4%; slowest worker 1.10x the mean.

| Candidate | Ceiling per update | Note |
|---|---|---|
| A3, overlap preparation with GPU work | ~1.0 s | Patch written, **not applied**: `out/perf-20260911/script-patch_a3.py` |
| Pinned-memory uploads | ~0.3 s | The handoff's follow-up to A3 |
| Rollout feature extraction | ~4.3 s | Largest remaining; new scope, profile first |
| GPU rollout inference | <= 31% of rollout | Track B: needs an explicit specification exception |

## 5. Open items

1. **A3** is approved by the user and unapplied. The design: one scoped producer thread owns the
   `DeviceBatch`, recomputes each epoch's shuffle from the same seed, assembles minibatches in order
   and sends them over a two-slot bounded channel; the main thread scores, backs and steps in the
   baseline order. `tch` tensors are `Send` but not `Sync`, which is why the producer owns the batch
   outright rather than borrowing it.
2. **Closed**: the device and host assembly paths agree after a full CPU update on the batch digest
   `1392e609301d5c57`, the parameters `1581cb5521877039` and the Adam state `186025bfe101dc45`
   (`gap-state-*.jsonl`). Memory over five CUDA updates: GPU 8,241 MiB peak of 24,576 (34%), host RSS
   8.01 GiB peak (`gap-memory.csv`). The uncommitted patch that made this possible adds
   `--no-device-batch` and per-update parameter/Adam digests under `--hash-games`; keep or commit it.
3. **Not run, by the user's decision**: the plan's 10-20 updates per condition with a checkpointing
   boundary (five per condition was used instead), so checkpoint-writing time is still unmeasured.
   CUDA events were not used either; attribution rests on synchronisation boundaries and one Nsight
   timeline.
4. **ti4-bridge golden fixtures** missing on main (section 1).
5. Seven stale worktree metadata entries would clear with `git worktree prune` (metadata only) —
   ask first.
6. The reviewer and the bridge are other agents' work; this session did not touch them.

## 6. Constraints this session worked under (they still hold)

- **Never create a folder outside the repository** (globally forbidden by the user until they say
  otherwise: no worktrees, no sibling checkouts, no new target directories, nothing in the Windows
  temp directory). Inside the repository, **ask before creating any folder**, including `out/`
  subfolders. Approved so far: `out/perf-20260911/` (5 GB cap, currently ~2.0 GB) and six
  `checkpoint-*` subfolders inside it.
- **Do not push**, and do not switch the shared checkout's branch: other sessions use this working
  tree. Commit before long pauses.
- **Run the ti4-sim behaviour suite before merging any engine change**; its integrity check fails on
  any change to authored-bot play, and re-baselining needs the user's approval and an entry in
  `plans/evidence/M08-021.md`.
- Benchmarks only on an idle machine; the scripts refuse to run otherwise.

## 7. Exact next actions

```bash
# resume training (needs a new folder: ask the user first)
out/blank-shaped-4layers/ppo_update.exe --bundle out/blank-shaped-4layers/continue/checkpoint-45496 \
  --stage 2 --rounds 4 --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 \
  --learning-rate 3e-4 --seed-base 2400208800 --updates 10000 --report-every 100 --device cuda \
  --waste-penalty 1 --fleet-weight 0.03 --tech-weight 0.1 --strategy-diversity-weight 1 \
  --r1-bonus 2 --fleet-hoard-penalty 1 --zero-fleet-penalty 10 --trade-goods-hoard-weight 0.1 \
  --styx-bonus 1 --out <approved folder>
# ... but rebuild the trainer first if the -16% is wanted: the frozen ppo_update.exe in
# out/blank-shaped-4layers/ predates 22266e1.

# apply A3 (approved, unapplied)
python out/perf-20260911/script-patch_a3.py && cargo test -q -p ti4-mlp --lib

# prove any optimiser change exact (CPU, one update, must match checkpoint-140-baseline)
bash out/perf-20260911/script-a2_verify.sh   # pattern to copy; edit the binary names
```

Files to read first: `plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md` (pi's plan, still the
governing document), `out/perf-20260911/report-performance.md`, then `provenance.md` in the same
folder, `AGENTS.md`, and `plans/EXECUTION_STATE.md`.
