# M09-025 open review items

## Independent Tier-C review of `e74a18e` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the Claude Opus 5 implementation.

Scope reviewed: package specification, `cb5fb54..e74a18e`, the committed libtorch manifest,
`ti4-tensor`, workspace dependency/configuration changes, evidence, and the retained local
artifact. Focused gate rerun: `cargo test -p ti4-tensor -- --test-threads=1` — **10 passed, 0
failed**. Passing tests do not close the fail-closed and determinism gaps below.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-025-1 | **HIGH** | The committed SHA-256 manifest is never checked by the build or runtime. `.cargo/config.toml` points directly at mutable gitignored bytes, while `build.rs` stages every DLL it finds and silently skips any same-named DLL already in `target`. A changed `out/libtorch-2.9.1-cpu` can therefore be linked, and an older staged DLL can continue to be loaded, while every committed checksum remains unchanged. This does not satisfy the package invariant that the pin is the bytes named by the manifest. | Add a fail-closed verifier over the same libtorch source bytes that are staged/linked. Validate at least every linked DLL and required license file against the committed manifest before staging, reject missing/extra linked binaries as appropriate, and verify or replace an existing staged target rather than accepting it by filename. Errors must fail the build; do not discard filesystem errors. Add mutation tests proving a changed source DLL and a stale staged DLL are refused. Record a durable acquisition/recovery recipe instead of claiming the checksum list itself can reproduce the omitted 368 MB artifact. |
| F-M09-025-2 | **HIGH** | `configure_deterministic` returns `Ok` after checking only intra-op threads. `pin_interop_threads` catches and discards failure, even though `backend()` may then report an inter-op count other than 1. The standalone test proves the happy path only when configuration is the process's first libtorch operation; it does not make the public hook enforce the setting. This contradicts the package's explicit “intra-op and inter-op thread counts pinned” and “settings asserted to have taken effect” gate. | Make the public deterministic configuration fail when either reported thread count differs from the pin. Structure tests so process-global configuration is performed first and only once, and add a negative test/process demonstrating that late configuration cannot be reported as success. Do not use a swallowed panic as a successful configuration path. |
| F-M09-025-3 | **HIGH** | `gather_reduce` sorts only by column. Rust's stable sort preserves caller order among duplicate columns, so duplicate contributions with different f32 values are accumulated in caller order. Since duplicates are explicitly legal and f32 addition is non-associative, permutations such as large positive, large negative, and small values can produce different results. The current order-independence test contains no duplicates, and the duplicate test uses only two benign values. | Define and implement a canonical secondary order (including a deliberate policy for signed zero and NaN/non-finite values), or combine duplicates through another explicitly deterministic rule. Add a permutation regression with at least three numerically order-sensitive contributions to the same column and require bit identity. |
| F-M09-025-4 | **MEDIUM** | `to_vec` converts any tensor-conversion error into an empty vector with `unwrap_or_default()`. This helper is specifically documented for assertions and evidence, so an unsupported dtype/device/conversion can masquerade as a legitimate empty result and make downstream checks vacuous. | Return `Result<Vec<f32>, TensorError>` (or fail loudly in a test-only helper), preserve the conversion reason, and update callers to distinguish an actual empty tensor from conversion failure. Add a wrong-dtype regression. |

### Additional test-harness concern

The library unit tests call the process-global configuration from parallel tests, and one test
temporarily changes the intra-op count to 2. The serial rerun above is green, but the default test
harness does not provide that serialization guarantee. Resolve this as part of F-M09-025-2 so the
gate itself is deterministic rather than scheduler-dependent.

### Disposition

M09-025 is **not accepted**. M09-026 cannot be accepted on top of this tensor boundary until the
four findings are corrected and independently rechecked. O-M09-025-2 through O-M09-025-4 and
O-WORKSPACE-1 remain recorded; this review does not silently close them.

## Dependency-root corrections: F-M09-024b1-3 and M09-025 F1–F4 (2026-08-25)

