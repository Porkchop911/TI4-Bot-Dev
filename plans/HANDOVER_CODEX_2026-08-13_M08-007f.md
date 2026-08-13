# M08-007f checkpoint handover — 2026-08-13

Objective:
Advance M08 authored-bot objective planning toward a factual, trainable policy surface without
reading hidden state.

Oracle commit:
`37061c511a4780d4c0719e0342533a498cd4b457`; integrity guard last returned
`oracle integrity verified: 238 files`.

Active milestone/package:
M08-007 objective planning. M08-007d/e/f are complete: public spend capacity, resource/influence
purchase reservation, and trade-good reservation. The next package must be a distinct public goal
family or public token-reserve slice.

Status and completed acceptance criteria:

- `Observed::available_spend` returns only public ready resource/influence capacity and delegates
  to authoritative engine payment accounting.
- Engine payment options provide additive `payment_kind` metadata, preserving IDs and executors.
- Revealed, unscored, plausible Monument/Golden Age/Sway Council/Manipulate Law objectives steer
  payment toward preserving public capacity.
- Revealed, unscored, plausible Trade Routes/Centralize Trade objectives preserve the final public
  trade good during an offered payment.
- Secret objectives, decks, schedule persistence, mixed-cost planning, and payment execution
  remain out of scope.

Current branch and HEAD:
`wp/m08-007f-public-trade-good-reserves` at `66dfad2 Reserve public trade good objectives`.

Working-tree state:
Clean before writing this handover. This new handover is the only intended uncommitted path until
its focused checkpoint commit is made.

Tests last run and exact results:

- `cargo fmt --all --check`: passed.
- `cargo test -p ti4-policy`: 54 passed, 0 failed.
- `cargo test -p ti4-engine`: 705 unit tests and 5 doc tests passed, 0 failed (immediately before
  the final policy-only clippy correction).
- `cargo clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings`: passed after that
  correction.
- `git diff --check`: passed.
- oracle integrity guard: passed, 238 files.

Compatibility evidence:
`plans/evidence/M08-007d.md`, `plans/evidence/M08-007e.md`, and
`plans/evidence/M08-007f.md`. Each includes focused decision-boundary and mutation evidence.
Independent review remains owner-waived as documented 2026-08-11; self-review is recorded.

Decisions made and rationale:

- Prefer aggregates over exposure of `GameState` or per-card exhaustion; those aggregates are also
  direct factual inputs for M09/M10 later.
- Use the smallest revealed threshold that is at least half funded as a bounded substitute for the
  oracle's active path. Do not imply exact path-planning parity.
- Preserve payment IDs and executor behavior; only add option payload metadata required for policy
  interpretation.

Open review findings or blockers:
No blocker. The key remaining objective-planning gaps are token reserve facts, other public demand
families (traits, structures, fleets), secret goal planning, mixed-resource planning, schedule,
and plans. M08 remains far from its exit gate; M09/M10 cannot be entered under strict milestone
ordering.

Next exact action/command:
Read the required fresh-session documents, verify Git state, then scope `M08-007g` for a public
token-reserve observation/scoring slice only if a legal current choice exposes the relevant token
spend. Start with:
`PYTHONDONTWRITEBYTECODE=1 python tools\oracle_integrity_guard.py` followed by
`rg -n -C 4 "token|pool|spend" crates\ti4-engine\src\strategy_cards.rs crates\ti4-engine\src\choice.rs crates\ti4-policy\src\bot.rs`.

Files to read first after compaction:
`AGENTS.md`, `plans/SCOPED_PERMISSIONS.md`, `plans/EXECUTION_STATE.md`, `plans/MASTER_PLAN.md`,
`plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/INDEX.md`, `plans/M08_AUTHORED_BOTS.md`, and
`plans/evidence/M08-007f.md`, `plans/evidence/M08-007e.md`, `plans/evidence/M08-007d.md`.
