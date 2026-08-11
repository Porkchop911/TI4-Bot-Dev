# M00-005 — Artifact inventory

## Package details
- **ID:** M00-005
- **Title:** Artifact inventory
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-005
- **Dependencies:** M00-002 (Tracked-file scope ledger) ✅

## Objective
Catalogue JSON, JSON.GZ, Parquet, checkpoints, profiles, baselines, map pools, telemetry, decision logs, and TTS captures with schema/version evidence.

## Work packages

### M00-005a — Content JSON files (engine/content/)
- **Objective:** Inventory all 35 JSON content files under engine/content/ with schema, version, and dependency evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `engine/content/*.json` (35 files)
- **Evidence output:** `plans/evidence/M00-005a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005b — ML configuration files (configs/ml/)
- **Objective:** Inventory ML configuration files under configs/ml/ with parameter schema evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `configs/ml/*.json` (4 files)
- **Evidence output:** `plans/evidence/M00-005b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005c — Baseline policies (docs/baselines/)
- **Objective:** Inventory baseline policy JSON files under docs/baselines/ with version and faction evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `docs/baselines/*.json` (12 files)
- **Evidence output:** `plans/evidence/M00-005c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005d — Map pools (data/map_pools/)
- **Objective:** Inventory map pool JSON.GZ files with schema/version evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `data/map_pools/*.json.gz` (2 files)
- **Evidence output:** `plans/evidence/M00-005d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005e — Training artifacts (out/training/)
- **Objective:** Inventory training directories with manifest.json, concept_ontology.json, parameter_schema.json per training run.
- **Dependency:** M00-002
- **Oracle read scope:** `out/training/` (~70 directories)
- **Evidence output:** `plans/evidence/M00-005e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005f — Output JSON files (out/)
- **Objective:** Inventory benchmark, champion, evaluation, and pipeline output JSON files.
- **Dependency:** M00-002
- **Oracle read scope:** `out/*.json`, `out/champions/*.json`
- **Evidence output:** `plans/evidence/M00-005f.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005g — Surrogate data (JSON.GZ)
- **Objective:** Inventory surrogate data JSON.GZ files under out/stage1_pg_* and out/stage2_pg_* directories.
- **Dependency:** M00-002
- **Oracle read scope:** `out/*_surrogate/*.json.gz`
- **Evidence output:** `plans/evidence/M00-005g.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005h — PyTorch checkpoints and reports
- **Objective:** Inventory .pt model files with purpose evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `out/*.pt`, `out/evolution_gpu_*/surrogate.pt`
- **Evidence output:** `plans/evidence/M00-005h.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005i — Log files and TTS captures
- **Objective:** Inventory log files under out/ and TTS capture JSON files under runs/capture/.
- **Dependency:** M00-002
- **Oracle read scope:** `out/*.log`, `out/runlogs/*/stdout.log`, `out/runlogs/*/stderr.log`, `runs/capture/*.json`
- **Evidence output:** `plans/evidence/M00-005i.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-005j — Parquet test artifacts
- **Objective:** Inventory Parquet files under .pytest_tmp/ with schema evidence.
- **Dependency:** M00-002
- **Oracle read scope:** `.pytest_tmp/**/*.parquet` (3 files)
- **Evidence output:** `plans/evidence/M00-005j.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

## Compatibility invariants
- All artifacts are read-only inventory. Zero changes to oracle source, test, or configuration files.
- Every artifact row must cite the exact oracle path and file size.
- Completion of all ten children (a–j) is required to close M00-005.

## DoD
- Every artifact category with its file count, schema/version, and Rust relevance documented.
- Zero unlisted artifact category.
