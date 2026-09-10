# TTS bridge v1 — authoritative Rust bot game

## Objective

Run an all-bot game using the current Rust engine's exact decision surface and current Rust MLP,
with Tabletop Simulator acting as the visual executor through the retained loopback Python/Lua
transport.

The historical Python engine is not a legality or choice authority for this mode.

## Authority boundaries

- Rust `GameState` is authoritative.
- Current Rust choice producers supply every decision and continuation without translation or
  reconstruction.
- The current Rust MLP receives the same seat-private observation and options as ordinary Rust
  simulation.
- Rust applies the selected option and emits engine events.
- The bridge translates complete engine effects into TTS commands.
- The existing loopback Python server and Lua executor may transport commands and outcomes.
- TTS telemetry is used for observable reconciliation, not to replace authoritative Rust state
  between decisions.
- A refusal, incomplete translation, timeout, or reconciliation mismatch stops the run.
- Dry-run is the default. Live TTS mutation remains P3 and requires explicit operator authority.

## First implementation slice

1. Make the existing offline imported-state simulation reproducible.
2. Detect and report repeated decision/state/answer cycles with a compact diagnostic.
3. Root-cause and fix the round-one livelock without changing the policy's decision surface.
4. Add a regression test proving deterministic progress from the offending state.
5. Add a differential trace proving the bridge adapter sees the same choices and policy features as
   the ordinary Rust simulation path.

## Acceptance criteria

### Offline progress gate

- A fixed capture, checkpoint, engine seed and policy seed completes the requested round within a
  documented step bound at greedy evaluation temperature.
- No identical `(state fingerprint, choice fingerprint, selected option)` tuple repeats beyond the
  cycle threshold.
- Every selected non-pass option changes authoritative state or advances a typed continuation.
- Two repeated runs produce identical decision and final-state fingerprints.

### Exact decision-surface gate

At every decision, the bridge path and ordinary simulation path agree on:

- acting player;
- typed decision source and context;
- option count and order;
- option IDs, kinds and canonical payloads;
- seat-private policy feature fingerprint;
- selected option;
- resulting engine events and state fingerprint.

Acceptance is zero unexplained differences across complete deterministic games and multiple fixed
seeds.

### Translation gate

- Every engine event reached in the qualification corpus is translated or explicitly refused.
- A partially translatable effect queues no commands.
- Command ordering and command IDs are deterministic.
- Outcomes are matched to command IDs; silence is never success.

### Dry-run qualification gate

- One complete deterministic all-bot game finishes through the bridge coordinator.
- No Python-generated legal choice enters the path.
- There are no silent fallbacks, untranslated effects, or unresolved reconciliation differences.

### Live qualification gate

This gate is outside the initial P1/P2 implementation slice and requires explicit P3 authority.

- A disposable TTS table executes a complete all-bot game.
- Every command batch is confirmed and reconciled before the next externally visible batch.
- Any mismatch stops execution without automatic correction.

## Permissions

Permission class required: P1 for source, tests and documentation; P2 for bounded local builds and
offline simulation; P3 only for later live TTS execution.

Writable paths: `D:/Projects/ti4-engine-rs` package-owned bridge, MLP example, tests, and evidence
files.

Read-only external paths: `D:/Projects/ti4-engine` at pinned commit `37061c5`, only when exact wire
or executor behavior needs inspection.

Network access: none.

Processes/ports: bounded Cargo tests and simulations; loopback bridge self-tests only.

Expected generated artifacts: bounded diagnostics under ignored `out/`; no training artifacts.

Destructive actions: none.

External-state changes: none during offline work.
