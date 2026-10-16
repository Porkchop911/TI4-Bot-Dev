# Handover 2026-10-08: unify the shared tree, then BF-22 and BF-24

Objective and normative sources:
- AGENTS.md, plans/SCOPED_PERMISSIONS.md, plans/BASE_FACTIONS_PLAN_2026-10-02.md (rows BF-20..25),
  plans/HANDOVER_2026-10-04_BASE_FACTIONS_PI.md (BF-22a/b, BF-24a/b, BF-25a/b rows),
  plans/evidence/BF-20-21-ROSTER.md (what landed and the BF-22 follow-up list).
- Operator, 2026-10-08: "prompt a new session to pick those pieces up and unify from there".
  Meaning: the policy/training/replayer/review crates hold long-running uncommitted work from
  other sessions; this session owns bringing that work and the faction roster together.

Active workstream/package and acceptance status:
- Every faction in content is implemented and claimed (ledger n/n), including Thunder's Edge
  (Last Bastion, Deepwrought, Crimson, Ral Nel, Firmament, Obsidian). Evidence: plans/evidence/BF-*.md.
- BF-20/21 done as an OPT-IN: `seating::FactionRoster { InScope (default), Wide }`,
  `seating::seat_roster`, `ti4_sim::run::Table::seated_with_roster`,
  `crates/ti4-sim/examples/wide_roster_smoke.rs`. Default rollouts, trainers and tests are unchanged.
- BF-23 dropped (operator: authored bots are deprecated).
- BF-22 and BF-24 not started: they touch crates with other sessions' uncommitted work (below).
- BF-25 (exit: full workspace suite, sampled wide-roster pairing soak, frontier exit review) after them.

Branch and HEAD; dirty paths and owners:
- Shared checkout D:\Projects\ti4-engine-rs on `wp/base-factions`, HEAD b5084fe5 (BF-20/21).
  origin/main = 1b4fcec8; b5084fe5 is NOT merged to main yet.
- Uncommitted, NOT owned by the faction work (check the owners before touching):
  - game.rs: income experiment hunks (`round_income`, `STATUS_INCOME_TRADE_GOODS`, ~47 lines).
    Faction commits filtered them out with a hunk filter + `git apply --cached --recount`.
  - Power projection line (see memory "power projection line 2026-10-02"): ti4-policy
    (critic.rs, features.rs, lib.rs, progress.rs, untracked power_facts.rs, power_map.rs),
    ti4-training (lib.rs, reward.rs, rollout.rs), ti4-mlp (bot.rs, lib.rs, examples incl.
    untracked economy/power/spend audits, faction_trial.rs), ti4-replayer (gui/project/store + tests),
    ti4-review (gui/lib/main + tests, untracked power.rs, examples/), scripts/*.psd1, ppo_train.ps1.
  - plans/INDEX.md, tools/pi_rpc_bridge.py, .pi/ (Pi tooling).
  - `nul` at repo root and `target-cuda-repack/`: unknown origin; do not delete.

Checks actually run and exact results (b5084fe5, from the BF-20/21 agent; the coordinator did not rerun):
- `cargo test -p ti4-model -p ti4-engine`: engine 2803 passed / 1 ignored, 0 failed.
- `cargo test -p ti4-policy -p ti4-sim`: 273 passed; 51 passed / 1 ignored.
- `cargo check --workspace --all-targets`: exit 0.
- wide_roster_smoke `0 40 10`: 40 games, 0 failures, all 32 aliases seated.
- base_faction_soak `all 0 25 10`: 675 games, 0 failures.
- Last merged-tree test (1b4fcec8): only known failures: ti4-server ws_lifecycle
  `running_takeover_closes_old_subscription_and_refuses_old_choices` (flaky, came with upstream
  PR #3 from the Feelx234 fork; fails on plain origin/main too) and ti4-sim
  `profile::tests::fixture_capture_is_deterministic` (needs an out/ pool absent in other checkouts).

Decisions, review debt, blockers, and limitations:
- Engine must ask every decision itself; per-seat UI bundling/filters live in ti4-server
  (reaction_modes moved there 2026-10-07). Do not add engine-side bundling: it changes what bots see.
- Subagents: Sonnet only at top level, at most 2 concurrently; each Sonnet agent may spawn up to
  2 Haiku helpers for grunt work. No new folders or worktrees outside the repo; ask before any new
  folder inside it. Never delete or overwrite data; never cargo clean.
- Merges to main by refs only (merge-tree, commit-tree, then fetch . <sha>:main and push). Never
  update-ref the checked-out branch. Previous merges were tested in the existing idle worktree
  .worktrees/m03-010-timing-resolver (detach to the merge commit, test, check its branch out again).
  The operator approved that use on 2026-10-07.
- Open faction-level gaps are listed per faction in plans/evidence/BF-*.md (e.g. direct
  Galaxy::adjacent callers ignoring breach adjacency / Void Tether, Myru Vos per-unit identity).

Next exact action:
1. Ask the operator (or the owning sessions) which of the uncommitted policy/training/mlp/replayer/
   review/game.rs work is finished and should be committed, and which is abandoned or experimental.
   Commit finished work in coherent, reviewed commits on wp/base-factions; leave the rest untouched.
2. BF-22: opt-in wide roster in ti4-training (rollout, stage1, teacher_corpus, bench_generation,
   heads), ti4-mlp capture_offline_pilot, ti4-policy bot.rs; policy battle `FACTIONS` width with an
   explicit schema/vocabulary version bump and a defined old-checkpoint compatibility/rejection rule
   (no silent reinterpretation); faction-decomposition features verified for every new faction.
3. BF-24: every new decision kind from the faction packages renders with a readable presentation in
   the replayer/review (owner/target/cost/consequence), with a manual park/answer/resume check per
   kind (memory: UI is done only when seen on the manual path). Known new kinds include plot_card,
   follow|enervate_primary, Seethe/Extract choices, breach placement, galvanize, ocean cards.
4. BF-25 exit gate, then merge to main and push.

Files to read first: this file, plans/evidence/BF-20-21-ROSTER.md, plans/EXECUTION_STATE.md,
plans/BASE_FACTIONS_PLAN_2026-10-02.md, `git status --short --branch`, `git log --oneline -10`.
