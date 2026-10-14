# Draft: agent policy refresh and plan archival

Date: 2026-10-06. Status: **proposal for Claude review; not adopted**.

This draft proposes changes to AGENTS.md, related Pi policies, and planning structure. It does not change those instructions, close any review finding, move any plan, or authorize a training run. The user reports that Qwen Flash Next has replaced Qwen 3.8 27B in Pi; no capability comparison or installed provider/model identifier was verified during this review.

## Review request for Claude

Review this proposal against the current branch, source, tests, evidence, and operator decisions. Report concrete corrections and unresolved decisions. Distinguish implemented behavior from reviewed/qualified behavior and stale documentation from genuinely unfinished work. Do not execute the migration or archive files solely because this proposal exists.

In particular, check:

1. Whether the proposed general instructions retain every important correctness and operational boundary.
2. Which Pi limits are still useful with the current model and which should become task-specific.
3. Whether there is newer closure evidence for the outstanding findings listed below.
4. Which historical documents are still normative dependencies of tests, scripts, or active plans.
5. Whether the current activation/diplomacy work is represented accurately before replacing the resume state.

## Findings motivating the change

- AGENTS.md and PI_WORK_PACKAGE_STANDARD.md hard-code Qwen 3.8 27B and Pi v0.84.2. Historical evidence naming older models should remain unchanged.
- The mandatory reading order includes an approximately 650 KB EXECUTION_STATE.md containing about 8,900 lines of history and conflicting old status. HANDOVER_COMPACT.md names an older branch/HEAD; the execution-state tail stops at September 17 while the inspected latest commits are dated September 18. These are repository observations, not claims about current running processes.
- The strict M00-to-M13 sequence does not describe the later diplomacy, arena, and activation workstreams. Original qualification and release obligations may nevertheless remain open.
- PI_WORK_PACKAGE_STANDARD.md prescribes one to five files, 200–500 net lines, and four to twelve tests, and discourages cross-crate packages. These are poor proxies for coherence and risk.
- AGENTS.md requires review before committing, while PI_WORK_PACKAGE_STANDARD.md commits before review. Local checkpoint commits and integration acceptance need separate definitions.
- Managed Pi defaults are 600 seconds total, 120 seconds to first edit, and one allowed tool error. A read-only investigation cannot satisfy the edit requirement. An abort does not establish that the task is too large.
- tools/pi_rpc_bridge.py detects edits by comparing Git status text and HEAD. Further edits to a file already dirty at task start can leave both unchanged. Status text is also truncated to 2,000 characters. This can produce false no-edit aborts; relaxing prose alone will not fix the implementation.
- Workspace boundaries still name D:\Projects\ti4-engine-rs, while the reviewed checkout is elsewhere. Portable repository boundaries must remain as strict as the current permission intent.

## Proposed division of responsibility

| File | Responsibility |
|---|---|
| AGENTS.md | Short, model-independent project rules, startup procedure, verification, review, and safety |
| plans/SCOPED_PERMISSIONS.md | Detailed authority and external-state boundaries, expressed using the resolved checkout root |
| plans/PI_WORK_PACKAGE_STANDARD.md | Pi task specification, model configuration, review routing, and execution guidance |
| plans/PI_RPC_CONTROL.md | Managed-controller ownership, endpoints, watchdog semantics, and supervision |
| plans/EXECUTION_STATE.md | Short current resume state, with explicit evidence and last-verified provenance |
| plans/INDEX.md | Active workstreams plus links to blocked work and historical/archive indexes |
| plans/archive/ | Historical plans, handoffs, and execution logs; archival does not imply completion |

Target approximately 100–150 lines for AGENTS.md and a few pages for current state. These are editing goals, not enforced limits. Keep reusable architecture/contracts under docs where appropriate; do not archive a live specification merely because its implementation is old.

## Proposed AGENTS.md text for review

The following is a candidate replacement, subject to reconciliation with existing obligations.

