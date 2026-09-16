# GPU inference smoke — 2026-09-16

User request: set up a smoke test for GPU inference during simulation. This explicitly authorizes
a bounded experimental GPU path. Production PPO and its CPU inference contract remain unchanged.

Scope: one diagnostic example, this evidence file, and an execution-state entry. P1/P2 local
build and bounded CPU/CUDA execution; existing worktree and target directory only. Read the named
completed checkpoint and held-out pool without modifying them. No network, ports, training,
optimizer updates, checkpoint publication, new directories, commits, or changes to existing dirty
source. Reuse the installed pinned CUDA libtorch. Check for competing trainers before running.

The test collects ordinary CPU PPO inputs from one four-round game, replays a bounded evenly
spaced sample through single-decision CPU scoring and mixed-head/faction CPU/GPU batches, then
runs the same seeded game through the existing decider on CUDA. All paths include actor and critic
when the bundle has a critic. The live GPU path is unbatched: it checks integration, not the speed
of a future central batching service. Replay timings include host packing, uploads, output copies,
and CPU softmax, but exclude engine execution and feature extraction. Live games report their
own complete rollout times and decision counts; divergent games cannot establish a speedup.

Bounds: four rounds, 10,000 engine steps per rollout phase, 512 replay decisions by default,
three timing repeats, batch sizes 1/8/32, one CPU and one GPU game. No dataset files; small JSONL
stdout may be saved in existing target (under 1 MiB). In-memory recordings are released between
games. Process budget: five minutes for execution; cancel the owned process if it exceeds that.

Acceptance: bundle/pool provenance reported; CUDA required (no fallback); production decider status
consumed; errors and step caps refuse; actor distributions and critic outputs finite; replay
matches CPU within explicit probability/critic tolerances; report greedy disagreements, total
variation, log-probability drift, and live trajectory digest equality without claiming bitwise
CPU/GPU equivalence. Unknown/malformed flags refuse. Timings synchronize the device, warm up first,
alternate CPU/GPU order between repeats, and retain every sample.

Implemented `crates/ti4-mlp/examples/gpu_inference_smoke.rs`. It accepts required `--bundle` and
`--map-pool`, optional `--seed`, `--samples`, `--repeats`, `--temperature`, and `--diplomacy`.
Samples are bounded to 32..2048 and repeats to 1..5; no rounds override exists. Tolerances are
predeclared in the example: maximum option-probability error and per-decision total variation
each <= 0.001, critic error <= 0.01 + 0.0001 * abs(CPU value). Changed zero-probability support
refuses. Greedy/common-draw disagreements are reported separately and do not imply exact parity.

Run on RTX 3090 using installed libtorch 2.9.1 cu128. No trainer was running at preflight.
Branch `codex/diplomacy-v1`, HEAD `85d01517a196315a5062b706199513c7570f15c0`, existing dirty source
preserved. Inputs: final checkpoint `checkpoint-212544`, held-out pool, seed 1261600101,
temperature 2.5, diplomacy enabled, four rounds. No other benchmark or build ran during timing.

Both live games passed with 2,649 decisions, 2,371 non-forced PPO records, and 2,753 events.
CPU time was 2.6233903 s; unbatched CUDA time was 4.4717058 s. Both had identical final-state and
ordered-event hashes, with final VP by seated faction: Letnev 5, Sol 4, L1Z1X 4, Jol-Nar 5,
Hacan 4, Xxcha 2. These are single-game, fixed-order integration observations, not a statistically
qualified throughput comparison. The reported live timing excludes the initial device model copy.

Replay used 512 real decisions, 2,764 options, and all 15 heads, including 161 diplomacy decisions.
Median and range of three warmed timing passes over the identical sample:

| Path | Decisions per batch | Median seconds | Range seconds | Decisions/s at median |
|---|---:|---:|---:|---:|
| Ordinary CPU actor + critic | 1 | 0.149042 | 0.148888–0.156619 | 3,435 |
| CPU mixed actor + critic | 1 | 0.158286 | 0.154551–0.162848 | 3,235 |
| CPU mixed actor + critic | 8 | 0.089975 | 0.089072–0.092255 | 5,690 |
| CPU mixed actor + critic | 32 | 0.084536 | 0.083758–0.084835 | 6,057 |
| CUDA mixed actor + critic | 1 | 0.347621 | 0.346514–0.349268 | 1,473 |
| CUDA mixed actor + critic | 8 | 0.058878 | 0.057669–0.059024 | 8,696 |
| CUDA mixed actor + critic | 32 | 0.026307 | 0.025429–0.026618 | 19,462 |

