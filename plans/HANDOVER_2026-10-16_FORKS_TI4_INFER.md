# Handover: fork integration and the ti4-infer test, 2026-10-16

For a local Claude Code session on the owner's machine. This picks up from a cloud session that
could read everything but could not push to `main` or delete branches. Background on the forks
is in `plans/FORK_COORDINATION_PLAN_2026-10-14.md` (same branch).

Remotes used below:

```sh
git remote add pp    https://github.com/ProbablyPlayable/ti4-bot-dev   # if missing
git remote add feelx https://github.com/Feelx234/ti4-bot-dev           # if missing
git fetch origin && git fetch pp && git fetch feelx
```

## 1. Open actions, in order

### 1a. Fast-forward `main` to ProbablyPlayable's `main` (verified, not yet pushed)

- `origin/main` was `9bbded7a`; `pp/main` is `ded0b4d5`, 61 commits on top, a pure fast-forward.
- Owner decision pending: push it, or open a PR from `ProbablyPlayable:main` instead.
- Check that nothing moved, then push without force (a non-fast-forward is refused):

  ```sh
  git fetch origin main && git fetch pp main
  git merge-base --is-ancestor origin/main ded0b4d5 && echo ff-ok
  git push origin ded0b4d5:refs/heads/main
  ```

- What it brings: the cleanup overnight (one setup path, one wasm host, `ti4-view` folded into
  `ti4-host`, `web-shared/`, generated wire types), lobby seating, combat odds, reaction
  settings. Deletions are only in the fork server, host, wasm and web code (including the unused
  planning runner). Nothing in `plans/` or `AGENTS.md` changes.
- Verified on `ded0b4d5` (Linux, cloud):
  - `cargo fmt --check` clean.
  - Clippy on the browser-game crates: 0 warnings. Whole workspace: 0 errors; 6 style warnings in
    `ti4-mlp` examples (`type_complexity`, one `large_enum_variant`).
  - Fast tests, browser-game crates: 4,008 passed, 0 failed.
  - Whole workspace with Linux libtorch 2.9.1: 5,054 passed, 4 failed, 78 ignored, and 3
    `ti4-replayer` test binaries (`feed`, `online`, `shell`) that pass every test and then abort
    on exit inside libtorch (`free(): invalid pointer`). `main` does the same.
  - The 4 failures already exist on `main`: `ti4-review` `semantic_golden` and `ti4-mlp`
    `smoke_refusals` (2) are in `EXECUTION_STATE.md`; `ti4-sim`
    `profile::tests::fixture_capture_is_deterministic` needs the local pool
    `out/pools/full_np8_12_train.json`. On the owner's machine these may well pass.
  - `web`/`web2`: typecheck clean; web2 lint, format and 467 unit tests pass; `web` unit tests
    2,022 passed, 11 failed, all in one file that needs WebAssembly JSPI (Node 22 lacks it;
    `main` fails 9 of them the same way). Browser e2e not run.
- On Linux the torch crates only built with a local, uncommitted SHA manifest for the Linux
  libtorch; the repo pins only the Windows build. Irrelevant on Windows.

### 1b. Delete `wp/power-projection` (verified fully on `main`)

The owner approved the delete; the cloud session got HTTP 403.

```sh
git push origin --delete wp/power-projection     # tip was b29118d1ad85f232e734d331039195a7647a89c5
```

Power projection reached `main` in `c8563be7` (2026-10-08) by a different route, so git still
lists the branch's 7 commits as missing. Every line the branch added is on `main` or was reworked
there later. The one exception, the AGENTS.md Pi/Qwen note, was removed from `main` on purpose in
`52423f47`.

### 1c. Test ProbablyPlayable's `dev` (`ti4-infer`) on the owner's machine

`pp/dev` is `c955cd15`, 21 commits past `pp/main`. It adds `ti4-infer` (a libtorch-free MLP
forward with hand-written SIMD, the `.ti4m` file, `ti4-pack`), moves `ti4-policy::mlp` to one
decision pipeline behind `Scorer`, adds `--rollout-backend infer|infer-int8` to `ppo_update`,
and deletes `ti4-advisor` and `ti4-bot-agent`. The owner doesn't use those two crates and is fine
with the delete. Claims and design: `crates/ti4-infer/README.md` and
`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md` on `pp/dev`.

Not merged yet: the owner wants to test the speed claims first, because this is the training
stack.

## 2. What the cloud run measured (4-core 2.8 GHz Xeon VM; AVX2, FMA, AVX-512 VNNI, **no AVX-VNNI**)

Worktree at `pp/dev`, checkpoint `examples/reviewer/checkpoint-473312`, libtorch 2.9.1 CPU,
libtorch pinned to one thread by `ti4-tensor` (`set_num_threads(1)`), and rollout actors loaded
without autograd, so the comparison is single-thread against single-thread.

**Accuracy (f32):** worst logit gap 1.9e-5 relative, the same option drawn in every one of 4,328
decisions, PPO log-prob gap 2e-5. For f32 it is the same policy.

**Speed, debug test build** (what `cargo t-mlp-parity` and the README report):

| | libtorch | ti4-infer | ratio |
|---|---|---|---|
| Forward per decision, in game (AVX2) | 970 µs | 369 µs | 2.6x |
| Forward per decision, rollout bench (AVX2-FMA) | 841 µs | 315 µs | 2.7x |
| Critic per recorded decision | 217 µs | 57 µs | 3.8x |
| Whole 2-round game | 13.1 s | 12.3 s | 1.07x |