```markdown
# Repository agent instructions

## Objective and sources

Develop and maintain the TI4 Rust engine and its policy, training, review, and bridge tools according to the current operator request and accepted project contracts.

Official rules, accepted Rust specifications, versioned API/artifact contracts, and focused tests govern behavior. Python behavioral parity is not an acceptance criterion unless the current task explicitly requests it. The historical Python repository remains read-only; inspect pinned tracked content without creating caches or outputs there.

## Start or resume

Read this file, plans/SCOPED_PERMISSIONS.md, the current section of plans/EXECUTION_STATE.md, and the active task's plan and evidence. Inspect Git status, branch, HEAD, and recent commits. Read architecture notes and older evidence when relevant to the task or its dependencies.

Verify stale handovers against repository state. Historical process IDs and run descriptions are not live status. Record uncertainty instead of guessing.

Pi-specific policies apply when using Pi. Managed-controller rules apply only to controller-owned sessions.

## Work scope

Execute dependency-ready work within the operator's authorized scope. Preserve unresolved milestone, qualification, and release gates; a later workstream does not silently waive them.

Use one coherent, independently reviewable change at a time. Split unrelated behavior, unsafe overlapping edits, or changes too large to verify together. Cross-crate changes are allowed when required for an end-to-end behavior or contract migration.

Read relevant producers, consumers, callers, and tests before editing. Add meaningful regression tests for behavior changes and verify driven-game reachability where isolated handlers could otherwise pass unused. Documentation-only changes need accurate content and link checks rather than arbitrary test counts.

Do not expand scope, invent game data, or silently defer acceptance criteria. Record limitations and follow-up work explicitly.

## Correctness

- Generate legal choices; failed transitions must be atomic.
- Preserve deterministic ordering, RNG/replay contracts, stable choice IDs, and canonical projections.
- Enforce hidden-information boundaries through typed observations and APIs.
- Use exact arithmetic for rules legality; specify tolerances for training mathematics.
- Validate artifact schemas, bounds, references, and checksums before mutation; publish checkpoints atomically.
- Fail visibly on parser/model/bridge/worker errors and incomplete games.
- Never weaken tests, regenerate goldens without semantic justification, or fabricate completion, review, or performance evidence.

## Verification and review

Run focused checks first, then affected-crate checks and formatting/lints appropriate to the diff. Run workspace and special campaigns when shared contracts, milestone exits, or release criteria require them. Diagnose flaky failures instead of rerunning until green.

Match independent review to risk. Timing, legality, payments, hidden information, schema migrations, training mathematics, bridge security, unsafe code, release gates, and material performance claims retain their specified review requirements. Do not waive existing Tier-C/D obligations through this policy refresh. Routine documentation corrections may use author verification unless they alter policy, permissions, or accepted contracts.

A local checkpoint commit may precede independent review if clearly marked unreviewed. Integration or completion requires the relevant tests, evidence, and resolved required reviews. Stage only task-owned paths. Never describe self-review as independent review.

For performance claims, use the applicable benchmark protocol, comparable workloads and environments, raw measurements, variance, and passing semantic gates. Document limits on comparability.

## Progress, failure, and handover

Keep current state concise: branch/HEAD, task, dirty paths and ownership, checks actually run, decisions, open findings, blockers, and next action. Put detailed commands/results in task evidence and historical narrative in dated logs.

Persist state before compaction or handoff; verify it on resume. Compact according to actual context pressure and task boundaries, without a fixed package count or forced session termination when the harness can continue safely.

Diagnose failures before retrying. Escalate for repeated lack of progress, unexplained invariant failures, nondeterminism, or architectural uncertainty. Distinguish test failures from environment/tool failures. Do not automatically shrink a correct task because a watchdog or infrastructure failure interrupted it.

## Safety and authority

Treat the resolved authorized checkout as the repository write boundary. Follow plans/SCOPED_PERMISSIONS.md; delegation does not broaden authority. Other worktrees and repositories require their own authorized scopes.

Preserve unrelated changes; avoid destructive reset/checkout/clean and shared-history rewriting. Resolve and verify exact paths before bulk movement or deletion. Retain cited raw evidence, artifact provenance, secrets boundaries, and ownership checks for processes.

Proceed on routine reversible decisions within the authorized task. Obtain explicit authority where scoped permissions require it for remote changes, deployment, live TTS control, private production data, destructive data handling, or changes to accepted scope/security/licensing. Existing operator authorization remains effective for its stated scope.

Only one controller may own a managed Pi session. Use that controller for prompts, steering, status, abort, and compaction; do not launch a second owner against the same session. Follow plans/PI_RPC_CONTROL.md for controller tasks and bounded supervision.
```

