# MLP bots without libtorch: inference in ti4-host, on the server and in the page (2026-10-15)

Status: planned. Step 1 done (2026-10-15). Step 2 done (2026-10-15). Step 3 done (2026-10-15). Step 4 done (2026-10-15). Step 5 done (2026-10-15). Step 6 done (2026-10-15). Step 7 done (2026-10-15).

## Context

The trained MLP policies (`plans/CHAMPIONS.md`) can play only through `web/` and a chain of three
processes:

1. `ti4-server` spawns one `ti4-bot-agent` per MLP seat (`session/registry.rs:3417-3518`, gated by
   `TI4_BOT_PASSWORD` and off when `TI4_PUBLIC` is set).
2. The agent joins the game over the WebSocket like a person, and POSTs every choice to
   `ti4-advisor`.
3. `ti4-advisor` scores the choice with `ti4-mlp` on libtorch (`tch`), and nobody starts it but a
   person.

web2 does not offer the MLP bot, the page cannot run it, and the advisor's features already differ
from `MlpBot`'s (it drops the battle and deal facts and uses `Baseline::default()`,
`ti4-advisor/src/lib.rs:348-409`).

The network is small enough to run without libtorch. The forward pass (`ti4-mlp/src/lib.rs:1-37`,
`trunk` `:1218`, `logits` `:1245`) is:

- a weighted embedding-bag sum over `W1 [capacity ≤ 65536, width]`, plus the faction identity
  (`embedding[33,16]` zero-padded) and `b1`, then ReLU;
- `W2 [width,width]` (stored `[out,in]`) with `b2`, then ReLU;
- 0–8 residual blocks `z + W_out·relu(W_in·z + b_in) + b_out`;
- one scalar logit per option: `z·(w_shared[h] + delta[f][h]) + b_shared[h] + b_delta[f][h]`.

`width` is 128 or 256. A decision has 6.2 options on average and at most a few hundred, with about
85 active columns per option. Feature extraction (`ti4_policy::projection::mlp_choice_features`) is
already pure Rust. Only `ti4-tensor` and `ti4-mlp` need `tch`.

Goal: the MLP is a bot kind like the rule bot. It is played in-process by `ti4-host` on both hosts
(the server and the engine of the page), with hand-written SIMD kernels (SSE on x86-64, simd128 on
wasm) and no new crate dependency. `ti4-advisor` and `ti4-bot-agent` are removed.

Decisions (owner, 2026-10-15):

- **The page downloads the model from the server**, `GET /api/models/<sha256>.ti4m`, as f32. The
  bits are those of training.
- **MLP bots are on every server**, the public one included. They are in-process bot kinds, and
  the password, the agent process and `TI4_MAX_ACTIVE_BOTS` go away.
- **Several named models.** Each model is its own bot kind, `mlp:<name>`, listed by the setup
  options.
- **web/'s advisor client and its MLP add-bot UI are deleted** with the crates.
- **No new dependency.** The kernels use `core::arch`; safetensors, the model file and the
  manifest are parsed by hand with `serde_json` and `sha2`, which the workspace already has.

## Design

### A. The crate `ti4-infer`

A new crate with no `tch`, clean for wasm: the numbers only. It knows nothing of the game and
depends only on `serde_json` and `sha2`. It does not depend on `ti4-policy`, and `ti4-policy` does
not depend on it: the decision pipeline is in `ti4-policy` behind a trait (section D).

```
ti4-infer   format, kernels, Model                        (serde_json, sha2)
ti4-policy  features, the decision pipeline, Scorer       (no ti4-infer, no tch)
ti4-host  → ti4-policy, ti4-infer
ti4-mlp   → ti4-policy, ti4-infer, tch
```

| Module        | What it holds                                                                                                                                                                                                                                                                                                      |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `format`      | The `.ti4m` file: its reader and its writer (section B).                                                                                                                                                                                                                                                           |
| `bundle`      | The reader of a training bundle directory (`ti4-mlp/src/bundle.rs` layout): the `safetensors` header (u64 length, JSON, raw f32 LE) parsed by hand, and the checks of `manifest.json` (schema 7–12, `projection_abi`, the SHA-256 of each file, f32 only, shapes). It is only used by the pack tool and the tests. |
| `simd/`       | The `Lanes` trait, the five backends and the dispatch (section C). This is the only module with `#[allow(unsafe_code)]`. The crate restates the workspace lints with `unsafe_code = "deny"`, as `ti4-tensor` does.                                                                                                 |
| `kernel`      | `bag`, `gemm` (packed panels), `readout`, generic over `Lanes`.                                                                                                                                                                                                                                                    |
| `model`       | `Model`: the sections of one file and `forward(faction, head, options: &[Vec<(u32, f32)>]) -> Vec<f32>` with a reused scratch buffer. It is `Send + Sync` and shared by `Arc` across seats and games.                                                                                                              |
| `bin/pack.rs` | `ti4-pack <bundle dir> <name> <out.ti4m>`.                                                                                                                                                                                                                                                                         |

### B. The model file `.ti4m` (the prepacking)

The pack tool reads a bundle, checks it, and writes one file. The server and the page read only
this format.

```
"TI4M" | u32 version = 1 | u32 header length | header JSON | zero pad to 64
sections, each 64-byte aligned, little-endian:
  w1         f32 [capacity][width]          rows, as gathered
  embedding  f32 [33][16]
  b1, b2     f32 [width]
  w2         f32 packed panels (below)
  r{i}_in, r{i}_out  packed panels; r{i}_b_in, r{i}_b_out f32 [width]
  readout    f32 [33][heads][width]         w_shared[h] + delta[f][h], added once in f32
  readout_b  f32 [33][heads]                b_shared[h] + b_delta[f][h]
  slots      the vocabulary JSON            Vocabulary::from_json_for_inference
  battle     battle_predictor.json          schemas 11/12 only
trailer: SHA-256 of all bytes before it
```

