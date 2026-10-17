# Current execution state

## Current work — 2026-10-11, correctness review before training

D:/Projects/ti4-engine-rs, wp/base-factions, HEAD a8ed02533f3b2c1cc3bb38f49b280b5ba1b7e12a plus scoped dirty work. No other sessions active per operator. GPU/training continuation STOPPED by operator; no automatic restart. Existing engine/training/UI edits and all artifacts preserved; nothing staged/committed/pushed.

- Historical CPU baseline: 72 games/432 seat-games, exit 0, matching those historical seeds. Old release binary; not current-engine qualification.
- BF-22 bounded independent review findings fixed: census refuses incomplete games, documentation corrected, nonzero separate-critic migration covered. Fresh pinned-vocabulary smoke uses existing read-only inference compatibility; exact checksum retained, strict training loader unchanged. Smoke refusals 2 passed; one-round pinned smoke exit 0; affected integration 15 passed/0 failed/1 ignored. Evidence: evidence/BF-22-REVIEW-2026-10-11.md.
- Workspace before BF-22b fixes: 4061 passed/1 failed/6 ignored, exit 101. Only failure ti4-review semantic_golden frame 7; do not regenerate or silently waive it.
- BF-22b independent gpt-6.1-sol review found incoming-opponent context, Heaven's Eye early repair, repeated Letnev repair on resumed rounds, and omitted Naaz/Nekro combatants. Scoped fixes/regressions in battle.rs, tactical_plan.rs, battle_arena.rs. First four fixes: full policy/training libs 469 passed/0 failed. Later planetary Maximum guard targeted checks 2 passed. Independent gpt-6.1-sol final source re-review passed, including Maximum cargo refusal. Correctly pinned CPU workspace completed: 4066 passed/1 failed/6 ignored, only reviewer semantic golden. First wrong-path compile failure retained. Probe subsequently repaired/checked separately: 3 regressions pass, 576 engine fights error-free, bounded Yin comparison gap 1.94pp with 0/32 |z|>3; backend/timing emission metadata verified. Retained v8 predictor predates corrected labels and is not qualified by old metrics. Evidence: evidence/BF-22B-REVIEW-2026-10-11.md.
- BF-25 sampled pair campaign passed: 153 original-18 pairs, 306 primary games, 153 matching replays, zero failures, fresh release build and at most16 workers. DEFAULT twelve-faction ledgers 147/147 assets, zero missing/blocked/mech/breakthrough/leader gaps. Diagnostic helpers corrected for active printings/source sets, independent Luna review passed. Engine 2813 passed/0 failed/1 ignored; all engine/sim example callers compile. Evidence: evidence/BF-25-PAIR-SOAK-2026-10-11.md.
- BF-24 added real ground_combat casualty, copied Garv/Gunn exploration and Naaz survivor and Saar/Naaz ground-effect sustain-to-Gate fixtures (Luna, root frontier review); full live integration 15 passed/0 failed. Public invasion::resolve assigns normal casualties automatically; Saar/Naaz sustain now also covers real Game component legality/payment/staged delivery; ordinary invasion manual casualties and browser presentation remain open. Evidence: evidence/BF-MANUAL-CHOICE-ACCEPTANCE.md.
- Current workspace completed: 4068 passed/1 failed/6 ignored, exit 101; sole failure reviewer semantic_golden. Snapshot predates the newest Garv/Gunn, Naaz and Deorbit fixtures; the affected live suite subsequently passed15. Exact Rust reviewer candidate retained separately; frozen fixture unchanged. Bounded Hacan intervention restores17 chosen identities after one forced answer is omitted, but does not restore old trajectory. Captured-map18→112 normalization is a pre-existing dirty hunk under review; regeneration still needs diagnosis and operator approval per BF-wave-B. Next: semantic golden disposition and remaining BF-24 subtype fixtures; broader corrected-label calibration and wide ground network remain open. BF-23 authored-bot rebaseline dropped. No long training first.
- Saar operator replay diagnosed2026-10-12: branch0 reconstructed402frames/455recordedchoices without divergence. Free2inf+2fighter covered by Winnu agent and Yssaril copied agent discounts4; actual factory moved into110. Original ends401 before later exploration. Explicit LazulRex CULTURAL continuation draws Freelancers, legally offering1unit without factory and influence payment; temporary flag clears. Misleading carrier-only continue-to-production wording fixed (Luna author/root frontier review), three regressions added. Final engine2816passed/0failed/1ignored; live integration15passed/0failed. No desktop rebuild, training, commits or golden refresh. Evidence: evidence/SAAR-REPLAY-2026-10-12.md.
- Current ownership: coordinator owns review fixes/evidence/Cargo; smaller agents now only Luna/Terra per operator. Two Luna agents own bounded manual-fixture and golden-candidate diagnostics; root owns review and Cargo. Earlier launch/archive entries below are historical.

## Verified continuation — 2026-10-11

**STOPPED by operator.**
Operator requested immediate stop2026-10-11. Owned controller31004 stopped first,then trainer49308; both exits verified. All outputs retained. Latest saved:D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-resume800-20261011\checkpoints\checkpoint-92;M10-034 PPO, 1 update(s) from D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-wide-long1000-20261010\checkpoints\checkpoint-16448, roster Wide. No eval/automatic restart. Next:baseline and correctness reviews before further training.
 Earlier launch notes below are historical.