All CPU figures are **one thread**, not the existing parallel rollout pool. CUDA batch 32 is
about 5.67x the ordinary single-thread CPU replay throughput and 3.21x CPU batch 32. These ratios
exclude feature construction, game execution, queue latency, partial-batch occupancy, and concurrent
CPU contention. They are not projected training speedups. The experiment supports testing a central
batching service; simply placing each live decider on CUDA was slower in this smoke.

Numerical results across replay variants: maximum probability error 0.000063505, maximum total
variation 0.000063505, maximum critic error 0.000002862, maximum log-probability error 0.000366211;
zero support changes and zero sampled-choice changes for the fixed common draws. CPU mixed batch 1
changed 6/512 greedy winners; all other mixed paths changed 7/512. The mixed scoring arithmetic itself
can therefore change choices even on CPU. Numerical tolerance success is not a bitwise-equivalence
or greedy-policy qualification. Live CUDA uses the ordinary per-decision entry point, not mixed
batching, so its matching game hashes do not validate a live batched rollout.

Verification completed: release build succeeded; the example's two regression tests passed
(malformed distribution/nonfinite and wrong critic rejection; small drift with a changed greedy
choice remains visible). CLI checks refused `--rounds 5` and `--samples 0` with exit code 2.
The first CLI-only invocation omitted the CUDA DLL path and did not start normally; that owned
process was stopped, the environment was corrected, and both checks passed. This happened after
the benchmark and does not affect its timings. Always set the DLL path as below.
The first global `-D warnings` Clippy attempt was blocked by 17 pre-existing ti4-mlp library
warnings. The new example instead has its own `#![deny(warnings)]`, preserving strict checks of
this code without altering unrelated library lint policy. The final source was rebuilt and both
regression tests and the full smoke rerun passed, with identical recorded input and game hashes
to the initial run. Numbers above are from this final run. Formatting and whitespace checks passed.
Final scoped Clippy passed (exit 0), with warnings denied inside the new example and no warnings
from that example. Existing library warnings remain; their full output is saved in
`target/gpu-inference-smoke-clippy-scoped-20260916.log`. No independent production qualification
review was performed or claimed. All smoke/build processes exited; no background task remains.

Reproduce from this worktree in PowerShell (paths below name existing local installations/inputs):

```powershell
$env:LIBTORCH = 'D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$env:LIBTORCH/lib;$env:PATH"
cargo build --offline --release -p ti4-mlp --example gpu_inference_smoke
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
$smokeArgs = @(
    '--bundle', 'D:/Projects/ti4-engine-rs/out/ppo-diplomacy-my-run/checkpoints/checkpoint-212544',
    '--map-pool', 'D:/Projects/ti4-engine-rs/out/pools/full_np8_12_holdout.json',
    '--diplomacy', '--samples', '512', '--repeats', '3'
)
& ./target/release/examples/gpu_inference_smoke.exe @smokeArgs
if ($LASTEXITCODE -ne 0) { throw 'Smoke failed' }
```

Run only while no other trainer/build/benchmark is using the hardware. Expected execution is
seconds rather than minutes; stop the owned test if it exceeds five minutes. Output is JSONL to
stdout; no output directory or checkpoint is created. For a repeat, use a fresh log filename.

Local evidence in existing ignored target:

- `target/gpu-inference-smoke-final-20260916.jsonl`, SHA-256
  `ad1bd2f6308a0ad1a56f5d72106f5945f71e53d8132a1dcff516b2e0628c05f4`.
- Tested executable SHA-256:
  `0642f02299af3417bacdb95d599426544fefa95a3f757060e039963ccaaa0d67`.
- Tested example source SHA-256:
  `900d0d6ae506d3a25ed6ccc382057be13e017d31885c2ed24d727968eca9cb60`.
- Initial run retained at `target/gpu-inference-smoke-20260916.jsonl`, SHA-256
  `8d79c31b01225a2165c26b910f4842e5b9e7b2370ccb975f0a67048a622f0e5b`.
- Bundle manifest SHA-256:
  `89ece63a54fd23321d4087f2d7c202d9a963d6c8c6d523c58ea19d0f0ca5ae56`.
- Pool SHA-256:
  `aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5`.
- Sample PPO-input digest:
  `36103466cfc1807bdbd2ae03a3aa6bf7d421581cccd0a55a1b503468ebd8d0df`.
- Matching live final-state SHA-256:
  `df98c037c3b00d59753e578a0cbf9052871e620523f2386c1820c214a1cb32d1`.
- Matching live event SHA-256:
  `536810c26bcf1212fb54791a7137d77ae99b3df9952cb61ad09cc0a534f4ccce`.

Next experiment, not implemented: bounded cross-game inference service with timeout/partial-batch
flush, fixed weights per rollout phase, actual sampled behavior probabilities, and per-game RNG.
Compare true four-round rollout wall time against the current CPU worker pool at equal game counts.
Do not infer performance from replay ratios or promote this smoke to the production PPO path.