- The header has the name, the label shown in the lobby, the temperature, `width`, the head names,
  the faction roster, `capacity`, the number of residual blocks, and the source: the sha of the
  bundle manifest, the schema and `projection_abi`. It also has a table of the sections (offset,
  length, layout).
- The critic and the value head are left out. No bot reads them.
- **Panel layout of a dense matrix** (`W[out][in]` in the bundle): `out/8` panels. Each panel holds,
  for each `k` in `0..in`, the 8 weights `W[p*8 .. p*8+8][k]` side by side, so a panel is
  `in × 8` f32 (8 KiB at width 256) and fits L1. `width % 8 == 0` is checked at pack time and at load.
- Loading copies the file once into a 64-byte-aligned buffer. Each section is a slice into it, and
  nothing is converted.

### C. The kernels: one source, five backends

```rust
pub(crate) trait Lanes {            // N × f32: 4, or 8 on AVX2
    const N: usize;  const SPLAT: usize;  const MR: usize;
    type V: Copy;  type Readout: Readout;            // the 4 lanes of the readout
    fn zero() -> Self::V;  fn splat(x: f32) -> Self::V;
    fn load(src: &[f32]) -> Self::V;  fn load_splat(src: &[f32]) -> Self::V;
    fn store(dst: &mut [f32], v: Self::V);
    fn add(a: Self::V, b: Self::V) -> Self::V;  fn mul(a: Self::V, b: Self::V) -> Self::V;
    fn mul_add(acc: Self::V, a: Self::V, b: Self::V) -> Self::V;   // add(acc, mul(a, b)) but on Avx2Fma
    fn relu(v: Self::V) -> Self::V;
}
pub(crate) trait Readout: Lanes { fn hsum(v: Self::V) -> f32; }   // (l0 + l1) + (l2 + l3)
```

The trait is safe: a load or a store checks the length of its slice, and the `unsafe` of the
intrinsics stays in `simd/`.

**The backends**

| Backend   | Selected by                                                    | Instructions                                                                                       |
| --------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `Sse`     | `cfg(target_arch = "x86_64")`                                  | `core::arch::x86_64` `_mm_*_ps`. SSE2 is the baseline of x86-64, so there is no runtime detection. |
| `Simd128` | `cfg(all(target_arch = "wasm32", target_feature = "simd128"))` | `core::arch::wasm32` `f32x4_*`.                                                                    |
| `Scalar`  | the default of every other target; available on every target   | `[f32; 4]` with the same lane arithmetic. The reference of the tests.                              |
| `Avx2`    | x86-64, `is_x86_feature_detected!("avx2")` once at load        | `_mm256_*_ps`, 8 lanes, a multiply then an add. The default on a native host with AVX2.            |
| `Avx2Fma` | x86-64, AVX2 and FMA detected, and asked for by the caller     | `_mm256_fmadd_ps`. Not bit-identical to the others; see below.                                     |

A model picks its backend once, when it is loaded: the public `ti4_infer::Backend` enum,
`Backend::best()` (the fastest deterministic one available, the default of a loaded model),
`Backend::fastest()` (`Avx2Fma` where the CPU has it), `Model::with_backend` (refuses one the
target or CPU lacks), and `Model::backend()`; `ti4_infer::backend()` is `Backend::best()`. A
forward pass matches the backend once (`simd::run`) and runs the forward monomorphized for it,
so the kernels have no dispatch. For the AVX2 backends `run` calls a
`#[target_feature(enable = "avx2")]` (or `"avx2,fma"`) function after `is_x86_feature_detected!`,
and the forward, the kernels and the lane operations are `#[inline(always)]`, so the whole
forward is compiled into that function with its features. Nothing on that path may pass an
intrinsic through a closure to a generic function of `std` (`array::from_fn`, `map`): that
function is compiled without the features and cannot inline it. The `Avx2` types are private to
`simd/`, so `run` is the only way to them. There is no relaxed SIMD in wasm:
its fused multiply-add may or may not fuse, by engine and CPU, and the page keeps `Simd128`.

**`Avx2` keeps the bits.** In `bag` and `gemm` every lane is its own output, and the sum of each
output runs over `k` in the same order whatever the vector width, so 8 lanes give the bits of 4.
`readout` keeps its 4-lane partial sums and `hsum` (below) on every backend.

**`Avx2Fma` is fast, not identical.** Each multiply-add rounds once, so its logits differ from the
others by about one rounding per term. It is used where no other host must reproduce the game: the
rollouts of training (step 5), and on the server when `TI4_INFER_FMA=1`. Its tests compare it with
`Scalar` within `1e-4 · max(1, |logit|)`, not bit for bit. A game on the server with FMA is replayed
from its saved decisions as any game is; only the test that plays one table on both hosts runs
without it.

**The same bits everywhere** (every backend but `Avx2Fma`)

- There is no FMA: always a multiply, then an add.
- Every loop has a fixed order of accumulation.
- The ReLU is defined once as `x > 0 ? x : 0`. On SSE that is `_mm_max_ps(x, zero)`, on wasm
  `f32x4_pmax(zero, x)` (not `f32x4_max`, which treats −0 and NaN differently), and on `Scalar` the
  same comparison.

So the deterministic backends give bit-identical logits, and a game is the same game on the server and in
the page.

**The kernels**, all with `width` a multiple of 8:

- `bag`: for each option, `acc[width] = Σ v_j · W1[col_j]` over the columns. The columns are sorted
  and duplicates folded first, with the order and tie-break of `ti4-tensor/src/lib.rs:442-534`
  (`total_order_key`, moved into `ti4-infer`; the sort is an unstable sort of one `u64` key per
  pair, the column above the key of the value, which gives the same order). An empty bag is a
  zero row. The accumulator walks `width/N` vectors in blocks of 8 that stay in registers. The epilogue is `+ identity`, then `+ b1`, then ReLU: the order
  of `trunk`.