- Live branch wp/base-factions, HEAD a8ed0253. Operator permits GPU and confirms no other sessions active. Existing engine/training/UI dirty work preserved; no rebuild or source change.
- Card-tag pilot already finished; operator retained tags. Active training line: faction-focus1,GAE lambda.95,wide roster. Stopped at229; latest saved checkpoint16448 contains200updates.
- Launched800update warm start with fresh seeds1264300100, retaining all old runs. Reuse fingerprinted Oct9 binaries (8432ba71 + dirty build), not current HEAD; optimizer/focus history reset. Controller: scripts/continue_focusgae_20261011.ps1. Fresh output out/ppo-focusgae-FG-resume800-20261011.
- Controller preflight/syntax and independent script review pass; launched PID31004 at11:01CEST, CUDA smoke passed exit 0 (parameters moved,Adam advanced); training childPID49308 active; first checkpoint92 published/reloaded identically. Luna paired-VP helper self-tests pass. Check status.json for live phase. After training: paired greedy600seed wide eval final vs T checkpoint3584; original-six regression watch. Exact provenance/checks/limits: evidence/FOCUS-GAE-CONTINUATION-2026-10-11.md. Phase/PID: run status.json after launch. COMPUTE_QUEUE.md holds reservation.
- No completion/qualification claim. BF22reviews,BF24manual coverage,BF25exit remain open. Older Oct6 state below is historical and has not been fully reconciled.


Last verified for documentation maintenance: 2026-10-06. This is a resume point, not a fresh engine/training qualification report.

## Repository and ownership

- Checkout: D:/Projects/ti4-engine-rs; branch wp/base-factions. Inspected HEAD: 573baba32cde9b28e6dd78240b4aec2615405ce6. Main/origin/main were aligned before this archive task. Resolve current HEAD/status on resume; do not use this recorded hash as live state.
- Shared checkout is intentionally dirty. Unrelated engine/income, ML/policy/training, replayer/review/UI, scripts, and tools/pi_rpc_bridge.py changes remain owned by other sessions. Do not stage, format, reset, or fix them as part of documentation work. The pre-existing R02 index entry is preserved in the working tree and excluded from this task's commit.
- Ask peers/operator what is running; this task did not establish live process ownership and did not run Cargo or stop any process.

## Active workstreams and recorded evidence

- **Base and later factions:** [BF plan](BASE_FACTIONS_PLAN_2026-10-02.md), [October 6 Cabal queue](HANDOVER_2026-10-06_CABAL_PI.md), [Cabal evidence](evidence/BF-cabal.md). The latest top-level historical state records all twelve BF factions complete at cb2374fd; current commit history has since added Nomad/Cabal, original-six closure, and Empyrean. These recorded asset results do not close BF20–25 integration: runtime/default roster, versioned policy roster/schema migration, simulator rebaseline, manual UI, paired soak, and exit review remain separately gated.
- **Diplomacy/trade:** [unification](ENGINE_DIPLOMACY_UNIFICATION.md), [trade rework](TRADE_REWORK_2026-09-22.md), [operator bug plan](BUG_PLAN_2026-09-20.md). Historical adapter/journal, leader hooks, browser QA, and review obligations require individual disposition.
- **Policy/training:** [operator guide](PPO_TRAINING_HOWTO.md), [complete decision contract](STAGE2_COMPLETE_DECISION_CONTRACT.md), [OBS-012](OBS-012_DECISION_COMPLETENESS_QUALIFICATION.md), [activation rework](ACTIVATION_REWORK_PLAN_2026-09-17.md). Older pilot outcomes are measurements, not promotion/qualification; live trainers/checkpoints were not inspected in this task.
- **Tools:** [R01](R01_REVIEW_VIEWER.md) and [R02](R02_REPLAYER.md). R02 working-tree/index ownership is separate from this documentation commit.
- **Release:** [M11 bridge](M11_TTS_BRIDGE.md), [M12 qualification](M12_QUALIFICATION.md), [M13 cutover](M13_CUTOVER.md). Obligations and rollback criteria remain open until independently verified.

## Checks, limits, and review debt

- No Rust builds, tests, soaks, training, simulation rebaseline, or browser QA were rerun for archival. Consult package evidence for command/build provenance rather than treating historical counts as checks of this shared dirty tree.
- Historical full-workspace limitations include ti4-review semantic_golden (recorded operator exception) and ti4-mlp smoke_refusals failing before the pool check because local vocabulary generation is invalid. Do not silently convert these to fresh passes or general waivers.
- Preserve outstanding Tier-C/D review debt from BF shared timing/legality/hidden-information seams and M09/M10/OBS records unless later evidence explicitly resolves it. Historical diagnostic notes retain five event-share deviations and a pending attribution/rebaseline decision; archival makes no acceptance claim about them.
- The [archive manifest's active follow-up register](ARCHIVE_MANIFEST_2026-10-06.md#surviving-obligations-active-follow-up-register) retains residual fleet/capacity, exploration, leader, diplomacy, and historical simplification findings. Fleet-supply enforcement and Analytical/Rin review files remain active.
- October handovers remain available for source/evidence details but may contain older authority/status. Current AGENTS.md and later explicit operator decisions govern; verify any contradictory historical completion, branch, model, or process note before acting.

## Documentation task and next action

Adopted AGENTS.md and archived only the obsolete/superseded records enumerated in the [manifest](ARCHIVE_MANIFEST_2026-10-06.md). Moves use plain git mv, with no cleanup, artifact overwrite, or new archive subdirectories. Link repair and independent Tier-A diff review passed before the documentation commit; exact checks are recorded in the manifest.

Next: resume the relevant workstream only after checking current Git state, ownership, evidence, and the open gates above. Verify main/origin publication from Git before assuming this archive migration is published; this file does not claim an unverified push.

The [verbatim historical execution state](archive/EXECUTION_STATE_HISTORY_2026-10-06.txt) preserves all previous entries, including superseded/conflicting notes. It is plain text so original Markdown references remain literal historical paths. Current navigation is here, in [INDEX.md](INDEX.md), and in the manifest; dated history is not a live resume point.
