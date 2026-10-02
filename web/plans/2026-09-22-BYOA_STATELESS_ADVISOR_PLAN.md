# Stateless "Bring Your Own Advisor" (BYOA) Architecture & Redaction Unification Plan

## 1. Overview & Architectural Goals

This plan specifies the implementation of the **stateless "Bring Your Own Advisor" (BYOA)** architecture and the unification of server-side state redaction.

### Core Principles

1. **Zero ML / PyTorch in `ti4-server`**:
   The game server remains a pure-Rust, lightweight router. It handles game authority, lobbies, session persistence, and WebSocket subscriptions, compiling quickly with zero C++ or `libtorch` dependencies.
2. **Symmetrical, Headless Clients**:
   Bots and humans connect using the exact same WebSocket wire protocol and unguessable `seat_token` capability model. The server makes no distinction between a human in React, a headless Python script, or an external neural bot daemon.
3. **Single Source of Truth for Redaction (Eliminating Drift Liability)**:
   `ti4-server`'s wire projections (`project_player_view`) currently perform redundant custom hand-scrubbing. We refactor `ti4-server` to build its wire projections on top of the engine's authoritative `ti4_model::view::view_for`. Non-server crates remain untouched (facedown draw decks remain unscrubbed for now, as requested).
4. **Stateless Snapshot Evaluation**:
   The engine's `GameState` is fully serializable (`serde`). The advisor service holds no session state, stores no database, and requires no replay history. Given a single `(redacted GameState, GalaxyLayout, PlayerId, Choice)` snapshot, it reconstructs the evaluator's galaxy context and returns policy probabilities and critic values. The projection preserves the actor's observable private holdings but never depends on facedown draw-deck identities or order.
5. **AI Advisor for Human Players**:
   Human players can visualize policy recommendations (probability badges per option) and the raw critic output in the web UI by querying the stateless advisor. This phase does not assign the critic output a calibrated win-probability meaning.

---

## 2. Architecture & Data Flow

```
                      ┌─────────────────────────────────┐
                      │           ti4-server            │
                      │  (Pure Rust, zero ML runtime)   │
                      └────────┬───────────────┬────────┘
          WebSocket (seat_token)│               │WebSocket (seat_token)
                               ▼               ▼
      ┌──────────────────────────────┐   ┌──────────────────────────────┐
      │     Human Browser Client     │   │      Headless Bot Agent      │
      │  - React 19 + SVG UI         │   │  - Connects as seated player │
      │  - Receives Choice & State   │   │  - Receives Choice & State   │
      │  - Displays Advisor Badges   │   │  - Submits choices via WS    │
      └──────────────┬───────────────┘   └──────────────┬───────────────┘
                     │ HTTP POST                        │ HTTP POST / In-Process
                     ▼                                  ▼
      ┌─────────────────────────────────────────────────────────────────┐
      │                    Stateless Advisor Service                    │
      │                  (`ti4-advisor` / standalone)                   │
      │  - Links `ti4-mlp` & `ti4-policy` with `libtorch`               │
      │  - Evaluates `(redacted GameState, GalaxyLayout, PlayerId, Choice)` │
      │  - Returns `{ head, value: V(s), options: [{ id, p, logit }] }` │
      └─────────────────────────────────────────────────────────────────┘
```

---

## 3. Work Packages

```
[BYOA-01: Redaction Unification in ti4-server]
                      │
                      ▼
[BYOA-02: Expose Redacted State, Galaxy Layout, and Engine Choice]
                      │
         ┌────────────┴────────────┐
         ▼                         ▼
[BYOA-03: Stateless Advisor]  [BYOA-04: Headless Bot Agent]
         │
         ▼
[BYOA-05: Web Client AI Advisor UI]
          │
          ▼
[BYOA-06: Optional Public Protocol Extraction]
```

---

### Work Package BYOA-01: Redaction Unification in `ti4-server`

#### Objective

Refactor [`crates/ti4-server/src/projection.rs`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-server/src/projection.rs) so that client views are derived strictly from `ti4_model::view::view_for`, eliminating the dual-maintenance drift risk where `ti4-server` independently implements private hand scrubbing.

#### Technical Details

1. In `projection.rs`, replace manual filtering of `held_action_cards` and `held_secret_objectives` in `project_player_view`:
   - Run the authoritative `ti4_model::view::view_for(state, viewer_seat)` first.
   - For a spectator viewer, redact all players using `ti4_model::view::redact_player`.
   - Populate `PlayerView` directly from the resulting redacted `Player` struct:
     - Cards matching the engine's `ti4_model::view::HIDDEN` marker (`"?"`) are excluded from `held_action_cards` and `held_secret_objectives`.
     - Hand counts (`action_cards_count`, `secret_objectives_count`) reflect the total hand size without disclosing private card identities.
