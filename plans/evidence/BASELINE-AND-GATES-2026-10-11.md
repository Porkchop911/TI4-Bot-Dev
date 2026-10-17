# Baseline and gate reconciliation — 2026-10-11

Status: CPU baseline and bounded checks complete; GPU training remains stopped at saved checkpoint-92. BF-22 / BF-24 / BF-25 qualification remains OPEN.

Operator correction: stop long training immediately; continue agreed baseline/correctness work. User authorized smaller subagents and confirmed no other sessions active. GPU continuation is stopped; no automatic restart/eval. This package uses CPU only, reads current sources, preserves unrelated dirty code and retained artifacts, and updates documentation/evidence only. No commit/push.

Checkout: D:/Projects/ti4-engine-rs, wp/base-factions at a8ed02533f3b2c1cc3bb38f49b280b5ba1b7e12a, intentionally dirty. Exact pre-existing paths remain in git status. No source hunks changed in this package.

## Baseline contract and result

Policy: out/ppo-cardtag-T-wide-pilot50-20261009/checkpoints/checkpoint-3584.
Evaluator: existing Oct9 release binary SHA256 69BCDE77A0CA3BCA9488FB21329F9776BD2FA93FEB271F667BCB720FC7DDF839, provenance 8432ba71 + dirty build; NOT current source a8ed0253.
Validation pool: out/pools/full_np8_12_holdout.json (runtime verified role).
Command: target/release/examples/clearance_eval.exe --bundle <policy> --map-pool <pool> --roster wide --seeds 12 --seed-base 900000000 --rounds 4 --temperature 0.001 --diplomacy --per-seat out/baseline-wide12-20261011.per-seat.tsv (actual flags separated by spaces).
RAYON_NUM_THREADS=16; OMP_NUM_THREADS=1; MKL_NUM_THREADS=1; no GPU optimiser/inference.
Result: exit 0;72 games, 432 seat-games;22.4 s;table VP 3.495, clearance 64.35%. Log:out/logs/baseline-wide12-20261011.log.
Exact key/value comparison with matching rows of historical out/eval-greedy600-cardtag-T-20261009.per_seat.tsv:432 rows, 0 missing, 0 different.
This demonstrates reproducibility of the pinned old binary/policy evaluation, not current-engine qualification, a policy improvement, or a new benchmark.

## Fresh focused current-source checks

One Cargo coordinator; compilation -j8, tests 8 threads. MLP checks explicitly use pinned CPU libtorch out/libtorch-2.9.1-cpu. Full logs retained; real native exit codes captured.

- cargo test -j8 -p ti4-engine --lib unit_upgrade -- --test-threads=8:15 passed, 0 failed. Log:out/logs/check-upgrade-gains-20261011.log.
- cargo test -j8 -p ti4-policy --lib reallocating -- --test-threads=8:2 passed, 0 failed.
- cargo test -j8 -p ti4-engine --lib a_technology_whose_waiver_was_declined_is_not_offered_again -- --test-threads=8:1 passed, 0 failed. Both logs:out/logs/check-bf22-migration-20261011.log.
- cargo test -j8 -p ti4-mlp --lib capacity_growth_keeps_every_logit_and_adds_zero_rows -- --test-threads=8:1 passed.
- same with copying_a_fallback_row_keeps_the_logit_of_a_moved_name:1 passed.
- same with lambda_returns_span_monte_carlo_to_one_step_bootstrap:1 passed. All 0 failed;log:out/logs/check-bf22-actor-20261011.log.

Total 21 focused tests passed. No workspace-wide pass, calibration campaign, UI/manual qualification, or performance claim.

## Source review and independent assistance

Root inspected vocabulary::append_reallocating,Actor::grow_input_capacity/copy_input_rows,migrate_bundle_append_names and the declined-waiver research loop. Append keeps existing indices; vocabulary growth validates a copy before assignment; input tables grow in both actor and separate critic; OOV initialization copies fallback rows. This is a bounded source inspection, not full BF22 Tier-C closure. Existing admitted floating-point variation and original-six compatibility obligations remain.

Luna continuation_checklist independently inventoried BF gate evidence. training_review (gpt-5.6-sol) independently traced current GAE records/returns/critic ordering. Root inspected the same per-seat return conversion and helper maths. No current episode-boundary mismatch found: returns/values generated from same seat records, conversion before combining seats, gamma = 1, freeze rejects invalid critic/targets. Four-round terminal-zero treatment is consistent with the established four-round episodic objective; it would need reconsideration for continuing-game training.

Follow-ups: public lambda_returns silently truncates mismatched lengths; caller temporarily substitutes 0 for absent critic even though record/freeze validation refuses missing values; no end-to-end reward-to-GAE regression. These are defensive/test gaps, not demonstrated faults in this equal-length,validated caller. No speculative code edits.

## Concrete remaining queue

1. BF22 Tier-C migration/roster review: complete all consumer/version/compatibility checks, resolve findings, then close review. Implemented migration is no longer blocked on capacity; old opening status is stale.
2. BF22b version 8 review: review feature/version gates and unsupported contexts. Train/export/swap/eval already recorded, so that old Open item is stale. Wide ground predictor remains scope; Yin Van Hauge probe lacks timing-resolver coverage (9.21pp mean disagreement,32/200|z|>3); Heaven's Eye timing repair fixture-only. These must remain explicit calibration limits.
3. BF24: enumerate current decision producers/types; map readable labels and real park/answer/resume evidence per new kind. Two borrowed-agent manual cases are covered; generic map/lineup work is not whole-BF24 acceptance. Ground casualty/sustain still open.
4. BF25: reconcile original18 faction pair gate with later accepted30 family/32 alias opt-in wide contract; freeze explicit pair/seat/seed schedule before running sampled campaign. Existing one-faction replacement soaks do not prove every pair. Then workspace suite,ledgers,frontier exit review and report. BF-23 authored-bot rebaseline was dropped by operator; do not resurrect it.

Next bounded work: finish BF22 version/consumer review and add missing acceptance checks only where a concrete coverage gap warrants them. No long training until the baseline/review prerequisites and experimental question are explicit.


Independent Tier-A documentation review: Luna continuation_checklist found two stale Open-list entries and ambiguous checkpoint wording. Both completed items moved out of Open; baseline status now explicitly says GPU training remains stopped. No unsupported qualification/causal claim found. No source fix or acceptance gate waived.

