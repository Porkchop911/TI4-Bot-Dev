# M00-004b — Module Entry Points and CLI Tools Inventory Work Packages

## Scope
Partition M00-004b (engine/, bridge/, tools/ module entry points and CLI tools) into atomic Pi work packages. Tools partition in progress.

## Status
Bootstrap complete; do not treat M00-004b as complete.

## Tools/ partition

### M00-004b1 — tools/__init__.py
- **Target paths:** `tools/__init__.py`
- **P0/P1 scope:** P0 read `D:\Projects\ti4-engine\tools\__init__.py`; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b1.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b2 — Benchmarking tools
- **Target paths:** `tools/benchmark_engine.py`, `tools/benchmark_m3_devices.py`, `tools/bootstrap_ml_cuda.ps1`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b2.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b3 — Analysis tools
- **Target paths:** `tools/analyze_margin_by_faction.py`, `tools/analyze_score_tails.py`, `tools/attribute_victory_points.py`, `tools/measure_tactical_waste.py`, `tools/tech_census.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b3.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b4 — Audit tools
- **Target paths:** `tools/audit_oracle_headroom.py`, `tools/audit_scoring_decision_space.py`, `tools/trace_equality.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b4.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b5 — Policy analysis tools
- **Target paths:** `tools/characterize_complete_policy.py`, `tools/policy_coverage.py`, `tools/screen_policy_groups.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b5.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b6 — Comparison/ridge tools
- **Target paths:** `tools/compare.py`, `tools/compare_m3_ridge.py`, `tools/ridge_choice_sets.py`, `tools/ridge_learning_curve.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b6.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b7 — Data capture/labeling tools
- **Target paths:** `tools/decision_capture.py`, `tools/label_decisions.py`, `tools/ml_manifest.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b7.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b8 — Deepsets/sweep tools
- **Target paths:** `tools/deepsets_choice.py`, `tools/dial_sweep.py`, `tools/sweep_m3_deepsets.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b8.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b9 — Evaluate tools (part 1: action/activation/archived/cooperation/letnev)
- **Target paths:** `tools/evaluate_action_mode_model.py`, `tools/evaluate_activation_model.py`, `tools/evaluate_archived_stage1_six_player.py`, `tools/evaluate_cooperation_pair.py`, `tools/evaluate_letnev_guide.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b9.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b10 — Evaluate tools (part 2: ml_audit/promoted/research/rolling/sanitized/shallow)
- **Target paths:** `tools/evaluate_ml_audit.py`, `tools/evaluate_promoted_tactical_compatibility.py`, `tools/evaluate_research_model.py`, `tools/evaluate_rolling_strategy.py`, `tools/evaluate_sanitized_activation_search.py`, `tools/evaluate_shallow_tactical_search.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b10.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b11 — Evaluate tools (part 3: stage1/strategy/tactical)
- **Target paths:** `tools/evaluate_stage1_weight_transfer.py`, `tools/evaluate_strategy_compatibility.py`, `tools/evaluate_strategy_model.py`, `tools/evaluate_strategy_policy.py`, `tools/evaluate_tactical_macro_model.py`, `tools/evaluate_tactical_plan_model.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b11.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b12 — Evolution tools
- **Target paths:** `tools/evolution_gpu_analysis.py`, `tools/evolve_save52_complete_league.py`, `tools/evolve_save52_league.py`, `tools/evolve_save54_three_player.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b12.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b13 — Explain/export tools
- **Target paths:** `tools/explain.py`, `tools/export_strategy_model.py`, `tools/export_tactical_macro_model.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b13.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b14 — Legacy extraction
- **Target paths:** `tools/extract_asyncti4.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b14.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b15 — Feature census
- **Target paths:** `tools/feature_census.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b15.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b16 — SOL policy tool
- **Target paths:** `tools/full_sol_policy.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b16.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b17 — LLM tools
- **Target paths:** `tools/llm_bias.py`, `tools/llm_match.py`, `tools/llm_probe.py`, `tools/llm_regret.py`, `tools/llm_series.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b17.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b18 — Tactical planning tools
- **Target paths:** `tools/macro_plan_series.py`, `tools/measure_tactical_waste.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b18.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b19 — ML choice sets
- **Target paths:** `tools/ml_choice_sets.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b19.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b20 — Optimization tools
- **Target paths:** `tools/optimize_complete_sol_policy.py`, `tools/optimize_sol_weights.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b20.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b21 — Symmetry analysis
- **Target paths:** `tools/orthogonal_group_screen.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b21.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b22 — Save file patching
- **Target paths:** `tools/patch_save.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b22.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b23 — TTS tools
- **Target paths:** `tools/play_tts.py`, `tools/send.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b23.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b24 — Training progress
- **Target paths:** `tools/policy_gradient_progress.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b24.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b25 — Policy rescoring
- **Target paths:** `tools/rescore_sanitized_activation.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b25.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b26 — Map rotation
- **Target paths:** `tools/rotation_suite.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b26.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b27 — Score analysis
- **Target paths:** `tools/score_breakdown.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b27.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b28 — Simulation runner
- **Target paths:** `tools/simulate.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b28.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b29 — Capability/tournament tools
- **Target paths:** `tools/single_seat_capability.py`, `tools/sol_bowl.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b29.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b30 — SOL tools
- **Target paths:** `tools/sol_capability_ablation.py`, `tools/sol_capability_audit.py`, `tools/sol_planning_factorial.py`, `tools/sol_scoring_schedule.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b30.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b31 — Matchup tool
- **Target paths:** `tools/sota_matchup.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b31.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b32 — Training scripts
- **Target paths:** `tools/train_m1_hybrid.py`, `tools/train_m1_pilot.py`, `tools/train_m2_pilot.py`, `tools/train_m3_deepsets.py`, `tools/train_stage1_policy_gradient.py`, `tools/train_tactical_macro_pilot.py`, `tools/train_tactical_plan_pilot.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b32.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b33 — Training infrastructure
- **Target paths:** `tools/training_analysis.py`, `tools/training_archive.py`, `tools/training_index.py`, `tools/training_lineage.py`, `tools/training_schema.py`, `tools/training_surrogate.py`, `tools/training_surrogate_fit.py`, `tools/training_telemetry.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b33.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b34 — CUDA verification
- **Target paths:** `tools/verify_ml_cuda.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b34.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b35 — Efficiency measurement
- **Target paths:** `tools/vp_per_compute.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b35.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b36 — Bridge/collect/check tools
- **Target paths:** `tools/bridge.py`, `tools/build_map_pool.py`, `tools/check_ml_conformance.py`, `tools/check_round_one.py`, `tools/collect_ml_rollout_pilot.py`, `tools/collect_tactical_macro_pilot.py`, `tools/collect_tactical_plan_pilot.py`, `tools/ablate_round_variation.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b36.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.

### M00-004b37 — SOL policy evaluation
- **Target paths:** `tools/evaluate_complete_sol_policy.py`
- **P0/P1 scope:** P0 read oracle; P1 write evidence
- **Evidence file:** `plans/evidence/M00-004b37.md`
- **Acceptance rule:** Top-level entry points, `__main__` block, argparse/click flags, and exclusions with exact lines.