2. In `crates/ti4-server/tests/projection_redaction.rs`:
   - Maintain 100% passing tests proving opponent/spectator secret objectives and action cards remain strictly redacted.
3. **Zero changes to `ti4-model` or `ti4-engine`**.

#### Tests to Add

- Actor, opponent, and spectator projections prove that `PlayerView` derives hand identities and counts from `view_for`, including Search Warrant's revealed-secret exception.
- Regression coverage proves hidden markers never appear as held card identities in the server wire view.

#### Status

Completed 2026-09-22. `project_game_view_with_map` now applies `view_for` once for player viewers and `redact_player` to every player for spectators before creating `PlayerView`s. The projection filters only the engine's `HIDDEN` markers from identity lists while retaining the redacted hand lengths as public counts. Added Search Warrant and spectator-marker regression tests. Verified with `cargo fmt --check`, `cargo test -p ti4-server --test projection_redaction` (7 passed), and `cargo test -p ti4-server` (49 passed).

---

### Work Package BYOA-02: Expose Redacted State and Galaxy Layout in Server Protocol

#### Objective

Provide every connected client with an independently advisor-evaluable WebSocket payload: the engine-level redacted `GameState`, a reconstructible `GalaxyLayout`, and the actor's engine `Choice` when one is pending.

#### Technical Details

1. **WebSocket State Attachment**:
   - Add the redacted `GameState` returned by `ti4_model::view::view_for` to `InitialSnapshotMsg` and `StateUpdateMsg` for every viewer. The actor's own observable holdings remain present; opponents' holdings are represented by the engine's hidden markers.
   - Add the same redacted state to actor-only `PendingChoiceMsg`. Each of these messages is independently usable without a client cache or an HTTP round-trip.
   - Do not add a `GET /api/games/:game_id/state` endpoint, special headers, query parameters, or optional capability. State delivery is part of the normal WebSocket protocol.
2. **Explicit `GalaxyLayout` Contract**:
   - Add a serializable, versioned `GalaxyLayout` containing the active source set, main-map system placements (`system_id`, `q`, `r`), and off-map system IDs.
   - Attach the layout beside the redacted state in every `InitialSnapshotMsg`, `StateUpdateMsg`, and actor `PendingChoiceMsg`.
   - Do not serialize `ti4_content::galaxy::Galaxy` or reuse presentation-oriented `BoardTileView` as this contract. `Galaxy` carries mutable derived wormhole state, while `BoardTileView` includes display-only/synthetic entries. The advisor reconstructs the galaxy from `GalaxyLayout` and derives dynamic wormhole state from the supplied `GameState`.
3. **Engine Choice Wire Contract**:
   - Send the serde-serialized engine `Choice` directly in `PendingChoiceMsg`, retaining only the server envelope fields required for safe submission (`nonce`, `game_version`, and game/protocol identifiers).
   - Remove `PendingChoiceDto` once all server and web protocol consumers use `Choice`. It contains no additional decision information; its only unique field is the submission nonce, which belongs in the envelope.

#### Tests to Add

- WebSocket integration coverage proves every `InitialSnapshot` and `StateUpdate` contains that viewer's exact `view_for` state and reconstructible `GalaxyLayout`.
- Actor-only pending-choice coverage proves the message contains the exact engine `Choice`, redacted state, and layout; non-actors receive neither choice nor state-bearing choice message.
- Galaxy-layout round-trip coverage rebuilds `Galaxy`, applies state-derived wormhole effects, and verifies topology-sensitive adjacency agrees with the authoritative session.

#### Status

Completed 2026-09-22. The server protocol carries the viewer-redacted engine `GameState` and a versioned `GalaxyLayout` on snapshots, updates, and actor pending-choice messages. Pending choices use the serde engine `Choice` with a nonce envelope; the Rust DTO module was removed. Server integration tests now use the envelope and engine `Choice` fields, and the layout round-trip test reconstructs on-map and off-map topology, including wormhole adjacency. Verified with `cargo fmt --all`, `cargo test -p ti4-server` (50 passed), and `npm test -- --run` (153 passed).

---

### Work Package BYOA-03: Stateless Advisor Service (`ti4-advisor`)

#### Objective

Build a standalone, stateless HTTP/IPC service crate `crates/ti4-advisor` that links `ti4-mlp` and `ti4-policy` to evaluate any position on demand.

#### Technical Details

