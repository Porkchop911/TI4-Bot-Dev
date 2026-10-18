# Wiki-to-MLP production diagnostic pilot

Operator request: start implementing [MLP_APPLICATIONS](../../docs/strategy-wiki/MLP_APPLICATIONS.md).
Checkout `wp/rule-bot`, starting HEAD `65a1f0a0`. No weight update, feature/vocabulary migration,
checkpoint promotion, branch switch or commit. Existing unrelated dirty paths remain untouched.

## Package and boundaries

First implementation slice: `hacan-production-bottleneck` and `jolnar-opening-infantry`,
restricted to an already-open production window. Other catalog families and the complete
Warfare/tactical activation, transport, landing and scoring chains remain pending. This slice
does not assign good/bad strategy labels or claim gameplay improvement.

Writable paths: new `crates/ti4-mlp/examples/strategy_probe.rs`; optional diagnostic capture in
`crates/ti4-mlp/src/bot.rs`; `tools/analyze_strategy_probe.py`; this evidence; wiki MLP proposal/navigation/log; current execution
state and compute queue. Results/logs use existing `out/strategy-wiki/extractions/` only.
No new directories without the operator's standing required permission. On 2026-10-16 an
asynchronous question requested permission for Cargo's normal subfolders under existing
`target/`. Operator subsequently approved subfolders within the worktree. Builds and native scratch use worktree `target/`; no folders outside the worktree are authorized by this update.

Sources: [case catalog](../../docs/strategy-wiki/MLP_CASE_CATALOG.json),
[Hacan C02/C21](../../docs/strategy-wiki/summaries/hacan.json),
[Jol-Nar C02/C08](../../docs/strategy-wiki/summaries/jolnar.json).
These remain draft strategy evidence with unreviewed current-rules compatibility.
Engine implementation, not those draft claims, supplies legal offers and observed outcomes.

## Experiment

Six normal POK faction deployments, with synthetic public-position modifications. Hacan:
20 trade goods, 40 trade goods at unchanged production capacity, and 20 with an additional
legal home dock. Jol-Nar: starting two infantry, six additional infantry, and no spendable
planet/TG resources. Every case uses ordinary production-use technology hooks and refresh,
including Sarween. No galaxy, card reaction timing, home activation or Warfare reachability
is claimed. Synthetic examples do not read the previously inspected validation map pool.

Three branches: unchanged MLP; force the first offered infantry production; force stopping
at the first production choice. All later choices use the unchanged MLP. The model samples
even at the overridden root, preserving its random stream and recording proposed versus
executed actions. These two forced choices are experimental alternatives, not teacher targets.

Actual bot-side diagnostic capture is disabled by default and retains post-augmentation raw
feature vectors, mapped sparse inputs, legal offers, resolved head, probabilities and sampled
index for ordinary decisions. The probe recomputes logits from those exact inputs, drains
the buffer after each choice, and records seat-visible resource/token facts. Fleet-package
and plan-executed decisions are outside this tracing API's documented scope.

Default four deterministic bot streams, six variants and three branches: 72 branch results,
each repeated for exact JSON equality, 144 production continuations. Independent cases use
16 CPU workers and one actor copy per chunk; no GPU required. Each production continuation
has a 256-choice limit. Failures are retained and cause process exit 2. Outputs use exclusive
file creation, contain checkpoint-file checksums checked again after inference, catalog and
probe source hashes, and source case definitions. Existing outputs are never overwritten.

## Checks and review

Written integration checks cover production capacity/resource contrasts, initial Jol-Nar
infantry/Sarween, diagnostic-on/off sampling equality, captured probability replay, controlled
branch execution and repeated output equality. Compilation and execution status is recorded
in the dated continuation below; the initial source-only checkpoint did not run Cargo.
`rustfmt --edition 2024` ran on the two task source files; tracked bot diff whitespace check
passed. No Cargo or benchmark success is claimed.

Independent reviewer `/root/review_probe` approved the static source/design review. Findings fixed:
capture was initially placed at the synthetic fleet sampler rather than ordinary inference;
moved it to ordinary `decide`. Production discount setup explicitly uses `production_used`
then `refresh`. Also retained fixed-setup public objectives/own secrets accurately in report
metadata, retained choice details for fleet-supply diagnosis, and preserved both attempts on
replay mismatch. This is source review only: compilation, tests, inference and strategy remain
unverified. Final formatting/whitespace checks passed and 69 local file links resolved across
the proposal, bot navigation, log and package evidence (anchor semantics not checked).

