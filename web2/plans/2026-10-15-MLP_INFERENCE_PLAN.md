# MLP bots without libtorch: inference in ti4-host, on the server and in the page (2026-10-15)

Status: planned. Step 1 done (2026-10-15). Step 2 done (2026-10-15).

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
| `simd/`       | The `Lanes` trait and the three backends (section C). This is the only module with `#[allow(unsafe_code)]`. The crate restates the workspace lints with `unsafe_code = "deny"`, as `ti4-tensor` does.                                                                                                              |
| `kernel`      | `bag`, `gemm` (packed panels), `readout`, generic over `Lanes`.                                                                                                                                                                                                                                                    |
| `model`       | `Model`: the sections of one file and `forward(faction, head, options) -> Vec<f32>` with a reused scratch buffer. It is `Send + Sync` and shared by `Arc` across seats and games.                                                                                                                                  |
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

### C. The kernels: one source, three backends

```rust
pub(crate) trait Lanes {            // 4 × f32 in every backend
    type V: Copy;
    fn zero() -> Self::V;  fn splat(x: f32) -> Self::V;
    unsafe fn load(p: *const f32) -> Self::V;  unsafe fn store(p: *mut f32, v: Self::V);
    fn add(a: Self::V, b: Self::V) -> Self::V;  fn mul(a: Self::V, b: Self::V) -> Self::V;
    fn relu(v: Self::V) -> Self::V;  fn hsum(v: Self::V) -> f32;   // (l0 + l1) + (l2 + l3)
}
```

**The backends**

| Backend   | Selected by                                                    | Instructions                                                                                       |
| --------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `Sse`     | `cfg(target_arch = "x86_64")`                                  | `core::arch::x86_64` `_mm_*_ps`. SSE2 is the baseline of x86-64, so there is no runtime detection. |
| `Simd128` | `cfg(all(target_arch = "wasm32", target_feature = "simd128"))` | `core::arch::wasm32` `f32x4_*`.                                                                    |
| `Scalar`  | every other target                                             | `[f32; 4]` with the same lane arithmetic. Native tests also call it explicitly.                    |
| `Avx2`    | x86-64, `is_x86_feature_detected!("avx2")` once at load        | `_mm256_*_ps`, 8 lanes, a multiply then an add. The default on a native host with AVX2.            |
| `Avx2Fma` | x86-64, AVX2 and FMA detected, and asked for by the caller     | `_mm256_fmadd_ps`. Not bit-identical to the others; see below.                                     |

A model picks its backend once, when it is loaded (`Model::with_backend`, default the best
deterministic one), so the kernels have no per-call dispatch. There is no relaxed SIMD in wasm:
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

So the three backends give bit-identical logits, and a game is the same game on the server and in
the page.

**The kernels**, all with `width` a multiple of 8:

- `bag`: for each option, `acc[width] = Σ v_j · W1[col_j]` over the columns. The columns are sorted
  and duplicates folded first, with the order and tie-break of `ti4-tensor/src/lib.rs:442-534`
  (`total_order_key`, moved into `ti4-infer`). An empty bag is a zero row. The accumulator walks
  `width/4` vectors, unrolled by 8. The epilogue is `+ identity`, then `+ b1`, then ReLU: the order
  of `trunk`.
- `gemm`: the micro-kernel is **MR = 4 option rows × NR = 8 outputs**. It keeps 8 vector
  accumulators, loads 2 vectors of the panel per `k` and broadcasts 4 scalars of A: 11 of the 16
  XMM registers, and the same shape on wasm, whose engines map v128 onto them.
  - The panels are the outer loop and the row blocks the inner loop, so a panel stays in L1 while
    every option of the block streams past it.
  - The tail of 1–3 rows uses the same kernel with `const MR: usize`.
  - A (the activations) is broadcast once per block of 32 rows into `[rows][k][4]`, so the
    micro-kernel loads each broadcast in place of building it at every panel (on SSE a broadcast
    is a shuffle). The panels are the outer loop inside a block of rows.
  - The epilogue is fused: bias and ReLU (layer 2), or the residual add (blocks).
  - K = width is at most 256, so there is no K blocking.
