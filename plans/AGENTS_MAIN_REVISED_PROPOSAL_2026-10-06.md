# Revised AGENTS.md proposal from current main

Status: **draft for Claude review; not adopted**. Date: 2026-10-06.

Source: working-tree AGENTS.md inspected in D:\Projects\ti4-engine-rs, branch wp/base-factions, HEAD 573baba32cde9b28e6dd78240b4aec2615405ce6. The source includes the uncommitted October 4 testing-discipline addition. The earlier proposal was based on the September 18 diplomacy worktree and is not the baseline for this revision. This file is saved next to that proposal for review; only this proposal is edited. Main instructions are not adopted or changed, and no commits, moves, deletions, or directory creation are performed.

Revision: incorporates the six corrections and minor rendering/reviewer corrections in the operator's attached feedback. The operator explicitly approved **one plans/archive/ folder** for later archival implementation on 2026-10-06. No additional subdirectories are approved. The folder will not be created during this draft revision, and individual moves still require a reviewed manifest and authorized implementation.

Current-main facts checked during this revision:

- AGENTS.md already states there is no Pi/Qwen default implementer, by operator decision 2026-10-02. PI_WORK_PACKAGE_STANDARD.md still contains the obsolete default.
- EXECUTION_STATE.md is 788,243 bytes and 10,294 lines; its tail contains October BF-NAAZ work. It is bloated, not stale in the manner claimed for the older checkout.
- HANDOVER_2026-10-06_CABAL_PI.md explicitly describes Pi package queues with a coordinator who reviews, commits, and merges. Pi never commits. It calls the backend Qwen 3.8 Flash, but uses a placeholder provider/model ID. No installed model identifier has been verified here; neither that display name nor Qwen Flash Next establishes one.
- Current tools/pi_rpc_bridge.py still compares Git status text and HEAD for first-edit detection; status text is truncated to 2,000 characters. Edits to already-dirty files can remain invisible, and unrelated edits can count as progress. This is verified by source inspection, not a runtime reproduction.
- Its defaults remain 600 seconds total and 120 seconds to first edit. Cabal packages already specify raised per-task budgets where needed. This draft does not replace those defaults.
- At the main inspection, planning directories included artifacts, evidence, and reviews; no archive directory existed. The operator has now approved creating one plans/archive/ folder during later archival implementation. This approval resolves the destination choice, not the archive shortlist or adoption.

The Sonnet-only/two-subagent cap and the folder/data-preservation rules below come from the operator's corrections in this conversation. Normal edits to tracked files are allowed. The protection against deletion/overwrite concerns retained data not created as throwaway, especially gitignored artifacts without Git recovery.

## Candidate replacement AGENTS.md

Review the following as the proposed file contents. Testing discipline is retained verbatim from current main, apart from its placement. Git/filesystem safety retains the existing protections, with explicit operator restrictions and coordinator ownership. Review tiers retain concrete mappings while removing the requirement to use an obsolete Qwen reviewer.

````markdown
# Repository agent instructions

## Mission and sources

Maintain the TI4 Rust engine and its policy, training, review, and bridge tools according to the current operator request, accepted contracts, and active workstreams in plans/INDEX.md. Progress independently without trading correctness, determinism, security, or evidence for apparent speed.

Python behavioral parity is not an acceptance criterion (project decision 2026-08-21). The historical reference is D:\Projects\ti4-engine, branch codex/fully-learned-policy, commit 37061c5. Treat it as read-only: never edit, format, stage, commit, clean, reset, generate caches in, or run a command that writes artifacts into it. Inspect pinned tracked content with non-mutating Git commands. A dirty reference tree or unavailable Python runtime does not block Rust work. Official rules, accepted Rust specifications, and package tests govern behavior; claim Python parity only when explicitly tested.

## Start and resume

Read this file, plans/SCOPED_PERMISSIONS.md, plans/INDEX.md, the short current plans/EXECUTION_STATE.md, the active workstream plan/task and evidence, then git status --short --branch and the last five commits. Read plans/MASTER_PLAN.md, architecture notes, and historical evidence when relevant to the task or an unresolved gate. Until execution state is split, read its current position and latest relevant entries; do not mistake historical entries for current status.