- `gemm`: the micro-kernel is **MR option rows × 2 vectors of outputs** (one panel on 4 lanes,
  two panels on 8), one accumulator each. MR is 6 on `Sse`, `Avx2` and `Avx2Fma` (12
  accumulators, 2 weight vectors and a broadcast: 15 of the 16 XMM or YMM registers, and a
  decision of 6 options is one call) and 4 on `Simd128` and `Scalar` (V8 keeps an XMM register
  for itself).
  - The columns are the outer loop and the row blocks the inner loop, so the panels stay in L1
    while every option of the block streams past them. A width that is an odd multiple of 8 leaves
    one panel on 8 lanes, done with one vector.
  - The tail of fewer than MR rows uses the same kernel with `const ROWS: usize`.
  - A (the activations) is packed once per block of 48 rows, by groups of the rows of one call
    and in a group `[k][row][SPLAT]`, so the micro-kernel walks A and the panels side by side with
    no bounds check. On `Sse`, `Simd128` and `Scalar` each element is stored 4 times (`SPLAT = 4`),
    so the micro-kernel loads each broadcast in place of building it at every panel (on SSE a
    broadcast is a shuffle); on AVX `vbroadcastss` broadcasts from memory in one load
    (`SPLAT = 1`).
  - The epilogue is fused: bias and ReLU (layer 2), or the residual add (blocks).
  - K = width is at most 256, so there is no K blocking.
- `readout`: `z · w[f][h] + b[f][h]` for every option, with 4-lane partial sums and `hsum`, on
  the 4-lane `Readout` of the backend (`Sse` for both AVX2 backends, unfused).

**Build.** `.cargo/config.toml` gets `[target.wasm32-unknown-unknown] rustflags = ["-C",
"target-feature=+simd128"]`. Every browser with JSPI, which the engine already requires, has
simd128. The native aliases are not affected.

**Performance target**, single thread, release, x86-64 (measured by an ignored test,
`ti4_infer::bench`; the reference machine is a Core i5-12600K, a P-core at 4.9 GHz):

| Model at width 256 | Backend   | 6 options | 512 options | Measured (6 / 512)        |
| ------------------ | --------- | --------- | ----------- | ------------------------- |
| 0 residual blocks  | `Sse`     | < 30 µs   | < 4 ms      | 27 µs / 3.0 ms            |
| 2 residual blocks  | `Avx2`    | < 55 µs   | < 5 ms      | 49 µs / 5.1–5.2 ms        |
| 2 residual blocks  | `Avx2Fma` | < 35 µs   | < 4 ms      | 38.5–39.5 µs / 4.0–4.1 ms |

The bench asserts the targets that the code meets (both of `Sse`, and `Avx2` at 6 options) and
prints the three it misses, marked "printed, not asserted". It also prints `Scalar` and `Sse` at
2 residual blocks (Sse 90 µs / 8.5 ms) for the comparison. Without FMA, SSE does 8 multiply-adds
a cycle, so a residual block costs about 30 µs at 6 options; 8 lanes halve that and FMA halves it
again: on `Avx2Fma` a dense layer of 6 options takes about 6 µs, against a bound of 5 µs at 2 FMA
a cycle. What keeps `Avx2Fma` above 35 µs is the bag: with 2 residual blocks the weights (1.3
MB) and the rows of 6 options (0.5 MB) do not fit the L2 together, so the bag reads its rows from
L3, about 10 µs (2 of them the sort of random columns) on every backend. The 512-option case is
bound by the memory of the bag (about 43 MB of rows read from a 21 MB table with random columns,
1.5 ms), so the vector width helps it less. Prefetching the rows (the next block of a row, the
rest of a row, the rows of the next option) and walking each row in one pass were tried, and each
was slower. Decisions are batched within a decision (the options are the rows), not across
decisions.

### D. One decision pipeline, shared with training

The pipeline is the part of `MlpBot::decide` that does not touch tch. It is the module
`ti4_policy::mlp`, with no dependency on `ti4-infer` or tch. A `Pipeline` belongs to one seat: it
holds the vocabulary (`Arc<Vocabulary>`), the faction row, the setup baseline, and the state a
seat carries from one decision to the next (the fleet plan, the memo of resolved columns, the
power maps). A decision is two calls with the forward pass between them, so that PPO can keep what
lies between them:

- `features(model, choice, seen) -> Features`: `Planned(option)` when the seat's fleet plan
  answers the choice, else `Score(Prepared)` with the faction row, the head and its row, the
  options as columns, and the options of the choice they stand for;
- `pick(prepared, logits, temperature, rng) -> Picked`: the option, its index and the
  probabilities. `pick_with(prepared, probabilities, rng)` draws from a distribution computed
  elsewhere (the GPU batch client returns probabilities, not logits);
- after an activation, `fleet(model, choice, seen, picked) -> Option<Prepared>`: the fleet
  decision, scored and picked the same way; its pick becomes the plan;
- `decide(scorer, choice, seen, temperature, rng)` runs all of it for a host that only plays;
- `carry()` and `restore(carry)`: what the later decisions of the seat depend on (the plan, the
  power maps, the table of the activation); `follow(scorer, choice, seen, chosen, temperature,
  rng)` gives a recorded decision again and changes the pipeline as `decide` did (section E).

`ti4-host` calls `decide`. `MlpBot` calls the steps and records the columns and probabilities for
PPO in between. Together they cover:

- the fleet plan packages (`planned`, the menu of `tactical_plan::packages`) and their trace
  (`PlanTrace`, drained by `take_trace`);
- `mlp_choice_features` (with the card-text families when the vocabulary places them), plus the
  battle facts when the model has a predictor, the deal facts when the vocabulary assigns
  `FACT_SCORE`, and the power facts when it assigns `power_facts::NAMES`;
