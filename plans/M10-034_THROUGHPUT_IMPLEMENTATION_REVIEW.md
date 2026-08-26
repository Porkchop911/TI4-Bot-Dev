# M10-034 throughput implementation review

Reviewed frontier: `944bc539967a281e57e636dfc18d4f007348e0ed`

Verdict: **changes requested**. The two optimizations appear correct and the measured gain is
credible, but the committed tree and operational handoff are not yet clean.

## Findings

### [P1] The 3,000-update training run is no longer running

There is no live `ppo_update` process. `out/train_run001.log` ends during update 120 with exit code
`0xffffffff`. The last retained artifact in `out/checkpoints/run-001` is
`checkpoint-13552`, written at the update-100 report boundary at 22:08:59. The driver explicitly
states that resume is not built yet, so the work after that checkpoint cannot simply continue in
place.

This is operational rather than a defect in `944bc53`, but it must be made explicit before anyone
assumes the original training is still progressing. Decide whether to restart from the original
bundle with the faster binary, preserve the update-100 artifact as a separate run, or add a reviewed
resume path later. Do not silently describe a fresh run as a continuation.

### [P2] The committed dependency graph is incomplete

`Cargo.lock` has an uncommitted addition of `rayon` to the `ti4-mlp` package dependency list. Rayon
was added to `crates/ti4-mlp/Cargo.toml` in `200d5b9`, but the corresponding lockfile delta is still
only in the working tree. Commit the lockfile change with the package that owns it so a clean
checkout has the dependency graph that was actually compiled and benchmarked.

### [P2] `gather_reduce_batch`'s public documentation contradicts its implementation

`crates/ti4-tensor/src/lib.rs:378-399` still says the function gathers every distinct row once,
builds a `[distinct, width]` block, performs an `[options, distinct]` matmul, and contains an
`expect` that looks up a distinct column. The current function instead builds an embedding-bag
index list, contains no such matmul, and contains no such `expect`.

The new inline explanation at lines 412-418 correctly describes the current code, making the
contradiction especially visible. Rewrite the public rustdoc to describe the fused embedding-bag
path and its actual determinism/panic contract. The older claims in M09/M10 evidence should be
marked historical rather than used as evidence for the current implementation.

### [P2] The latest source delta is not rustfmt-clean

`cargo fmt --all -- --check` reports drift at the new `MlpBot::sharing` call in
`crates/ti4-mlp/examples/ppo_update.rs:136-138`; the chained calls need one additional indentation
level. The command also reports older M10-034-era drift in the same example and MLP sources. Format
the package-owned delta and keep unrelated pre-existing drift scoped separately if necessary.

### [P3] The changed boundary cases lack direct regression tests

The existing tests exercise non-empty batched gathering, and the 30-update campaign exercises the
shared actor in practice, but there is no focused regression for:

- an all-empty `gather_reduce_batch` input;
- a mixed empty/non-empty batch preserving zero rows in their original positions;
- six `MlpBot`s sharing one actor without changing their independent RNG, counters, or PPO records.

The new emptiness fold is straightforward and appears correct, but these tests would keep the
optimization from being accidentally reversed or subtly changed later.

### [P3] Removing `MlpBot::actor_mut` is an unrecorded public API break

The workspace has no caller, and mutation is incompatible with freely shared ownership, so removal
is understandable. It is nevertheless a public method removed as part of a throughput change.
Record the API decision explicitly, or preserve an invariant-safe mutation mechanism if downstream
users are in scope.

## What looks good

- `Rc<Actor>` is the appropriate ownership mechanism here. It permits sharing among seats and games
  confined to one Rayon worker while making cross-thread sharing impossible at the type boundary.
- Collection remains in deterministic chunk/job order.
- Replacing the global `distinct` sort/dedup with an emptiness fold does not affect aggregation
  order or the tensors passed to `embedding_bag`.
- The post-change benchmark processed slightly **more** decisions, not fewer. Over the first 30
  updates, the baseline processed 3,991,389 decisions in 12.10 s/update (about 10,999 decisions/s),
  while the new build processed 4,025,389 in 9.72 s/update (about 13,800 decisions/s). The resulting
  approximately 1.25x throughput gain is therefore not an artifact of a smaller workload.

The benchmark establishes the combined gain; it does not separately attribute optimizer and
rollout improvement to each code change. That is acceptable for the combined patch, but isolated
ablation runs would be needed for causal per-change claims.

## Verification performed

- `cargo test -p ti4-tensor --lib --locked`: **14 passed**.
- `cargo test -p ti4-mlp --lib --locked`: **62 passed**.
- Narrow `cargo clippy --no-deps -D warnings` for both libraries: **passed**.
- `cargo fmt --all -- --check`: **failed**, including the new call-site drift above.
- Full all-target Clippy could not reach the reviewed targets under `-D warnings` because of the
  known workspace `ti4-engine::game::apply_tactical` `too_many_lines` warning; this is not attributed
  to `944bc53`.

---

## Response — Claude Opus 5, 2026-08-26

All six findings accepted. Frontier for this response: the commit that follows this entry.

| finding | disposition |
|---|---|
| [P1] run no longer running | acknowledged; operational, see below |
| [P2] `Cargo.lock` incomplete | fixed — the `rayon` delta is committed with this change |
| [P2] `gather_reduce_batch` rustdoc contradicts implementation | fixed |
| [P2] not rustfmt-clean | fixed — `cargo fmt --all -- --check` passes |
| [P3] boundary cases lack regressions | fixed — three tests added |
| [P3] `actor_mut` removal unrecorded | recorded below |

### [P1] The run was stopped deliberately

The operator asked for it to stop; it was killed at update ~121, which is the `0xffffffff` exit.
`checkpoint-13552` from the update-100 report boundary is retained and was verified on write.

The review's instruction is accepted without qualification: **a fresh run will not be described as
a continuation.** Updates 101-121 are discarded, not banked. Any restart from the original bundle
is run-002 and starts at update 0 with its own baseline window.

### [P2] The rustdoc was the same defect the commit claimed to fix

Worth stating plainly rather than just fixing. `944bc53` corrected the *inline* comment about the
distinct-row gather and left the *public* contract above it describing the same replaced
implementation — including a `# Panics` section for an `expect` that no longer exists. Correcting
the smaller instance and walking past the larger one is exactly the failure the commit message
claimed to be closing.

The rewrite describes the fused embedding-bag path, keeps both superseded shapes and their
measurements under an explicit heading (they are the reason for the current design), and records
that `embedding_bag`'s CUDA backward is not run-to-run reproducible, pointing at
`plans/M10-032_DETERMINISM_FINDING.md`.

### [P3] `MlpBot::actor_mut` removal — recorded API decision

`actor_mut` returned `&mut Actor` and had no caller anywhere in the workspace. It is incompatible
with shared ownership by construction: `Rc<Actor>` cannot yield `&mut` while any seat holds a
handle, and the reason the sharing is sound is precisely that inference never mutates the model. A
mutation accessor would be a way to break that property, not a feature being given up.

Removed rather than preserved through `Rc::get_mut`, which would have compiled and then failed at
runtime for exactly the sharing case it exists to support. There are no downstream consumers of
this crate outside the workspace. `MlpBot::new` keeps its signature and wraps.

### On the causal attribution caveat

Accepted as stated. The 1.25x is a combined figure. The per-phase split is suggestive — rollout
4.70s to 3.99s alongside a change that only touches rollout allocation, optimise 7.41s to 5.73s
alongside a change that only touches the gather — but the two were measured together and no
ablation was run. No per-change causal claim is made on that basis.
