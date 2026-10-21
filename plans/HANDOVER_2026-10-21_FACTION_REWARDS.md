# Handover 2026-10-21 — per-faction reward terms, r7/r8 training

> **All work on this line happens on branch `DEVniko`, in the worktree
> `D:\Projects\ti4-engine-rs\.claude\worktrees\devniko-99016b`.** Don't create side branches or new
> worktrees for it, and don't switch that worktree's branch. Commit directly on DEVniko (operator
> rule). Merges to `main` are refs-only (`merge-tree` / `commit-tree` / `update-ref`), done only on
> the operator's request.

```text
Objective and normative sources:
  Train the 32 per-faction separate models with rewards derived from the SCPT faction guides and the
  topical episodes. Spec: plans/FACTION_REWARD_CONDITIONS_2026-10-20.md; the §6 (SCPT
  re-extractions) and §7 (rebalanced base terms) sections win over earlier rows. The code table in
  crates/ti4-training/src/faction_terms.rs is authoritative.
  Evidence: plans/evidence/SCPT-TOPICAL-REEXTRACTIONS-2026-10-20.md.

Active workstream/package and acceptance status:
  Implemented and committed on DEVniko: faction terms, shared Fracture/Styx/relic shaping, the
  unused-after-scoring option, wasted-activation counting in production_eval, and the replayer
  windows (Goals, Faction state). r7 done. r8 STOPPED by the operator at update 1280, pending a
  decision on the expansion weights.

Branch and HEAD; dirty paths and owners:
  DEVniko at the commit that adds this file (merged into main the same day). Uncommitted files in
  the DEVniko worktree belong to ANOTHER session (the guide-card work: reward.rs, progress.rs,
  ppo_update.rs, production_eval.rs, ppo_train.ps1, rule-bot guide_card.rs, docs/strategy-wiki
  regenerations, COMPUTE_QUEUE log lines). Don't commit, revert or reformat them. Stage only your own
  hunks: write a patch, then `git apply --cached`.

Checks actually run and exact results; evidence:
  ti4-training 171 passed, ti4-policy 303 passed (with the unused-after-scoring test);
  ti4-review lib tests passed; ppo_update and ti4-replayer build. The ti4-review semantic_golden
  failure is known and ignored.
  r7 greedy eval: out/ppo-async-faction-terms-blank-20261020-r7/eval320_r7_waste.txt
  (320 holdout seeds, T 0.001, 5 rounds, diplomacy): 3.79 VP per seat-game, 3.54 wasted activations
  per seat-game (28% of 12.7 tactical actions).

Decisions, review debt, blockers, and limitations: see below.

Next exact action/command; files to read first: see "Next" below.
```

## Runs

| Run (`out/…`) | Config (`scripts/…`) | State |
|---|---|---|
| `ppo-async-faction-terms-blank-20261020-r7` | `long_async_faction_terms_r7_20261020.psd1` | **Done**, 10800 updates, from blank (`blank-like16448-diplomacy-arena-20261017`), seed base 1277058500, 5 rounds. Training log at the end: 0.74 VP/round, 15.8% of seat-games at 6+ VP |
| `ppo-async-faction-terms-waste11-20261021-r8` | `long_async_faction_terms_r8_waste11_20261021.psd1` | **Stopped** at update 1280 (STOP file); `checkpoints/<faction>/checkpoint-1280` for all 32 factions. Started from r7 checkpoint-10800: waste-penalty 11, unused-resources 0.2 counted after status-phase scoring, 10000 updates planned |

Plumbing for both is the earlier blank separate run's: 60 seat-game batches, publish every 120, fewest-updates-first target, STOP file, checkpoints every 500.

## Operator decisions on this line (2026-10-20/21)

