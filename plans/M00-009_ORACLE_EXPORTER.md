# M00-009 — Oracle exporter

## Package details
- **ID:** M00-009
- **Title:** Oracle exporter
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-009
- **Dependencies:** M00-007 (Canonical projection spec), M00-008 (Fixture selection) ✅

## Objective
Create a new-repo tool that invokes the old repo read-only and emits versioned NDJSON projections. Repeated export must be byte-identical.

## Work packages

### M00-009a — Exporter design
- **Objective:** Design the Oracle exporter tool (CLI, module structure, configuration).
- **Dependency:** M00-007, M00-008
- **Evidence output:** `plans/evidence/M00-009a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009b — State projection implementation
- **Objective:** Implement state projection emission from GameState.
- **Dependency:** M00-007a
- **Evidence output:** `plans/evidence/M00-009b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009c — View projection implementation
- **Objective:** Implement view projection emission from GameView.
- **Dependency:** M00-007b
- **Evidence output:** `plans/evidence/M00-009c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009d — Choice projection implementation
- **Objective:** Implement choice projection emission from Choice.
- **Dependency:** M00-007c
- **Evidence output:** `plans/evidence/M00-009d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009e — Event projection implementation
- **Objective:** Implement event projection emission from Event.
- **Dependency:** M00-007d
- **Evidence output:** `plans/evidence/M00-009e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009f — Outcome projection implementation
- **Objective:** Implement outcome projection emission from GameState at game end.
- **Dependency:** M00-007e
- **Evidence output:** `plans/evidence/M00-009f.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009g — Error projection implementation
- **Objective:** Implement error projection emission from exceptions.
- **Dependency:** M00-007f
- **Evidence output:** `plans/evidence/M00-009g.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-009h — Reproducibility verification
- **Objective:** Verify repeated export is byte-identical.
- **Dependency:** M00-009b through M00-009g
- **Evidence output:** `plans/evidence/M00-009h.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

## Compatibility invariants
- The exporter must invoke the Python oracle read-only (no file writes to oracle).
- Repeated export of the same game must be byte-identical.
- All projections must follow the schema from M00-007h.

## DoD
- Oracle exporter tool designed and documented.
- All projection types implemented.
- Reproducibility verified.
