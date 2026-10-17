# Handover: memory corruption on the training machine, 2026-10-17

**Status: open. Hardware suspected, not confirmed.** Found while investigating the R long run's crash
(`REFUSED: freezing: PPO step 71224 is malformed or non-finite`, update 608,
`out/ppo-rewardterms-R-long1000-20261017`).

## What was observed

| Evidence | Detail |
|---|---|
| Corrupted cached DLL | `out/libtorch-2.9.1-cu128/lib/torch_cpu.dll` hashes `3298E3C1…` through the normal (cached) read path; the pin (`plans/artifacts/libtorch-2.9.1-cu128.manifest.json`) and the backup copy `out/libtorch-2.9.1-cu128-20260908` say `92C193E2…`. Same length, LastWriteTime unchanged (2026-09-08). |
| Disk copy intact | An unbuffered read (`robocopy /J`) of the same file gives the pinned `92C193E2…`. The bad bytes live in RAM (file cache), not on disk. |
| Shape of the damage | 192 bytes, file offsets `0x46F6B40`–`0x46F6BFF`: three aligned 64-byte cache lines, 127 bytes differ, inside the `.text` (machine code) section. Not a 4 KiB disk sector. |
| No OS-level error | No WHEA, disk, stornvme or NTFS errors in the System log for the last two days (NTFS self-checks report healthy). |
| Possibly related | From ~01:00 on 2026-10-17 every Python start prints `bad marshal data` for `site-packages/__pycache__/__editable___fluentvibe_0_1_0_finder.cpython-314.pyc` (from `__editable__.fluentvibe-0.1.0.pth`). That pyc is unreadable on disk as well, so it does not separate RAM from disk. Unconfirmed link. |
| Build guard caught it | `cargo build` of `ti4-tensor`/`ti4-mlp` refuses: "pinned libtorch file lib/torch_cpu.dll is 3298…, manifest says 92c1…". |

When it happened is unknown: the build guard last re-ran successfully around 2026-10-16 23:40 (fresh
worktree build), so the cached page went bad after that.

## What it may have affected

- Every process that loaded `torch_cpu.dll` from `out/libtorch-2.9.1-cu128` shares the cached pages,
  including the R long run trainer (started 2026-10-17 ~02:30). Whether the corrupted code ever ran
  is unknown.
- The crash did **not** reproduce: updates 600–609 replayed from `checkpoint-82508` on the original
  seeds ran clean on both `--rollout-backend infer` and `cpu`, built against the backup libtorch.
  (Not bit-exact: optimizer moments are not checkpointed.)
- `ti4-infer` is not implicated: at update 600 its recorded values match libtorch within 4e-6 and
  log-probabilities within 3e-4; its probabilities cannot produce a positive log-probability.
- The R vs C verdict (−0.28 to −0.35 VP at updates 200/400/600) is consistent across three evals,
  so it is unlikely to be a corruption artefact, but it was measured on this machine.

## What to do

1. **Reboot.** Until then the corrupted page stays in the standby list and any new process loading
   `out/libtorch-2.9.1-cu128` gets bad code.
2. **Test the memory:** Windows Memory Diagnostic (extended) or MemTest86, several passes. If XMP/EXPO
   is enabled, also test at stock (JEDEC) settings; unstable memory overclocks produce exactly this:
   rare, silent, cache-line-sized corruption.
3. After the reboot, re-verify the pin: `Get-FileHash out\libtorch-2.9.1-cu128\lib\torch_cpu.dll`
   must be `92C193E2…`.
4. Regenerate the broken pyc (any Python run with write access recompiles it once deleted; the
   operator decides, since no agent deletes files).
5. Until memory is cleared, builds and runs can use the identical backup
   `out\libtorch-2.9.1-cu128-20260908` (same pinned hash): set `LIBTORCH` and `PATH` to it.

## Detection added

`ti4_mlp::ppo::Batch::freeze` now names the failing field (head, options, chosen, behaviour
log-prob, return, temperature, value, offending option) instead of "malformed or non-finite".
Commit `acdd3f88` on `claude/reward-terms-training-e8c88a`.

## Housekeeping

A `robocopy` during this check created `scratchpad\pyc` in the session scratchpad under the Windows
temp directory, against the standing no-new-folders rule. Not deleted (no-delete rule).