- the columns of the vocabulary, with the memo of resolved keys, as `Columns = Vec<(u32, f32)>`
  per option;
- `decision_head` and its resolution in the model's `HeadLayout` (`Legacy`, 14 heads; `Diplomacy`,
  15), which lives here with `FACTION_ROSTER`; `ti4-mlp` re-exports both;
- `softmax` at the temperature (`probabilities`: each `f32` logit divided by the temperature
  rounded to `f32`, as libtorch divides a tensor by a scalar, so the bits are those of
  `Actor::probabilities`);
- the draw (`draw`, the cumulative walk of a uniform `[0, 1)`).

The model behind it is two traits in `ti4_policy::mlp`:

- `ModelInfo`: what the features need of a model: `layout() -> HeadLayout`,
  `battle_predictor()`, and `temperature()` (default 0.25).
- `Scorer: ModelInfo`: `logits(faction, head, options: &[Columns]) -> Vec<f32>`.
- `ti4_infer::Model::forward` takes the same lists, `&[Vec<(u32, f32)>]`, so neither side
  converts, and the two crates share no type. The type is concrete, not generic, so that the
  forward pass is compiled in `ti4-infer` with its own optimization.
- `ti4-host` implements `Scorer` for `ti4_host::bots::MlpModel`, around an `Arc` of the
  `ti4_infer::Model` (`HeadLayout::of(model.heads())`, the predictor from `model.battle_json()`).
- `ti4-mlp` implements `Scorer` for its `Actor` on tch, `ModelInfo` for the GPU batch client
  (no battle predictor: the service refuses an arena bundle), and `Scorer` for
  `ti4_mlp::infer::InferModel`, a `ti4_infer::Model` built from an actor (step 5). `MlpBot` takes
  any of them (`sharing`, `batched`, `on_infer`), so training, evaluation and play run one
  pipeline.
- The tch `Scorer` stays because tch still plays: the GPU batch client, the parity test, and the
  evaluations of a checkpoint during training.
- The PPO records, the GPU batch client and the critic stay in `ti4-mlp`.
- `ti4-policy` is wasm-clean: `rand` without default features, as `ti4-rule-bot` has it, so
  nothing pulls in `getrandom`. `cargo wasm-policy` checks it for `wasm32-unknown-unknown`.

### D2. Rollouts of training on `ti4-infer`

Self-play needs only the forward, and on the CPU a decision of about 6 × 256 is mostly the dispatch
of tch ops. `ti4-mlp` has the module `infer`:

- `parts_of(actor, slots)` copies the tensors of an `Actor` into `Parts` in memory (no bundle on
  disk; the source names no manifest, all zeros, and the schema and ABI that a bundle of the actor
  would have). `InferModel::from_actor(actor, slots, backend)` packs them with `format::write` and
  loads them with `format::read`: the code path of a `.ti4m`, so the rollouts play the bits that a
  host would play with the same weights. It takes 40 ms for the reviewer checkpoint (23 MB), once
  per update, so there is no separate in-memory constructor.
- `InferModel` implements `ModelInfo` and `Scorer` (a thread-local `Scratch`), and is `Send +
  Sync`: one `Arc<InferModel>` serves every rollout worker, and after each PPO update the workers
  get a fresh one of the updated actor. `MlpBot::on_infer` seats it.
- The backend of the rollouts of a run: `ppo_update --rollout-backend cpu | infer | gpu-batched`.
  `cpu` (tch, one actor copy per worker) stays the default. `infer` is `Backend::fastest()`
  (`Avx2Fma` where the CPU has it); the frozen benchmark of `--opponent` is packed once and plays
  on it too. The update stays on tch: it computes the probabilities of the current weights on tch
  in every epoch, as before, against the behaviour probability that the rollout recorded from the
  distribution it drew from (that of `ti4-infer`, within `2 · 1e-4 · max(1, |logit|) / T` of tch's
  log-probability). `--check-likelihood` rescores on tch with that tolerance on `infer`, and with
  1e-6 on `cpu`.

**The critic.** A recorded decision needs `V(s)` at the moment it is played (the behaviour value of
the advantage, `CriticMode::Shared` and `Separate`); the update does not recompute it. On tch the
CPU bot calls `Actor::value` after the draw, and the GPU service returns it with the
probabilities. On `ti4-infer` the shared critic is the trunk of the policy with the readout
`w_value`, `b_value`, so `Model::with_value(w, b)` adds that readout to the in-memory model of
training, and `Model::value_with` runs the trunk on the critic columns and ends with it, with the
order of every sum of a logit. A value head is never in a `.ti4m`: `format::write` packs `Parts`,
which have none, and `format::read` gives a model without one. A separate critic has its own trunk,
which a `Model` does not carry: `--rollout-backend infer` refuses it, and `InferModel::value`
refuses a model without a value head. `BatchMean` records no value. This keeps the whole decision
off tch; calling tch for the value alone would cost a tch forward per recorded decision (54 µs).

**The bench** (`infer_rollouts`, reviewer checkpoint, width 256, a game of two rounds at
temperature 1 with a value at every recorded decision, 806 decisions of which 756 recorded; libtorch
release, `ti4-infer` optimized, the engine, the features and the bot a debug build):

| Rollout backend       | Forward per decision | Critic per recorded decision | Forward + critic per game |
| --------------------- | -------------------- | ---------------------------- | ------------------------- |
| tch                   | 297 µs               | 54 µs                        | 281 ms                    |
| `ti4-infer` `Avx2Fma` | 165 µs               | 16.5 µs                      | 145 ms                    |

The forward stage includes the softmax and the cold caches of a game (the parity test measures the
forward alone: tch 375–405 µs, `Avx2` 165 µs). The whole game takes 4.8 s on either backend in this
debug build, where the features (about 4 ms a decision) dominate; in a release build of the
rollouts the forward and the critic are the part that `ti4-infer` halves.

