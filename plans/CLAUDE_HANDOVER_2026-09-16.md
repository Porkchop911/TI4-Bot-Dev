# Claude handover — structured diplomacy continuation

Date: 2026-09-16 (Europe/Vienna)

## Start here

- Repository root: `C:/Users/Niko/Documents/ChatGPT/ti4-engine-rs`
- Active worktree: `C:/Users/Niko/Documents/ChatGPT/ti4-engine-rs/diplomacy-worktree`
- Branch: `codex/diplomacy-v1`
- HEAD: `85d01517a196315a5062b706199513c7570f15c0`
- Tree: heavily dirty and intentionally uncommitted. Do not reset, clean, rebase, or overwrite it.
- Read `AGENTS.md`, `plans/EXECUTION_STATE.md`, and
  `plans/DIPLOMACY_HANDOVER_2026-09-15.md` before editing.
- Independent Tier-C review is still required before committing or integrating the legality/schema
  work.

## Live PPO process — do not interrupt

A user-started PPO trainer is active:

```text
PID:        76444
Started:    2026-09-16 00:18:21 local
Executable: C:/Users/Niko/Documents/ChatGPT/ti4-engine-rs/diplomacy-worktree/
            target/release/examples/ppo_update.exe
Output:     D:/Projects/ti4-engine-rs/out/ppo-diplomacy-my-run/checkpoints
```

At 2026-09-16 09:53 local, the newest complete bundle was:

```text
D:/Projects/ti4-engine-rs/out/ppo-diplomacy-my-run/checkpoints/checkpoint-188216
```

Its manifest is valid and records:

- schema 10;
- projection ABI 3;
- 15 heads including `diplomacy`;
- shared critic;
- width 256, two-layer trunk, two residual blocks;
- source: `checkpoint-240`, 900 PPO updates;
- git commit `85d01517a196315a5062b706199513c7570f15c0`.

This run was launched directly rather than through the new launcher. Its directory contains
checkpoints but no `launch.json`, `stdout.log`, `stderr.log`, or `pid.txt`. The manifest does not
record reward/exploration arguments. The conversation says the user intended temperature 2.5,
learning rate 1e-4, scalar waste penalty 10, VP 1.1, objective 0.35, secret 0.35, R1 bonus 3,
R1 shaping 0.1, clearance 0.5, fleet 0.03, tech 0.1, strategy diversity 0.5, fleet-hoard 1,
zero-fleet 10, trade-good hoard 0.05, Styx 16, and Fracture entry 0.3. Treat those values as
conversation evidence, not authenticated artifact metadata. Confirm them from the user's terminal
history if they matter.

`styx-bonus = 16` is very large relative to `vp-weight = 1.1` (about 14.5 VP of reward). This is an
exploratory run and must not be promoted without behavioral inspection and held-out evaluation.

Read-only status command:

```powershell
Get-Process -Id 76444 -ErrorAction SilentlyContinue
Get-ChildItem 'D:\Projects\ti4-engine-rs\out\ppo-diplomacy-my-run\checkpoints' -Directory |
  Sort-Object LastWriteTime
```

Do not start another CUDA trainer while PID 76444 is alive unless the user explicitly asks.

## What was completed after the previous handover

### Engine/model/policy continuation

- Contact option IDs now use seating-order indices instead of faction names. Duplicate factions are
  independently addressable; engine, policy, and reviewer resolve the same seat.
- Added typed agenda assurance `SignalStatement::WillVote { agenda, outcome }`.
  - generated only for concrete revealed agenda choices;
  - judged from the typed recorded ballot;
  - unresolved deadline becomes broken;
  - reviewer renders it as a sentence;
  - policy exposes bounded signal-kind and signal-statement features without raw identity leakage.
- Added `MAX_SIGNAL_CANDIDATES = 6`; the derived contact bound is 36 bundles + 6 signals + no-op =
  43. The measured fixed probe observed 8 options.
- Explicit output versions:
  - `ti4-diplomacy-log-v2`;
  - `ti4-offline-selfplay-v3`;
  - `seat-authorized-canonical-mlp-v3`.
- Older declared corpus/schema pairs remain accepted by readers rather than being silently
  relabeled.

### PPO diplomacy path

- `crates/ti4-training/src/rollout.rs` now exposes
  `play_with_capabilities_and_decider_factory_digest`.
- `crates/ti4-mlp/examples/ppo_update.rs` accepts explicit `--diplomacy`.
- Without the flag, legacy PPO remains diplomacy-disabled.
- With the flag, the driver refuses a bundle lacking the appended diplomacy head.
- A CUDA smoke test passed: 96 two-round games, 114,689 decisions, 13.3 seconds, parameters moved,
  and Adam advanced.

### Operator interface

The original guide was too dense and raw PowerShell continuation commands were error-prone. It has
been replaced with a config-file workflow:

- `scripts/ppo_train.ps1` — validated launcher;
- `scripts/ppo_run.example.psd1` — editable configuration;
- `plans/PPO_TRAINING_HOWTO.md` — short operational guide, reward summary, monitoring, errors, and
  advanced appendix.

Normal usage:

```powershell
Set-Location 'C:\Users\Niko\Documents\ChatGPT\ti4-engine-rs\diplomacy-worktree'
Copy-Item .\scripts\ppo_run.example.psd1 .\scripts\my_ppo_run.psd1
.\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1 -DryRun
.\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1
```