Live preflight: 72,731,304 KiB free physical RAM; no cargo/rustc process observed at inspection.
Operator explicitly says all compute is free; older queue reservations are stale. No task
build/simulation process has started. Future commands must retain full logs and exit status.

## Next commands after build-folder permission

Use the already existing root CPU libtorch distribution via `LIBTORCH`, with `CARGO_TARGET_DIR`
set to this checkout's existing `target`, one Cargo coordinator, `-j8`. Run example integration
checks, then affected MLP library tests; inspect failures before retrying. Builds may need an
existing approved temporary directory override to keep native tool scratch inside the repo.
After passing checks, build/run the probe against the same checkpoint-16448 recorded in
`out/rule-bot/holdout-only-20261013-settings.json`; write a new uniquely named report in the
existing extraction folder. Analyze root rankings, OOV mappings and downstream production
separately, and record all failures. No tuning or promotion from synthetic quantities alone.

## 2026-10-16 continuation after folder approval

Operator: "If it’s a sub folder in the worktree it’s fine". Created only worktree-local
`target/strategy-probe-scratch` and Cargo-managed subfolders. Set both TEMP and TMP to that
scratch directory for native builds/tests; `LIBTORCH` points read-only to the existing root
`out/libtorch-2.9.1-cpu`, and CARGO_TARGET_DIR to this worktree's existing `target`.
Cargo uses offline/locked dependencies, `-j8`; tests use 16 threads. Probe uses 16 workers.

- `cargo test --offline --locked -j8 -p ti4-mlp --example strategy_probe -- --test-threads=16`:
  3 passed, 0 failed; producer exit 0. Trace on/off sampling, exact feature-input probability
  replay, production constraints and controlled deterministic continuations exercised.
- `cargo test --offline --locked -j8 -p ti4-mlp --lib -- --test-threads=16`:
  109 passed, 0 failed; producer exit 0. Bot library source did not change after this run.
- Full logs and exit markers: existing extraction directory `strategy-probe-20261016-tests-01.*`
  and `strategy-probe-20261016-libtests-01.*`. Release build in progress, log
  `strategy-probe-20261016-build-01.log`; build completed exit 0 in 4m04s.
- Added `tools/analyze_strategy_probe.py` to validate distributions, raw-to-sparse mappings,
  legal/proposed/executed IDs, paired initial-state equality, exact case/variant/branch coverage,
  failure retention and production outcomes; output is diagnostic, never training targets.
  Independent `/root/review_probe` reviewed this analysis logic and independently confirmed
  both persisted test logs/exit markers. Stream count for the default run must independently
  equal four (72 branch results); omission of an entire final stream cannot be detected from
  the current raw manifest alone.

## Completed production diagnostic (final v2)

Initial executable launch failed before main with Windows exit -1073741515 (missing DLL search
path); no report was written. Release DLLs were in `target/release/deps`, not `examples`.
Added the existing pinned CPU libtorch `lib/` to this process's PATH. Launch then succeeded.
Both attempts and exit codes are retained as `run-01.*` and `run-02.*`; no model error was
hidden. Runtime environment changes were process-local.

First report `results-01.json`/`summary-01.json` is **superseded for final fleet conclusions**:
it ended at gross production before mandatory fleet/capacity removal. Investigation and the
independent reviewer identified this limit. Extended the same probe to call engine
`fleet::enforce_seeing`, capture actual removal-head decisions, retain pre-enforcement/final
home units, and require zero final fleet and capacity excess. No engine rule was changed.
The three example tests passed again (tests-02, exit 0); release rebuilt in 54.95s
(build-02, exit 0). Independent reviewer approved the extension's static design.

Final command (process-local PATH includes the existing CPU libtorch `lib` directory):