- **Horizon:** 5 rounds.
- **Per-faction cap:** faction shaping is capped at 1.0 VP. Letnev's breakthrough is 0.5 VP by round 1 and exempt from the scale-down.
- **Shaping vs kept:** all faction terms are pure shaping, taken back at the horizon. The one exception is the doubled home-loss charge (Cabal, Hacan, Muaat, Winnu), which is kept.
- **Breakthrough rows:** set one by one with the operator. Later revised per SCPT #428 (Cabal, Sardakk, Mahact → H; Ghost and Keleres due by R1; Hacan and Naalu → M). Xxcha stays H by operator decision.
- **Fracture/Styx/relic shaping:** the same for every faction, with no faction multiplier.
- **Waste rule:** if a run's greedy eval shows more than 0.1 wasted activations per seat-game, continue with waste-penalty 11 for another 10k updates. This rule is what triggered r8.
- **Unused resources:** doubled to 0.2 and counted after status-phase scoring (`--unused-after-scoring 1`).
- **Arena work** (`plans/ARENAS_USEFULNESS_PLAN_2026-10-21.md`): **on hold**.
- **Open: expansion weights.** The operator thinks expansion reward may need raising. r7/r8 use `r1-expansion-weight` 1.0 (r6 used 2.0) and `round-expansion-weight` 0.15 (r6: 0.25). Their per-game sizes (~0.75 VP each) are estimates: no tool reports planets held yet.

## Limits and review debt

- **Status-phase snapshot not checked live.** `--unused-after-scoring` assumes the trainer records a status-phase snapshot after scoring, at the command-token redistribution. That hasn't been confirmed in a live game. If no such snapshot exists, the count silently falls back to the old pre-scoring value.
- **Tier-C review outstanding.** Faction terms and the discovery shaping are reward code, so this review is still owed. Implemented by Claude (Sonnet agents for the per-faction terms and the replayer windows), reviewed by the coordinator only.
- **Mixed-hunk staging.** The reward and replayer commits were staged from mixed files by patch. Each was tested only with the other session's uncommitted code also in the tree.
- **Codex handover partly done.** `plans/HANDOVER_2026-10-20_NONFACTION_GUIDES_CODEX.md`: the rule-bot session transcribed the 17 topical episodes, but its notes dropped per-faction detail. The 428/455/459/447 re-extractions in the evidence file replace them for rewards.

## Review pages (private claude.ai artifacts)

- Faction Reward Ledger (who is rewarded for what, base terms included): https://claude.ai/artifact/BAKqEdguaLy7CwmfyKy3ye. It's generated by `cargo run -p ti4-training --example faction_terms_report` plus the r7 config.
- r7 Training Progress (snapshot): https://claude.ai/artifact/Xf8qMxtiYRA4XxrrQxYqu4

## Next

1. **Decide the expansion weights with the operator.** Optional first step: add planets held per round, and their resource + influence value, to production_eval. Then compare r7's final models with the earlier blank run's.
2. **Resume r8 from checkpoint-1280** with the agreed weights:
   - copy the r8 config with a new `Run` dir and seed base;
   - set `Bundle` to `...r8\checkpoints\sol\checkpoint-1280`;
   - launch only on the operator's go, with a line in `plans/COMPUTE_QUEUE.md`.
3. **Greedy eval after any run:**
   ```powershell
   $env:PATH = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908\lib;' + $env:PATH
   .\target\release\examples\production_eval.exe --bundle <run>\checkpoints\sol\checkpoint-N `
     --map-pool D:\Projects\ti4-engine-rs\out\pools\full_np8_12_holdout.json --roster wide --diplomacy `
     --rounds 5 --temperature 0.001 --seeds 320 --seed-base 900000000 --out <run>\eval320.tsv
   ```
   The TABLE line gives VP and `wasted_act` per seat-game.
4. **Dashboard:** `python scripts/router_dashboard.py <run dir>`. **Graceful stop** (PowerShell): `New-Item -ItemType File '<run dir>\STOP'`.

Read first: AGENTS.md, this file, the reward spec §2/§6/§7, `plans/COMPUTE_QUEUE.md`, and the `crates/ti4-training/src/faction_terms.rs` header.