Verify handovers against repository state after compaction. Do not rely on memory where durable state answers the question. Historical PIDs and run descriptions are not live process status. If state disagrees, investigate before editing.

## Workstreams, ownership, and coordination

Execute dependency-ready packages in the active operator-authorized workstreams. The original M00–M13 order is historical planning context; unresolved M11–M13 bridge, qualification, cutover, and rollback obligations remain gates for their own outcomes. Later work does not waive them.

There is no repository-wide Pi/Qwen default implementer (operator decision 2026-10-02). The coordinator follows the current task's authorized implementation route. For Claude subagents, use Sonnet only, at most two at a time, with disjoint edit scopes. Do not start overlapping writers. Follow explicit ownership of intentionally dirty files/hunks; unrelated work is not yours to repair or format.

Several sessions may use the same working tree. Do not switch the shared checkout's branch while another session may be working in it. Ask peers what they are running instead of guessing from timestamps; use authorized coordination channels or ask the operator to establish ownership when direct peer contact is unavailable.

For operator-authorized Pi queues, Pi implements one specified package and reports; Pi never stages, commits, merges, switches branches, or begins the next package on its own. The coordinator reviews the diff and evidence, verifies checks, resolves findings, then stages exact scoped changes and commits. Task-specific ownership and command restrictions remain binding. Follow plans/PI_WORK_PACKAGE_STANDARD.md and plans/PI_RPC_CONTROL.md when using Pi; every Pi task prompt must explicitly name plans/PI_RPC_CONTROL.md for required reading. Managed-controller rules do not govern unrelated harnesses.

Use one coherent, reviewable behavior change, with explicit dependencies, writable paths, permissions, acceptance criteria, resource bounds, and evidence. Remove arbitrary file/line/test counts; cross-crate work is allowed when needed for a complete behavior. Inspect relevant callers and consumers and test driven-game reachability, not only isolated handlers. Split unrelated or unsafe overlapping work. Do not silently shrink scope; record deferred, excluded, or intentionally changed behavior in the scope ledger and evidence.

## Accuracy rules

- Legal actions are generated, not accepted by late rejection. Invalid or failed transitions are atomic.
- Determinism must not depend on hash-map iteration, scheduling, filesystem order, locale, or wall-clock time.
- Rules legality uses exact arithmetic. Floating-point tolerances are restricted to policy/training math and specified by tests.
- Hidden information is enforced through typed views and API boundaries, not convention.
- Preserve stable choice IDs, canonical projections, and implemented/partial/unimplemented registries until an explicit project decision changes scope.
- Never claim parity from aggregate outcomes alone; use decision-boundary differential evidence.
- Never claim a speedup without the applicable M00 protocol, the same machine/workload, raw measurements, variance, and passing semantic gates.
- Never turn parser errors, bridge refusals, worker crashes, or incomplete games into apparent success.
- Validate schema version, size limits, references, and checksums before mutating state. Keep checkpoint writes atomic and recoverable.

## Testing discipline

### Multi-minute CPU-bound commands (operator rule, 2026-10-04)

CPU-bound commands estimated to take several minutes (roughly two minutes or more) MUST use
reasonable bounded parallelism when their work can run independently. On this workstation the
operator authorizes all 16 physical cores (2026-10-04 hour-run update); use up to 16 workers
for independent CPU work while preserving memory headroom and other active workloads. Record the selected worker count and any reason for a lower
count in package evidence. Do not silently run a parallelizable long campaign serially.

Set parallelism at the layer that does the work: Cargo `-j8` bounds compilation, Rust test
`-- --test-threads=8` bounds independent tests, and a single long simulation/seed loop requires
bounded per-seed/per-case workers or deterministic shards. Cargo build jobs alone do not parallelize
that loop. Preserve every original case and deterministic per-case seed/results; collect results
in canonical order and keep separate mutable game/RNG state per worker. Do not weaken coverage,
share unsafe mutable state, or change benchmark protocols to claim a speedup. When parallelizing
existing test/campaign code requires edits outside the active package, record a bounded follow-up
and identify the bottleneck; do not make unrelated edits to force concurrency.

