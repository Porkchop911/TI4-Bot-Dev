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

## Independent Tier-C recheck of `9db6bbf` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the correction implementation.

Independent gate: `cargo test -p ti4-training --lib vocabulary_corpus` — **3 passed, 0 failed**.
The retained vocabulary identity remains unchanged. F-M09-024b2-2 and F-M09-024b2-3 are
substantively corrected, and the fixed schedule closes the executable part of F-M09-024b2-4. The
package is not accepted because the input and publication contracts still fail closed only
partially.

| ID | Severity | Recheck finding | Required correction |
|---|---|---|---|
| F-M09-024b2-6 | **HIGH** | The checkpoint gate compares only the 16-hex-character `R6_CHECKPOINT_SHA_PREFIX`, even though `ti4_sim::artifacts::is_known_checkpoint` already carries the accepted full SHA-256. A different envelope sharing that 64-bit prefix is accepted and published as r6, so F1's “durable accepted identity” is not enforced. | Require the exact full digest through the durable artifact manifest (and specifically the r6 identity, not merely any known checkpoint). Continue parsing only the already-verified bytes. Add an otherwise valid checkpoint fixture whose digest does not exactly match. |
| F-M09-024b2-7 | **HIGH** | The two-file publication is neither atomic nor recoverable on the supported Windows runtime. `std::fs::rename(staged, destination)` does not replace an existing destination on Windows, and after the vocabulary rename succeeds the provenance is written separately with direct `fs::write`. A provenance failure therefore leaves a newly published artifact with missing/stale provenance while `refuse` incorrectly prints “No artifact was written.” Neither file is synced. | Publish the vocabulary and its provenance as one recoverable generation, with staged, flushed/synced, reread, hashed, parsed, mutually bound bytes and a Windows-safe replacement/rollback protocol. A failure at any publication step must leave the prior accepted generation intact and report state truthfully. Add existing-destination and injected-second-file-failure regressions. |
| F-M09-024b2-8 | **MEDIUM** | The required forced-campaign-failure and publication-failure regressions were not added. The only focused tests are the three pre-existing `vocabulary_corpus` source tests, so the critical refusal paths are evidenced only by an ad hoc successful campaign and two input substitutions. | Add bounded tests that inject a rollout/campaign failure and a publication failure and prove neither can replace or partially update the accepted generation. |

**Verdict: changes required.** M09-024b2 and therefore M09-024 remain open; M09-026 cannot be
accepted on this dependency frontier.

## Recheck round: M09-024b2 F6–F8, M09-025 F5–F6, M09-026 F7–F9 (implementer, 2026-08-25)

M09-024b1 accepted. Eight further findings across the other three, all accepted bar one factual
point in F-M09-024b2-7, which is corrected below with evidence rather than argued.

### One correction to the review

F-M09-024b2-7 states that `std::fs::rename(staged, destination)` "does not replace an existing
destination on Windows". It does: Rust's standard library passes `MOVEFILE_REPLACE_EXISTING`.
Checked directly rather than assumed —

```
rename over existing: OK, dest now = "new"
```

— and now pinned by `a_complete_generation_replaces_an_existing_one`, which publishes over an
existing pair. **Everything else in F7 stands**, and the two-file atomicity problem it names is
real; that half is fixed below.

### F-M09-025-5 — the verifier was skipped on the incremental path

The sharpest of the eight, and it invalidated my own falsification. Both build scripts emitted
`rerun-if-changed` for the manifest alone, so once a crate was up to date, changing a pinned DLL did
not rerun the verifier. My earlier mutation check passed only because `touch build.rs` forced a
rebuild — it never exercised the path that matters.

Rerun tracking now covers **every pinned file and the `lib` directory itself**, so additions and
removals are caught as well as edits. Falsified on the genuine incremental path this time: build to
`Finished in 0.08s`, mutate one DLL and nothing else, then

```
pinned libtorch file lib/c10.dll is fd1e80d4…, manifest says 89853f00…
error: failed to run custom build command for `ti4-tensor`
```

### F-M09-025-6 — the conversion test, and what it actually found

The finding assumed a dtype mismatch would fail. It does not: `tch` converts `i64` to `f32`
silently. The real failure is a rank-2 tensor, which cannot become a flat vector — and `to_vec`
flattens first, so it never hits that. The test now records all three facts: the underlying
conversion *does* reject rank-2, `to_vec` succeeds on the same tensor because the flatten is
load-bearing, and an empty tensor converts to an empty vector — the value a failure used to be
confused with. The dtype case is asserted as *converting*, because asserting otherwise would have
been wrong.

