# M00-004 — Interface inventory

## ID and title
M00-004 — Interface inventory

## Milestone and dependencies
- Milestone: M00 — Oracle and baseline
- Dependencies: M00-002 (Tracked-file scope ledger) ✅

## One-sentence objective
Catalogue supported Python entry points, CLI tools, bridge endpoints, wire messages, and public construction APIs without modifying any oracle source.

## Package map

### M00-004a — Public construction APIs
- **Objective:** Inventory every public constructor, factory, and class used to build game state, content, and policy objects.
- **Dependency:** M00-002
- **Oracle read scope:** `engine/state.py`, `engine/content/`, `engine/ml/`, `engine/learned_policy.py`, `engine/policy_linear.py`
- **Evidence output:** `plans/evidence/M00-004a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)
- **DoD:** Every public class/constructor with its init signature and dependencies listed; zero unlisted oracle construction path.

### M00-004b — Module entry points and CLI tools
- **Objective:** Inventory every `engine/*.py`, `bridge/*.py`, `tools/*.py` entry point, `if __name__ == "__main__"` block, and CLI tool.
- **Dependency:** M00-002
- **Oracle read scope:** `engine/`, `bridge/`, `tools/` (all individually listed in M00-002 ledger)
- **Evidence output:** `plans/evidence/M00-004b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)
- **DoD:** Every module with its entry points and CLI flags documented; zero unlisted tool or module.

### M00-004c — Bridge HTTP endpoints
- **Objective:** Inventory every HTTP route, handler, and response schema in the TTS bridge server.
- **Dependency:** M00-002
- **Oracle read scope:** `bridge/server.py`, `bridge/commands.py`, `bridge/perform.py`, `bridge/importer.py`, `bridge/reconcile.py`, `bridge/hexsummary.py`, `bridge/outcomes.py`, `bridge/explain.py`, `bridge/person.py`
- **Evidence output:** `plans/evidence/M00-004c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)
- **DoD:** Every endpoint with method, path, request schema, and response schema documented; zero unlisted route.

### M00-004d — Wire commands, messages, and telemetry
- **Objective:** Catalogue every wire message type, command schema, telemetry event, and Lua bridge helper contract.
- **Dependency:** M00-002
- **Oracle read scope:** `bridge/commands.py`, `bridge/audit.py`, `tts/bridge_system_helper.lua`, `tts/bridge_executor.lua`, `tts/bridge_strategy_helper.lua`, `tts/bridge_explore_helper.lua`, `out/` telemetry artifacts
- **Evidence output:** `plans/evidence/M00-004d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)
- **DoD:** Every message/command type with its JSON schema documented; zero unlisted wire protocol element.

### M00-004e — Reconciliation and independent review
- **Objective:** Cross-check all children for completeness, resolve gaps, and obtain independent frontier review.
- **Dependency:** M00-004a, M00-004b, M00-004c, M00-004d
- **Evidence output:** `plans/evidence/M00-004e.md`, `plans/reviews/M00-004_FRONTIER_REVIEW.md`
- **Permissions:** P0 (read oracle), P1 (write evidence and review)
- **DoD:** All children reconciled; no orphaned interfaces; independent review passes with no findings.

## Compatibility invariants
- All four inventory children are documentation-only. Zero changes to oracle source, test, or configuration files.
- Every inventory row must cite the exact oracle path and line number or discovery command.
- Completion of all five children (a–e) plus independent review is required to close M00-004.
- No implementation changes, no code generation, no oracle inspection beyond read-only discovery.
