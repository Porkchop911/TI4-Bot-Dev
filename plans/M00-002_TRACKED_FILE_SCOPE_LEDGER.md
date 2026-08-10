# M00-002 — Tracked-file scope ledger

## ID and title
M00-002 — Tracked-file scope ledger

## Milestone and dependencies
- Milestone: M00 — Oracle and baseline
- Dependencies: M00-001 (Environment record) ✅

## One-sentence objective
Map every tracked file in the Python oracle repository to a milestone or explicit exclusion so that zero branch-specific files remain unclassified.

## Exact Python source references
- Read-only: `D:\Projects\ti4-engine` — all 429 tracked files via `git ls-files`

## Exact Python test references
- N/A (this is an inventory task, not a behavioral test)

## Allowed Rust edit paths
- `plans/M00-002_TRACKED_FILE_SCOPE_LEDGER.md` (this spec file)
- `plans/evidence/M00-002.md` (evidence file)
- `plans/EXECUTION_STATE.md` (execution state update)

## Permission class and scoped access declaration
Permission class: **P1** — Allowed inside the Rust repository.
- Writable paths: `D:\Projects\ti4-engine-rs\plans\`, `D:\Projects\ti4-engine-rs\plans\evidence\`
- Read-only external paths: `D:\Projects\ti4-engine` (oracle, must remain clean)
- Network access: none required
- Processes/ports: none
- Expected generated artifacts: scope ledger markdown (~5–10 KB), evidence file (~2 KB)
- Destructive actions: none
- External-state changes: none

## Inputs and outputs
**Inputs:**
- Complete list of 429 tracked files from `git ls-files` in the oracle repository
- M00–M13 milestone descriptions to determine classification targets

**Outputs:**
- Scope ledger mapping each file to a milestone (M00–M13) or explicit exclusion
- Zero unclassified branch-specific files

## Invariants and compatibility class
- **Invariant:** Every tracked file must be classified; zero exceptions allowed without documented rationale.
- **Compatibility class:** exact — the classification is a deterministic mapping from file path to milestone/exclusion.

## Explicit non-goals
- Not analyzing file contents in depth (that comes in later packages).
- Not modifying any oracle files.
- Not creating new files or directories in the oracle repository.

## Tests to add
No code tests. Verification:
1. `git ls-files` count matches ledger row count.
2. Every row has a valid milestone tag (M00–M13) or exclusion reason.
3. No duplicate file paths.

## Commands to run
```bash
cd D:\Projects\ti4-engine && git ls-files > /tmp/oracle_files.txt
wc -l /tmp/oracle_files.txt
# Then classify each file by directory and naming pattern
```

## Expected evidence
`plans/evidence/M00-002.md` containing:
- Total tracked file count
- Classification breakdown by milestone/exclusion
- Full ledger (file path → milestone or exclusion)
- Confirmation of zero unclassified files

## Known traps
- Some files may serve multiple milestones — assign to the primary one and note secondary relevance.
- `__pycache__/` directories should not appear in tracked files but verify.
- Content JSON files may span multiple milestones (e.g., faction content used by M07).
- Documentation files (`README.md`, etc.) may be excluded or assigned to M00/M13.

## Definition of done
- [x] All 429 tracked files listed and classified ✅
- [x] Each file mapped to a milestone (M00–M13) or explicit exclusion with rationale ✅
- [x] Zero unclassified files remain ✅
- [x] Evidence file written with full ledger and summary statistics (CORRECTED version) ✅
- [ ] `plans/EXECUTION_STATE.md` updated to reflect completion and next package
- [ ] Independent frontier review completed
> **NOTE:** Ledger now has the exact 429-path oracle set, sequential unique numbering (1–429), one canonical primary M00–M13/EXCLUDED classification per row with secondary relevance in rationales, reconciled summaries/sections/glob references. Pending independent frontier review plus EXECUTION_STATE update before formal completion.
