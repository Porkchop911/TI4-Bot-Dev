# M00-006 — Compatibility classification

## Package details
- **ID:** M00-006
- **Title:** Compatibility classification
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-006
- **Dependencies:** M00-005 (Artifact inventory) ✅

## Objective
Classify every artifact and file from the oracle as `exact`, `semantic`, `translated`, `intentional-change`, or `not-applicable`. Produce the M00 compatibility ledger.

## Work packages

### M00-006a — Content files classification
- **Objective:** Classify all 29 content JSON files from M00-005a for compatibility.
- **Dependency:** M00-005a
- **Evidence output:** `plans/evidence/M00-006a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006b — Map pools classification
- **Objective:** Classify map pool JSON.GZ files from M00-005d.
- **Dependency:** M00-005d
- **Evidence output:** `plans/evidence/M00-006b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006c — Policy baselines classification
- **Objective:** Classify baseline policy JSON files from M00-005c.
- **Dependency:** M00-005c
- **Evidence output:** `plans/evidence/M00-006c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006d — Training artifacts classification
- **Objective:** Classify training directories from M00-005e.
- **Dependency:** M00-005e
- **Evidence output:** `plans/evidence/M00-006d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006e — Output JSON classification
- **Objective:** Classify output JSON files from M00-005f.
- **Dependency:** M00-005f
- **Evidence output:** `plans/evidence/M00-006e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006f — Surrogate and checkpoint classification
- **Objective:** Classify surrogate data and PyTorch checkpoints from M00-005g/h.
- **Dependency:** M00-005g, M00-005h
- **Evidence output:** `plans/evidence/M00-006f.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006g — Logs and captures classification
- **Objective:** Classify log files and TTS captures from M00-005i.
- **Dependency:** M00-005i
- **Evidence output:** `plans/evidence/M00-006g.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006h — Parquet and test artifacts classification
- **Objective:** Classify Parquet test artifacts from M00-005j.
- **Dependency:** M00-005j
- **Evidence output:** `plans/evidence/M00-006h.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-006i — Compatibility ledger consolidation
- **Objective:** Produce the consolidated M00 compatibility ledger.
- **Dependency:** M00-006a through M00-006h
- **Evidence output:** `plans/evidence/M00-006i.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- Every artifact must have exactly one classification.
- No artifact may be unclassified.
- `exact` classification requires that Rust must produce byte-identical or structurally identical output.
- `semantic` classification allows structural differences as long as behavior is equivalent.
- `translated` classification means Rust must load the artifact via a conversion step.
- `intentional-change` requires documented rationale for divergent behavior.
- `not-applicable` means the artifact is not part of the Rust compatibility surface.

## DoD
- Every artifact category with its classification documented.
- Consolidated ledger with artifact count per classification.
