# OBS-011 — vocabulary and corpus migration

## Package

Runs `ti4-training`'s existing §4.5 discovery pipeline (`examples/vocabulary_discovery.rs`,
`vocabulary_corpus.rs`) against the codebase as it stands after `OBS-004`-`OBS-010`, and publishes
the resulting vocabulary as a new local generation. Discovers names empirically (content corpus,
the accepted r6 champion checkpoint's own weight names, and a bounded replay campaign over the
§6.1 teacher seed schedule) rather than enumerating them from source, so every fact this session
added — the `content` family (`OBS-008EFGHI1`-`OBS-008h3`), the enlarged `critic-state` namespace
(`OBS-010`), the `current_votes` payload (`OBS-009`) — is picked up automatically with zero manual
registration.

- Dependencies: `OBS-003`-`OBS-010` (everything this generation's names were discovered from).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-011` row); MLP plan §4.5
  (the three discovery sources) and §6.1 (the fixed teacher seed schedule the tool will not accept
  a substitute for).
- Writable paths: none in the tracked repository. `out/` is git-ignored (`.gitignore:24`); the
  published generation lives entirely in local build state, exactly like the seven generations
  already on disk from earlier sessions. This spec and its evidence are the only tracked record.

## What ran

```
cargo run --release -p ti4-training --example vocabulary_discovery -- \
    --checkpoint out/stage2_r6/final10000.json \
    --map-pool out/pools/full_np8_12_train.json \
    --out out/vocabulary/slots.json
```

Both inputs were already present from an earlier session and passed the tool's own integrity
gates unmodified: the checkpoint's SHA-256 matched `ti4_sim::baseline::R6_CHECKPOINT_SHA256`
exactly, and the pool passed `read_and_verify_pool_role` for the `Train` role. Neither file was
touched by this package.

## Result

- **Sources**: r6 champions 1,868 names (316 unique), content 295 names (185 unique), replay
  14,328 names over 768/768 completed games, zero failures (185 unique). Every source contributed
  something no other source did, so all three stayed load-bearing (the tool refuses to publish
  otherwise).
- **Union**: 14,829 names across 44 families -- up from the previous accepted generation's 11,147
  slots under `oov_registry_version: 4`.
- **`critic-state`**: 221 names, all 221 in distinct columns, none out of vocabulary -- well above
  the tool's 60-name floor and nearly double the 121 names the family's *introduction* found,
  reflecting `OBS-010`'s actor-inventory and opponent-slot enrichment.
- **`content`**: 134 names -- did not exist as a family at all before this session's
  `OBS-008EFGHI1`.
- **Gates**: capacity 20,480 (`v_cap`), under the reviewed 24,576 ceiling; the double build over
  reversed input was byte-identical (order-independence proven, not assumed).
- **Published**: digest `fa3d6f945988cc9f210fffafff115422c9bf883c077ae8aac8aaf483d1ec41fc`,
  14,877 slots, `oov_registry_version: 10`, `oov_count: 48`, 1,619,478 bytes. The pointer at
  `out/vocabulary/current.json` now names this generation; the previous one
  (`e30b9165...`, `oov_registry_version: 4`) is untouched on disk, exactly as
  `publish_generation`'s immutable-generation-directory design guarantees.

## "Old bundles load with declared OOV semantics or fail with a clear version error"

Not re-derived empirically here -- the property this generation-bump exercises is exactly what
`vocabulary.rs`'s test suite (34 tests, exercised on every OOV bump this session:
`OBS-008EFGHI1`, `OBS-008F2`) already proves exhaustively for every version gap, including the
one now live: the previous accepted generation's own provenance declares
`oov_registry_version: 4`, six versions behind the new registry's 10, and
`version_four_remains_refused_for_inference` already proves a v4 vocabulary is refused for
inference under the current code (only the one-version-back window, now v9, loads). Nothing about
running the real discovery campaign changes that proof; it is the same code path the campaign's
own published artifact will itself be checked against six versions from now.

## Invariants and boundaries

- No source file changed. This package is a data-pipeline run, not a code change -- the first of
  that shape this session.
- The seed range, rotation count, and horizon are the tool's own fixed §6.1 schedule; not
  configurable, so this generation carries the same evidence label as every prior one from the
  same tool.
- The previous generation was not deleted or altered -- `out/vocabulary/generations/` now holds
  eight immutable generations; only the `current.json` pointer moved.

## Tests and commands

- The tool's own nine gates (source non-emptiness, source uniqueness, capacity ceiling, critic
  floor, critic column distinctness, double-build determinism, artifact size cap, digest
  round-trip, atomic pointer commit) all passed -- see the captured output above.
- No engine/policy/training source changed, so the existing suites are unaffected; not re-run for
  this package since nothing they cover changed.

## Definition of done

A new vocabulary generation is discovered from the codebase as it stands after this session's
`OBS-004`-`OBS-010` work and published locally, passing every one of the discovery tool's own
gates including the critic-namespace floor `OBS-010` exists to satisfy; the OOV-registry
backward-compatibility property is confirmed to still hold across the version gap this bump
created; no tracked file changed.
