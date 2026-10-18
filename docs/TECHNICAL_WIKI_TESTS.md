---
type: Test Surface
title: Tests and verification
description: Choose targeted checks for engine, replay, bot, protocol, and client changes.
timestamp: 2026-10-18
source_commit: 1f3556d1
confidence: high
resource: docs/TECHNICAL_WIKI_TESTS.md
citations:
  - .github/workflows/ci.yml
  - crates/ti4-server/tests/it/fork_equivalence.rs
  - web2/package.json
---

# Tests and verification

[Technical wiki](TECHNICAL_WIKI.md) · [Engine](TECHNICAL_WIKI_ENGINE.md) · [Bots](TECHNICAL_WIKI_BOTS.md) · [Interfaces](TECHNICAL_WIKI_INTERFACES.md) · [Invariants](TECHNICAL_WIKI_INVARIANTS.md)

Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18. Commands below are read from manifests and test files. They describe **how to run** a check; no pass is implied by its inclusion here. Start with the smallest check that crosses the changed boundary, then broaden only if a remaining risk calls for it. The [CI workflow](../.github/workflows/ci.yml) runs `cargo test --workspace` as a workspace gate.

## Pick by symptom

| Symptom or change | First test surface | Then, if the boundary crosses |
|---|---|---|
| Content ID or source-scope mismatch | [Engine content-ID test](../crates/ti4-engine/tests/content_ids_resolve.rs), [content validator](../crates/ti4-content/src/validator.rs) | Game setup or a specific rule's tests. |
| Missing, illegal or wrongly delivered option | [Engine decision inventory](../crates/ti4-engine/tests/decision_delivery_inventory.rs), affected rule's unit tests | [Server rejection test](../crates/ti4-server/tests/it/session_rejections.rs) for network submission. |
| Replay, preview, batch or RNG divergence | [Fork equivalence](../crates/ti4-server/tests/it/fork_equivalence.rs), [batch fork differential](../crates/ti4-server/tests/it/batch_fork_differential.rs) | [Crash recovery](../crates/ti4-server/tests/it/crash_recovery.rs) or [replayer rebuild](../crates/ti4-replayer/src/rebuild.rs) when persistence is involved. |
| Private state leaks into a response | [Projection redaction](../crates/ti4-server/tests/it/projection_redaction.rs) | [Protocol roundtrip](../crates/ti4-server/tests/it/protocol_roundtrip.rs), [Wasm/host path](../crates/ti4-wasm/src/host_api.rs). |
| Bot feature, bundle or training change | [Policy golden fixtures](../crates/ti4-policy/tests), [MLP API refusal tests](../crates/ti4-mlp/tests/api_boundary.rs), [MLP critic invariance](../crates/ti4-mlp/tests/critic_invariance.rs) | [Training return fixture](../crates/ti4-training/tests/golden_returns.json) and a controlled simulation. |
| Browser rendering or flow | [web package scripts](../web/package.json), [web2 package scripts](../web2/package.json) | Relevant browser integration or Playwright test, matched to online or local mode. |
| Advisor/agent integration | [Bot agent unit tests](../crates/ti4-bot-agent/README.md) | Its documented opt-in [real E2E](../crates/ti4-bot-agent/tests/real_e2e.rs) after the pinned runtime and checkpoint are available. |

## Command examples

From the repository root, `cargo test -p ti4-engine` checks that crate. For a single named integration test, use `cargo test -p ti4-engine --test content_ids_resolve` (from the [test file](../crates/ti4-engine/tests/content_ids_resolve.rs) and [crate manifest](../crates/ti4-engine/Cargo.toml)). The [server integration entry](../crates/ti4-server/tests/it/main.rs) groups many cases in one integration target; `cargo test -p ti4-server --test it fork_equivalence::` filters that module. On Windows, build requirements for learned crates may include the pinned libtorch described in the [advisor README](../crates/ti4-advisor/README.md).

For web clients, use the scripts declared in each manifest: `npm test` in `web/` runs the content check then Vitest; `npm test` in `web2/` runs Vitest. `web2` also declares `npm run typecheck`, `npm run build`, and desktop/phone Playwright projects under `npm run e2e`. `web` declares its own `test:e2e` and offline/smoke variants. See [web scripts](../web/package.json), [web2 scripts](../web2/package.json), and [web2 Playwright config](../web2/playwright.config.ts). Match the test to the client and mode you changed; `web2` local Wasm and online server paths are distinct at [entry](../web2/src/main.tsx).

## How to interpret a result

A focused unit pass establishes the local rule or data transform, not the full hosted flow. A fork comparison establishes equivalence for the covered seed and decision prefix, not every possible game. A policy golden fixture guards feature/head serialization; it is not a playing-strength measurement. A bot tournament needs a documented roster, seed set, maps, seat rotation, checkpoint, cutoff and failure treatment; see [simulation](../crates/ti4-sim/src/lib.rs), [evaluation](../crates/ti4-training/src/evaluation.rs), and [dated experiment evidence](../plans/evidence). Keep observed test results in the work item or [evidence](../plans/evidence), rather than marking a route here as globally passed.

For wiki-only edits, check local Markdown links, headings and source references, then record the check in the [wiki log](TECHNICAL_WIKI_LOG.md). This page is a map of test surfaces; source files and the current CI run remain the authority.