**The draw of a bot.** The RNG is `ChaCha8` from `(game seed, seat, decision index)`, as the rule
bot's choice is (`ti4_rule_bot::choose`). The temperature is the model's (header, default 0.25, the
evaluation temperature of `plans/MLP_TEMPERATURE_REGIME.md`).

### E. Bot kinds in ti4-host

- `ti4_host::setup::bot_kind` keeps `RULE = "smart"` and `RANDOM = "random"`. An MLP kind is
  `mlp:<name>` (`ti4_host::bots::MLP_PREFIX`); a bare `mlp` is not a kind.
- `ti4_host::bots`:
  - `MlpModel`, the `Scorer` of the pipeline: an `Arc` of the `ti4_infer::Model` with its
    `HeadLayout::of(model.heads())`, its vocabulary (`Vocabulary::from_json_for_inference` of the
    slots) and its predictor (from `model.battle_json()`), checked at load; a thread-local
    `Scratch`. It compares by name and digest.
  - `BotKind { Smart, Random, Mlp(MlpModel) }`, written as its id. `BotKind::parse(id)` finds the
    model in the registry of the process (`register`, `loaded`, `models`); a model that is not
    there is the error "the model '<name>' is not loaded". The server loads its directory into it
    at start, the page loads a model with `ti4_model_load`. A loaded name replaces the model of
    that name; a seat that plays the old one keeps it.
  - `Bot`, one seat of a game: `Smart`, `Random`, or `Mlp` with the model and the `Pipeline` of the
    seat (its faction row, and its baseline taken from the game as it starts; a faction with no row
    is played by the rule bot). `bots::choose(bot, choice, seen, seed, place, index)` is the one
    answer of a bot seat on both hosts, in place of the server's `RandomBotDecider` with `.smart()`
    and of the wasm `pick` and `Replayed::decide`. An MLP seat draws from
    `ti4_rule_bot::random::decision_rng(seed, place, index)`, as the rule bot does, at the model's
    temperature; without a seat observation, or when the pipeline or the model refuses, the rule bot
    answers.
- **What an MLP seat carries, and recovery.** The rule bot keeps nothing between two decisions; an
  MLP seat's pipeline does: the fleet plan of the activation it is moving, its power maps
  (`PowerCache`, which is frozen during an activation and refreshed only where the board changed,
  so it is not a pure cache), and the table of power of its latest activation. That is the `Carry`
  of `ti4_policy::mlp::Pipeline` (the memo of columns only caches the vocabulary and is left out).
  So that a game played again goes on as the game that was not interrupted:
  - A host that plays a game again from its decisions gives each recorded decision of an MLP seat
    to `Bot::follow` (`Pipeline::follow`), which changes the pipeline as `decide` changed it: the
    plan's answer, the power maps and the table, and after an activation the fleet decision drawn
    with the same generator (one draw for the activation, whatever its distribution, then the fleet:
    one forward pass). The features of the options and their forward pass are not computed. The
    server does it in its `ReplayingDecider` (a recovered game, a new worker after an undo or a
    redo), the page in `saved` (a saved game).
  - The server never starts a worker from a copy of the game that skips the decisions of an MLP
    seat: a table with an MLP seat has no step-boundary copy (`SessionConfig::resume`, the tail copy
    of `replay_boundary`), and its worker replays the whole log.
  - The page keeps the carry of every seat with each checkpoint and restores it with the
    checkpoint (an undo); the copy of the game for a draft plays on with copies of the bots
    (`Bot::detached`) at the carry of its checkpoint; a peek of a seat's answer (the wait of a table
    before a reaction window) restores the carry after it.
  - A seat that changes hands in the middle of an activation (a person takes it over, or the page
    switches its seats) is followed only for the decisions that its bot made, as live.
- `setup_options(host, delay, models: &[ModelEntry])`: the host lists the kinds from the models
  that it is given, `ModelEntry { name, label }`. Each is a `BotKindOption` with the id
  `mlp:<name>` and the label of the model, after the rule bot and the random seat. The page still
  never writes a kind (`web2/docs/agents/lobby.md`).
- **Wire.**
  - The kinds of a seat are strings, ``"human" | "smart" | "random" | `mlp:${string}` `` in
    TypeScript (`#[ts(type)]` on the newtypes `SeatKind` and `Bots` and on `BotKindOption::id`).
  - `Local.rule_seats` (a mask) is `Local.bots: (string | null)[]`, the kind of each seat. A save
    of format 2 with `rule_seats` reads as `smart` on those seats (a field that is read and never
    written); the format stays 2.
  - `ti4_setup_options(request_len)` reads `OptionsRequest { players, models }` from the request
    buffer.
