# R01 implementation evidence — interactive learned-game reviewer

## Scope and authorization

The operator requested an overhaul of the failed offline-bundle plan and a one-pass implementation
of a native, omniscient learned-game reviewer. The operator explicitly waived R01's independent
review/package cadence for this add-on only. Normal safety, bounds, testing, evidence, and workspace
quality requirements remained in force.

The requirements were confirmed interview-style before implementation. The locked behavior is:

- select a learned checkpoint and map pool with native file-picker buttons;
- select a current MLP bundle or learner/accepted legacy profiles, seed, and one of six faction
  rotations;
- open on the real post-deployment starting table before the first simulator step;
- advance by one `Game::step()`, next resolved decision, next top-level action, a bounded variable
  number of any of those units, end of round, or end of game;
- stop long runs at a clean engine-step boundary;
- expose the omniscient table, legal options, selected option, model scores/probabilities, and every
  projected feature value (plus exact weight/contribution for linear profiles);
- navigate captured history without mutating or branching simulation state;
- autosave, reopen view-only sessions, and export a self-contained read-only HTML replay.

## Delivered architecture

- `ti4-review` is a real adapter over `ti4-engine`, `ti4-sim`, `ti4-training`, `ti4-model`,
  `ti4-policy`, and `ti4-mlp`; hand-authored review JSON is not the simulation path.
- `ti4-training::setup_game_with_decider_factory` exposes the exact established setup baseline
  without stepping it. The reviewer captures that state as frame zero.
- A tracing learned decider records every choice resolved inside an engine step, including nested
  decisions. Each option contains its policy score, probability, and decomposed feature rows.
- Schema-6 MLP input accepts the bundle directory, `manifest.json`, or `slots.json`. The latter two
  resolve to the containing directory and pass through `ti4_mlp::bundle::read`, which verifies the
  complete inventory, digests, vocabulary, shapes, runtime identity, heads, and faction roster.
  MLP feature rows are honestly labelled nonlinear: there is no single fixed input weight to show.
- The native Windows GUI uses `eframe`/`egui` with `rfd` common-controls file dialogs. Dependencies
  are pinned to versions compatible with the workspace's Rust 1.94.1 toolchain.
- Long GUI commands execute in bounded slices (at most 128 engine steps per UI update), so Stop
  remains responsive and always takes effect between `Game::step()` calls.
- Session JSON and standalone HTML writes are bounded and adjacent-temp/backup based. The maximum
  serialized artifact is 512 MiB; a command is capped at 2,000,000 engine steps and Run N at
  1,000,000 requested units.

## Verification results

All commands ran from `D:\Projects\ti4-engine-rs` on 2026-08-28.

```text
cargo fmt --all -- --check
passed

cargo clippy -p ti4-review --all-targets -- -D warnings \
  -A clippy::too-many-lines -A clippy::type-complexity -A clippy::missing-panics-doc
passed

cargo clippy -p ti4-training --lib -- -D warnings \
  -A clippy::too-many-lines -A clippy::type-complexity -A clippy::missing-panics-doc
passed

cargo test -p ti4-review
3 passed; 0 failed

cargo test --workspace
passed; all crate, binary, example, and doc-test targets completed with exit code 0

git diff --check
passed (Git emitted only its configured Cargo.lock LF-to-CRLF notice)
```

The Clippy allowances cover existing findings outside the add-on: one long engine function, one
training PPO type, and existing training panic-documentation findings. No `ti4-review` warning was
suppressed by them.

## Real workflow smoke evidence

The operator-reported failing input was reproduced with
`out/checkpoints/run-011/checkpoint-154720/slots.json`. After the repair, that exact selection
resolved its sibling bundle and completed a real MLP decision:

```text
cargo run -p ti4-review -- simulate \
  --checkpoint out/checkpoints/run-011/checkpoint-154720/slots.json \
  --map-pool out/pools/save52_noadj_train.json --seed 42 --rotation 0 \
  --unit decision --count 1 --out out/reviews/r01-mlp-slots-smoke.ti4review.json

saved 2 frames; steps=1 decisions=1 actions=0 target=true outcome=InProgress
cargo run -p ti4-review -- validate out/reviews/r01-mlp-slots-smoke.ti4review.json
valid: 2 frames, InProgress
```

The captured strategy decision contained 8/8 logits, 8/8 probabilities, the sampled option
`pok5trade`, and 396 projected feature rows. The session records the normalized checkpoint bundle
directory rather than misrepresenting `slots.json` as a profile table.

The full-game smoke used the real repository artifacts
`out/stage2_r6/final10000.json` and `out/pools/save52_noadj_train.json`, learner profiles, seed 44,
and rotation 2:

```text
cargo run -p ti4-review -- simulate --checkpoint out/stage2_r6/final10000.json \
  --map-pool out/pools/save52_noadj_train.json --seed 44 --rotation 2 \
  --table learner --until end --out out/reviews/r01-complete-smoke.ti4review.json

saved 2923 frames; steps=2922 decisions=3233 actions=1754 target=true outcome=Completed

cargo run -p ti4-review -- validate out/reviews/r01-complete-smoke.ti4review.json
valid: 2923 frames, Completed

cargo run -p ti4-review -- render out/reviews/r01-complete-smoke.ti4review.json \
  out/reviews/r01-complete-smoke.html
passed
```

The validated session was 349,290,465 bytes and the self-contained HTML was 349,283,551 bytes,
both below the declared bound. Equality between the trace's 3,233 decisions and the engine table
log proved nested choices were not discarded.

A separate accepted-profile smoke used seed 42, rotation 5, and `--unit action --count 1`. It
stopped after exactly one top-level action: 20 frames, 19 engine steps, 19 decisions, and 7,505
feature-contribution rows; validation passed with `InProgress`, demonstrating resumable partial
capture and the alternate profile-table selector.

Finally, `target/debug/ti4-review.exe` was launched with no arguments. The native process remained
healthy after the startup interval and was then stopped by exact PID; this verifies the default GUI
entry path without leaving a background process.

## Review disposition

No independent R01 review was requested or performed, per the operator's explicit one-add-on
waiver. The implementation itself was still exercised by focused tests, the full workspace suite,
both policy-table smoke paths, complete-game simulation, session validation, HTML rendering, and
native GUI startup.

## Visual board and player-sheet follow-up

The operator fixed the ownership semantics: a thick system edge represents exclusive **space**
control, while every planet independently uses its controller's background color. The native and
standalone HTML renderers now share a fixed six-seat palette and implement that distinction.
Planets are circles inside their systems with name, resources/influence, trait, technology
specialty, and legendary labels. Units are grouped by owner and true content `baseType`; fighters,
destroyers, cruisers, carriers, dreadnoughts, flagships, war suns, infantry, mechs, PDS, and space
docks use different geometric silhouettes with count, damage slash, and galvanize ring.

Player sheets replace the former raw summary with colored stat badges and grouped cards for
strategy cards, controlled planets, on-board unit classes, technology, scored and held secret
objectives, action cards, relics/fragments, leaders, plots, and breakthroughs. Exact complete JSON
remains available behind a disclosure control.

A fresh schema-6 MLP smoke captured 37 tiles and 42 planets, including 29 trait-bearing, 9
technology-specialty, and 5 legendary planets. The two-frame session validated, the enhanced HTML
export was 90,998 bytes, and its embedded JavaScript passed `node --check`. Focused compilation and
all four `ti4-review` tests passed. Browser-based visual inspection could not run because the
desktop browser-control runtime failed to initialize; no browser-pass claim is made.
