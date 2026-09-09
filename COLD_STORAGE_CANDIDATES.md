# Cold-storage candidates — ti4-engine-rs

Generated 2026-09-07. **List only — nothing has been moved or modified.**
Sizes measured with `du -sh` on this machine; "G" = GiB.

## Safe to move (regenerable build artifacts)

| Path | Size | Why it is safe |
|---|---|---|
| `target/` | 130 G | Rust build output (`cargo build/test`). Gitignored, fully regenerable by rebuilding. |
| `.worktrees/*/target/` (8 worktrees: m03-007a, m03-007b, m03-010 … m03-015) | 24 G total | Same build output inside each local git worktree. Move only the `target/` subdir of each worktree — do **not** move whole worktree dirs (they are linked to `.git`; a moved worktree needs `git worktree repair`). |
| `target-cuda/` | 7.3 G | Second build dir for CUDA-linked builds (`debug/`, `release/`). Gitignored, regenerable by rebuilding with the CUDA toolchain. |

## Safe to move (local state / one-off outputs, gitignored)

| Path | Size | Why it is safe |
|---|---|---|
| `out/checkpoints/` | 28 G | Training-run checkpoints (`run-031`, `sweep-*`, `stage1-*`, …). Not in Git. Cold-store rather than delete if they back any benchmark/evidence claims; otherwise regenerable by re-running. |
| `out/torch-cu128-staging/` + `out/libtorch-2.9.1-cu128/` + `out/libtorch-2.9.1-cpu/` | ~9 G total | Downloaded PyTorch/LibTorch distributions and staging copies. Re-downloadable from official sources. |
| `out/corpus/` | 2.2 G | Generated corpus data for benchmarks. Regenerable by the scripts in `scripts/`. |
| `out/reviews/`, `out/arena/` | ~3.1 G total | Review/arena run outputs. One-off measurement artifacts; cold-store if cited as evidence, else regenerable. |
| `out/stage2_*` dirs (r1–r6, c1x) + large top-level files (`instr3.log` 313 M, `py_retest_stage2_pychamp.json` 278 M, `py_baseline_u100.json` 255 M, `py_default_u25.json` 193 M, other logs/JSONs) | ~4 G total | One-off benchmark/training measurement outputs. Cold-store if they are the raw evidence behind any report; otherwise regenerable by re-running the campaign. |
| `.tmp-m00-011/`, `.tmp-m00-009h2/` | ~140 M | Leftover temp dirs from old M00 packages. Gitignored (`.tmp-m00-*`). No references in active plans. |
| `.backup/` | 2.5 M | Local git bundles taken before handing the repo to another agent. Already a backup artifact — ideal cold-storage material. |

**Total reclaimable: ≈ 208 GB** (130 + 24 + 7.3 + 47 + ~0.15 G).

## Do NOT move (active / essential)

- `crates/`, `plans/`, `docs/`, `scripts/`, `tools/` — source and active project state.
- `fixtures/` — tracked, checksummed fixture corpora (`mlp-baselines/`, `legacy_entropy/bounded-v1/`); part of the repo (104 files in Git).
- `.git/` (~605 M) — repository metadata; worktrees under `.worktrees/` depend on it.
- Root files: `Cargo.toml`, `AGENTS.md`, `RECOVERY.md`, `engine-rules-audit.md`, etc.
- `.pi-control/` (14 M, mostly `events.jsonl`) — active RPC controller state; do not move while the bridge/controller is running. If disk pressure demands it later, archive `events.jsonl` only when no controller session is live.
- `.cargo/`, `.github/`, `.claude/`, `.pi/` — tiny config dirs, leave in place.

## Notes before moving

1. Move (don't delete) anything under `out/` that is cited as raw evidence in `plans/evidence/*.md` or milestone reports — cold storage preserves it.
2. After moving a worktree's `target/`, the next build in that worktree just recompiles; no Git repair needed.
3. If you move whole directories, keep the same relative layout so paths referenced by scripts (`out/...`) can be restored trivially.
