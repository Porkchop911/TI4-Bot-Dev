---
type: Component
title: Interfaces and hosting
description: Trace projected game state, protocol messages, browser clients, and review surfaces.
timestamp: 2026-10-18
source_commit: 1f3556d1
confidence: high
resource: docs/TECHNICAL_WIKI_INTERFACES.md
citations:
  - crates/ti4-host/src/protocol/mod.rs
  - crates/ti4-host/src/state_update.rs
  - crates/ti4-view/src/projection.rs
---

# Interfaces and hosting

[Technical wiki](TECHNICAL_WIKI.md) · [Engine](TECHNICAL_WIKI_ENGINE.md) · [Bots](TECHNICAL_WIKI_BOTS.md)

Use this route to trace a client view, submitted decision, native inspection action or bridge message. Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18.

| Surface | First files |
|---|---|
| Shared projections and protocol | [ti4-view](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-view/src/lib.rs), [projection](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-view/src/projection.rs); [ti4-host](../crates/ti4-host/src/lib.rs), [protocol](../crates/ti4-host/src/protocol/mod.rs), [state update](../crates/ti4-host/src/state_update.rs) |
| Online service | [ti4-server](../crates/ti4-server/src/lib.rs), [HTTP](../crates/ti4-server/src/http), [WebSocket](../crates/ti4-server/src/ws), [session](../crates/ti4-server/src/session) |
| Browser clients | [web README](../web/README.md), [web entry](../web/src/main.tsx); [web2 entry](../web2/src/main.tsx), [web2 UI rules](../web2/AGENTS.md) |
| Local browser engine | [ti4-wasm](../crates/ti4-wasm/src/lib.rs), [host API](../crates/ti4-wasm/src/host_api.rs); it builds on host/view rather than the online server process |
| Online bot service/client | [advisor](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-advisor/src/lib.rs) and [bot agent](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-bot-agent/src/lib.rs); their READMEs describe the service boundary |
| Native inspection and branches | [review](../crates/ti4-review/src/lib.rs), [replayer](../crates/ti4-replayer/src/lib.rs), [rebuild](../crates/ti4-replayer/src/rebuild.rs) |
| Tabletop Simulator | [bridge crate](../crates/ti4-bridge/src/lib.rs), [M11 contract](../plans/M11_TTS_BRIDGE.md); the `bridge/server.py` mentioned in both is absent from this checkout, so verify that dependency before following the documented loopback route |

## One online decision, end to end

1. The [engine](../crates/ti4-engine/src/choice.rs) creates a choice owned by a specific player. The [server session](../crates/ti4-server/src/session) owns the live game and pending request.
2. The [host protocol](../crates/ti4-host/src/protocol/mod.rs) defines versioned client and server messages (`PROTOCOL_VERSION` is 3 at this commit). Parsing rejects a different version. The [host state-update builder](../crates/ti4-host/src/state_update.rs) computes a message for each viewer.
3. [View projection](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-view/src/projection.rs) starts from model redaction, then builds player, board, combat and pending-choice views. `state_update_for` includes reaction modes and auto-resolved notes only for the owning player; this is a privacy boundary, not a formatting detail.
4. The selected client receives the update and renders the pending offer. [web](../web/src/main.tsx) and [web2](../web2/src/main.tsx) are separate entry points. [web2](../web2/src/main.tsx) chooses online, single, local, gallery and example modes from URL parameters; its [online session](../web2/src/dev/OnlineApp.tsx) is distinct from its [local table](../web2/src/session/localTable.ts).
5. The client submits an option ID. Follow the [protocol client message](../crates/ti4-host/src/protocol/client.rs), [server WebSocket handling](../crates/ti4-server/src/ws), and [session](../crates/ti4-server/src/session) to the engine's [validation](../crates/ti4-engine/src/choice.rs). A rejected action, stale version, missing pending choice, and wrong rendering are different failures.

For a wrong browser decision, capture the engine offer, viewer role, protocol payload, rendered option ID and submitted ID in that order. Test redaction separately from UI layout; see [invariants](TECHNICAL_WIKI_INVARIANTS.md) and [tests](TECHNICAL_WIKI_TESTS.md). Shared host code does not prove the two browser clients behave identically.

## Local browser mode

[Wasm](../crates/ti4-wasm/src/lib.rs) runs the game behind a plain C ABI for local play. It writes JSON or error text to a response buffer and shares host/view concepts with the online path, but it does not call the online server process. [web2 local table](../web2/src/session/localTable.ts) validates setup changes through the Wasm transport and serializes updates. If only local mode fails, inspect [Wasm host API](../crates/ti4-wasm/src/host_api.rs), [transport](../web2/src/session/wasmTransport.ts), then the relevant local UI. If both modes fail with the same projected fact, start in [view](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-view/src/projection.rs).

## Native and auxiliary surfaces

[Review](../crates/ti4-review/src/lib.rs) inspects learned games; [replayer](../crates/ti4-replayer/src/lib.rs) adds manual control and branch rebuilding. Its [control types](../crates/ti4-replayer/src/control.rs) bind control to a physical player ID and require a matching pending-choice fingerprint and offered ID before a manual submission. Its [rebuild](../crates/ti4-replayer/src/rebuild.rs) verifies a replay prefix. This is a separate path from an online WebSocket submission.

The [advisor](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-advisor/README.md) evaluates a viewer-redacted snapshot and legal choice over loopback HTTP; the [bot agent](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-bot-agent/README.md) receives an online pending choice, calls the advisor and submits an offered option. Their READMEs document the runtime and opt-in end-to-end test. The [bridge crate](../crates/ti4-bridge/src/lib.rs) and [M11 plan](../plans/M11_TTS_BRIDGE.md) describe a Tabletop Simulator connection, but `bridge/server.py` referenced there is absent in this checkout. Treat that route as a dependency gap until an implementation and test are present.

For a replay divergence, trace the [replayer rebuild](../crates/ti4-replayer/src/rebuild.rs) and engine snapshot path in [the engine route](TECHNICAL_WIKI_ENGINE.md). For UI changes, follow the relevant client's own instructions and screenshot/test workflow. For bridge capability or deployment, consult current plans and evidence before inferring acceptance from a module's presence.