Only one coordinator may run Cargo against the shared target directory. Prefer one invocation
covering independent requested packages instead of competing Cargo processes. If work must remain
serial because of dependencies, shared resources, reproducibility, or memory constraints, state the
concrete reason before the multi-minute run and record it. A long single-core tail should trigger
inspection of CPU activity and the remaining case, not repeated blind reruns.

Long commands must expose useful progress and preserve full logs and the real command exit status.
Do not rely on `cargo ... | grep ... | head ...` as verification: it hides progress, truncates
failures, can close the producer's pipe, and without pipefail can report the filter's success in
place of Cargo's failure. In Bash use `set -o pipefail` and `tee` to an existing ignored output
area; retain the producer status and report it. In PowerShell capture `$LASTEXITCODE` immediately
after the native command and propagate failure. Timeouts/incomplete runs are failures to complete,
not passing evidence. Monitor long jobs without launching duplicate builds or interrupting another
session's owned process.

During a package, run the narrowest useful tests first, then the affected crate. Before merging a
milestone, run the entire workspace suite and every milestone-specific gate. Do not repeatedly rerun
a flaky test until it passes; diagnose and remove the nondeterminism.

When a package explicitly claims Python compatibility, a source Python test is not considered covered
merely because a similarly named Rust test exists. Otherwise, Rust acceptance tests trace to the
package's named rules/specification rather than to Python test names.

Do not update golden fixtures simply to make a failure disappear. Regenerate a fixture only through
its versioned, package-approved process, inspect the semantic diff, and record why it is correct.

## Review and completion

The implementer must not be the sole reviewer. Use this mapping; task-specific requirements may be stronger:

| Tier | Areas | Required review |
|---|---|---|
| A | Documentation, repetitive content fixtures | Independent coordinator/reviewer pass, not the author |
| B | Ordinary model/rule/policy code | Independent review plus milestone/workstream integration tests |
| C | Timing, legality, payments, hidden information, schema migration, training mathematics, bridge security | Frontier-model review |
| D | Unsafe code, cutover, claimed performance gate | Two independent frontier passes |

Do not describe an independent review as complete unless a different reviewing agent or model actually performed it.

Architecture decisions, security boundaries, milestone exit reviews, repeated invariant failures, nondeterminism, and material performance claims also require frontier involvement. Retain existing outstanding Tier-C/D obligations; this refresh does not resolve them. Record reviewer identity, findings, fixes, rejected suggestions with rationale, and exact rerun results. Reviewers must preserve the review trail when changing critical code.

The coordinator reviews and verifies before committing task-owned changes. A commit is a recorded code state, not proof of independent qualification: when an operator-authorized checkpoint retains review debt, label it explicitly and keep the package open. Do not mark completion until specified tests, evidence, and required reviews are resolved; no TODO may stand in for accepted scope.

At a milestone/workstream exit, close rows or record approved exceptions, run full workspace and specific gates, reconcile scope/test/artifact/difference ledgers, verify the historical reference remained untouched if accessed, resolve required frontier reviews, and publish the report/current state.

Diagnose failures before retrying. Distinguish infrastructure/tool failures from invariant failures. Repeated failure of the same invariant needs fresh independent diagnosis; a third failure, architecture conflict, nondeterminism, or unexplained normative mismatch requires frontier diagnosis before further implementation. If blocked, move only to dependency-ready non-overlapping work that cannot hide or compound the blocker; otherwise checkpoint and stop. Never fabricate authority.

## Current state and handover

Keep plans/EXECUTION_STATE.md short: current workstreams, package ownership, branch/HEAD, last-verified facts, checks, decisions, open reviews, blockers, dirty paths, and next action. Put exact commands/results in package evidence. Preserve older state verbatim in a dated file within plans/archive/; do not append unlimited history to the resume point.

