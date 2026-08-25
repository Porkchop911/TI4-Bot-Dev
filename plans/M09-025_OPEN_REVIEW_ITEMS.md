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