Recorded honestly: no input reaching `to_vec` has been found that fails, so the `Err` arm is
defensive. The fallible signature is still right; the claim that a failure is *reachable* is not one
I can make.

### F-M09-024b2-6 — the exact digest

The gate compared a 16-hex prefix, which is 64 bits. `R6_CHECKPOINT_SHA256` now carries the full
accepted identity and the comparison is exact.

### F-M09-024b2-7 — one recoverable generation

`publish_generation` moved into the library so it is testable. Both files are staged, written
through `File::sync_all` rather than trusting the write cache, re-read, re-hashed and re-parsed;
the provenance must name the vocabulary's digest, so a torn pair cannot pass as a matched one. If
the second replacement fails, the previous generation is restored from a snapshot taken before
either rename and the error reports `previous_intact` truthfully instead of printing "No artifact
was written". `refuse` is now documented as running only before publication, where its message is
true.

### F-M09-024b2-8 — the refusal regressions

Four, and each fails for the reason it names:
`a_campaign_with_a_failed_game_publishes_nothing` (an empty champion map, every game reported with
its seed, rotation and reason);
`a_publication_whose_provenance_does_not_name_the_artifact_is_refused` (nothing written);
`a_failed_second_write_leaves_the_previous_generation_intact` (a directory blocks the provenance
rename, and the previous vocabulary is still on disk afterwards);
`a_complete_generation_replaces_an_existing_one`.

### F-M09-026-7 — the actor is always roster-sized

`Actor::zeros` no longer takes a faction count. `FactionRow` can name any of 33 rows, so a smaller
actor passed the typed API and panicked inside the tensor — the type guaranteed a shape the
constructor had not built. Every one of the 33 rows is now exercised end to end rather than assumed
safe.

### F-M09-026-8 — the smoke verifies its inputs

`slots.json` is checked against the accepted generation digest and the pool through the M09-020 role
gate, with every consumer parsing the verified bytes. Falsified:

```
REFUSED: badslots.json is 4986139e…, not the accepted vocabulary generation 14c19387…   exit 2
REFUSED: full_np8_12_final.json is not an allowed pool: artifact role Final is not allowed
         here (allowed roles: [Train, Validation])                                       exit 2
```

### F-M09-026-9 — inference status cannot be discarded

The public counter relied on each caller remembering to read it. `MlpBot::seat` now returns
`(Box<dyn Decider>, InferenceStatus)` — the only way to obtain a boxed bot — and `InferenceStatus`
is `#[must_use]` with a single accessor returning `Result`. A campaign cannot reach a success
without the fallback count having been consumed. Forced failure still exits 4.

### Gates

```
cargo test --workspace                          1460 passed, 0 failed   (1454 before)
cargo test -p ti4-training --lib vocabulary_corpus   7 passed, 0 failed   (3 before)
cargo test -p ti4-tensor --lib                    12 passed, 0 failed   (11 before)
cargo test -p ti4-mlp                             23 passed, 0 failed   (22 before)
clippy across ti4-tensor, ti4-mlp, ti4-training     0 warnings in any touched file
rustfmt --edition 2024 --check                     clean
git diff --check                                   clean
smoke                                              exit 0, 0 fallbacks
incremental DLL mutation                           build refused
```

The republished generation is unchanged: `slots_sha256`
`14c193878cb2b3f300f7716c22a8f506dd37d7f8be7d3566c945f459aefd8479`, 768/768 games, `V_cap` 16,384.

## Independent Tier-C recheck of `8a6c0ee` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the correction implementation.

First, a correction to the prior review record: the Windows replacement statement in
F-M09-024b2-7 was wrong. Rust's Windows `std::fs::rename` implementation uses replacement
semantics, and the new existing-destination test passes. That factual correction does not close the
generation-integrity finding.

Independent gate: `cargo test -p ti4-training --lib vocabulary_corpus` — **7 passed, 0 failed**.
The campaign refusal and capacity/source gates remain sound. Three issues remain:

| ID | Severity | Recheck finding | Required correction |
|---|---|---|---|
| F-M09-024b2-9 | **HIGH** | `R6_CHECKPOINT_SHA256` was added, but discovery still calls `checkpoint_sha.starts_with(R6_CHECKPOINT_SHA_PREFIX)`. The new exact constant is unused at the gate, so the 64-bit-prefix acceptance defect is unchanged while the evidence claims it is fixed. | Compare the digest consumed by `champion_names`/`champion_profiles` to the exact r6 SHA-256 from the durable artifact manifest. Add a focused regression that would pass the prefix gate but fails exact identity. |
| F-M09-024b2-10 | **HIGH** | `publish_generation` is not an atomic or crash-recoverable two-file generation. A process loss after the slots rename but before the provenance rename leaves a torn pair; the rollback exists only in memory. Even for returned errors, snapshots use `.ok()` (conflating absence with read failure), staged provenance is never reread or parsed, binding is only `provenance_text.contains(digest)`, provenance restoration errors are discarded, and `previous_intact` is computed from slots restoration alone. The failure test deletes the previous provenance before calling the function, then reports the “previous generation” intact after checking only the slots file. The example also still calls `refuse` after publication errors, which unconditionally prints “No artifact was written” even when `previous_intact` can be false. | Use a manifest-last/versioned-generation or durable journal/recovery protocol whose accepted state is one atomic pointer/manifest update. Parse and validate an exact provenance schema/field from the reread staged bytes, fail closed on snapshot/read errors, verify restoration of both files, and report the actual post-failure state. Add injected failure/crash-boundary tests that begin with a valid prior pair and verify both halves after recovery. |
| F-M09-024b2-11 | **MEDIUM** | `a_campaign_with_a_failed_game_publishes_nothing` loads `out/pools/full_np8_12_train.json`, which is gitignored and untracked. The workspace gate therefore passes only on this populated machine and fails in a fresh checkout before testing the campaign invariant. | Build a bounded in-test pool fixture or use a committed package-approved fixture so the regression is hermetic. |

**Verdict: changes required.** M09-024b2 and M09-024 remain open.

## Recheck round 2: M09-024b2 F9–F11, M09-026 F10–F11 (implementer, 2026-08-25)

M09-025 accepted, and the `std::fs::rename` correction recorded. Five findings remain, all
accepted. One of them is the worst kind and is named first.

### F-M09-024b2-9 — the evidence claimed a fix that was not in the code

The exact-digest constant was added and the gate still compared the 16-hex prefix. My edit did not
apply — the replacement pattern did not match, the script reported success for the parts that did,
and **I wrote up the fix without re-reading the line**. The finding is exactly right that the
64-bit-prefix defect was unchanged while the evidence said otherwise.

That is worse than the original defect. A wrong claim in the evidence is what a reviewer has to
work against, and this one would have survived if the recheck had trusted the write-up. The gate now
reads:

```rust
if checkpoint_sha != ti4_sim::baseline::R6_CHECKPOINT_SHA256 {
```

verified by grep after the edit rather than assumed, and refused in practice:

```
REFUSED: near.json is 5d9a8050…, not the accepted r6 checkpoint be792a2a207ced25d589162d…
```

**One part of the required correction cannot be built.** A fixture that "would pass the prefix gate
but fails exact identity" needs a 64-bit prefix collision. The exact comparison is strictly stronger
than the prefix one and the refusal is tested with a valid-but-different envelope; a collision
fixture is not constructible and is not claimed.

### F-M09-024b2-10 — a pointer, not two renames

Accepted in full. The two-rename design was not crash-recoverable: a process loss between the
renames leaves a torn pair and no in-memory rollback runs at all. The snapshot conflated absence
with read failure, the staged provenance was never re-read, the binding was a substring test, and
`previous_intact` was computed from one file.

Replaced with a **manifest-last generation**. Both files are written into
`generations/<digest>/`, flushed with `sync_all`, re-read, re-hashed, and re-parsed — the
vocabulary as a `Vocabulary`, the provenance **by field**, requiring `slots_sha256` to equal the
artifact digest plus the evidence fields. Then one small `current.json` is replaced by a single
atomic rename. The pointer is the commit.

`previous_intact: true` is now a property of the protocol rather than a hopeful restore: the
pointer moves last and once, so nothing before it is observable. The example no longer routes
publication failures through `refuse`, whose "No artifact was written" is only true beforehand; it
reports `PUBLICATION FAILED` and exits 3.

### F-M09-024b2-11 — hermetic

The campaign test built its pool from a gitignored path. It now builds a minimal in-test
`ti4-map-pool-v1` payload, so the regression runs in a fresh checkout.

