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

Milestone-specific exit gates remain binding. Dependency-ready packages may run in parallel only
when their `Depends` entries are satisfied and their edit scopes do not overlap.

## Operator-authorized independent tooling

| Plan | Outcome | Relationship to migration milestones |
|---|---|---|
| [R01 — Offline game review viewer](R01_REVIEW_VIEWER.md) | Read-only offline game inspection from validated review artifacts | Optional; does not gate or depend on M10 training. |
| [R02 — Interactive branching game replayer](R02_REPLAYER.md) | Manual seat control and verified non-destructive play-from-frame branches | Optional sibling to R01; R01 remains read-only and unchanged. |
| [PPO training operator guide](PPO_TRAINING_HOWTO.md) | Copy-pasteable build, smoke-test, foreground/background launch, monitoring, migration, reward, and flag reference | Operational companion to M10; does not replace checkpoint evaluation or qualification. |

## Active workstreams and archive

Work follows authorized dependencies rather than an automatic M00-to-M13 sequence. The milestone table above remains a contract map; open qualification and release gates still apply.

| Workstream | Entry points | Status |
|---|---|---|
| Base and later factions | [BF plan](BASE_FACTIONS_PLAN_2026-10-02.md), [Cabal queue](HANDOVER_2026-10-06_CABAL_PI.md), [current state](EXECUTION_STATE.md) | Current committed faction work is distinct from outstanding BF20–25 runtime/roster/schema/simulation/UI/exit qualification. |
| Diplomacy and trade | [Unification](ENGINE_DIPLOMACY_UNIFICATION.md), [trade rework](TRADE_REWORK_2026-09-22.md), [bug plan](BUG_PLAN_2026-09-20.md) | Accepted decisions and unresolved follow-through remain active; historical handovers are archived. |
| Policy and training | [Training guide](PPO_TRAINING_HOWTO.md), [decision contract](STAGE2_COMPLETE_DECISION_CONTRACT.md), [activation plan](ACTIVATION_REWORK_PLAN_2026-09-17.md) | Historical experiments are not promotion or qualification evidence. |
| Bridge and release obligations | [M11](M11_TTS_BRIDGE.md), [M12](M12_QUALIFICATION.md), [M13](M13_CUTOVER.md) | Retained; not declared complete by implementation or archival. |

See the [archive manifest and surviving obligations](ARCHIVE_MANIFEST_2026-10-06.md) for every move and successor. The [verbatim historical state](archive/EXECUTION_STATE_HISTORY_2026-10-06.txt) is a text record, not a live resume point. Its original relative references are retained literally; current navigation is in this index, the manifest, and task evidence.