- `readout`: `z · w[f][h] + b[f][h]` for every option, with 4-lane partial sums and `hsum`.

**Build.** `.cargo/config.toml` gets `[target.wasm32-unknown-unknown] rustflags = ["-C",
"target-feature=+simd128"]`. Every browser with JSPI, which the engine already requires, has
simd128. The native aliases are not affected.

**Performance target**, single thread, release, x86-64 (measured by an ignored test,
`ti4_infer::bench`):

| Model at width 256 | Backend   | 6 options | 512 options |
| ------------------ | --------- | --------- | ----------- |
| 0 residual blocks  | `Sse`     | < 30 µs   | < 4 ms      |
| 2 residual blocks  | `Avx2`    | < 55 µs   | < 5 ms      |
| 2 residual blocks  | `Avx2Fma` | < 35 µs   | < 4 ms      |

The bench asserts these targets, and prints `Scalar` and `Sse` at 2 residual blocks for the
comparison. Without FMA, SSE does 8 multiply-adds a cycle, so a residual block costs about 30 µs
at 6 options; 8 lanes halve that and FMA halves it again. The 512-option case is bound by the
memory of the bag (about 43 MB of rows read from a 21 MB table with random columns), so the vector
width helps it less. Decisions are batched within a decision (the options are the rows), not
across decisions.

### D. One decision pipeline, shared with training

The pipeline is the part of `MlpBot::decide` (`ti4-mlp/src/bot.rs:771-…`) that does not touch tch.
It moves into `ti4-policy`, as the module `ti4_policy::mlp`, with no dependency on `ti4-infer` or
tch. It is two calls, so that PPO can keep what lies between them:

- `features(scorer_info, choice, seen, baseline) -> Prepared`: the faction row, the head row, the
  options as columns, and the options of the choice they stand for;
- `pick(prepared, logits, temperature, rng) -> (ChoiceOption, probabilities)`.

`ti4-host` calls both in turn. `MlpBot` calls both and records the features and probabilities for
PPO in between. Together they cover:

- the fleet plan packages (`planned`, `bot.rs:419-603`);
- `mlp_choice_features`, plus the battle facts when the model has a predictor and the deal facts
  when the vocabulary assigns `FACT_SCORE`;
- `sparse_from`, with its memo of resolved keys;
- `decision_head` and the resolution of the layout head (`resolve_layout_head`, `layout_head_index`,
  the Legacy (14) and Diplomacy (15) layouts of `ti4-mlp/src/lib.rs:199-214`);
- `stable_softmax` (`lib.rs:1743-1789`) at the temperature;
- the draw of `MlpBot::sample` (`bot.rs:605-615`, the cumulative walk).

The model behind it is a trait in `ti4_policy::mlp`, `Scorer::logits(faction, head, options) ->
Vec<f32>`, with what the pipeline needs of a model (heads, roster, vocabulary, battle predictor,
temperature):

- The options are plain column lists, `&[(u32, f32)]` per option. `ti4_infer::Model::forward` takes
  the same lists, so neither side converts, and the two crates share no type.
- `ti4-host` implements `Scorer` for a newtype around `Arc<ti4_infer::Model>`.
- `ti4-mlp` implements it for its `Actor` on tch, and for `ti4_infer::Model` (step 5). `MlpBot`
  takes either, so training, evaluation and play run one pipeline.
- The tch `Scorer` stays because tch still plays: the GPU batch client, the parity test, and the
  evaluations of a checkpoint during training.
- The PPO records, the GPU batch client and the critic stay in `ti4-mlp`.
- `ti4-policy` becomes wasm-clean in this step: `rand` without default features, as `ti4-rule-bot`
  has it.

### D2. Rollouts of training on `ti4-infer`

Self-play needs only the forward, and on the CPU a decision of about 6 × 256 is mostly the dispatch
of tch ops. `ti4-mlp` gets:

- `Parts` built from the tensors of an `Actor` in memory (no bundle on disk), and a `Model` from
  them. After each PPO update the rollout workers get a fresh `Arc<Model>`.
- A choice of backend for the rollouts of a run: tch (CPU or the GPU batch client) or `ti4-infer`
  (`Avx2Fma` where the CPU has it). The update stays on tch, and recomputes the probabilities.
- The critic is not in a `Model`. Rollouts that need a value call tch for it, or the critic is
  added to the in-memory `Model` of training, never to a `.ti4m`.
- A bench of a decision and of a game on tch and on `ti4-infer`, whose numbers go in the PR.

**The draw of a bot.** The RNG is `ChaCha8` from `(game seed, seat, decision index)`, as the rule
bot's choice is (`ti4_rule_bot::choose`). The temperature is the model's (header, default 0.25, the
evaluation temperature of `plans/MLP_TEMPERATURE_REGIME.md`).

### E. Bot kinds in ti4-host

- `ti4_host::setup::bot_kind` keeps `RULE = "smart"` and `RANDOM = "random"`. An MLP kind is
  `mlp:<name>`.
- `BotKind { Smart, Random, Mlp(Arc<Model>) }` and one `bots::choose(kind, choice, seen, seed,
  seat, index)` replace the two call sites of each host:
  - the server's `RandomBotDecider` with `.smart()` (`session/worker.rs:1141-1175`);
  - the wasm `pick` (`lib.rs:700-730`) and `Replayed::decide` (`:1148-1161`).
- `setup_options(models: &[ModelInfo])`: the host lists the kinds from the models that it is given.
  Each is a `BotKindOption` with the id `mlp:<name>` and the label of the model. The page still
  never writes a kind (`web2/docs/agents/lobby.md`).
- **Wire.**
  - The kinds of a seat become a string: `"human" | "smart" | "random" | \`mlp:${string}\``via`#[ts(type)]`. This changes `SeatKind`, `Bots`and`BotKindOption`in`web-shared/src/wire/generated/`.
  - `Local.rule_seats` (a mask) becomes `Local.bots: (string | null)[]`, the kind of each seat.
  - A save of format 2 with `rule_seats` is read as `smart` on those seats (serde default, a few
    lines).
- **The server**
  - It loads every `.ti4m` of `TI4_MODEL_DIR` at start, listed by `index.json`, and logs each name
    and sha. With no directory, no MLP kind is offered.
  - `SeatController::{BotSmart, BotRandom}` become `SeatController::Bot(BotKind)`. MLP kinds are
    in-process kinds (`IN_PROCESS_BOT_KINDS`, `add_in_process_bots`, `/lobby/bots`) and are
    persisted with the table, so a recovered game keeps its MLP seats.
  - A table whose model is no longer loaded refuses to start, with the name of the model.
- **The engine of the page**
  - A new export `ti4_model_load()` takes the bytes of a `.ti4m` through the input buffer of
    `ti4_host_in(len)` and checks the trailer sha.
  - It keeps the model in a static registry by name.
  - `ti4_host_start` refuses a table with a kind whose model is not loaded.
  - `ti4_setup_options` gets the model list in its request.
- **ti4-policy in the wasm build.** `ti4-host` and `ti4-wasm` depend on `ti4-policy` (made
  wasm-clean in step 4) and on `ti4-infer`. The step records the size of `ti4.wasm` before and after.

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
- The bench asserts the targets of section C.

### Step 4: one pipeline in `ti4-policy` (section D)

- Move the pipeline into `ti4_policy::mlp` (`features`, `pick`, `Scorer`), with no `ti4-infer` and no
  tch. `ti4_infer::Model::forward` takes the column lists. `ti4-policy` becomes wasm-clean.