1. **Crate Setup (`crates/ti4-advisor`)**:
   - Independent workspace crate depending on `ti4-mlp`, `ti4-policy`, `ti4-model`, `ti4-content`, `axum`, and `serde`.
   - Leaves `ti4-server` untouched by `libtorch`.
2. **API Contract**:
   - `POST /evaluate`:
     ```json
     {
       "state": {/* Redacted GameState JSON */},
       "galaxy_layout": {/* source set, placements, off-map systems */},
       "player": "p1",
       "choice": {/* Engine Choice JSON */},
       "temperature": 0.25
     }
     ```
   - **Evaluation Pipeline**:
     1. Deserializes `(state, galaxy_layout, player, choice)`.
     2. Reconstructs `Galaxy` from `galaxy_layout`, then applies dynamic wormhole state from `state`.
     3. Constructs in-memory `Observed::new(&state, content, sources, Some(&galaxy))`.
     4. Calls `ti4_policy::projection::mlp_choice_features(&observed, &choice, &player, ...)`.
     5. Runs forward pass through the preloaded `Actor` and extracts logits and softmax probabilities.
     6. Runs value pass on `CriticInput` to compute $V(s)$.
   - Returns:
     ```json
     {
       "head": "tactical_movement",
       "value": 1.425,
       "options": [
         { "option_id": "move|24|0", "probability": 0.682, "logit": 2.45 },
         { "option_id": "decline", "probability": 0.318, "logit": 1.12 }
       ]
     }
     ```
3. **Execution Availability**:
   - Service integration tests use a bounded timeout to detect hangs. No performance latency claim or gate is part of this phase.
4. **Startup & Configuration**:
   - CLI flags: `--checkpoint <DIR>` (default: `examples/reviewer/checkpoint-473312`), `--port <PORT>` (default: `8081`).

#### Status

Implementation completed and Linux-verified 2026-09-22. Added the standalone loopback-only `ti4-advisor` crate with a preloaded, validated bundle and `POST /evaluate`; it validates bounded requests, reconstructs `Galaxy` from the versioned layout, applies state-derived wormhole effects, and returns logits, normalized probabilities, and the raw critic value. Layout/source validation tests were added. `cargo fmt --all`, `cargo metadata --no-deps --format-version 1`, `git diff --check`, and `cargo test -p ti4-server` (50 passed) succeed. The Linux CPU libtorch 2.9.1 archive is pinned by `plans/artifacts/libtorch-2.9.1-cpu-linux.manifest.json`; with `LIBTORCH=out/libtorch-2.9.1-cpu-linux`, `LIBTORCH_BYPASS_VERSION_CHECK=1`, and its `lib/` directory on `LD_LIBRARY_PATH`, `cargo test -p ti4-advisor` passes (5 passed). The missing Windows `XNNPACK.lib` installation remains unverified.

#### Tests to Add

- `POST /evaluate` returns one finite logit and probability per supplied engine option; probabilities sum to one and the raw critic value is finite.
- Invalid state, choice, galaxy-layout, source-set, and checkpoint compatibility inputs return bounded client errors without panicking.
- A topology-sensitive position proves reconstructed layout and state-derived wormhole effects reach the same policy-feature projection as in-process evaluation.
- Service integration uses a bounded timeout to report a hung evaluation as failure.

---

### Work Package BYOA-04: Headless Symmetrical Bot Agent (`ti4-bot-agent`)

#### Objective

Implement a lightweight, standalone bot runner that connects to `ti4-server` as a normal player over WebSocket, proving end-to-end symmetry between humans and bots.

`ti4-bot-agent` is a separate workspace crate, not a binary in `ti4-advisor`. It is an ordinary
WebSocket and HTTP client and must not link `ti4-mlp`, `ti4-policy`, `ti4-tensor`, or libtorch.
It may initially depend on `ti4-server` only for the versioned public wire DTOs and
`PROTOCOL_VERSION`; BYOA-06 describes the optional removal of that implementation dependency.

#### Technical Details

1. **Standalone CLI Binary**:
   - Runs as an independent process:
     ```bash
     cargo run -p ti4-bot-agent -- \
       --server ws://localhost:8080 \
       --game game_123 \
       --seat p2 \
       --token <seat_token> \
       --advisor http://127.0.0.1:8081
     ```
   - `--server`, `--game`, `--seat`, `--token`, and `--advisor` are required. The advisor URL
     is a local deployment choice, not a server-provided capability.
   - `--temperature <positive-finite-f64>` defaults to `0.25`. The default selection mode is
     deterministic argmax. Stochastic selection is enabled only with `--sample-seed <u64>` and
     uses a seeded `ChaCha` RNG; it never uses wall-clock entropy.
   - Dependencies are limited to protocol DTOs, Tokio/WebSocket transport, HTTP/JSON, and the
     optional seeded sampler. The agent holds no game state beyond the currently received message.
