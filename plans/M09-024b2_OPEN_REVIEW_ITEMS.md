# M09-024b2 open review items

## Independent Tier-C review of `45c6a2d` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the Claude Opus 5 implementation.

Reviewed the package specification, discovery implementation, retained artifact, evidence, and
the upstream projection contract. Independent checks at current HEAD:

- `cargo test -p ti4-training --lib vocabulary_corpus` — **3 passed, 0 failed**;
- retained `out/vocabulary/slots.json` — **1,137,045 bytes**, SHA-256
  `14c193878cb2b3f300f7716c22a8f506dd37d7f8be7d3566c945f459aefd8479`, 10,997 slots,
  capacity 16,384, registry version 2, OOV count 40;
- the retained concrete unit families are `commit-unit`, `load-unit`, `move-unit`,
  `produce-unit`, and `transit-unit` (plus fixed `faction-start-unit`). Thus the current artifact
  does not demonstrate an actual unknown-suffix contamination, but it remains downstream of
  F-M09-024b1-3 and must be regenerated or identity-confirmed after that predicate is fixed.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-024b2-1 | **HIGH** | Neither discovery input is identity- or role-verified. The checkpoint is opened once by `champion_names`, reopened for profiles, and never checked against the durable r6 checksum. The pool is reopened through `MapPool::load` without the M09-020 role gate. A different valid checkpoint or pool can therefore publish a valid-looking vocabulary under the same evidence labels, and the two checkpoint consumers need not consume the same bytes. | Read each input exactly once. Verify the checkpoint bytes against the durable accepted identity, verify the pool bytes as the Train role, and parse all consumers from those same immutable buffers. Retain before/after identities or otherwise ensure the source cannot change across the run. Add wrong-checkpoint, wrong-role pool, and same-path replacement regressions. |
| F-M09-024b2-2 | **HIGH** | `replay_names` discards every `Rollout.error` and increments `games` unconditionally. A seating failure, illegal choice, or horizon failure therefore contributes a partial name set but is reported as one of 768 successful games; the caller then publishes the artifact. | Return a fallible campaign result carrying seed, rotation, and reason; reject any failed rollout and require exactly 768 successful games before construction/publication. Add a forced-failure regression that proves no artifact is published. |
| F-M09-024b2-3 | **HIGH** | The executable enforces only `Vocabulary`'s global 65,536 limit. It never enforces this ruling's reviewed `V_cap <= 24,576` ceiling. It also merely prints unique source contributions: empty or wholly redundant sources still publish successfully despite the package definition requiring every source to be non-empty and independently load-bearing. | Turn the 24,576 ceiling, exact source count, non-empty source sets, and positive unique contribution for all three sources into fail-closed gates before publication, with focused negative tests. |
| F-M09-024b2-4 | **HIGH** | `--rounds` accepts any parseable value while the output schema and evidence record no schedule identity. A one-round or zero-round pass can therefore write `out/vocabulary/slots.json` and print the same “manifest” fields as the required fixed four-round run. | Remove the override for this package or reject every value except the approved four-round horizon. Record the exact seed range, rotations/faction order, horizon, input hashes, and projection/version identity in durable machine-readable provenance tied to the artifact. Add a wrong-horizon refusal test. |
| F-M09-024b2-5 | **MEDIUM** | Publication is a direct `fs::write`; the digest is calculated from the in-memory string after the write and the written bytes are never reread/validated. An interrupted or short write can destroy the previous artifact, and the printed checksum can describe bytes that are not on disk. | Write a sibling temporary file, flush as required by the artifact policy, verify the exact staged bytes by loading them and hashing the staged file, then atomically replace the destination only after every campaign gate passes. Add a publication-failure/no-partial-output regression. |

### Disposition

M09-024b2 is **not accepted**. Its measured artifact currently matches the recorded checksum and
layout, but the program does not establish that those bytes came from the approved inputs and
complete campaign. M09-024 remains open. The package also remains downstream-blocked by
F-M09-024b1-3.
