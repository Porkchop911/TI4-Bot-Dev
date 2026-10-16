# ti4-infer

The forward pass of the trained MLP policies without libtorch: the `.ti4m` model file, `ti4-pack`
(bundle → `.ti4m`), and hand-written SIMD kernels. It knows nothing of the game and depends only on
`serde_json` and `sha2`. The decision pipeline that feeds it is `ti4_policy::mlp`; `ti4-host` plays
the MLP bots with it, and `ti4-mlp` runs the rollouts of training on it.

The design and the history of each decision are in
`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md` (sections A–C, D2, H). This file keeps the findings
that matter when the crate is changed.

## Commands

- Tests: `cargo t-web infer` (web set), `cargo t-mlp-parity` (against libtorch; Linux env in
  `crates/ti4-mlp/README.md`).
- Bench: `cargo t-web bench --ignored --nocapture`. `ti4-infer` is optimized in every dev profile
  (`Cargo.toml`, `[profile.dev.package.ti4-infer]`), so debug builds give the real numbers.
- Pack: `cargo b-pack`, then `target/release/ti4-pack <bundle dir> <name> <out.ti4m> [--quant int8]`;
  the models of a deploy: `scripts/pack-models.sh`.

## Backends

| Backend    | Where                                  | Bits                                     |
| ---------- | -------------------------------------- | ---------------------------------------- |
| `Scalar`   | everywhere; the reference              | the reference                            |
| `Sse`      | x86-64                                 | = `Scalar`                               |
| `Simd128`  | wasm (the page)                        | = `Scalar` (checked by a vitest fixture) |
| `Avx2`     | x86-64 with AVX2; the native default   | = `Scalar`                               |
| `Avx2Fma`  | opt-in: rollouts, `TI4_INFER_FMA=1`    | within `1e-4 · max(1, \|logit\|)`        |
| `Avx2Vnni` | int8 models only; training rollouts    | see "int8" below                         |

The deterministic backends give the same bits because there is no FMA, every sum runs in a fixed
order whatever the vector width (a lane is its own output in `bag` and `gemm`), and the readout
keeps its 4-lane partial sums on every backend. So a game is the same game on the server and in the
page. Relaxed SIMD in wasm is not used: its multiply-add may or may not fuse.

## Speed (i5-12600K, width 256)

| Model / case                          | Scalar | Sse    | Avx2   | Avx2Fma | Avx2Vnni (int8) |
| ------------------------------------- | ------ | ------ | ------ | ------- | --------------- |
| 0 blocks, 6 options                   | 32 µs  | 27 µs  | 15.5 µs | 13 µs  | 11.6 µs         |
| 0 blocks, 512 options                 | 2.9 ms | 3.0 ms | 2.3 ms | 2.1 ms  | 1.08 ms         |
| 2 blocks, 6 options                   | 155 µs | 90 µs  | 49 µs  | 39 µs   | 25.6 µs         |
| 2 blocks, 512 options                 | 13.8 ms | 8.5 ms | 5.1 ms | 4.1 ms | 1.96 ms         |
| Real decision (parity games)          |        |        | 168 µs |         | 27.8 µs         |
| Rollout game, forward + critic        |        |        |        | 142 ms  | 31.5 ms         |

libtorch on the same decisions: 405 µs per decision, 279 ms per rollout game.

- The bench uses random columns and hot caches. A real decision gathers about 1,300 rows of W1
  (1 KiB each in f32) from L3 or memory, so the bag dominates, and int8 (a quarter of the bytes)
  gains far more in real play (6x) than in the bench (1.5–2x).
- Without FMA, SSE does 8 multiply-adds a cycle: a residual block costs about 30 µs at 6 options.
  8 lanes halve that, FMA halves it again.
- Prefetching the rows of the bag, in three variants, was slower every time.

## Pitfalls found

- **Generic forward = unoptimized forward.** A generic function is compiled in the crate that calls
  it, with that crate's profile. A generic option type made the forward compile in `ti4-mlp` at
  opt-level 0: 3 ms per decision instead of 0.2 ms. `Model::forward` takes the concrete
  `&[Vec<(u32, f32)>]`.
- **No intrinsics in closures passed to `std`.** The AVX2 paths are `#[target_feature]` functions
  with `#[inline(always)]` kernels. A closure handed to `array::from_fn` or `map` is compiled
  without the features and cannot inline the intrinsics: that alone made AVX2 4x slower.
- **Broadcasts are shuffles on SSE.** `gemm` packs the activations of a block of rows once, rather
  than broadcasting per panel.

## int8 (training rollouts only)

Weights: W2 and the residual matrices i8 with one scale per output, in VNNI panels; W1 i8 rows with
one scale per row (dequantized in the bag). Biases, embedding, readout and value readout stay f32;
the residual stream stays f32. Details in `src/quant.rs`.

- **Per-row u8 activations do not work.** The option drawn at temperature 0.001 changed in 20% of
  decisions (19.8% with one scale per decision). Each dense layer therefore quantizes its input as a
  base row (the midpoint over the decision's options) plus small deltas. The error of the base is a
  shift common to every option, which the softmax ignores.
- **Accuracy against libtorch** (reviewer 473312, 4,335 decisions): centred logit gap relative to
  `max(1, largest |logit|)` worst 0.104, p99 0.027, median 0.0013 (bound `INT8_LOGIT_BOUND = 0.2`).
  The drawn option at temperature 0.001 differs in about 2.6% of decisions. Value within 0.041
  (bound 0.1). In a short PPO run at temperature 1, the worst recorded log-probability is 0.67 from
  libtorch's.
- **Where the error comes from:** mostly i8 W1. i16 W1 rows (twice the bytes) would bring the
  changed-draw rate to 0.8%; i8 W1 alone 2.2%; i8 W2 alone 0.4%. Not done yet.
- **Use:** the int8 policy is visibly not the trained policy. Fit for wide self-play and evaluation
  sweeps; for PPO rollouts, prefer `Avx2Fma` until i16 W1 (or a measured PPO run) shows it learns the
  same. `ti4-host` refuses int8 files, so no game or server plays them.