- **The server**
  - It loads every `.ti4m` of `TI4_MODEL_DIR` that `index.json` lists (`ti4_server::models`: an
    entry has `name`, `sha256` and an optional `file`, default `<name>.ti4m`; the sha and the name
    of the header must be the index's), before it recovers its games, and prints each name, label,
    sha and backend. A model that does not load stops the start. With no directory, no MLP kind is
    offered. `TI4_INFER_FMA=1` loads them on `Backend::fastest()`.
  - `SeatController::{BotSmart, BotRandom}` are `SeatController::Bot(BotKind)`, written
    `{"Bot": "<id>"}`; a record of an earlier build with `BotSmart` or `BotRandom` reads as
    `smart` or `random`. MLP kinds are in-process kinds (`in_process_bot_kinds()`,
    `add_in_process_bots`, `/lobby/bots`; a seat is named by the label of its model and a number)
    and are persisted with the table, so a recovered game keeps its MLP seats.
  - A table whose model is not loaded refuses to start (`LobbyError::ModelMissing`, 409), with the
    name of the model; a recovered game whose model is not loaded is not recovered, with the name.
- **The engine of the page**
  - `ti4_model_load(len)` takes the bytes of a `.ti4m` through the input buffer of
    `ti4_host_in(len)`, checks the trailer sha, loads the model on `Simd128` and puts it in the
    registry by name (`ti4_host::bots`). The input buffer is emptied after it.
  - `ti4_host_start` refuses a game with a kind whose model is not loaded, with its name.
  - `ti4_setup_options` gets the model list in its request. `ti4_infer_check` stays: the vitest of
    the bits of the wasm forward uses it.
- **ti4-policy in the web build.** `ti4-host` and `ti4-wasm` depend on `ti4-policy` and
  `ti4-infer`, and `ti4-policy` is in the package set of every web alias, so it is built and its
  unit tests run with `cargo t-web` (its one long test is `slow_`). `ti4.wasm` (`wasm-release`) is
  7.55 MB raw, 1.91 MB gzip and 1.31 MB brotli, against 6.89, 1.73 and 1.20 MB before.
- **The model of the tests** (`ti4_host::bots::test_model`, `test_model_bytes`): the synthetic
  model of `ti4_infer::synthetic` at width 16 with one residual block, a vocabulary of the
  reserved columns that also places the power facts and the deal value, a battle predictor of fact
  version 8 (schema 12), temperature 1 (so its seats also pick the fleets of the menu and play
  their plans), on `Scalar`. It packs in microseconds; the reviewer checkpoint would make the games
  of the tests slow in a debug build.

### F. Download and the lobby in web2

- **The server's routes**
  - `GET /api/models` returns `[{ id, label, sha256, bytes }]`.
  - `GET /api/models/<sha256>.ti4m` serves the file from `TI4_MODEL_DIR` with
    `Cache-Control: immutable`, and the `.br` that the pack script writes next to it when it is
    smaller.
  - The address is the same origin, so the CSP of `deploy/public/Caddyfile` allows it.
- **Single player** (`localTable.ts`)
  - It asks `/api/models` once and passes the list to `ti4_setup_options`. With no server, or no
    model, there is no MLP kind, as with any kind the options do not name.
  - When a seat gets an MLP kind, the page fetches its file into the Cache API (keyed by the sha
    URL, so a file is fetched once per browser) and hands the bytes to the Worker as a transferred
    `ArrayBuffer`, which calls `ti4_model_load`.
  - "Start game" waits until the files of the table are loaded. The footer says the progress
    ("Loading Stage 2 r5 · 12 / 21 MB"), as it says why a start waits.
- **A saved single-player game with MLP seats** loads its models before `ti4_host_start`.
  - When a model cannot be had (no server, a name the server dropped), the page says so in place of
    the game, as when the engine does not load.
  - The saved decisions would replay without it, but the game cannot go on.
  - The game decides with the model the name has now. The save keeps the name, not the sha.
- **The lobby** (`select/lobby.ts`, `model/lobby.ts`, `select/players.ts`)
  - Every `mlp:` kind of the options is a button in the row of an open seat, labelled by its model,
    after "Rule bot" and "Random".
  - Its sign is "✦" with the label of the model on hover, in the lobby, in the Table sheet and in
    the player table (pattern 4). `KIND_SIGNS` matches the prefix `mlp:`.
  - "Fill with rule bots" stays the usual target.
  - The row of an open seat must fit with three models at 1920×1080 and on a phone. A server offers
    at most three models (`index.json` marks them `offered`).
- **Docs.**
  - `web2/docs/agents/lobby.md`: the kinds, the sign, and the load before the start.
  - `web2/docs/agents/local-play.md`: the model in the Worker, the Cache API, and a saved game with
    a model.
  - `crates/ti4-wasm/README.md`: the new export.
  - `web2/AGENTS.md`: the crate list and the `ti4-infer` gate.

### G. The models of a deploy

- `models/index.json` (committed) names each offered model: name, label, temperature, the source
  bundle and the sha of its manifest, the sha of the packed file, and `offered`.
- `scripts/pack-models.sh` runs `ti4-pack` for every entry into `out/models/` (gitignored), writes
  the `.br`, and checks the shas against the index.
- `deploy/public/deploy.sh` uploads `out/models/` to `/var/lib/ti4-public/models`. `env.example`
  gets `TI4_MODEL_DIR` and loses `TI4_BOT_PASSWORD`, `TI4_ADVISOR_URL`, `TI4_BOT_AGENT_BIN` and
  `TI4_MAX_ACTIVE_BOTS`.
- **Tests do not need `out/`.** They pack the committed bundle `examples/reviewer/checkpoint-473312`
  in memory (`ti4_infer::bundle` + `format`), or a small synthetic model with seeded random weights
  (width 128, 2 residual blocks).

### H. int8 weights with AVX-VNNI (training rollouts only)

The rollouts of training (D2) are the hot path that most needs speed. Their weights are quantized
to int8 and the dense layers run on AVX-VNNI (`vpdpbusd`: u8 × i8 summed into i32, 4 products a
lane). No wasm, and the page and the server's games keep f32.

- **Weights.** W2 and the residual matrices: i8, one f32 scale per output (symmetric, max-abs),
  packed in VNNI panels (for 8 outputs, 4 consecutive `k` side by side). W1: i8 rows with one scale
  per row, so the bag, bound by memory, reads a quarter of the bytes. Biases, the identity
  embedding, the readout and the value readout stay f32.
- **Activations.** After each ReLU they are ≥ 0, so they are quantized to u8 per option row (scale
  max/255) in the epilogue of the layer that makes them. The kernel sums in i32; its epilogue
  rescales by the two scales, adds the f32 bias, applies the ReLU (and the residual add in f32),
  and quantizes again for the next layer.
- **The file.** A `.ti4m` of `version` 1 with `"quant": "int8"` in the header and the int8
  sections, written by `ti4-pack --quant int8` and by the in-memory path of D2. The f32 reader
  refuses it unless the backend is `Avx2Vnni`.
- **Accuracy.** Not bit-identical: the logits are within a bound measured on the parity games
  and written here by step 10, and the rate at which the drawn option changes at temperature 0.001
  is reported. PPO recomputes the probabilities on tch, as with `Avx2Fma`.

## Plan (one PR per step, in order)

### Step 1: `ti4-infer` with the scalar backend

- The crate and its lints. Add it to the package set of `c-web`, `l-web`, `t-web`, `t-web-slow`,
  `wire-web` and `b-web` in `.cargo/config.toml`.
- The safetensors and bundle reader, the `.ti4m` writer and reader, `ti4-pack`.
- `Model::forward` over `Scalar`.
- Tests:
  - a round trip of the file;
  - each refusal (bad sha, wrong schema, width not a multiple of 8, truncated);
  - the forward of the synthetic model against a plain triple-loop reference written in the test.

### Step 2: SSE and simd128 with packed panels

- `Lanes` for `Sse` and `Simd128`, `gemm` on panels, the fused epilogues, and the rustflags of the
  wasm target.
- Native tests assert **bit equality** of `Sse` and `Scalar` on random shapes: options 1–37 to cover
  every MR tail, widths 128 and 256, 0–2 residual blocks.
- A Node test (`web2`, vitest) runs the wasm engine's forward on the synthetic model and compares
  the logits bit for bit with a fixture written by the native test (`TI4_WRITE_WIRE=1`, next to the
  other fixtures). This proves simd128 == SSE.
- The ignored bench, and its numbers in the PR.

### Step 3: AVX2 and AVX2 with FMA (section C)

- `Avx2` and `Avx2Fma` in `simd/`, the detection at load, `Model::with_backend`, and
  `ti4_infer::backend()` naming the backend of a model.
- Tests: `Avx2` bit-equal to `Scalar` on the shapes of step 2; `Avx2Fma` within
  `1e-4 · max(1, |logit|)` of `Scalar`. A test skips with a message on a CPU without the feature.
- The bench asserts the targets of section C that the code meets, and prints the others.

### Step 4: one pipeline in `ti4-policy` (section D)

- The pipeline in `ti4_policy::mlp` (`Pipeline`, `features`, `pick`, `fleet`, `decide`,
  `ModelInfo`, `Scorer`), with no `ti4-infer` and no tch. `ti4_infer::Model::forward` takes the
  column lists. `ti4-policy` is wasm-clean (`cargo wasm-policy`).
- `ti4-mlp` implements `Scorer` for its `Actor`, and `MlpBot` calls the pipeline. Seeded games of
  `MlpBot` before and after the move give the same game, the same PPO records and the same plan
  trace, bit for bit.
- Aliases for the policy and training crates (they link libtorch): `l-mlp` (Clippy of
  `ti4-policy`, `ti4-mlp`, `ti4-sim`, `ti4-training`, `ti4-review`, `ti4-cli`, `ti4-advisor`,
  `ti4-replayer`, every target) and `t-mlp` (their unit tests).
- A parity test in `ti4-mlp` (`tests/infer_parity.rs`, alias `t-mlp-parity`, which builds
  `ti4-infer` optimized (dev profile) and shows the output) plays seeded games of the
  reviewer checkpoint, again with a small synthetic battle predictor of fact version 8 so that
  the battle facts and the fleet decisions run, and of one schema-12 bundle when `out/champions/`
  has it (skipped with a message otherwise). Every seat is an `MlpBot` shadowed by a `Pipeline`
  with its seed:
  - For every decision it checks `|logit_tch − logit_infer| ≤ 1e-4 · max(1, |logit|)` and the same
    option at temperature 0.001 (two options within that bound of each other are a tie, counted).
  - It checks that `MlpBot` answers the option the pipeline draws, and that each PPO step it
    records has the pipeline's features, head, option and behaviour probability, bit for bit.
  - It prints the time of the forward pass of a decision on tch and on `ti4-infer`, in the game
    and again at once. Measured on the reference machine (reviewer checkpoint, width 256, 7.5
    options of about 165 columns): tch 375–400 µs, `ti4-infer` on `Avx2` 165 µs (warm: 320–345
    against 160 µs); the worst relative logit difference 6.1e-5.
- On Linux the pinned libtorch is `out/libtorch-2.9.1-cpu-linux` (`scripts/install_libtorch_cpu_linux.sh`):
  `LIBTORCH=$PWD/out/libtorch-2.9.1-cpu-linux LIBTORCH_BYPASS_VERSION_CHECK=1
  LD_LIBRARY_PATH=$LIBTORCH/lib cargo t-mlp-parity`. An exported `LIBTORCH` overrides the `[env]`
  of `.cargo/config.toml`. The doc of the alias says so.

### Step 5: rollouts of training on `ti4-infer` (section D2)

- `ti4_mlp::infer` (`parts_of`, `InferModel`, `Scorer` for it), `MlpBot::on_infer`,
  `Model::with_value` and `Model::value_with` in `ti4-infer`, and `--rollout-backend infer` in
  `ppo_update`.
- The critic: the value readout of a shared critic on the in-memory model (section D2).
- Tests:
  - `ti4-infer`: the value head is the readout on the trunk, bit for bit with a head of the same
    weights; a model without one refuses a value.
  - `ti4_mlp::infer` (`cargo t-mlp`): a random actor of the diplomacy heads with two residual
    blocks gives every head's logits and the value within `1e-4 · max(1, |x|)` on `Scalar` and on
    `Backend::fastest()`; a separate critic gives no value head.
  - `tests/infer_rollouts.rs` (libtorch, in `cargo t-mlp-parity`, one test at a time): two seeded
    PPO updates on each rollout backend, a fresh model per update, every behaviour probability and
    value recorded on `ti4-infer` checked against the actor's (worst log-probability gap 2.3e-5,
    worst relative value gap 5e-7); and the bench of section D2.

