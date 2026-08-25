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

## M09-024b2 F1–F5 correction and regeneration (implementer, 2026-08-25)

All five accepted. The common thread: the program produced a correct artifact and could not
demonstrate that it had. Every one of these is now a gate, and the run publishes only if all of them
pass.

### F-M09-024b2-1 — inputs are read once and verified

The checkpoint was opened by `champion_names` and again by the driver for the profiles, neither read
checked against the durable accepted identity; the pool was opened through `MapPool::load` with no
role gate. A different valid checkpoint or pool could publish under the same evidence labels.

Both inputs are now read **once**, verified over those exact bytes, and every consumer parses from
that one buffer: the checkpoint against `R6_CHECKPOINT_SHA_PREFIX`, the pool through M09-020's
`read_and_verify_pool_role(.., &[ArtifactRole::Train])`, which returns the verified buffer precisely
so the parse cannot reopen the path. `champion_names` and `champion_profiles` take `&[u8]`, not a
path, so a second read is no longer expressible.

**Falsified.** A truncated checkpoint:

```
REFUSED: …wrong.json is a42a40a8…, not the accepted r6 checkpoint (expected prefix be792a2a207ced25)
No artifact was written.        exit 2
```

The holdout pool in place of the Train pool: **exit 2**. The existing artifact was byte-identical
after both refusals.

### F-M09-024b2-2 — a failed game is a failed campaign

`replay_names` discarded every `Rollout.error` and incremented the game count regardless, so a
seating failure or an illegal choice contributed a partial name set and was reported as one of 768
successes. It now returns a `Campaign` or a `CorpusError::Campaign` carrying seed, rotation and
reason for each failure, and requires the completed count to equal the expected 768 exactly. A seat
with no champion profile is a failure too, rather than something to substitute a default for.

### F-M09-024b2-3 — the reviewed ceiling and the source gates are enforced

Only `Vocabulary`'s global 65,536 limit was checked; this branch's reviewed **24,576** ceiling was
not, and the unique source contributions were printed rather than required. All four are gates now:
exactly three sources, each non-empty, each contributing at least one name no other source did, and
`V_cap ≤ 24,576` with the message naming the architecture review a larger value would need.

### F-M09-024b2-4 — the schedule is fixed and recorded

`--rounds` is gone. The seed range, faction order, rotations, horizon and tile-seed offset are
constants, because a one-round pass could otherwise write the same file and print the same manifest
fields as the approved run. `out/vocabulary/slots.provenance.json` now records all of it beside the
artifact — input digests, pool role, schedule, completed games, content scope, and the registry
version — in machine-readable form.

### F-M09-024b2-5 — publication verifies the bytes that landed

A bare `fs::write` can truncate the previous artifact and leave a short file, and a digest taken
from memory describes bytes that may not be on disk. The artifact is now staged to a sibling file,
re-read, re-hashed against the expected digest, and **re-parsed as a vocabulary**, before an atomic
rename replaces the destination. Any failure removes the staging file and leaves the previous
artifact untouched.

### Regeneration under the corrected predicate

F-M09-024b1-3 changed the unit-family admission rule, so the artifact had to be rebuilt and its
identity re-confirmed rather than assumed.

```
slots_sha256  14c193878cb2b3f300f7716c22a8f506dd37d7f8be7d3566c945f459aefd8479
```

**Identical to the artifact reviewed at `45c6a2d`.** The reviewer's observation was that the
retained unit families were all approved ones, so no contamination existed — that is now confirmed
empirically rather than argued. Everything else is unchanged: 10,997 slots, `V_cap` 16,384, registry
v2, 40 reserved columns, 768 completed games, 1,137,045 bytes, double build byte-identical, 314 s.

### Gates

```
cargo test -p ti4-training --lib vocabulary_corpus     3 passed, 0 failed
cargo test --workspace                              1454 passed, 0 failed
cargo clippy -p ti4-training --all-targets             0 warnings in either file
rustfmt --edition 2024 --check                         clean
git diff --check                                       clean
wrong checkpoint                                       exit 2, artifact untouched
wrong-role pool                                        exit 2, artifact untouched
approved inputs                                        768/768 games, artifact published
```

### Still open

M09-025 F1's durable acquisition/recovery recipe — the manifest pins the bytes but does not yet say
how to reproduce the omitted 368 MB. That is the last item from this round.