2. **Event Loop**:
   - Connects to `ws://localhost:8080/ws/games/:game_id`.
   - Sends `ClientMessage::Subscribe { game_id, seat_token }`.
   - Extracts a pending choice from every independently evaluable actor message:
     `ServerMessage::InitialSnapshot.pending_choice`, `ServerMessage::StateUpdate.pending_choice`,
     and `ServerMessage::PendingChoice`. Reconnection therefore never relies on a separate
     pending-choice broadcast or a client-side state cache.
   - Before evaluation, verifies the protocol version and game ID, that `choice.player` matches
     `--seat`, and that the choice has non-empty, unique option IDs.
   - Posts the exact received `(state, galaxy_layout, player, choice, temperature)` tuple to the
     stateless advisor. The agent does not reconstruct state, synthesize a choice, or retain a
     previous snapshot.
   - Refuses an advisor response unless it has one finite logit and finite non-negative probability
     for each offered ID, has no duplicate or unknown IDs, and has a positive finite total
     probability. Argmax breaks equal scores by the choice's original stable option order.
   - Sends `ClientMessage::SubmitChoice { nonce, expected_version, option_id }` only after the
     selected ID is proven to be offered by that exact received `Choice`.
3. **Failure, Reconnect, and Submission Semantics**:
   - HTTP evaluation and WebSocket reads/writes have bounded timeouts. Advisor refusal, malformed
     server data, unsupported protocol versions, and exhausted reconnect attempts exit non-zero
     with a diagnostic; the agent never substitutes a locally guessed action.
   - On `ActionRejected`, discard the evaluated choice and wait for a newly delivered pending
     choice. Never retry a rejected nonce/version or submit a result produced for an earlier
     snapshot.
   - On a transport disconnect, reconnect with bounded exponential backoff controlled by
     `--max-reconnects`, then resubscribe using the same seat token. Never resend a cached
     submission after reconnect; act only from the new initial snapshot or a later message.
   - Exit successfully on `GameOver`.
4. **Zero Rules Engine Coupling**:
   - The bot runner does not link libtorch because it delegates evaluation to the advisor service
     over HTTP. It uses engine `Choice` only as a wire value and never invokes rules, projection,
     or legality code locally.

#### Status

Implemented 2026-09-22. `ti4-bot-agent` is a standalone workspace crate and CLI with no direct
ML, policy, tensor, or libtorch dependency. It validates advisor output before submitting a legal
option, defaults to stable-order argmax, supports reproducible seeded sampling, and reconnects only
to act from a newly received snapshot or pending-choice message. The server protocol re-exports
`Choice` so the agent treats it as a wire value rather than importing engine APIs directly.
Verification: `cargo fmt --all --check`, `cargo test -p ti4-bot-agent` (6 passed), and
`cargo test -p ti4-server` (50 passed). Independent review remains required before package closure.

#### Tests to Add

- Unit coverage proves stable-order argmax selection, reproducible seeded sampling, and rejection
  of malformed advisor outputs (unknown IDs, duplicates, non-finite values, incomplete choices,
  and invalid probability totals).
- A mock-advisor WebSocket integration test proves each submission uses only an option ID offered
  by the received engine `Choice`, with that exact nonce and game version.
- Disconnect and reconnect coverage proves a bot resumes from `InitialSnapshot.pending_choice`
  without any retained state or cached submission.
- A bounded automated match with one scripted seat and two bot-agent seats, backed by a stub
  advisor, advances a fixed number of decisions or rounds without server errors or hung tasks.
- A negative end-to-end test proves an advisor response containing an unknown option ID results in
  no submission.

#### Non-Goals

- Loading a checkpoint, linking libtorch, or offering in-process policy evaluation.
- Introducing server-only authentication or a bot-specific WebSocket message. Seat tokens and the
  shared client protocol remain the sole authority model.
- Claiming a full-game completion test when the scripted scenario cannot deterministically reach
  its declared round/decision bound.

---

### Work Package BYOA-05: Web Client AI Advisor UI

#### Objective

Add an "AI Advisor" toggle and visualization to the React web client in `web/`, rendering policy recommendations and position evaluations for human players.

#### Technical Details

1. **Advisor Hook (`web/src/hooks/useAdvisor.ts`)**:
   - Configurable setting: `Advisor Service URL` (default `http://localhost:8081`, with ability to disable).
   - When a `pendingChoice` arrives and the viewer is the actor, asynchronously queries `POST /evaluate` with the attached redacted state, `GalaxyLayout`, and engine `Choice`.
   - Holds `{ value, options: Map<string, OptionAdvice>, head, loading }`.