- `ti4-mlp` implements `Scorer` for its `Actor`, and `MlpBot` calls the pipeline.
- A parity test in `ti4-mlp` (needs libtorch; a new alias `t-mlp-parity = "test -p ti4-mlp --test
  infer_parity"`) plays seeded games of the reviewer checkpoint and of one schema-12 bundle when
  `out/champions/` has it.
  - For every decision it checks `|logit_tch − logit_infer| ≤ 1e-4 · max(1, |logit|)` and the same
    option at temperature 0.001.
  - It also checks that `MlpBot`'s features equal the pipeline's, and times a decision on tch and
    on `ti4-infer`.
- On Linux the pinned libtorch is `out/libtorch-2.9.1-cpu-linux` (`scripts/install_libtorch_cpu_linux.sh`):
  `LIBTORCH=$PWD/out/libtorch-2.9.1-cpu-linux LIBTORCH_BYPASS_VERSION_CHECK=1
  LD_LIBRARY_PATH=$LIBTORCH/lib cargo t-mlp-parity`. The doc of the alias says so.

### Step 5: rollouts of training on `ti4-infer` (section D2)

- `Parts` from the tensors of an `Actor`, `Scorer` for `ti4_infer::Model` in `ti4-mlp`, and the
  choice of backend for the rollouts of a run.
- The critic: how the rollouts get their values.
- Tests (libtorch): a `Model` built from an `Actor` gives the logits of the `Actor` within the bound
  of step 4; a short seeded PPO run on each backend.
- The bench of a decision and of a game, tch against `ti4-infer`.

### Step 6: MLP bot kinds in ti4-host, the server and the engine of the page (section E)

- `BotKind`, `bots::choose`, `setup_options(models)`, the wire changes (`cargo wire-web`), `Local.bots`.
- The server loads `TI4_MODEL_DIR`; in-process MLP seats; persistence.
- The wasm registry and `ti4_model_load`.
- Tests:
  - `both_hosts.rs` plays a table with an MLP seat on both hosts and gets the same game;
  - a server test recovers a game with an MLP seat;
  - the kind lists in `random_bot_lobby.rs`, `lobby_settings.rs`, `ti4-wasm/tests/it/host_setup.rs`
    and `ti4-wasm/src/lib.rs:1850` gain the model of the test.

### Step 7: remove the advisor and the bot agent

Remove:

- `crates/ti4-advisor`, `crates/ti4-bot-agent` and their workspace entries;
- `spawn_bot_for_lobby` and `active_bots` in `session/registry.rs`;
- `POST /lobby/add-bot` and its password check (`http/games.rs:678-705`), and the `"mlp"` kind of
  the "bot service";
- the env vars in `bin/server.rs`;
- in `web/`: `src/services/advisorService.ts`, the `/advisor` proxy of `vite.config.ts`, the add-bot
  UI of `components/Lobby.tsx` and `useLobbySession.ts`, and `protocol/types.ts:74`.

Update `docs/RANDOM_BOTS.md` and `docs/OFFLINE.md`. The Linux libtorch setup of
`crates/ti4-advisor/README.md` ("Native runtime") moves to `crates/ti4-mlp/README.md`. The removed tests are the ones of the crates and
the "+ mlp" cases of `random_bot_lobby.rs`.

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

## Verification

- Each Rust step: `cargo fmt --all -- --check`, `cargo l-web` and `cargo t-web`. When `ti4-host` or
  `ti4-wasm` changed, also `web2/scripts/build-wasm.sh` and `npm test`. When the server changed,
  also `cargo b-web`.
- Step 2: the bench (`ti4_infer::bench`, run with `ti4-infer` optimized: the command is in its
  module) runs. The wasm bit-equality test passes on the release build.
- Step 3: the bench meets the targets of section C.
- Steps 4 and 5: `cargo t-mlp-parity` passes with libtorch.
- Step 6: `cargo t-web-slow` (both hosts).
- Step 9: `npm run typecheck`, `npm run lint`, `npm test`, `npm run e2e`, and `npm run shots` for
  the lobby. Then a manual game: `cargo b-web && TI4_MODEL_DIR=out/models target/debug/server`,
  `?single` with two MLP seats, and the Network tab shows that the model was fetched once and served
  from the cache on a reload.