Checkpoint before compaction, handoff, or a workstream exit. Finish or safely stop commands first; do not compact during an unknown mutation. Update state/evidence and record Git status and dirty-path ownership. Use the current harness's supported compaction mechanism when needed, then verify state on resume.

Handover format:

```text
Objective and normative sources:
Active workstream/package and acceptance status:
Branch and HEAD; dirty paths and owners:
Checks actually run and exact results; evidence:
Decisions, review debt, blockers, and limitations:
Next exact action/command; files to read first:
```

## Git and filesystem safety

- Work only inside the operator-authorized repository checkout, except for read-only historical-reference inspection. Resolving the checkout root is not permission to write to another checkout.
- Follow plans/SCOPED_PERMISSIONS.md; delegation never broadens those permissions.
- Preserve unrelated changes. Never use destructive reset or checkout commands.
- Follow coordinator branch/commit ownership and task-specific branch rules; do not automatically create or switch a branch per package. Commit only scoped changes after review and checks.
- Do not commit build products, large training outputs, private captures, or copied artifacts without the repository's artifact policy and a checksum manifest.
- Do not rewrite shared branch history.
- No folders outside the repository. Ask before creating one inside it. Do not run git worktree add outside the repository; obey stricter task-specific bans on worktree creation.
- Never delete or overwrite anything you did not create as throwaway, especially gitignored data (out/, checkpoints, runs, pools, libtorch), which has no second copy. Normal edits to tracked files are fine. Append new run/checkpoint directories; never write over existing ones.
- Do not use rm -rf / Remove-Item -Recurse -Force, git clean (especially -x), git checkout -f, git reset --hard, git worktree remove/prune, or cargo clean. Worktree/junction cleanup can follow links and destroy retained out/ data. There is no archival cleanup step.
- Do merges with refs only where possible: merge-tree, commit-tree, update-ref. The coordinator reviews inputs/results and changes only authorized refs. Never update-ref a branch checked out in the shared tree: the unchanged working tree would silently show the merge reversed as uncommitted changes. If a merge has conflicts, stop and ask rather than resolving them in the shared checkout.
- scripts/stage2_confirm.ps1 deletes out\confirm as a side effect. Do not run it against retained data; a separate reviewed fix must remove the destructive behavior before that use.
- Before any bulk movement, resolve and verify exact absolute targets are inside this repository. Archive only through plain git mv operations listed in a reviewed manifest, to non-existing filenames in existing approved directories; never overwrite a destination.
- Keep secrets, access tokens, machine-specific paths, and personal TTS data out of Git.

## Autonomous decision policy

Proceed without asking for routine implementation choices that follow from accepted plans, rules, tests, or architecture. Prefer the smallest reversible decision and record it. Stop and request authority for choices materially changing public behavior, accepted compatibility, security, licensing, deployment, external systems, or destructive data handling when not already authorized. Existing scoped operator authorization persists. Archive proposals and review recommendations do not themselves authorize adoption.
````

## Companion changes required before adoption