2. **Visual Badges on Choices**:
   - On `PendingChoiceModal`, `Board` candidate hexes, and workflow drawers:
     - Render percentage badges next to legal options: `[ 68% ]`, `[ 24% ]`, `[ 8% ]`.
     - Highlight the top recommended choice with an accessible star or "AI Recommended" pill.
3. **Position Evaluation Display**:
   - Mount an unobtrusive raw critic-value display in the header status bar or Player Sheet. Do not present it as a win probability or expected progress until its semantics are defined and calibrated.
4. **Non-Blocking & Graceful Degradation**:
   - If the advisor service is offline or unreachable, the UI functions normally with no errors or latency impact on human turns.

#### Tests to Add

- Vitest coverage verifies advisor badges and the raw critic value render from a successful advisor response.
- Disabled, failed, and timed-out advisor requests leave choice selection and submission usable without visible errors.
- A new pending-choice nonce discards stale advisor results rather than rendering advice for a prior decision.

---

### Work Package BYOA-06: Optional Public Protocol Extraction

#### Objective

Extract the versioned online-client wire DTOs and `GalaxyLayout` from `ti4-server` into a small
public `ti4-protocol` crate, so that the server, advisor, bot agent, and generated web bindings all
depend on the same contract rather than clients depending on the server implementation crate.

#### Preconditions

- BYOA-01 through BYOA-05 are complete and their protocol compatibility tests are green.
- This is optional cleanup, not a prerequisite for deploying the bot agent. Do not start it merely
  to rename imports or reorganize modules.

#### Technical Details

1. Move only wire-contract types: `ClientMessage`, `ServerMessage`, their envelopes/status/error
   DTOs, protocol version/bounds constants, `GameView` wire types, and `GalaxyLayout`/placement
   DTOs. Keep session workers, HTTP routes, storage, and projection construction in `ti4-server`.
2. Preserve serde tags, field names, defaults, bounds, stable ordering, and protocol version exactly.
   This is a compatibility-preserving schema move, not a protocol redesign.
3. Update `ti4-server`, `ti4-advisor`, and `ti4-bot-agent` to import the shared crate. The advisor
   must no longer depend on `ti4-server` solely to deserialize `GalaxyLayout`.

#### Tests to Add

- Existing server protocol round-trip, size-bound, lifecycle, advisor layout, bot-agent, and web
  contract tests pass without fixture changes.
- Cross-crate serialization fixtures prove byte-for-byte JSON equivalence before and after the move.
- Dependency metadata confirms `ti4-bot-agent` and `ti4-advisor` do not depend on `ti4-server`.

#### Non-Goals

- Adding messages, fields, protocol versions, authentication modes, or behavior changes.
- Moving server runtime/session implementation into the protocol crate.

---

## 4. Verification & Testing Matrix

| Package     | Test Type                   | Acceptance Gate                                                                                                                                                                                                           |
| ----------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **BYOA-01** | Unit & Integration          | `cargo test -p ti4-server --test projection_redaction` passes (5/5). Proves `view_for` is the sole source of redaction truth.                                                                                             |
| **BYOA-02** | WebSocket Integration       | Integration tests verify each `InitialSnapshot`, `StateUpdate`, and actor `PendingChoice` carries a `GameState` matching `view_for`, an exact reconstructible `GalaxyLayout`, and the engine `Choice` only for its actor. |
| **BYOA-03** | Service Integration         | Unit/E2E test verifies `POST /evaluate` on standard positions returns finite probabilities summing to $1.0$ and a finite raw critic value within a bounded hang-detection timeout.                                        |
| **BYOA-04** | E2E Multiplayer             | Automated match test: 1 human player (scripted/mock) + 2 headless bot agents play through multiple game rounds over WebSocket without server errors.                                                                      |
| **BYOA-05** | Frontend Vitest             | Vitest component tests verify option badges render when advisor data is present, and degrade cleanly when advisor is disabled.                                                                                            |
| **BYOA-06** | Optional Wire Compatibility | Existing JSON fixtures and cross-crate serialization remain byte-for-byte compatible while clients cease depending on `ti4-server`.                                                                                       |

---

## 5. Non-Goals for this Phase

- Scrubbing draw decks (`action_card_deck`, `objective_deck`, etc.) inside `ti4-model` or `ti4-engine` (deferred to maintain zero non-server modifications).
- Embedding PyTorch / `libtorch` into `ti4-server`.
- Client-side WebAssembly neural network inference.
