# M08 tactical scoring handover — 2026-08-13

Objective:
Make simulated games useful for training by completing M08-005 public-board tactical scoring, then
continue with economy/development and objective planning.

Oracle commit:
`37061c511a4780d4c0719e0342533a498cd4b457` on `codex/fully-learned-policy`; integrity guard
verified 238 files before and after every child package. The oracle remains read-only.

Active milestone/package:
M08. M08-005 is complete through M08-005b (activation/movement), M08-005c (cargo/landing), and
M08-005d (production/combat closeout). Next ready package is M08-006 economy/development scoring.

Status and completed acceptance criteria:
- `ScoredBot::choose_seeing` now scores public activation prizes, removes useless activation
  candidates, scores useful movement, loads cargo toward an active prize, avoids surplus landings,
  and favors transports where troops are stranded.
- `Observed` exposes public active-system and movement-reachability facts without exposing a hand.
- `stranded_troops` now counts planet-only ground forces, fixing a real public-board valuation bug.
- Existing content-based combat choice scoring remains in the shared dispatcher. M08-005 did not
  change legal choices, cargo/invasion/production resolution, or payment rules.

Current branch and HEAD:
`wp/m08-005d-tactical-production-combat`, `0ee8a6b Score production from stranded troop demand`.
Parent commits: `852ca1d Score cargo and landings from public board state`; `4ffbb7a Score tactical
activations from public board state`.

Working-tree state:
Clean before writing this handover. This file is the only intended subsequent change until it is
committed.

Tests last run and exact results:
- `cargo fmt --all --check` passed.
- `cargo test -p ti4-policy` passed: 42 tests, 0 failed.
- `cargo test -p ti4-engine` passed: 703 unit tests plus 5 doc tests, 0 failed.
- `cargo clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings` passed.
- `git diff --check` passed.
- `cargo run -p ti4-sim --example diag --release` completed 24 scored games: 80 objective scores,
  top VP range 1–6, all `objectives_exhausted` at round 9, 0.2256 seconds/game. This is behavioral
  progress evidence, not parity or a performance claim.

Compatibility evidence:
`plans/evidence/M08-005b.md`, `plans/evidence/M08-005c.md`, and
`plans/evidence/M08-005d.md` contain oracle references, scope, commands, results, and mutation
checks. Mutating activation filtering, idle movement, cargo parsing, surplus landing, and lift
thresholds each made their focused tests fail before restoration.

Decisions made and rationale:
- Keep `choose` blind and use `choose_seeing` only when the engine can honestly pass public facts;
  this prevents silent changes in older window paths.
- Read opaque cargo indices only through labels because the index is private window bookkeeping.
- Do not approximate private available resources for production; use publicly visible stranded
  troop demand as the narrow added component.
- M08-005 was split into b/c/d before implementation to keep each decision boundary reviewable.

Open review findings or blockers:
Independent implementation review is owner-waived. No current blocker. Games still exhaust the
objective deck, so M08-006 and M08-007 remain necessary; do not claim simulation readiness yet.

Next exact action/command:
`git switch -c wp/m08-006-economy-development` from this checkpoint, then run
`PYTHONDONTWRITEBYTECODE=1 python tools/oracle_integrity_guard.py` and inspect the M08-006 oracle
sections in `D:\Projects\ti4-engine\engine\bots.py` before creating `plans/evidence/M08-006.md`.

Files to read first after compaction:
`AGENTS.md`, `plans/SCOPED_PERMISSIONS.md`, `plans/EXECUTION_STATE.md`, `plans/MASTER_PLAN.md`,
`plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/INDEX.md`, `plans/M08_AUTHORED_BOTS.md`,
`plans/evidence/M08-005b.md`, `plans/evidence/M08-005c.md`,
`plans/evidence/M08-005d.md`, and this handover.
