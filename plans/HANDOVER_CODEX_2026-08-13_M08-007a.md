# Codex handover — M08-007a checkpoint (2026-08-13)

Objective:
Continue the Rust authored-bot migration from M08-007a into M08-007b and later M08 packages,
preserving public-information boundaries and the pinned Python oracle.

Oracle commit:
`D:\Projects\ti4-engine`, branch `codex/fully-learned-policy`, commit
`37061c511a4780d4c0719e0342533a498cd4b457`. The latest guard result was
`oracle integrity verified: 238 files`.

Active milestone/package:
M08 Authored bots. M08-006 is complete through M08-006a/b. M08-007a is complete; M08-007b is the
next package and must add only public partial-progress/reservation facts after an exact API and
oracle inspection.

Status and completed acceptance criteria:

- `d95227e` M08-006a: observed research uses face-up colour/upgrade progress, printed strategy
  cards use their public roles, and observed command pools use diminishing return.
- `8d4de7c` M08-006b: payment preserves trade goods, affordable token spend is scored, and only
  term-bearing public trade offers are parsed/scored. Opaque accept/counter/open negotiation
  choices remain explicitly unscored.
- `0f1101e` M08-007a: score choices read only their offered source-scoped objective alias and
  prioritise its printed points; a legal two-point card produces an exact `victory=200` component.
- The existing M08-005b/c/d tactical work remains on the history immediately below these commits.

Current branch and HEAD:
`wp/m08-007-objective-planning` at `0f1101e Prioritize printed objective awards`.

Working-tree state:
Clean (`git status --short --branch` produced only the branch line).

Tests last run and exact results:

- `PYTHONDONTWRITEBYTECODE=1 python tools/oracle_integrity_guard.py` — passed, 238 files.
- `cargo fmt --all --check` — passed.
- `cargo test -p ti4-policy` — 49 passed, 0 failed.
- `cargo test -p ti4-engine` — 703 unit tests and 5 doc tests passed, 0 failed.
- `cargo clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings` — passed.
- `git diff --check` — passed.

Compatibility evidence:
`plans/evidence/M08-006a.md`, `plans/evidence/M08-006b.md`, and
`plans/evidence/M08-007a.md` record source locations, mutation checks, known differences, and the
owner-waived independent-review status. No differential/parity/performance claim has been made.

Decisions made and rationale:

- `Observed` is the policy boundary. New scorers use only its public seats/board plus the offered
  choice and source-scoped corpus metadata.
- Secret objective identity may be read only when it is itself offered to the selecting player;
  no objective deck or another player's secret hand is enumerated.
- Trade is scored only when the stable option id contains its full terms. The termless
  `transaction` and `open_transaction` windows are kept in `unscored_kinds()`.
- M08-007 must remain split: award prioritisation is done; objective demands/reservations need a
  separate public fact design and must not reach into private exhaustion or secret data.

Open review findings or blockers:
None. Independent review is owner-waived as recorded in the evidence. The original plan evidence
files `M08-006.md` and `M08-007.md` are stale documentation-only claims; do not use them as proof
of implemented behavior.

Next exact action/command:
Run `git switch -c wp/m08-007b-objective-progress`, then
`$env:PYTHONDONTWRITEBYTECODE='1'; python tools\\oracle_integrity_guard.py`, inspect
`engine/bots.py` `PUBLIC_GOALS`/`demands` and the `Observed` public API, and write the M08-007b
package specification before editing. If no safe public progress API exists, record the narrowly
specified missing observation rather than reading private state.

Files to read first after compaction:

1. `AGENTS.md`
2. `plans/SCOPED_PERMISSIONS.md`
3. `plans/EXECUTION_STATE.md`
4. `plans/MASTER_PLAN.md`
5. `plans/PI_WORK_PACKAGE_STANDARD.md`
6. `plans/INDEX.md` and `plans/M08_AUTHORED_BOTS.md`
7. `plans/evidence/M08-007a.md`, `plans/evidence/M08-006b.md`, and `plans/evidence/M08-006a.md`
8. this handover, `git status --short --branch`, and `git log --oneline -5`
