# Execution state

This file is the durable resume point for autonomous agents. Update it before every context
compaction, package commit, handoff, or milestone transition.

## Current position

- Oracle repository: `D:\Projects\ti4-engine`
- Oracle branch: `codex/fully-learned-policy`
- Oracle commit: `37061c5`
- Active milestone: M00 — Oracle and baseline
- Active package: M00-001 (reopened for correction)
- Status: **REOPENED** — original submissions found unreliable, corrected evidence written pending review
- Last completed package: none (M00-001 through M00-003 reopened after audit failure)
- Next dependency-ready package: M00-004 (after corrected packages are reviewed and committed)

## Repository state

- Expected branch: `main` until M01 defines implementation branches
- Last known commit: no commits yet
- Working tree: planning documents + corrected evidence uncommitted
- Existing Python repository must remain clean ✅

## Audit findings (AGENTS.md violations)

The following issues were identified during human audit and require correction before progression:

### M00-001 — Environment record (REOPENED)
| Issue | Original claim | Correct value | Severity |
|---|---|---|---|
| Python source file count | ~15,124 | **296 tracked** (*.py via git ls-files); 15K is on-disk including __pycache__/ | High — inflated by counting non-tracked files |
| Dependency enumeration | Partial selection | **Full pip list recorded** (all ~154 packages) | Medium — incomplete data |

### M00-002 — Tracked-file scope ledger (REOPENED)
| Issue | Original claim | Correct value | Severity |
|---|---|---|---|
| Test file count | 83 | **106** (find tests/ -name "test_*.py") | High — undercounted by 23 files |
| Nonexistent references | `test_dice.py`, `test_fleet.py` | **Do not exist** in oracle | Critical — hallucinated file paths |
| Ledger reliability | Claimed full 429-file ledger | Unreliable; detailed rows did not sum correctly | High |

### M00-003 — Test ledger (REOPENED)
| Issue | Original claim | Correct value | Severity |
|---|---|---|---|
| Missing modules | 103 files listed | **106 files** (missing 3) | Critical |
| Missing test count | Ledger summed to 2,043 | **2,097** (54 tests missing) | High — 2.6% of tests unaccounted for |
| `test_transactions.py` | Not in ledger | **39 tests** → BF-03 Tactical Pipeline | Critical |
| `test_tactical_plans.py` | Not in ledger | **8 tests** → new BF-21 category | Critical |
| `test_promotion_confirmation.py` | Not in ledger | **7 tests** → new BF-21 category | Critical |

### Process violations (AGENTS.md)
| Rule violated | Description | Severity |
|---|---|---|
| Context compaction protocol | Skipped mandatory compaction after 3 packages | High — AGENTS.md requires compaction at earliest of: 3 packages, 50-60% budget, subsystem switch |
| Package completion definition | Checkboxes not checked; no review records | Medium — "Completion means passing evidence, not merely the presence of code" |
| Evidence durability | No package commits made | Medium — AGENTS.md: "Evidence must not exist only in conversation" |

## Corrected evidence written (pending independent review)

All three packages have been corrected with ground-truth data derived from direct oracle inspection:

- `plans/evidence/M00-001.md` — Python source count fixed to 296 tracked; full pip list recorded
- `plans/evidence/M00-002.md` — Full reliable 429-file ledger; removed nonexistent file references; test file count corrected to 106
- `plans/evidence/M00-003.md` — Three missing modules added (54 tests); grand total verified at 2,097

## Last verification

- Oracle commit: `37061c511a4780d4c0719e0342533a498cd4b457` ✅
- Oracle tree status: clean ✅
- Tracked files: 429 (git ls-files) ✅
- Tracked Python files: 296 (*.py via git ls-files) ✅
- On-disk Python files: ~15,124 (includes __pycache__/ — not tracked) ⚠️
- Pytest collected tests: 2,097 in ~1.26s ✅
- Test files: 106 (find tests/ -name "test_*.py") ✅

## Tests and evidence

- No implementation tests have run in this repository.
- M00 evidence directory: `plans/evidence/` with 3 corrected files.
- **Status:** Corrected evidence written but NOT yet independently reviewed or committed.

## Decisions in force

- Windows-first isolated Rust rewrite.
- Public/semantic compatibility with translation layers where documented.
- Qwen 3.6 35B is the default implementer through Pi v0.84.1.
- Frontier review is mandatory at critical packages and every milestone gate.
- Context compaction checkpoint at least every three packages or 50–60% context usage.
- Scoped permissions are defined in `plans/SCOPED_PERMISSIONS.md`; package work defaults to P0/P1,
  P2 must be plan-required and evidenced, P3 requires explicit user authorization, and P4 is forbidden.

## Open blockers/findings

**BLOCKER:** M00-001 through M00-003 evidence has been corrected but requires independent review before:
1. Marking packages as complete (checkboxes must be checked)
2. Creating a planning baseline commit
3. Progressing to M00-004

**Specific blockers:**
- Corrected evidence files written but not yet reviewed by an independent agent pass
- No package commits exist — all work is uncommitted
- Mandatory context compaction was skipped after 3 packages (AGENTS.md violation)
- Package completion checkboxes remain unchecked in all three evidence files

## Next exact action

1. Create corrected M00-001 through M00-003 spec files with updated DoD checkboxes
2. Run independent review pass over the corrected evidence diffs
3. Commit a planning baseline (all M00 inventory work, no Rust code)
4. Compact context per AGENTS.md protocol
5. Resume M00-004 only after baseline commit and compaction are complete

## Compaction handover

### Handover summary (M00 audit correction)
```
Objective:
Audit and correct unreliable M00 inventory work; establish trustworthy baseline before proceeding.
Oracle commit:
37061c511a4780d4c0719e0342533a498cd4b457 (codex/fully-learned-policy) — verified clean
Active milestone/package:
M00 / M00-001 (REOPENED for correction)
Status and completed acceptance criteria:
M00-001 through M00-003 REOPENED. Corrected evidence written but NOT yet reviewed or committed.
Current branch and HEAD:
main / no commits yet
Working-tree state:
Uncommitted: AGENTS.md, README.md, plans/, plans/evidence/M00-{001,002,003}.md (corrected),
             plans/M00-00{1,2,3}_*.md (original specs)
Tests last run and exact results:
pytest --collect-only tests/ → 2,097 tests in ~1.26s (verified)
Compatibility evidence:
N/A for these packages (infrastructure/baseline, not behavioral).
Decisions made and rationale:
- Reopened M00-001 through M00-003 after human audit found unreliable data
- Rewrote all three evidence files with ground-truth data from direct oracle inspection
- Python source count corrected: 296 tracked (not ~15K on-disk)
- Three missing test modules added to M00-003 (54 tests total)
- Removed nonexistent file references from M00-002
Open review findings or blockers:
BLOCKER: Corrected evidence requires independent review before commit and progression.
Next exact action/command:
Create corrected spec files with DoD checkboxes, run independent review, commit planning baseline, compact context.
Files to read first after compaction:
plans/M00_ORACLE_AND_BASELINE.md, plans/INDEX.md, plans/evidence/M00-{001,002,003}.md (corrected)
```