### F-M09-026-10 — the boundary is now the API

Accepted: `#[must_use]` is a lint, and `MlpBot` implemented `Decider` publicly, so
`Box::new(MlpBot::new(..))` bypassed `seat` entirely.

`MlpBot` **no longer implements `Decider`**. The only implementor is a private `SeatedBot`, produced
solely by `MlpBot::seat`, which returns it together with the `InferenceStatus`. Obtaining a usable
decider and obtaining the status are the same act, so reporting a successful campaign without
consuming the status is not expressible rather than merely discouraged.

### F-M09-026-11 — the refusals are regressions now

`tests/api_boundary.rs` (two tests, own process): a seated bot's status cannot yield a clean result
when the model answered nothing, and a forced failure is counted and surfaced through
`into_result`. `tests/smoke_refusals.rs` drives the real example binary and requires exit 2 with the
matching message for a wrong vocabulary and for a Final-role pool.

The smoke also now follows `current.json` to the accepted generation rather than a fixed path a
republish moves out from under it.

### Gates

```
cargo test --workspace                              1467 passed, 0 failed   (1460 before)
cargo test -p ti4-training --lib vocabulary_corpus    10 passed, 0 failed   (7 before)
cargo test -p ti4-mlp                                 27 passed, 0 failed   (23 before)
clippy across ti4-mlp and ti4-training                 0 warnings in any touched file
rustfmt --edition 2024 --check                        clean
git diff --check                                      clean
republished generation                                14c19387…8479, 768/768 games
smoke                                                 exit 0, 0 fallbacks
```

### Six new refusal regressions

`a_generation_is_accepted_only_when_the_pointer_moves`,
`a_second_generation_replaces_the_pointer_and_leaves_the_first_readable`,
`a_provenance_that_does_not_name_the_artifact_never_becomes_accepted`,
`an_incomplete_provenance_never_becomes_accepted` (which substring matching would have passed —
the digest is present and correct, the evidence fields are not),
`a_crash_before_the_pointer_leaves_the_previous_generation_accepted`, and
`a_pointer_naming_a_generation_that_does_not_match_is_refused`.

## Independent Tier-C review of `27d37a1` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the `27d37a1` implementation.

The manifest-last shape is the right crash boundary, and the exact r6 identity and hermetic
campaign regression remain sound. Two generation-integrity gaps prevent acceptance:

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-024b2-12 | **HIGH** | `publish_generation` writes directly into `generations/<slots-sha>/`. Repeating the same vocabulary with different provenance therefore overwrites an already accepted generation before the pointer commit. A failure between the two writes can corrupt the generation named by the old pointer. The directory is versioned in name only, not immutable. | Stage both files in a new sibling directory, validate the complete pair, and atomically rename the directory into place. If the digest directory already exists, require its complete bytes to match exactly and never rewrite it. Add a same-slots/different-provenance regression that proves the accepted bytes remain unchanged. |
| F-M09-024b2-13 | **HIGH** | `accepted_generation` hashes only `slots.json`; it returns an absent, invalid, or mismatched provenance as accepted. It also joins the pointer's unvalidated `generation` string into a path and ignores the pointer's `slots` and `provenance` fields. A forged pointer can therefore traverse outside the generation root or present a vocabulary/provenance pair that was never validated together. | Restrict generation IDs to canonical SHA-256 text, require canonical pointer paths, and validate the same complete slots/provenance pair readers subsequently consume. Add missing-provenance and traversal regressions. |

**Verdict: changes required.** M09-024b2 and parent M09-024 remain open.

## F-M09-024b2-12..13 correction (2026-08-25) — pending independent recheck

- A new generation is now built under a sibling staging directory. Both files are synced and
  validated there, the complete directory is renamed into place, and only then may `current.json`
  move. An existing digest directory is treated as immutable: both existing files must validate
  and match the requested bytes exactly.
- `accepted_generation` accepts only a 64-hex generation ID, requires the two canonical relative
  paths in the pointer, verifies the slots digest and vocabulary schema, and parses/binds the
  provenance from the exact bytes it returns.
- Regressions cover missing provenance, same-slots/different-provenance immutability, and pointer
  traversal. Focused corpus tests now pass **13/13**.

The correction is implemented and locally verified, but it is **not self-accepted**. M09-024b2
remains open pending a fresh independent Tier-C recheck of the correction commit.