1. **Pi package standard:** remove the obsolete universal model default and numeric package/test quotas. Preserve the concrete tier mapping above. Document the current queue: Pi implements and reports; coordinator reviews, verifies, then commits/merges. Pi never commits. Record actual backend/version per run after verifying the installed provider/model ID. The main October 2 no-default decision and the October 6 explicit Pi route can coexist: one is general policy, the other task-specific authorization.
2. **RPC guide:** move controller ownership, endpoints, low-token supervision, task limits, one-package submission, steering/abort/compaction, and run-specific setup out of AGENTS.md. Preserve single-session ownership, loopback/token safety, and explicit cancellation/resource limits. Every Pi task prompt must explicitly name plans/PI_RPC_CONTROL.md for required reading; do not rely solely on Pi following the AGENTS.md link. The existing testing rule already requires a concrete reason for lower parallelism; no new Cabal-specific exception is needed. Shared Cargo ownership still applies to Pi queues and subagents.
3. **Watchdog code:** separate reviewed code change. Add a task kind; exempt read-only tasks from first-edit enforcement; use task-scoped content fingerprints, including already-dirty files and expected created paths. Test unchanged status after an owned edit, unrelated-writer changes, read-only completion, budget expiry, and failed/incomplete-task reporting. Do not claim runtime verification from this source inspection or change defaults in a prose-only update.
4. **State split:** preserve the existing state file verbatim through a manifest-listed plain git mv to a dated filename in the chosen approved archive location. Confirm with git diff -M --stat; no additional hashing step is required. Then create a short current-state file at the vacated original path, based on current main evidence, October queues, owned dirty hunks, outstanding reviews, and unknown live-process status. Check the staged diff and rename record before committing. No deletion or overwrite of retained data is needed.
5. **Permissions:** reconcile older permission text allowing cleanup/deletion with the clarified preservation rule; normalize checkout boundaries without granting sibling/worktree writes. Normal edits to tracked instruction files are recoverable and allowed within authorized adoption work. Retained and gitignored artifacts must not be deleted or overwritten; write new runs/checkpoints to new approved destinations. This draft remains unadopted.

## Archive preparation: one approved archive folder

The earlier shortlist remains a set of candidates, not current-main closure evidence. Recheck each document and inbound references against current main. Keep unresolved obligations in active state/backlog before reclassifying a document as historical. Review debt in the October BF wave is active even where source changes are committed.

Suggested manifest rows should contain exact source, exact destination, disposition, successor, unresolved-obligation destination, and reference repairs. Use the operator-approved plans/archive/ directory, with dated filenames and no additional subdirectories. Confirm the source is tracked and the destination does not exist before listing the exact git mv command. Archive moves are plain manifest-listed git mv operations, with no cleanup step. Folder approval does not authorize the individual moves or adoption.

- First-batch candidates to recheck: the fixed combined-production-payment, promissory-note-offer, and production-discount bug reports.
- Conditional candidates: superseded engine handoffs, dated resume snapshots, older arena plan/design/handoff files, Python-parity continuation planning, completed/retired experiment plans, and historical execution-state text.
- Keep discoverable and active until explicitly resolved or transferred: fleet-supply enforcement; leader invocation/correction review debt; Analytical/Rin independent review; OBS decision completeness and qualification; diplomacy follow-through; current activation and BF package queues/evidence; M09/M10 unresolved reviews; M11–M13 release obligations; accepted schemas/contracts, operational guides, champion provenance, known differences, and rules audit.

Do not move entire plans/evidence or infer completion from dates. Check links from source, tests, scripts, docs, plans, and evidence. Repair relative links after approved moves. If archival would require deleting a file or overwriting a destination, stop and revise the manifest. There is no cleanup step.

## Claude acceptance checklist

- Confirm this candidate was compared with current working-tree main, including its uncommitted AGENTS.md addition, rather than only HEAD or the diplomacy checkout.
- Verify testing discipline is preserved and all operator restrictions remain explicit; tighten wording rather than trading safety for a line count.
- Confirm coordinator review/commit ownership, Sonnet-only/two-subagent limits, task-specific Pi queues, and Tier-C/D obligations agree across companion documents.
- Check the handover template and short-state proposal against the active October queues and intentionally dirty shared checkout.
- Verify the backend identifier before recording a model choice; preserve historical model names in evidence.
- Produce the exact proposed archive manifest only after current-main closure and inbound-reference checks. Do not execute moves or adopt instructions merely because this draft exists.
- Confirm the clarified preservation rule permits normal tracked-file edits while forbidding destruction/overwrite of retained artifacts. Check the named dangerous commands, junction risk, refs-only integration, shared-checkout ownership rule, and stage2_confirm.ps1 side effect.
- Preserve the recorded operator approval for one plans/archive/ folder; do not infer approval for subdirectories, individual moves, or adoption.

Only this proposal file was revised. Main's AGENTS.md and its uncommitted diff, other plans, and the original proposal remain unchanged. No archive folder, archive moves, adoption, or commits were performed.
