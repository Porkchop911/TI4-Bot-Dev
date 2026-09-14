# Handover: stopped-corpus publication and offline BC-v3

Updated: 2026-09-14 14:14 Europe/Vienna.

## Live state

The committed launcher is running:

```text
scripts/publish_and_train_stopped_corpus.ps1
publisher PID: 70548
publisher executable: target-publisher/release/examples/capture_offline_pilot.exe
phase: 32-worker full structural validation of retained per-game parts
latest snapshot: 96.15 / 98.92 GiB compressed bytes read, 80,270 CPU-seconds, 33 threads
```

The final corpus and training output do not exist yet. This is correct: publication is atomic and
the final corpus name appears only after validation, assembly, hash gates, and manifest writing.
The parent launcher will then start CUDA training automatically.

Monitor without interrupting:

```powershell
Get-CimInstance Win32_Process -Filter "ProcessId=70548" |
  Select-Object ProcessId, ThreadCount, WorkingSetSize, ReadTransferCount, KernelModeTime, UserModeTime
Get-Content -LiteralPath 'E:\ti4-corpus\vponly-single-236464-20260914-partial.publish.log' -Tail 40
Get-Process offline_bc -ErrorAction SilentlyContinue
Get-Content -LiteralPath 'D:\Projects\ti4-engine-rs\out\offline-bc-v3-20260914-from-bcv2.log' -Tail 40
```

The interactive execution session in the originating Codex task is session `42509`; other tasks
should monitor by PID/files rather than attempting to attach to that session.

## Immutable inputs and intended outputs

- source staging (must remain untouched):
  `E:\ti4-corpus\vponly-single-236464-20260914.staging-59548`
- completed/planned games: 158,755 / 332,768
- complete pairs before structural validation: 100,492 (`good` 97,387, `bad` 21,
  `random` 3,084)
- final corpus: `E:\ti4-corpus\vponly-single-236464-20260914-partial`
- training inputs: final corpus `good` and `random`; `bad` is deliberately excluded
- initialization checkpoint:
  `D:\Projects\ti4-engine-rs\out\offline-bc-v2-20260913-from-318956`
- training output:
  `D:\Projects\ti4-engine-rs\out\offline-bc-v3-20260914-from-bcv2`
- training recipe: 5 epochs, batch 4096, micro-batch 2048, LR 3e-5, 32 parse workers,
  control-per-million 30,220, CUDA libtorch 2.9.1/cu128
- run evidence after training starts:
  `out\offline-bc-v3-20260914-from-bcv2.run.json` and `.log`

## Provenance

- current branch: `codex/fix-six-faction-leaders`
- recovery commits, newest first:
  - `a433c93` isolate publisher build artifacts
  - `d07097f` allow sparse parallel completion indexes
  - `af91f2d` avoid rebuilding into locked shared target
  - `8cb3c8b` load pinned libtorch before publisher process startup
  - `b5a6d6a` harden recovery and add committed launcher
- generation base commit: `6938019d690bf728f381bab4e203331982ebbf4d`, dirty worktree
- original generator SHA-256:
  `be6895cc64c77e38a23dc9b3a941bcfe191db669453348e4df2201961ce16b70`
- map-pool SHA-256:
  `106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df`
- BC-v2 manifest SHA-256:
  `65673a98ca753d67cc4e09b35122cba6a59b1d5cd7572308c23ceedcfdd1faf9`
- vocabulary slots SHA-256:
  `fa3d6f945988cc9f210fffafff115422c9bf883c077ae8aac8aaf483d1ec41fc`

## What was fixed

Recovery now fully parses every `CapturedDecision`; validates loss alignment and game, seat,
faction, policy, RNG, and per-seat sequence identity; rejects cross-bucket duplicate indexes and
checkpoint digest conflicts; handles failed games explicitly; records actual vs planned counts;
separates generation and publication provenance; validates the generation map-pool digest; uses
the requested Rayon pool; and publishes through a new sibling directory without modifying the
only source parts. Future single-checkpoint capture rotates temperatures across seats.

Focused test result: 20/20 passed. Strict workspace Clippy is blocked by pre-existing warnings in
unrelated MLP library code, recorded in `OFFLINE_PILOT_PARTIAL_PUBLISH.md`.

Eight unmatched kill artifacts were reported and excluded: good games 57024, 67405, 98504,
129806, 233773, 244111, 275367; random game 264908. Non-contiguous high game indexes are valid:
the parallel workers were assigned disjoint ranges and the killed run did not complete an index
prefix.

## Required follow-through

1. Let PID 70548 finish. Do not restart while it or its parent launcher is alive.
2. Confirm the final corpus root and `good/manifest.json`, `random/manifest.json` exist. Check root
   manifest values: `games_played=158755`, `games_planned=332768`, `policy_mode=single`, generator
   SHA and generation commit above, and shard hashes present.
3. Confirm `offline_bc.exe` starts automatically and the `.run.json` names exactly the BC-v2 base
   and the published `good + random` manifests. GPU utilization is expected mainly after parallel
   parsing/validation; CPU parse workers feed the GPU.
4. Let all five epochs finish. Verify the output manifest/checkpoint files and preserve the log.
5. Evaluate BC-v3 against both BC-v2 and the frozen checkpoint-318956 on the same holdout map pool,
   seeds, rotations, candidate seats, four-round horizon, and greedy temperatures used for
   `out/eval-bcv2-vs-ckpt318956.log`. Compare against the candidate==benchmark null, not margin 0.
6. Do not promote automatically. This corpus predates the leader fix and its original generator
   pinned temperatures to seat pairs; those caveats cannot be repaired after capture. Stratify the
   evaluation by faction and seat/temperature context.

## Failure recovery

If validation/publication fails, preserve the message and the `*.publishing-<pid>` directory. The
original `.staging-59548` source remains intact. Resolve the specific error, choose a fresh final
output name or deliberately remove only the verified incomplete publication directory, and rerun
the committed launcher. Never delete or rename the source staging as part of retry cleanup.

Four unrelated pre-existing formatting-only changes remain uncommitted and must not be staged:
`fracture_census.rs`, `objective_signal_audit.rs`, `rng_probe.rs`, and `route_conversion.rs`.