## Companion Pi policy changes

1. Put the default implementer in one configuration location: Qwen Flash Next through Pi, using the verified installed provider/model ID. Record actual model and Pi versions in new evidence. Do not rewrite historical evidence or assume a larger context budget without checking it.
2. Keep task prompts concrete: objective, dependency context, expected edit scope, relevant invariants, acceptance checks, resource bounds, and deliverables. Use the full permission declaration only for actions needing it; avoid repeating a large boilerplate for routine P0/P1 tasks.
3. Treat package-size numbers as examples, not requirements. Remove mandatory test counts and the blanket split at crate boundaries.
4. Keep one bounded task per managed RPC submission as a controller accounting unit. The supervisor can submit the next authorized task; this should not force all agents/harnesses to stop after one package.
5. Add a task kind such as investigation/review/implementation. Disable first-edit enforcement for read-only tasks. For implementation, use verified progress signals and task-specific deadlines. Retain cancellation, output locations, and resource bounds.
6. Fix edit detection using task-scoped content fingerprints or trustworthy successful-edit events, including already-dirty paths. Unrelated concurrent edits must not count as this task's progress. Add focused tests for a read-only task, an already-dirty file edit, and an unrelated writer before changing watchdog behavior.
7. Track infrastructure errors separately from repeated agent tool misuse. Recoverable errors should allow correction; hard time/cost budgets and unrecoverable failures still stop the task. Claude should propose actual defaults after checking typical investigation/build times; this draft does not invent replacement thresholds.
8. Preserve loopback binding, bearer-token secrecy, controller ownership, compact checkpoint summaries, and restrictions on ingesting full streaming transcripts. Low-token supervision remains useful with the replacement model.

## Archive shortlist and conditions

These classifications are based on inspected documents. They are not fresh execution of their acceptance tests or proof that every later regression is absent. Check inbound references across source, tests, scripts, docs, plans, and evidence before moving anything.

### Strong first-batch candidates

| File under plans/ | Evidence for historical classification |
|---|---|
| BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT.md | Explicit FIXED 2026-08-31; acceptance/regression coverage recorded |
| BUG_2026-08-29_PROMISSORY_NOTE_TRANSACTION_OFFERS.md | Explicit FIXED 2026-08-31; enumeration and scoring corrections recorded |
| BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md | Explicit FIXED 2026-09-04 with driven-game verification and linked evidence |

Preserve their evidence, including references to separately unresolved invocation/feature work. Suggested destination: plans/archive/bugs/.

### Candidates after current obligations are reconciled

| Group | Required preparation |
|---|---|
| HANDOFF_ENGINE_COMPLETION.md and PI_HANDOFF_ENGINE_REMAINDER.md | Explicit supersession/closure exists. Transfer or verify resolution of combat-capacity ordering gaps, simplifications, and audit work in the latter's remaining-items sections. |
| Dated HANDOVER* files, CLAUDE_HANDOVER_2026-09-16.md, DIPLOMACY_HANDOVER_2026-09-15.md, existing HANDOVER_COMPACT.md | Create a verified current resume point; migrate still-open obligations; retain history and provenance. |
| BATTLE_ARENA_PLAN_2026-09-16.md, BATTLE_ARENA_DESIGN_2026-09-16.md, BATTLE_ARENA_AGENT_HANDOFF_2026-09-16.md | Consolidate current arena input/coverage/integration contracts, user overrides, remaining action-card work, and unresolved advisory findings. Older implementation-status statements are stale. |
| CONTINUATION_PLAN.md and older algorithm/overnight-arena plans/results | Mark historical or superseded. Carry forward applicable findings; explicitly retire the old Python-parity objective rather than silently executing it. Classify individual experiments as completed, abandoned, or unresolved. |
| Older sections of EXECUTION_STATE.md | Preserve verbatim in dated history where feasible. Build current state from verified repository facts, not by selecting the last historical paragraph. |

