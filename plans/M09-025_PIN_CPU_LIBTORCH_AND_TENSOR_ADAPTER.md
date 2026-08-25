# M09-025 — Pin CPU libtorch/tch and tensor adapter

**ID and title.** M09-025 — Pin CPU libtorch/tch and tensor adapter.

**Milestone and dependencies.** M09; depends on row 019 (accepted). **Independent of M09-024** —
it can run in parallel with M09-024b1/b2 rather than after them.

**Normative references.** `docs/MLP_PLAN.md` revision 5 §7.1 (D19: "M09-025 pins the `tch` version
and matching libtorch distribution, verifies license and advisories, records compiler/CPU/driver/
runtime versions, and proves a CPU-only load"), §7.2 (D20: the deterministic thread/RNG
configuration), §4.4 (the manifest records pinned `tch`, libtorch, compiler and deterministic-thread
settings), and §8's risk row on Windows + libtorch.

**Acceptance test reference.** M09_LEARNED_POLICY row M09-025: "Pinned/license/advisory-reviewed P2
dependency; CPU deterministic tensor smoke and bounded adapter tests."

**Review tier.** C — architecture/dependency.

## Status — BLOCKED pending operator authorization

**P2, not authorized.** Operator decision D-2026-08-25-2 held this package. Nothing here has been
run: no crate added, no download, no `Cargo.toml` edit. This document exists so the authorization
question is concrete.

## Why the cost is much lower than the plan assumed

§8 records the expected friction as *"a large download, `LIBTORCH` env, DLLs on PATH."* Measured on
this machine, the download is probably **not needed at all**:

```
torch 2.9.1+cpu
cuda available: False
C:\Users\Niko\AppData\Roaming\Python\Python314\site-packages\torch\lib
24 files, 331.0 MB   (c10.dll, libiomp5md.dll, torch_cpu.*, …)
```

`tch` supports building against an existing PyTorch installation's libtorch (`LIBTORCH_USE_PYTORCH`)
rather than a separately downloaded distribution. §2's hardware note already records this install —
*"Installed torch is a CPU-only build (2.9.1+cpu)"* — so the branch's own CPU-only requirement is
already satisfied by bytes on disk.

**What is still a download, and it is small:** the `tch` and `torch-sys` source crates and their
transitive dependencies from crates.io — the ordinary `cargo add` path, kilobytes to a few
megabytes, not gigabytes.

**One thing that must be verified before this can be relied on, and cannot be verified offline:**
`tch` pins a specific libtorch version per release. If no `tch` release targets libtorch 2.9.1, then
either the pin moves to a `tch`/libtorch pair that does not match the installed Python torch, or a
matching libtorch distribution is downloaded after all. **The package must establish this first and
report it before anything is added to `Cargo.toml`.** Whether the fallback download is authorized is
a separate decision, not implied by authorizing this one.

## A provenance objection to linking the Python install in place

`LIBTORCH_USE_PYTORCH` points the build at a path inside a user-local Python installation. A
`pip install --upgrade torch` would then silently change the native library this project links
against — which is the opposite of "pinned", and exactly the class of defect this chain keeps
finding: a dependency resolved through a domain that something else controls.

**Recommendation:** copy the 331 MB library directory once into a project-local pinned location
(`out/libtorch-2.9.1-cpu/`, gitignored like the rest of `out/`), record a **SHA-256 manifest of
every file**, and point `LIBTORCH` at that copy. Cost: 331 MB of local disk, no download. Benefit:
the pin is a checksummed set of bytes this project owns, and a drift check is a manifest
comparison rather than a hope about pip.

The manifest is committed (it is text); the libraries are not, consistent with M09-020's artifact
policy and `.gitignore`'s `/out/`.

## Permission class and scoped access declaration

**Class: P2** — pinned external dependency.

