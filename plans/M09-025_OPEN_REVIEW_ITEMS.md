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
