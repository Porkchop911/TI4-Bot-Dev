# Execution state

This file is the durable resume point for autonomous agents. Update it before every context
compaction, package commit, handoff, or milestone transition.

It describes **the repository as measured**, not the plan. A milestone is complete when its
behaviour is implemented, tested, and reviewed — never because a document for it exists.
The previous version of this file claimed the migration was complete; see
[`AUDIT_2026-08-11_PLAN_VS_TREE.md`](AUDIT_2026-08-11_PLAN_VS_TREE.md) for what was
actually in the tree and how the two diverged.

## Current position

- Oracle repository: `D:\Projects\ti4-engine` (read-only)
- Oracle branch: `codex/fully-learned-policy`
- Oracle commit: `37061c511a4780d4c0719e0342533a498cd4b457` — verified clean
- Branch: `main`
- Planning: **M00–M13 documents written.** Implementation status is separate and below.
- Implementation: **M02 in progress.** Content layer done; state model not yet ported.
- Last completed package: M02-009…012 — content corpus, indexes, provenance, referential
  validation (`plans/evidence/M02-009_TO_012_CONTENT_LAYER.md`)
- Next dependency-ready package: M02-003/004/005 — port `engine/state.py` and
  `engine/units.py` onto the corpus (see "Next actions")

## Implementation status

Measured, not claimed. "Scaffold" means the file compiles and has a plausible shape but its
behaviour is a placeholder.

| Crate | Status | Detail |
|---|---|---|
| `ti4-content` | **Implemented** | 28-category corpus loader, source scoping, TE id fallback, manifest cross-check, canonical digests, referential validation, unit catalogue. 73 tests. |
| `ti4-model` | **Partial** | `id.rs` and `content_types.rs` sound. `state.rs` needs porting against `engine/state.py`; `view.rs` redaction is incorrect; `units.rs` is superseded by `ti4-content::units`. |
| `ti4-engine` | **Scaffold** | Phase flow runs, but `rules.rs` returns `Ok(true)` for every action, `tactical.rs` moves no units and treats every system as distance 1, `effects.rs` gives every unit combat value 1. No dice, no adjacency, no legality. |
| `ti4-policy` | **Stub** | 5 × `todo!()` |
| `ti4-sim` | **Stub** | 6 × `todo!()` |
| `ti4-training` | **Stub** | 6 × `todo!()` |
| `ti4-bridge` | **Stub** | 5 × `todo!()` |
| `ti4-legacy` | **Stub** | 4 × `todo!()` |
| `ti4-cli` | **Stub** | Prints hardcoded version strings |
| `xtask` | **Stub** | Prints a version string |

### Milestone implementation

| Milestone | Planning | Implementation |
|---|---|---|
| M00 Oracle and baseline | Written | **Partial** — corpus imported and checksummed. No oracle exporter, no fixtures, no differential corpus. Correctness baseline was only collected, never run. Performance baseline disputed (see audit). |
| M01 Repository bootstrap | Written | **Partial** — workspace, toolchain, lints, profiles exist. No CI, no coverage or mutation harness, no benchmark harness, no `benches/`. |
| M02 Content and model | Written | **In progress** — 009–012 done. 001 done. 003–008, 013–015 outstanding. |
| M03 … M13 | Written | **Not started** |

## Repository state

- Working tree: clean at the last commit
- Python oracle tree: clean, unmodified ✅
- Tests: **118 passing** (`cargo test --workspace`) — 73 `ti4-content`, 35 `ti4-engine`,
  9 `ti4-model`, 1 doc-test
- Integration tests: none. All tests are inline `#[cfg(test)]` modules.
- Content corpus: `crates/ti4-content/content/`, 29 files, 1,800 records, byte-identical to
  the oracle and checksummed in `CHECKSUMS.sha256`

## Open blockers and findings

1. **No oracle exporter exists.** `plans/M00-009_ORACLE_EXPORTER.md` was documented, never
   built. Until it is, no differential parity claim can be made, and M03-014, M04-015,
   M05-021, M06-018 and all of M12 are unimplementable. This is the single largest gap.