```powershell
./target/release/examples/strategy_probe.exe --bundle D:/Projects/ti4-engine-rs/out/ppo-focusgae-FG-wide-long1000-20261010/checkpoints/checkpoint-16448 --out out/strategy-wiki/extractions/strategy-probe-20261016-results-02.json --workers 16 --streams 4
./out/scpt-whisper-venv/Scripts/python.exe -B tools/analyze_strategy_probe.py out/strategy-wiki/extractions/strategy-probe-20261016-results-02.json out/strategy-wiki/extractions/strategy-probe-20261016-summary-02.json
```

Both producer exits 0, logs `run-03.*`/`analysis-02.*`. 72/72 branch results succeeded,
each repeated exactly (144 production-plus-enforcement continuations). 672 captured actor
decisions in the retained first passes: production 196, payment 392, removal via `other` 84.
No inference or guard failure. Final fleet/capacity excess zero in every result; all checkpoint
files matched their original hashes after the run. Main run wall time 19.877s, a single measured
workload duration, not a comparative speed claim.

Independent scheduling check: `--workers 1 --streams 1` wrote `serial-01.json`, exit 0;
all 18 results exactly equal the corresponding stream61016001 subset of the 16-worker run.
This deliberately serial small (<6s) verification is not a long serial campaign. Additional
exclusive-output and workers0 CLI checks returned exit2; existing raw SHA256 unchanged and
no invalid output created. Verification log/exit and `manifest-01.json` record exact coverage,
raw/summary/serial/source/binary SHA256. Raw+summary final reports are approximately 40+ MB;
they remain ignored local evidence, not committed artifacts.

### Findings (unchanged MLP branch, same four sampling streams per fixed state)

| Synthetic case | Gross production in all four streams | Removed by limits | Interpretation |
|---|---|---|---|
| Hacan, 20TG, capacity4 | flagship +3 dreadnoughts | 4 ships | Initial fleet already at3/3; replacement/removal choices vary by stream |
| Hacan, 40TG, capacity4 | flagship +3 dreadnoughts | 4 ships | More money does not increase production capacity |
| Hacan, extra dock, capacity7 | flagship +6 infantry | 1 ship | Flagship itself removed in1/4 streams; initial ships retained in that stream |
| Jol-Nar, two initial infantry, 20TG | flagship +3 infantry | 1 ship | Newly built flagship removed in2/4 streams; the starting space fleet then remains unchanged |
| Jol-Nar, eight initial infantry, 20TG | flagship +2 dreadnoughts | 3 ships | Leaves one production capacity unused; retained fleet varies |
| Jol-Nar, no ready spending, two initial infantry | 2 infantry | 0 | Sarween discount is used successfully; production capacity2 remains but there is no more funding |

These are fixed synthetic conditions, not 24 independent game maps or a population-strength
estimate. 20/40TG round-one positions are deliberately cash-rich stress cases; their frequency
in ordinary student play is unmeasured. Forced infantry-first changes downstream production
and removals but is not a universal superior-action label.

At the funded Hacan/Jol-Nar flagship root, `production:fleet_excess_after=1` and
`production:fleet_headroom_after=-1` are both emitted **and assigned in the actual vocabulary**;
their payload counterparts are also assigned. This is direct evidence the actor receives this
particular constraint, not proof that its learned weights use it well. Root OOV names are
limited here to `payload-bool:free_this_use:False`, `payload-number:available_resources`, and
`payload-number-kind:available_resources:produce`; unknown names alone do not justify a migration.

Recommended next intervention investigation: validate production/removal coordination in
engine-reachable student game positions (including realistic funds and capacity contrasts),
then compare choices by retained useful fleet, resource spent and the actual transport/landing
chain. If those continuations confirm avoidable waste, build narrow preference supervision;
do not add a duplicate fleet-excess feature or label every expensive ship wrong. The cash-free
Jol-Nar result specifically argues against assuming the bot cannot take free infantry.

Independent final raw/statistical review: `/root/review_probe` approved after separately parsing
raw/summary JSON, verifying72 results/four streams/replay flags/672 decisions/zero final excess,
matching raw SHA256, comparing all18 serial rows to the parallel subset, and checking the final
build/run/example-test exit markers. Reviewer confirmed the sampled production/removal cases;
approval is limited to synthetic diagnostics. No weights changed,
training launched, checkpoint promoted, holdout pool consumed, objective gain measured or
full-game strength improvement claimed. No task processes remain after final verification.
