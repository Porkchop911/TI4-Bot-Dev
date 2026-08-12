# Evidence index

## Classification rule

A file is a **placeholder** (generated stub, not yet work) if it contains any of:
- `## Package details` (the planning template header)
- `## Package specification` (the planning template header)
- `status: COMPLETE` (a stub's closing marker, not an actual completion record)

Everything else is **written** (real evidence for a package that landed).

This rule is re-applicable by any reader: `grep -lE "^## Package details$|^## Package specification$|status: COMPLETE" plans/evidence/*.md` lists all stubs.

## Written (real evidence)

| File | Notes |
|---|---|
| `M00-001.md` | Environment record (corrected) |
| `M00-002.md` | Tracked-file scope ledger (corrected v2) |
| `M00-003.md` | Test ledger (corrected) |
| `M00-004a.md` | Partial evidence: engine/state.py public construction inventory |
| `M00-004a5.md` | ... |
| `M00-004a6.md` | ... |
| `M00-004a7.md` | ... |
| `M00-004a8.md` | ... |
| `M00-004a9.md` | ... |
| `M00-004a10.md` | ... |
| `M00-004a11.md` | ... |
| `M00-004a12.md` | ... |
| `M00-004a13.md` | ... |
| `M00-004a14.md` | ... |
| `M00-004a15.md` | ... |
| `M00-004a16.md` | ... |
| `M00-004a17.md` | ... |
| `M00-004a18.md` | ... |
| `M00-004a19.md` | ... |
| `M00-004a20.md` | ... |
| `M00-004a21.md` | ... |
| `M00-004a21a.md` | ... |
| `M00-004a21b.md` | ... |
| `M00-004a21c.md` | ... |
| `M00-004a22.md` | ... |
| `M00-004a23.md` | ... |
| `M00-004a24.md` | ... |
| `M00-004b-B01.md` | ... |
| `M00-004b4.md` | ... |
| `M00-004b5.md` | ... |
| `M00-004b6a.md` | ... |
| `M00-004b6b.md` | ... |
| `M00-004b6c.md` | ... |
| `M00-004b6d.md` | ... |
| `M00-004b7.md` | ... |
| `M00-004b8.md` | ... |
| `M00-004b9a.md` | ... |
| `M00-004b9b.md` | ... |
| `M00-004b9c.md` | ... |
| `M00-004b9d.md` | ... |
| `M00-004b9e.md` | ... |
| `M00-004b10a.md` | ... |
| `M00-004b10b.md` | ... |
| `M00-004b10c.md` | ... |
| `M00-004b10d.md` | ... |
| `M00-012a.md` | ... |
| `M00-012b.md` | ... |
| `M00-012c.md` | ... |
| `M00-012d.md` | ... |
| `M00-012e.md` | ... |
| `M02-003_005_008_M04-003_006_007_STATE_AND_PHASES.md` | State model, views, phases and turn order |
| `M02-009_TO_012_CONTENT_LAYER.md` | Content layer |
| `M03-001_TO_005_CHOICE_MODEL.md` | Choice model |
| `M03-006_RNG_AND_DICE.md` | RNG and dice |
| `M03-008.md` | Typed event model |
| `M03-009.md` | Ability registration |
| `M04-001_002_GALAXY.md` | Galaxy |
| `M04-004_FACTION_SEATING.md` | Faction seating |
| `M04-016_STATUS_TOKEN_GAIN.md` | Status token gain |
| `M04-017_OBJECTIVE_SCORING.md` | Objective scoring |
| `M04-018_AGENDA_VOTING.md` | Agenda voting |
| `M05-001_002_TACTICAL_ACTION.md` | Tactical action |
| `M05-003_MOVEMENT_LEGALITY.md` | Movement legality |
| `M05-004_012-015_FLEET_AND_INVASION.md` | Fleet and invasion |
| `M05-004_TACTICAL_DRIVER.md` | Tactical driver |
| `M05-006_MOVE_APPLICATION.md` | Move application |
| `M05-010.md` | Combat roll effects |
| `M05-016-019_PRODUCTION.md` | Production |
| `M06-001_SPACE_COMBAT.md` | Space combat |
| `M06-016.md` | Generic reactions |
| `OBJECTIVE_TECH_LEADER_RELIC.md` | Objective, tech, leader, relic |
| `PLANET_CONTROL.md` | Planet control |
| `READY_PLANETS_FIELD.md` | Ready planets field |
| `SECONDARY_COST_WAIVER_TESTS.md` | Secondary cost waiver tests |
| `STRATEGY_CARD_ALIGNMENT.md` | Strategy card alignment |
| `STRATEGY_CARD_EFFECTS.md` | Strategy card effects |
| `STRATEGY_PHASE.md` | Strategy phase |
| `STRATEGY_PHASE_FLOW.md` | Strategy phase flow |
| `STRATEGY_SECONDARIES.md` | Strategy secondaries |
| `TACTICAL_PIPELINE.md` | Tactical pipeline |
| `THUNDERS_EDGE_VARIANTS.md` | Thunder's Edge variants |
| `CLOCKWISE_FROM_TEST.md` | Clockwise from test |
| `COMMAND_TOKEN_REFACTOR.md` | Command token refactor |
| `CORE_ENGINE.md` | Core engine |
| `FACTION_COST_WAIVERS.md` | Faction cost waivers |
| `FULL_ROUND_SIM.md` | Full round simulation |

## Placeholder (generated stubs, not yet work)

`M00-004a25.md` through `M00-004e.md` (M00-004 sub-tasks), `M00-005.md` through `M00-005j.md`, `M00-006a.md` through `M00-006i.md`, `M00-007a.md` through `M00-007h.md`, `M00-008a.md` through `M00-008j.md`, `M00-009a.md` through `M00-009j.md`, `M00-009h1.md`, `M00-009h2.md`, `M00-010a.md` through `M00-010f.md`, `M00-011.md` through `M00-011d.md`, `M00-013.md` through `M00-013g.md`, `M00-014a.md` through `M00-014e.md`, `M00-015a.md` through `M00-015d.md`, `M01-001.md` through `M01-013.md`, `M02-001.md` through `M02-016.md`, `M03-001.md` through `M03-007.md`, `M03-010.md` through `M03-016.md`, `M04-001.md` through `M04-007.md`, `M05-001.md` through `M05-009.md`, `M05-011.md` through `M05-024.md`, `M06-001.md` through `M06-015.md`, `M06-017.md` through `M06-020.md`, `M07-001.md` through `M07-011.md`, `M07-014.md` through `M07-018.md`, `M08-001.md` through `M08-017.md`, `M09-001.md` through `M09-018.md`, `M10-001.md` through `M10-030.md`, `M11-001.md` through `M11-022.md`, `M12-001.md` through `M12-023.md`, `M13-001.md` through `M13-016.md`.

## Filename drift from MASTER_PLAN.md

Per the handover, two package IDs in filenames drift from `MASTER_PLAN.md`:

| Filename | Plan slot | Note |
|---|---|---|
| `M05-004_TACTICAL_DRIVER.md` | M05-004 | Plan slot M05-004 is "Fleet composition"; this file covers tactical driver |
| `M06-001_SPACE_COMBAT.md` | M06-001 | Plan slot M06-001 is "Space combat"; this file covers space combat but the ID was reused |

These have not been renamed (per the instruction). The index records the drift so the next reader is not misled.
