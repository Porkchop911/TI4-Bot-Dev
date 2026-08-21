# Planning index

Autonomous agents must follow [`../AGENTS.md`](../AGENTS.md), obey
[`SCOPED_PERMISSIONS.md`](SCOPED_PERMISSIONS.md), and keep
[`EXECUTION_STATE.md`](EXECUTION_STATE.md) current, especially before context compaction.

| Order | Plan | Outcome |
|---:|---|---|
| 0 | [M00 — Historical reference and baseline](M00_ORACLE_AND_BASELINE.md) | Frozen scope, historical corpus, trustworthy benchmarks |
| 1 | [M01 — Repository bootstrap](M01_REPOSITORY_BOOTSTRAP.md) | Reproducible Windows Rust workspace and CI |
| 2 | [M02 — Content and model](M02_CONTENT_AND_MODEL.md) | Validated content corpus and complete domain state |
| 3 | [M03 — Choice, timing, replay](M03_CHOICE_TIMING_REPLAY.md) | Stable decisions, timing stack, deterministic replay |
| 4 | [M04 — Game skeleton](M04_GAME_SKELETON.md) | Setup, phases, turns, and generic complete games |
| 5 | [M05 — Tactical pipeline](M05_TACTICAL_PIPELINE.md) | Activation through production |
| 6 | [M06 — General rules](M06_GENERAL_RULES.md) | Economy, technology, objectives, cards, agendas, laws |
| 7 | [M07 — Factions and TE](M07_FACTIONS_AND_TE.md) | Current faction and Thunder's Edge behavior |
| 8 | [M08 — Authored bots](M08_AUTHORED_BOTS.md) | Existing scored bots, planning, valuation, explanations |
| 9 | [M09 — Learned policy](M09_LEARNED_POLICY.md) | Schemas 2–6, factual features, CPU MLP inference |
| 10 | [M10 — Simulation and training](M10_SIMULATION_AND_TRAINING.md) | Maps, batches, distillation/PPO, archives, evaluation |
| 11 | [M11 — TTS bridge](M11_TTS_BRIDGE.md) | Wire-compatible Windows/TTS integration |
| 12 | [M12 — Qualification](M12_QUALIFICATION.md) | Rules conformance, fuzzing, mutation, security, performance |
| 13 | [M13 — Cutover](M13_CUTOVER.md) | Workload-ready release and incumbent rollback path |

Milestones are sequential gates. Work packages inside a milestone may run in parallel only
when their `Depends` entries are satisfied and their edit scopes do not overlap.