2. **No independent review of any code package.** All 17 code evidence files record
   "Self-reviewed", which `PI_WORK_PACKAGE_STANDARD.md` forbids as the sole review.
3. **No CI.** M01-006/007/008/009 are marked complete but nothing runs on a push.
4. **Throughput gate is ~8× weaker than the master plan intends** — `M00-013a.md` labels a
   sequential measurement as 12-worker throughput. Changing a contractual gate needs
   authority; flagged, not corrected.
5. **`ti4-engine` behaviour is not oracle-derived.** Legality, movement, combat, and
   scoring are placeholders. They must be replaced against named oracle sources rather than
   extended.
6. **`ti4-model::view.rs` leaks hidden information** — both views copy
   `secret_strategies`, and the viewer's own cards are hardcoded empty.

## Next actions

In dependency order. Each is one package under `PI_WORK_PACKAGE_STANDARD.md`.

1. **M02-003/005 — port the state model.** `engine/state.py` `Player` (45 fields) and
   `GameState` (52 fields) onto the corpus. Two idioms must survive: duration-scoped
   effects stored as the sequence number they were played in (`combat_round_seq`,
   `activation_seq`, `production_seq`, `turn_seq`) rather than as flags a later step must
   clear; and `compare=False` on 20+ dict fields, which is load-bearing for state equality.
2. **M02-004 — system and planet state**, including the galaxy adjacency that does not
   currently exist anywhere.
3. **M02-008 — hidden views.** Port `engine/views.py`: two private sequences redacted to
   `"?"` with length preserved, plus the `leaks()` check so a newly added private field
   fails a test instead of leaking quietly. Replaces the current `view.rs`.
4. **M00-009 — build the oracle exporter.** Unblocks every differential deliverable.
5. **M01-006 — CI**, so that the 118 tests actually gate a change.

## Decisions in force

- Windows-first isolated Rust rewrite.
- The Python repository at `37061c5` is a read-only behavioural oracle.
- Public/semantic compatibility with translation layers where documented.
- Content is compiled into the binary; `ContentStore::from_dir` remains for regenerated or
  reduced corpora, and a test proves the two agree.
- Corpus files are committed byte-identical with SHA-256 checksums and `.gitattributes`
  pinning them against end-of-line translation.
- Frontier review is mandatory at critical packages and every milestone gate. Not yet
  satisfied for any code package.
- Scoped permissions per `SCOPED_PERMISSIONS.md`: packages default to P0/P1.

## Handover

```
Objective:
M02 — content and model. Content layer complete; state model next.
Oracle commit:
37061c511a4780d4c0719e0342533a498cd4b457 (codex/fully-learned-policy) — verified clean
Active milestone/package:
M02 / M02-009…012 complete; M02-003/005 (state model port) next
Status:
118 tests passing. ti4-content implemented; ti4-model partial; ti4-engine scaffold;
six crates still todo!().
Working-tree state:
clean
Tests last run and exact results:
cargo test --workspace -> 118 passed, 0 failed
Compatibility evidence:
Content semantics documented against engine/content.py and engine/units.py in
plans/evidence/M02-009_TO_012_CONTENT_LAYER.md. No differential fixture evidence exists
anywhere in this repository — do not report content parity as differential parity.
Decisions made and rationale:
- Content compiled in via include_str!, with from_dir retained and proven equivalent
- Record counts cross-checked against manifest.json at load
- Unknown source tags are load errors, not silent filter misses
- ContentType taxonomy replaced: the previous list invented 14 categories and omitted 14
Open review findings or blockers:
No independent review of any code package. No oracle exporter. No CI.
Next exact action:
Port engine/state.py Player and GameState onto the corpus (M02-003/005).
Files to read first:
plans/EXECUTION_STATE.md, plans/AUDIT_2026-08-11_PLAN_VS_TREE.md,
plans/M02_CONTENT_AND_MODEL.md, D:\Projects\ti4-engine\engine\state.py
```
