# M00-008 — Fixture selection

## Package details
- **ID:** M00-008
- **Title:** Fixture selection
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-008
- **Dependencies:** M00-003 (Test ledger), M00-007 (Canonical projection spec) ✅

## Objective
Select minimal fixtures covering setup, phases, tactical steps, payments, cards, factions, TE, policies, training, and bridge. Fixtures must enable differential parity testing against the Python oracle.

## Work packages

### M00-008a — Setup fixtures
- **Objective:** Select fixtures covering game setup (faction selection, strategy card draft, board setup).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008b — Phase fixtures
- **Objective:** Select fixtures covering all phases (strategy, action, status, agenda).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008c — Tactical step fixtures
- **Objective:** Select fixtures covering tactical steps (movement, combat, invasion, production).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008d — Payment fixtures
- **Objective:** Select fixtures covering payments (trade, fleet construction, technology purchase).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008e — Card fixtures
- **Objective:** Select fixtures covering cards (action cards, strategy cards, objectives, relics).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008f — Faction fixtures
- **Objective:** Select fixtures covering all factions (base + expansions).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008f.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008g — Thunder's Edge fixtures
- **Objective:** Select fixtures covering Thunder's Edge content (breakthroughs, galactic events, etc.).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008g.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008h — Policy fixtures
- **Objective:** Select fixtures covering policy baselines and learned policy inference.
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008h.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008i — Training fixtures
- **Objective:** Select fixtures covering training (stage 1/2, Parquet capture, surrogate data).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008i.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-008j — Bridge fixtures
- **Objective:** Select fixtures covering TTS bridge (HTTP endpoints, wire commands, telemetry).
- **Dependency:** M00-003, M00-007
- **Evidence output:** `plans/evidence/M00-008j.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

## Compatibility invariants
- Every fixture must be replayable from the Python oracle.
- Fixtures must cover all compatibility surfaces identified in M00-006.
- Minimal set: no fixture may be redundant with another.

## DoD
- Every fixture category with its selected fixtures documented.
- Zero untested compatibility surface.