| what | declared bound |
|---|---|
| **Network** | crates.io only, for `tch` + `torch-sys` + transitive source crates. **No libtorch download** unless the version check above forces one, which returns for a separate decision |
| **Reads** | the installed `torch` package directory, read-only |
| **Writes** | `out/libtorch-2.9.1-cpu/` (≈331 MB, gitignored); its committed SHA-256 manifest; `Cargo.toml` + `Cargo.lock`; one new crate `crates/ti4-tensor/` |
| **Does not** | touch CUDA, add any GPU dependency, or change any existing crate's behavior. No engine, policy, sim or training edits |
| **Reversibility** | the whole package is one revert plus deleting a gitignored directory |

## Deliverables

1. **The pin.** Exact `tch` version, exact libtorch version and build, and the SHA-256 manifest of
   the pinned library directory.
2. **License and advisory review.** libtorch is BSD-3-Clause with a bundled-dependency NOTICE;
   `tch`/`torch-sys` carry their own. Record every license actually shipped, and run an advisory
   check against the pinned versions. §10's artifact manifest wants provenance, not a claim.
3. **Environment record.** Compiler, CPU, OS, thread-library and runtime versions, per §7.1's
   "records compiler/CPU/driver/runtime versions". No driver on a CPU-only build; recorded as
   absent rather than omitted.
4. **`crates/ti4-tensor`** — a bounded adapter. Not the model: enough surface for M09-026 to build
   on, and nothing more. Tensor creation from a sparse `(index, value)` set, an embedding-bag style
   gather-and-reduce, a dense matmul, and the deterministic configuration hook.
5. **CPU-only load proof.** The library loads, reports CPU device, and reports **no CUDA**.
6. **Deterministic tensor smoke**, per §7.2: intra-op and inter-op thread counts pinned,
   deterministic-algorithm mode set, and the same input twice producing **bit-identical** output.
   §7.2 is explicit that if the installed API cannot enforce those settings the gate fails — so the
   test asserts the settings took effect, not merely that the calls returned.

## Invariants

1. **No CUDA anywhere.** Not a feature flag, not an optional dependency. M10-037 may add it later;
   nothing here may make CPU inference depend on a GPU path being present.
2. **The pin is bytes, not a version string.** A version string names what pip last installed; the
   manifest names what this project links.
3. **Determinism is asserted, not configured-and-assumed.** Setting a thread count and moving on is
   the failure mode §7.2 names.
4. **Nothing existing changes.** No behavior in any current crate moves. The workspace suite before
   and after must differ only by the new crate's tests.

## Explicit non-goals

- No model, no readouts, no heads (M09-026).
- No critic (M09-027), no bundle format (M09-028), no throughput gate (M09-029).
- No training, no optimizer, no CUDA.

## Known traps

- **The version-compatibility assumption.** "`tch` works with the installed torch" is the load-
  bearing claim of the cheap path and it is unverified. It is step one, and it reports back before
  `Cargo.toml` is touched.
- **The moving pin.** Linking into a Python site-packages directory is not a pin. See above.
- **Determinism that is set but not enforced.** §7.2's failure mode exactly.
- **Scope creep into the model.** The adapter exists so M09-026 has a floor. Every function it gains
  beyond that is a function M09-026's review will have to take on trust.
- **A green smoke test that proves the wrong thing.** "Two runs agree" is trivially true for a
  deterministic-by-accident tiny input. The smoke needs an input large enough that thread count
  could plausibly change reduction order, or it is not testing what it claims.

## Definition of done

Pin recorded with checksums; licenses and advisories reviewed and recorded; environment recorded;
adapter implemented with bounded tests; CPU-only load proven; deterministic smoke proven with the
settings asserted to have taken effect; workspace green; independent Tier-C review resolved.

**Authorship note.** Claude Opus 5 authors and cannot review it.

## The decision this needs

Authorize the P2 as declared — crates.io for the `tch` source crates, a local copy of the already-
installed libtorch, no gigabyte download — with the understanding that if no `tch` release matches
libtorch 2.9.1, the package stops and returns rather than downloading a different distribution on
its own initiative.