Fifteen findings landed across four packages. These are the two roots — the vocabulary admission
predicate and the tensor boundary — corrected first, because M09-024b2 and M09-026 both sit on them
and any fix above them would have to be redone.

### F-M09-024b1-3 — the wildcard reopened by suffix what the classification had just closed

Correct, and it is the same hole one level down. `role_of` mapped **every** family ending in
`-unit` to the shared transferable role, so `never-reviewed-unit:x` was admitted — after the F1
correction had made admission closed by default. Checkpoint names are a discovery source, so an
arbitrary historical `*-unit` family would have entered the dense vocabulary without the
architecture review a new family requires.

`APPROVED_UNIT_FAMILIES` now pins the five: `commit-unit`, `load-unit`, `move-unit`,
`produce-unit`, `transit-unit`. Admission is membership, not a suffix test, so an unrecognised
`-unit` family falls through to `None` like any other unclassified name.
`every_approved_unit_family_resolves_and_no_other_does` asserts both directions, including
`-unit` itself and `faction-start-unit-unit`, and confirms the fixed `faction-start-unit` keeps its
own role.

### F-M09-025-1 — the manifest is now a gate rather than a document

Correct and the most consequential of the four. M09-025 committed a SHA-256 manifest and pointed
`LIBTORCH` at gitignored bytes, and **nothing compared them**. A changed `out/libtorch-2.9.1-cpu`
would have been linked with every committed checksum still green. That is not a pin; it is a
filename.

`build.rs` now verifies every pinned file against the manifest **before** anything is linked or
staged, refuses any DLL present in the pinned directory but absent from the manifest, and stages by
**content** rather than by filename — an earlier version skipped a same-named file, so a stale DLL
from a previous pin kept being loaded. A missing manifest, an unreadable file, or a mismatch fails
the build; no filesystem error is discarded.

**Falsification, both halves.** One byte of `c10.dll` flipped:

```
error: failed to run custom build command for `ti4-tensor`
  pinned libtorch file lib/c10.dll is 9c6b3a65…, manifest says 89853f00…
  The pin is the bytes, not the path: restore the pinned copy or re-pin deliberately.
```

And a stale `c10.dll` planted in `target/debug`: the build replaced it, and the staged file's digest
afterwards matches the pinned one. Both reverted.

### F-M09-025-2 — a swallowed failure was being reported as success

Correct. `configure_deterministic` checked intra-op only, while `pin_interop_threads` swallowed
libtorch's refusal — so a configuration that did not take effect returned `Ok`. It now enforces
**both** counts and errors if either is not 1.

The harness concern is fixed too. A `CONFIG_LOCK` serialises every global configuration change,
because libtorch's thread counts and RNG are process-global and cargo runs a binary's tests in
parallel threads of one process — the gate was scheduler-dependent, which is not a gate. The
non-vacuity check that temporarily set the count to 2 is **removed** from the shared process; it
raced anything reading the value. `tests/interop.rs` is the process where global configuration is
safe to assert, and it remains one test.

### F-M09-025-3 — duplicates were summed in caller order

Correct, and the sharpest of the four. `gather_reduce` sorted by column, Rust's sort is stable, so
duplicate columns retained the **caller's** order. Duplicates are explicitly legal and f32 addition
is not associative, so the same feature contributed as large-positive, large-negative and small
gave a different sum depending on how the caller happened to order it — and a softmax over
near-tied logits turns a last bit into a different action.

The sort now breaks ties on the value's bit pattern under a total order, so `-0.0` and `+0.0` have
a defined relative order rather than comparing equal and falling back to caller order. Non-finite
values are refused before the sort: NaN has no place in a total order and none in a logit, and an
infinity poisons every sum downstream.

`duplicate_contributions_are_summed_in_a_canonical_order` runs **all six permutations** of
`[1e7, -1e7, 0.125]` into one column and requires bit identity, and asserts first that the fixture
really is order-sensitive in f32 — otherwise it would prove nothing.
`a_non_finite_feature_value_is_refused` covers NaN and both infinities, and checks a finite value
still passes so the guard is not rejecting everything.

