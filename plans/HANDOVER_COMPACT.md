# Compact handover — 2026-08-12

## Objective
Five quality packages landed this session. Next: M00-013 performance baseline.

## Oracle
- Repository: `D:\Projects\ti4-engine` (read-only)
- Branch: `codex/fully-learned-policy`
- Commit: `37061c511a4780d4c0719e0342533a498cd4b457` — verified clean
- Integrity manifest: `plans/oracle_integrity_manifest.json` (238 files)

## Active milestone/package
- **M06 partial** — content parity reached across all registries; remaining gaps blocked behind reaction system
- **Next ready package: M00-013** (performance baseline) — unblocked

## Status and completed acceptance criteria

### Sessions completed
- M06 content porting: public objectives 40/40, exploration 71/80, secrets 27/40, agenda effects 34/63, relics 5/17
- Content parity confirmed against oracle at pinned commit
- Five quality packages this session:
  1. `Dice::from_faces` + removed duplicated `seed_rolling` (427e558)
  2. `unimplemented()` gaps for secrets + agenda_effects (5bd23d8)
  3. Wiring guard for five subsystems (42d1bd6)
  4. Runnable doc-examples on Table, Decider, ContentStore (af6bac6)
  5. `plans/evidence/INDEX.md`: 86 written / 344 placeholder (3707cdc)

### Metrics
- Tests: **632 passing** (542 engine + 121 content + 68 model), **0 failed**
- Doc-tests: **3 runnable** (Table, Decider, ContentStore embedded) + 2 ignored (Window, TradeWindow)
- Clippy: clean under `-D warnings`
- fmt: clean
- Oracle integrity: verified clean (238 files)

## Working-tree state
- Branch: `wp/m06-003-structured-transactions`
- HEAD: `3707cdc` (INDEX.md)
- Clean (only untracked `.worktrees/` from co-agent)

## Tests last run and exact results
```
cargo test --workspace -> 542 engine + 121 content + 68 model passed; 0 failed
cargo test --doc --workspace -> 3 runnable + 2 ignored passed; 0 failed
cargo clippy --workspace --all-targets -> clean
cargo fmt --all --check -> clean
```

## Compatibility evidence
- `plans/oracle_integrity_manifest.json` (238 files) — `cargo run -p ti4-integrity guard` verifies clean
- `plans/evidence/INDEX.md` — separates written evidence from placeholders; classification rule re-applicable
- Content registries at or ahead of oracle parity (measured by registered alias vs. corpus)

## Decisions made and rationale
- **Dice::from_faces**: eliminates duplicated `seed_rolling` helpers; faces are known at construction
- **unimplemented() gap reporting**: `secrets::unimplemented()` and `agenda_effects::unimplemented()` report non-empty sets of unimplemented cards, verified by tests
- **Wiring guard**: `the_driver_still_wires_the_missing_subsystems` proves agenda, draft, objectives, transit, vote subsystems are called by the driver; verified by breaking each
- **Doc-examples**: made runnable where self-contained (Table, Decider); Window/TradeWindow remain `ignore` (need real game state)
- **INDEX.md**: classification rule — files with `## Package details`, `## Package specification`, or `status: COMPLETE` are stubs; all others are written evidence

## Open review findings or blockers
- Independent review: owner-waived (2026-08-11)
- **M03 timing chain blocked**: M03-007a/b, M03-010 through M03-015 held in `.worktrees/` by co-agent
- **M06-016 blocked**: requires M03-008 through M03-012 (typed event/timing resolver)
- **M05-010b blocked**: source registration/payment requires M06-016

## Next exact action
Execute **M00-013 performance baseline** per `plans/M00_ORACLE_AND_BASELINE.md`:
1. Read `plans/M00_ORACLE_AND_BASELINE.md`
2. Read `plans/evidence/M00-012.md` (benchmark protocol)
3. Create branch `wp/m00-013-performance-baseline` from HEAD
4. Run the performance baseline per the M00-012 protocol
5. Write evidence to `plans/evidence/M00-013.md`

## Files to read first after compact
1. `plans/EXECUTION_STATE.md` (current durable state)
2. `plans/M00_ORACLE_AND_BASELINE.md` (M00-013 spec)
3. `plans/evidence/M00-012.md` (benchmark protocol)
4. `plans/evidence/INDEX.md` (evidence classification)
5. `git status --short --branch` (verify tree state)
