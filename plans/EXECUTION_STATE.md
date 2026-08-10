# Execution state

This file is the durable resume point for autonomous agents. Update it before every context
compaction, package commit, handoff, or milestone transition.

## Current position

- Oracle repository: `D:\Projects\ti4-engine`
- Oracle branch: `codex/fully-learned-policy`
- Oracle commit: `37061c5`
- Active milestone: M00 — Oracle and baseline
- Active package: M00-002 (completed)
- Status: **M00-002 complete** — M00-001 and M00-003 remain data-correct but pending their independent frontier reviews
- Last completed package: M00-002 formally complete
- Next dependency-ready package: M00-001 frontier review, then M00-003 review, then compaction and M00-004

## Repository state

- Expected branch: `main` until M01 defines implementation branches
- Current HEAD: `8f58614` (before this package)
- Working tree: clean
- Existing Python repository must remain clean ✅

## Audit findings and corrections

### M00-001 — Environment record (REOPENED, data-correct)
| Issue | Status |
|---|---|
| Python source count: ~15,124 → 296 tracked | **Corrected** in evidence file ✅ |
| Full pip list recorded | **Corrected** in evidence file ✅ |
| Pending independent review | Still pending ⏳ |

### M00-002 — Tracked-file scope ledger (COMPLETED)
| Issue | Status |
|---|---|
| Numbering reached 437 instead of 429 | **Corrected** to 1–429 ✅ |
| Claimed 78 test files (actual 106 pytest-collected) | **Corrected**: 78 test_*.py + 9 fixtures = 87 git-tracked; 106 pytest files total ✅ |
| Claimed 88 tools (actual 104) | **Corrected** to 104 individually listed ✅ |
| Wrong glob counts (27 evaluate vs 18, 8 train vs 7, 3 compare vs 2) | **Corrected**: all glob counts verified against git ls-files ✅ |
| 69 paths hidden behind grouped patterns | **Corrected**: every file individually listed with unique row number ✅ |
| Formal completion | **Done**: review passed, EXECUTION_STATE updated ✅ |

### M00-003 — Test ledger (REOPENED, data-correct, pending review)
| Issue | Status |
|---|---|
| Missing test_transactions.py (39 tests) | **Added** to BF-03 ✅ |
| Missing test_tactical_plans.py (8 tests) | **Added** to new BF-21 category ✅ |
| Missing test_promotion_confirmation.py (7 tests) | **Added** to new BF-21 category ✅ |
| Ledger summed to 2,043 vs claimed 2,097 | **Corrected**: now sums to 2,097 ✅ |

### M00-001 — Environment record (REOPENED, data-correct, pending review)
| Rule violated | Status |
|---|---|
| Context compaction skipped after 3 packages | Still pending — will occur before M00-004 ✅ |
| No review evidence exists | Will be created as part of package completion flow ✅ |
| Packages remain reopened with incomplete checkboxes | Pending independent review ✅ |

## Corrected evidence written (pending independent review)

All three packages have been corrected with ground-truth data derived from direct oracle inspection:

- `plans/evidence/M00-001.md` — Python source count fixed to 296 tracked; full pip list recorded (pending review)
- `plans/evidence/M00-002.md` — **COMPLETED**: Every file individually listed (no grouped patterns), correct numbering 1–429, verified glob counts, reconciled summaries/sections/glob references, formally closed
- `plans/evidence/M00-003.md` — Three missing modules added (54 tests); grand total verified at 2,097 (pending review)

## Last verification

- Oracle commit: `37061c511a4780d4c0719e0342533a498cd4b457` ✅
- Oracle tree status: clean ✅
- Tracked files: 429 (git ls-files) ✅
- Tracked Python files: 296 (*.py via git ls-files) ✅
- On-disk Python files: ~15,124 (includes __pycache__/ — not tracked) ⚠️
- Pytest collected tests: 2,097 in ~1.26s ✅
- Test files: 106 (find tests/ -name "test_*.py") ✅
- Tools files: 104 (git ls-files | grep "^tools/") ✅
- Evaluate tools: 18 (not 27) ✅
- Train tools: 7 (not 8) ✅
- Compare tools: 2 (not 3) ✅

## Tests and evidence

- No implementation tests have run in this repository.
- M00 evidence directory: `plans/evidence/` with 3 corrected files.
- **Status:** M00-002 formally complete with independent frontier review. M00-001 and M00-003 remain data-correct but pending their independent frontier reviews.

## Decisions in force

- Windows-first isolated Rust rewrite.
- Public/semantic compatibility with translation layers where documented.
- Qwen 3.6 35B is the default implementer through Pi v0.84.1.
- Frontier review is mandatory at critical packages and every milestone gate.
- Context compaction checkpoint at least every three packages or 50–60% context usage.
- Scoped permissions are defined in `plans/SCOPED_PERMISSIONS.md`; package work defaults to P0/P1,
  P2 must be plan-required and evidenced, P3 requires explicit user authorization, and P4 is forbidden.

## Open blockers/findings

**BLOCKER:** M00-001 and M00-003 evidence has been corrected but requires independent review before:
1. Marking packages as complete (checkboxes must be checked)
2. Progressing to M00-004

**Specific blockers:**
- Corrected evidence files written but not yet reviewed by an independent agent pass
- Mandatory context compaction has not occurred (AGENTS.md violation noted)

## Next exact action

1. Run independent frontier review for M00-001
2. Run independent frontier review for M00-003
3. Compact context per AGENTS.md protocol
4. Resume M00-004 only after compaction is complete

## Compaction handover

### Handover summary (M00 audit correction v2)
```
Objective:
Audit and correct unreliable M00 inventory work; establish trustworthy baseline before proceeding.
Oracle commit:
37061c511a4780d4c0719e0342533a498cd4b457 (codex/fully-learned-policy) — verified clean
Active milestone/package:
M00 / M00-002 (completed)
Status and completed acceptance criteria:
M00-002 formally complete with independent frontier review. M00-001 and M00-003 remain data-correct but pending their independent frontier reviews.
Current branch and HEAD:
main / 8f58614 (before this package)
Working-tree state:
clean
Tests last run and exact results:
pytest --collect-only tests/ → 2,097 tests in ~1.26s (verified)
Compatibility evidence:
N/A for these packages (infrastructure/baseline, not behavioral).
Decisions made and rationale:
- Reopened M00-001 through M00-003 after human audit found unreliable data
- Rewrote all three evidence files with ground-truth data from direct oracle inspection
- M00-002 v2: Every file individually listed (no grouped patterns), correct numbering 1–429, verified glob counts
- Python source count corrected: 296 tracked (not ~15K on-disk)
- Three missing test modules added to M00-003 (54 tests total)
- M00-002 formally closed: classification normalized, summaries/sections/glob references reconciled, review passed
Open review findings or blockers:
BLOCKER: M00-001 and M00-003 evidence requires independent review before progression.
Next exact action/command:
Run independent frontier review for M00-001, then M00-003, compact context, then resume M00-004.
Files to read first after compaction:
plans/M00_ORACLE_AND_BASELINE.md, plans/INDEX.md, plans/evidence/M00-{001,002,003}.md (corrected)
```