**Falsification:** reverting to `sort_by_key(column)` fails exactly that test — *"permutation 1
produced a different sum"*.

### F-M09-025-4 — a helper that turned failure into an empty result

Correct. `to_vec` returned `unwrap_or_default()`, so a failed conversion produced an empty vector
that every downstream assertion accepted as a legitimately empty tensor. It is now
`Result<Vec<f32>, TensorError>` carrying the reason, with `to_vec_or_panic` for tests and
diagnostics — loud rather than silent.

### Gates

```
cargo test -p ti4-tensor                 11 + 1 passed, 0 failed   (9 + 1 before)
cargo test --workspace                 1444 passed, 0 failed      (1442 before)
cargo clippy (tensor, mlp, policy)        0 warnings in any touched file
rustfmt --edition 2024 --check            clean
git diff --check                          clean
```

### Still open from this round

M09-025 F1's "durable acquisition/recovery recipe" is not yet written — the manifest pins the bytes
but does not say how to reproduce the omitted 368 MB. M09-026's six findings and M09-024b2's five
are untouched; 024b2 must in any case be regenerated now that the unit-family predicate has changed,
which is what F-M09-024b1-3 required.

## Independent Tier-C recheck of `9bdb297..9db6bbf` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the correction implementation.

Independent gates: `cargo test -p ti4-tensor` — **11 unit + 1 integration passed, 0 failed**;
the acquisition/recovery recipe is now durably recorded in `plans/evidence/M09-025.md`.
F-M09-025-2 and F-M09-025-3 are closed, and F-M09-025-4's API no longer converts an error to an
empty result. Two review requirements remain.

| ID | Severity | Recheck finding | Required correction |
|---|---|---|---|
| F-M09-025-5 | **HIGH** | Both build scripts emit `cargo:rerun-if-changed` only for the manifest. Once a crate is up to date, modifying a gitignored pinned DLL or adding an extra DLL under `LIBTORCH/lib` does not make Cargo rerun the verifier. The next ordinary `cargo test` can therefore execute/link changed library bytes while the manifest gate is skipped. The recorded mutation run necessarily caused a rebuild by some other change and does not establish the normal incremental-build boundary. | Emit rerun tracking for every pinned source path and for the library directory (covering additions/removals), in both consuming crates, or centralize an equivalent verifier that Cargo is guaranteed to execute whenever usable libtorch bytes can change. Add an incremental regression: first complete an up-to-date build, mutate/add only a source library, invoke Cargo without touching Rust or the manifest, and require refusal. |
| F-M09-025-6 | **MEDIUM** | The correction did not add the required conversion-failure regression. All 11 library tests exercise successful `to_vec` conversions, so a future error-to-empty regression remains undetected even though the current return type is fallible. | Add a tensor/device/layout fixture that makes `to_vec` return `TensorError::Conversion`, and assert it cannot be confused with a genuinely empty tensor. |

**Verdict: changes required.** The recovery-recipe portion of F-M09-025-1 is closed, but the pin
is not an incremental Cargo gate yet. M09-025 remains open.

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

## Independent Tier-C recheck of `8a6c0ee` (2026-08-25) — accepted

Reviewer: Codex frontier model, independent of the correction implementation.

F-M09-025-5 is closed: both build scripts now tell Cargo to track every pinned source file and the
library directory, so changed, added, or removed usable library bytes rerun the verifier on the
incremental path. The recorded source-only DLL mutation is the right falsification.

F-M09-025-6 is also closed with a corrected premise. `tch` converts integer dtype to f32, and
`to_vec` deliberately flattens rank-2 tensors before conversion; the added test demonstrates those
facts and distinguishes a genuine empty tensor from the old error-to-empty behavior. The fallible
return type and fallible callers make a future silent `unwrap_or_default` regression a compile-time
API change even though no supported CPU input currently reaches the defensive conversion error.

Independent gates: `cargo test -p ti4-tensor --lib` — **12 passed, 0 failed**; touched code produced
no new Clippy warning.

**Verdict: accepted.** M09-025 has no open review finding.