### Step 6: MLP bot kinds in ti4-host, the server and the engine of the page (section E)

- `BotKind`, `Bot`, `bots::choose`, `setup_options(models)`, the wire changes (`cargo wire-web`),
  `Local.bots`; `Pipeline::{carry, restore, follow}` in `ti4-policy`.
- The server loads `TI4_MODEL_DIR`; in-process MLP seats; persistence.
- The wasm registry and `ti4_model_load`.
- Tests:
  - `both_hosts.rs` plays a table with two MLP seats on both hosts for two rounds and gets the same
    game; the saved game of the server, cut before the first move of every activation of an MLP
    seat, plays on in the page to the same game (without `follow` the cut after a planned fleet
    differs);
  - `mlp_bots.rs`: a game with two MLP seats recovered from its decisions after each such cut goes
    on as before (the same check); `SeatController` reads the records of every build;
    `random_bot_lobby.rs` adds an MLP seat to a lobby, starts it and recovers the running game with
    it;
  - the kind lists in `random_bot_lobby.rs`, `lobby_settings.rs`, `ti4-wasm/tests/it/host_setup.rs`
    and `ti4-wasm/src/lib.rs` gain the model of the tests; `host_setup.rs` loads a model by its bytes,
    refuses a changed one and a table with a model that is not loaded, and reads the `rule_seats` of
    an older save.

