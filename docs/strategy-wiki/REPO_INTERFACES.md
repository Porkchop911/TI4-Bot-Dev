# Online play, review and replay

[Repo guide](REPO_GUIDE.md) · [Engine](REPO_ENGINE.md) · [Bots](REPO_BOTS.md)

Use this route for browser behavior, multiplayer projections, online bots and native inspection tools. Source scope: `rule-bot`, navigation checked through 2026-10-18. Verify these implementation paths against the target branch before using them as current architecture.

## Which interface are you changing?

| Area | What it does / where to begin |
|---|---|
| [ti4-server](../../crates/ti4-server/src/lib.rs) | Authoritative online game service, protocol and seat-redacted projections. Follow [http/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/http), [ws/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/ws), [session/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session), [protocol/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/protocol), [projection.rs](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/projection.rs) |
| [web](../../web/README.md) | React browser client. [main.tsx](../../web/src/main.tsx) mounts [App.tsx](../../web/src/App.tsx); its imports route to session hooks, board/decision components and presentation helpers |
| [ti4-advisor](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-advisor/README.md) | Loopback service for MLP advice against a supplied redacted snapshot; documented evaluation endpoint and runtime setup live in its README |
| [ti4-bot-agent](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-bot-agent/README.md) | Headless WebSocket client that receives a pending online choice, asks an advisor and submits a legal option |
| [ti4-review](../../crates/ti4-review/src/lib.rs) | Native game inspection, distinct from branching/manual play. See [REVIEWER_HOWTO.md](../../plans/REVIEWER_HOWTO.md) and [view.rs](../../crates/ti4-review/src/view.rs) |
| [ti4-replayer](../../crates/ti4-replayer/src/lib.rs) | Manual-seat control and branching replay. Read [control.rs](../../crates/ti4-replayer/src/control.rs), [decider.rs](../../crates/ti4-replayer/src/decider.rs), [live.rs](../../crates/ti4-replayer/src/live.rs), [rebuild.rs](../../crates/ti4-replayer/src/rebuild.rs), [project.rs](../../crates/ti4-replayer/src/project.rs) |
| [ti4-bridge](../../crates/ti4-bridge/src/lib.rs) | Tabletop Simulator integration: wire/mapping/import/command and reconciliation-related code. Consult the [M11 contract](../../plans/M11_TTS_BRIDGE.md) and current evidence before assuming a live bridge capability is accepted |

The [replayer manifest](../../crates/ti4-replayer/Cargo.toml) distinguishes the `ti4-replayer` host binary from `ti4-join`; feature flags determine which host/simulation dependencies are present. Inspect the actual build configuration when a remote-seat build and full host behave differently.

Native review can assign seats to the headless [RuleBotDecider](../../crates/ti4-server/src/session/random_bot.rs) through [review configuration](../../crates/ti4-review/src/lib.rs); [replayer project configuration](../../crates/ti4-replayer/src/project.rs) and [the setup UI](../../crates/ti4-replayer/src/gui.rs) retain that choice. This is a local bot-seat route, separate from the online advisor/bot-agent path.

## Reading route: a wrong browser decision

1. Identify the pending engine `Choice` and `ChoiceOption` in [choice.rs](../../crates/ti4-engine/src/choice.rs).
2. Inspect the server's [protocol](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/protocol) and [projection](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/projection.rs) for what this viewer is allowed to receive.
3. Follow [App.tsx](../../web/src/App.tsx) into the relevant component, hook or presentation adapter. The [web source folders](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/web/src) and [package scripts](../../web/package.json) are the current client inventory.
4. Check that the UI interpretation and submitted option ID match the offered decision; then test the authoritative transition. Rendering an option is different from proving its legality.

For UI validation, read the [screenshot workflow](../../web/e2e/screenshots/README.md), [Playwright configuration](../../web/playwright.config.ts) and [web README](../../web/README.md). Repository instructions require visual evidence for UI changes, and folder creation still requires operator approval. Existing screenshot fixtures are not automatically real game evidence.

## Running or inspecting a game

Use [web/README.md](../../web/README.md) for local online play, [advisor instructions](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-advisor/README.md) and [bot-agent instructions](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-bot-agent/README.md) for the service/client path, and [reviewer instructions](../../plans/REVIEWER_HOWTO.md) for native inspection. Treat checkpoint examples and old measured results as dated examples. Current configuration, permissions and accepted execution evidence take precedence over historical prose.

For a replay divergence, start at [rebuild.rs](../../crates/ti4-replayer/src/rebuild.rs) and the engine fork/snapshot route in [Engine and content](REPO_ENGINE.md). Preserve the original trajectory when exploring a new branch.