Suggested destinations: plans/archive/handoffs/, plans/archive/designs/, plans/archive/experiments/, and plans/archive/execution/. Moving a document to history does not resolve its findings.

### Keep active until closure or explicit backlog transfer

- BUG_2026-08-29_LEAD_FLEET_SUPPLY.md: the recorded general enforcement gap is only partially addressed.
- LEADER-FIX-2026-09-13.md and the leader invocation bug: LEADER_FIX_001 evidence still records limitations and pending independent review. Check later evidence before deciding which earlier report is superseded.
- BUG-001_ANALYTICAL_RIN_UPGRADE_EXCLUSION.md: implementation evidence explicitly records outstanding independent frontier legality review.
- STAGE2_COMPLETE_DECISION_CONTRACT.md and OBS qualification: OBS-012 records incomplete counterfactual verification, unrun learning/performance ablations, and outstanding review in its evidence.
- Diplomacy follow-through: lossless journal ownership, leader favour hooks, reviewer browser QA, and qualification need individually verified disposition; do not treat all September 16 TODO items as current without checking later changes.
- Current activation plans/evidence and advisory findings: reconcile the September 18 arm-B continuations and subsequent source changes. A pilot result is not full qualification.
- M09/M10 review ledgers: inspected M09-030 Tier-D pass-1 and M10-034 PPO-driver records retain open findings. Search later resolutions before archival; neither stale wording nor code existence establishes closure.
- M11–M13 and the master plan: retain unreconciled bridge, qualification, release, and rollback obligations. Do not archive all migration milestones as completed.
- Operational guides, accepted schemas/contracts, KNOWN_DIFFERENCES.md, CHAMPIONS.md, and the rules audit remain discoverable. Historical sections may be separated without losing their active specifications or provenance.

## Safe implementation sequence after review

1. Reconcile operator decisions and actual branch/HEAD, dirty-path ownership, active workstreams, qualification gaps, and review debt. Do not infer live trainers from old PIDs.
2. Establish the new index and short current state, preserving the old execution history and evidence. Label last-verified timestamps and unknowns.
3. Update AGENTS.md, SCOPED_PERMISSIONS.md, and both Pi guides together so they agree on authority, reviews, commit timing, model configuration, and task scope. Runtime watchdog changes are a separate tested code change.
4. Create an archive manifest listing old path, new path, disposition, successor, and every surviving obligation with its active destination. Resolve and verify movement targets inside the checkout; use exact paths.
5. Move only the reviewed first batch, preserving contents. Repair links and internal relative paths; leave discoverable successor/archive pointers where required. Keep detailed evidence records available.
6. Check Markdown links, references from scripts/tests, archive manifest completeness, and the scoped Git diff. No source test suite is required for prose-only edits unless a build/test consumes those files. Runtime controller changes require their focused behavioral tests.

## Decisions for Claude to return

- Proposed corrected AGENTS.md text and any missing invariants.
- Verified provider/model identifier and where it should be configured.
- Whether any existing independent-review requirements can change, with a risk rationale; retain open Tier-C/D requirements unless explicitly resolved.
- Exact initial archive manifest and successor/backlog links, including items that should remain active.
- Current state disagreements and facts that cannot be established from this checkout.
- A separate watchdog patch plan with acceptance tests and measured/default-budget rationale.

## Review provenance

Inspected AGENTS.md, SCOPED_PERMISSIONS.md, MASTER_PLAN.md, INDEX.md, PI_WORK_PACKAGE_STANDARD.md, PI_RPC_CONTROL.md, selected execution-state sections, handovers, bug plans/evidence, arena/activation plans/evidence, and relevant review records. Read source excerpts for bridge watchdog/edit detection and production/transaction behavior. This was a document/source review, not an exhaustive code audit or a rerun of engine, training, or qualification suites.

At inspection, unrelated modifications existed in crates/ti4-engine/src/leaders.rs and scripts/activation_armB.psd1, with untracked plans/evidence/BF-SSRUU-L1Z1X-TIMING.md. This proposal does not own or change those paths.