The dry run was executed successfully. It validates paths, bundle manifest, diplomacy schema/head,
known flags, and conflicting waste settings, and prints the exact command without writing anything.
The real path additionally refuses an existing run directory and an existing PPO process, builds
the worktree executable, records `launch.json`, and creates background logs/PID.

## PPO incident history

### Failed first exploratory run

```text
D:/Projects/ti4-engine-rs/out/ppo-diplomacy-overnight-20260915
```

The first process reached update 25, then checkpoint publication refused because `GIT_COMMIT` was
set to `85d0151...-dirty-diplomacy-v2`. Bundle provenance accepts only 7–64 hexadecimal characters.
The resulting `checkpoint-5988.tmp` has tensors but no manifest and is not a bundle. Do not use or
promote it.

### Corrected exploratory run

```text
D:/Projects/ti4-engine-rs/out/ppo-diplomacy-overnight-waste-20260915
```

It used valid hexadecimal provenance and faction waste penalties `15,12,5,5,8,8`. Complete,
reload-verified bundles:

- `checkpoint-240` after update 1;
- `checkpoint-5560` after update 25.

The process is no longer running and stderr is empty. It appears to have been stopped externally
after update 25. Update 22 printed `|log r| inf` but later updates and checkpoint publication
continued; investigate before treating `checkpoint-5560` as sound. The user's current 900-update
run starts from `checkpoint-240`, not `checkpoint-5560`.

The how-to now explicitly says `GIT_COMMIT` must remain hexadecimal; dirty state belongs in the
saved diff and `launch.json`, not in that manifest field.

## Verification already completed

- `cargo test -p ti4-model`: 81 passed.
- `cargo test -p ti4-engine`: 1,331 unit tests plus content-ID, delivery-inventory, and doctests
  passed.
- `cargo test -p ti4-policy`: 248 passed, including the long nested-window campaign.
- `cargo test -p ti4-bridge --lib`: 62 passed.
- `cargo test -p ti4-review`: 33 passed.
- `cargo test -p ti4-mlp --lib -- --skip bundle::`: 88 passed, 14 filtered.
- Capture example: 20 passed.
- Offline-BC example: 6 passed.
- Release diplomacy soak, 60 seeds × 6 rounds: passed with no refused step.
  - 6,036 offers;
  - 3,370 counters;
  - 3,001 accepts;
  - 3,035 declines;
  - 2,701 settled deals;
  - 3,834 signals;
  - 3,273 signal outcomes;
  - 195 transactions;
  - 69 favours across all three implemented families.
- `cargo test -p ti4-sim --release`: 51/52. The sole failure is the documented missing map-pool
  fixture in this worktree; behavior otherwise reproduced inside v38. No rebaseline was performed.
- Strict scoped Clippy passed for changed engine, policy, and reviewer surfaces with only recorded
  unrelated baseline lints allowed.
- `cargo check --release -p ti4-mlp --example ppo_update` and the release build passed after adding
  the diplomacy switch.
- Current tree passes `git diff --check`.

Detailed evidence: `plans/evidence/DIPLOMACY_V2_CONTINUATION_2026-09-15.md`.

## Important unresolved work

1. **Unbounded journal:** `DiplomacyState::journal` remains append-only inside checkpointed
   `GameState`. Do not truncate it; that would break authenticated export. Design lossless sidecar
   ownership while preserving replay, reviewer, and corpus evidence.
2. **Independent Tier-C review:** still required before committing/integrating the legality and
   schema changes.
3. **Leader promise hooks:** `UseLeaderFor` has typed fulfillment only for Hacan and L1Z1X agents.
4. **Reviewer visual QA:** S/L reviewer HTML still needs a manual browser rendering check.
5. **Training qualification:** no held-out paired evaluation of diplomacy checkpoints, no fresh v3
   corpus/BC qualification, and no PPO promotion gate have passed.
6. **Artifact provenance:** PPO bundle manifests do not record reward/exploration flags. The new
   launcher records them in `launch.json`, but the live user run predates that workflow.
7. **PPO exact resume:** bundles are warm starts only. Adam moments, optimizer cursor, entropy
   schedule progress, and seed cursor are not stored.

## Next safe actions

1. Leave PID 76444 alone until it completes or the user asks to stop it.
2. After completion, inspect the newest directory under
   `D:/Projects/ti4-engine-rs/out/ppo-diplomacy-my-run/checkpoints` and verify `manifest.json` plus
   ordinary bundle load. Do not infer reward settings from the manifest.
3. Run held-out, seat-rotated evaluation of several checkpoints, including the starting
   `checkpoint-240`, intermediate checkpoints, and the final checkpoint. Inspect behavior around
   Styx before trusting aggregate VP because the intended Styx coefficient is exceptionally large.
4. Keep training/evaluation outputs out of Git. Do not delete the failed `.tmp` unless the user asks;
   it is useful incident evidence but not a checkpoint.
5. Before source integration, obtain independent Tier-C review and resolve or explicitly defer the
   checkpointed-journal architecture.

## Git scope warning

The worktree contains the earlier Claude v2 implementation plus this continuation, PPO plumbing,
operator docs, and launcher. Nothing in this continuation has been committed. Preserve unrelated
dirty files and do not attempt to split or stage packages until the live trainer is finished and an
independent review plan is agreed.