**Speed, release build** (`cargo test --release`, the configuration training actually runs):

| | libtorch | ti4-infer | ratio |
|---|---|---|---|
| Forward per decision, in game (AVX2) | 448 µs | 349 µs | **1.28x** |
| Forward per decision, rollout bench (AVX2-FMA) | 465 µs | 256 µs | **1.8x** |
| Critic per recorded decision | 104 µs | 18.6 µs | 5.6x |
| Forward + critic per game | 436 ms | 212 ms | 2.1x |
| Whole 2-round game (776 decisions) | 1,680 ms | 1,204 ms | **1.40x** |
| Feature extraction per decision | ~1.0 ms | ~1.0 ms | unchanged |

**Key finding: the headline f32 ratio is inflated by the debug build.** `ti4-infer` is optimised
in every profile (`[profile.dev.package.ti4-infer]`), but the Rust glue around tch in `ti4-mlp`
(building `SparseOption`s, tensors, the tch-rs calls) is not. libtorch's per-decision time halves
from debug to release (970 → 448 µs), while `ti4-infer` barely moves (369 → 349 µs). The README's
"libtorch 405 µs vs Avx2 168 µs" was measured the debug way. In release on this VM the real gain
is about 1.3–1.8x on the forward, 5.6x on the critic, and **about 1.4x on a whole rollout game**,
because feature extraction (~1 ms a decision) now dominates. The remark in the bench output, "(a
debug build of the engine and the features)", is a fixed string and is wrong under `--release`.

**int8 (`Avx2Vnni`) was not tested:** it needs AVX-VNNI (`is_x86_feature_detected!("avxvnni")`),
which this Xeon lacks even though it has AVX-512 VNNI. The README claims 27.8 µs per decision
(6x over AVX2) and 31.5 ms forward + critic per game, against the same debug-glue libtorch
baseline. Its own README also says int8 "is visibly not the trained policy": the drawn option
differs in about 2.6% of decisions. It recommends `Avx2Fma` for PPO until i16 W1 rows or a
measured PPO run shows the same learning.

## 3. What the local session should run

On the owner's machine, in a separate worktree, so the main checkout and `out/` stay untouched:

```sh
git fetch pp dev
git worktree add ../ti4-infer-test pp/dev
cd ../ti4-infer-test
# out/ is untracked: link or copy what ppo_update needs from the main checkout
#   out/pools/full_np8_12_train.json  (SHA-pinned Train role; holdout pools are refused)
#   the bundle to train from, e.g. the latest under out/checkpoints/mlp-critic
```

1. **Does the CPU have AVX-VNNI?** The bench prints `avx2-vnni: not on this CPU` if not.
   (Alder Lake and later Intel desktop chips and Zen 5 have it; Zen 4 has only AVX-512 VNNI.)

   ```sh
   cargo test -p ti4-infer --lib bench -- --ignored --nocapture --test-threads=1
   ```

   The bench asserts the README's i5-12600K times, so it may report "missed targets" on other
   CPUs; read the printed numbers, not the pass/fail.

2. **Parity and per-decision timing, release** (the number that matters):

   ```sh
   cargo test --release -p ti4-mlp --test infer_parity --test infer_rollouts -- --nocapture --test-threads=1
   ```

   Compare `forward per decision` and `the whole game` for libtorch against ti4-infer. On an
   AVX-VNNI CPU this also prints the int8 accuracy and speed.

3. **One real PPO update per backend** (end-to-end rollout throughput with the real pool and
   bundle; Rayon on every core, like real training):

   ```sh
   cargo build --release -p ti4-mlp --example ppo_update
   for b in cpu infer infer-int8 cpu infer; do
     ./target/release/examples/ppo_update --bundle <bundle> --updates 1 --seeds-per-update 4 \
       --no-checkpoint --out out/ppo-speedtest --rollout-backend $b
   done
   ```

   Read the `update 0  games …  rollout …  optimise …` line. Run each backend twice and
   interleave them; the first run warms the caches. Skip `infer-int8` without AVX-VNNI. `infer`
   requires a shared-critic bundle. Windows: use the same flags with `.exe`, from PowerShell,
   as `scripts/ppo_train.ps1` does.

4. **Before trusting int8 for training:** a short PPO run (`--updates` ≥ 20) with `infer` against
   `infer-int8` from the same bundle and seeds, comparing the training curves. The README says
   this has not been done.

**Decision rule to propose to the owner:** if release rollouts are ≥ ~1.3x faster on `infer`
(f32, same policy bits) and parity passes, merging `pp/dev` is low-risk; f32 `infer` can become
the rollout default. Keep `infer-int8` opt-in until step 4 shows it learns the same.

## 4. Left out on purpose (owner's call, nothing pending)

- `origin/codex/diplomacy-v1`: a work-in-progress snapshot plus old training configs.
- `origin/wp/online-multiplayer`: 4 commits from 2026-10-01, the `ti4-join` Windows installer.
- `origin/codex/bf-ghost*`: superseded by the Ghost work on `main`.

## 5. Cloud-session paths (not available locally; for reference only)

Logs on the cloud VM: `scratchpad/infer-logs/` (`bench.log`, `parity.log`, `release.log`) and
`scratchpad/cand-logs/` (the `ded0b4d5` verification).