### Step 7: remove the advisor and the bot agent

Remove:

- `crates/ti4-advisor`, `crates/ti4-bot-agent`, their workspace entries and their packages in
  `l-mlp` and `t-mlp`;
- `BotServiceConfig`, `spawn_bot_for_lobby` and `active_bots` in `session/registry.rs`, and the
  field `bot_service_enabled` of the lobby view (`bot_kinds` lists every kind that a host can add);
- `POST /lobby/add-bot` and its password check (`http/games.rs`, its rate limit in `public/`), and
  the `"mlp"` kind of the "bot service";
- the env vars in `bin/server.rs` (`TI4_BOT_PASSWORD`, `TI4_ADVISOR_URL`, `TI4_BOT_AGENT_BIN`,
  `TI4_MAX_ACTIVE_BOTS`), and their lines in `deploy/public/env.example`; the `process` feature of
  tokio in `ti4-server`;
- in `web/`: the add-bot UI of `components/Lobby.tsx` and `useLobbySession.ts`, and
  `bot_service_enabled` of `protocol/types.ts` and `decode.ts` (with the "with MLP" shot of
  `e2e/screenshots/AV-random-bots`). `advisorService.ts` and the `/advisor` proxy of
  `vite.config.ts` were gone before.

Update `docs/RANDOM_BOTS.md`, `docs/OFFLINE.md` and the MLP section of `web/README.md`
(`TI4_MODEL_DIR`, the kinds `mlp:<name>`). The Linux libtorch setup of
`crates/ti4-advisor/README.md` ("Native runtime", with the Windows part) moves to
`crates/ti4-mlp/README.md`. The removed tests are the ones of the crates, `bot_lobby_service.rs`,
the add-bot request of `public_playtest.rs` and the "+ mlp" cases of `random_bot_lobby.rs`.

### Step 8: the routes, the pack script and the deploy (sections F, G)

- `/api/models`, the file route, `models/index.json`, `scripts/pack-models.sh`, and the changes to
  `deploy.sh` and `env.example`.
- A server test fetches the list and a file, checks the sha, and checks the cache header.

### Step 9: web2

- The model list in `localTable.ts`, the Cache API fetch and the Worker load (`wasmTransport.ts`,
  `web-shared/src/engine/worker.ts`).
- The wait of "Start game", the lobby kinds and sign, the player table sign, and the docs of
  section F.
- Tests:
  - `tableContract.test.ts`: an MLP kind on both tables;
  - an e2e that starts a single-player game with an MLP seat and plays round 1 to the action phase;
  - the shots of the lobby with three models and 8 seats, desktop and phone.

### Step 10: int8 weights with AVX-VNNI, for the rollouts of training (section H)

- The int8 sections of section H, `ti4-pack --quant int8`, and the int8 `Model` from an `Actor` in
  memory (step 5's path).
- `Backend::Avx2Vnni`, detected at load; no wasm and no `.ti4m` offered by a server.
- `--rollout-backend infer-int8` in `ppo_update`.
- Tests: the int8 kernels against a plain integer reference, bit for bit; the int8 logits against
  f32 within the bound of section H; the parity games report the gap and the rate of changed
  options; a short seeded PPO run on int8.
- The bench: a decision and a game on `Avx2Fma`, `Avx2Vnni` and tch, and the speedup, in the PR.

## Verification

- Each Rust step: `cargo fmt --all -- --check`, `cargo l-web` and `cargo t-web`. When `ti4-host` or
  `ti4-wasm` changed, also `web2/scripts/build-wasm.sh` and `npm test`. When the server changed,
  also `cargo b-web`.
- Step 2: the bench (`cargo t-web bench --ignored --nocapture`; `ti4-infer` is optimized in the
  dev profile) runs. The wasm bit-equality test passes on the release build.
- Step 3: the bench runs, and meets the targets of section C that it asserts.
- Steps 4 and 5: `cargo t-mlp-parity` passes (step 5 adds `infer_rollouts` to it) with libtorch, and `cargo l-mlp` and `cargo t-mlp`
  are clean (`t-mlp` but for tests that need `out/`); step 4 also `cargo wasm-policy`.
- Step 6: `cargo t-web-slow` (both hosts).
- Step 9: `npm run typecheck`, `npm run lint`, `npm test`, `npm run e2e`, and `npm run shots` for
  the lobby. Then a manual game: `cargo b-web && TI4_MODEL_DIR=out/models target/debug/server`,
  `?single` with two MLP seats, and the Network tab shows that the model was fetched once and served
  from the cache on a reload.
