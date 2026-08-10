# Autonomous migration agent instructions

## Mission

Implement the Rust rewrite described in `plans/MASTER_PLAN.md` from M00 through M13 with minimal
human involvement. Progress independently, but never trade correctness, determinism, compatibility,
security, or evidence for apparent speed.

The behavioral oracle is the separate Python repository:

```text
D:\Projects\ti4-engine
branch: codex/fully-learned-policy
commit: 37061c5
```

Treat that repository as read-only. Never edit, format, stage, commit, clean, reset, generate caches
in, or run a command that writes artifacts into it. Copy required fixtures into this repository under
the process defined by M00. If the oracle commit is unavailable or its integrity guard fails, stop
implementation and record the blocker in `plans/EXECUTION_STATE.md`.

## Required reading order

At the beginning of a fresh session or after context compaction, read:

1. This file completely.
2. `plans/SCOPED_PERMISSIONS.md`.
3. `plans/EXECUTION_STATE.md`.
4. `plans/MASTER_PLAN.md`.
5. `plans/PI_WORK_PACKAGE_STANDARD.md`.
6. The active milestone plan linked from `plans/INDEX.md`.
7. The active package evidence file and the two most recent completed evidence files, if present.
8. `git status --short --branch` and the last five commits.

Do not rely on remembered context when durable repository state can answer the question.

## Execution order

- Execute milestones strictly from M00 through M13.
- Do not start a milestone until the preceding exit gate is met and its frontier review is resolved.
- Execute work packages in dependency order.
- Keep at most one package in progress unless packages have disjoint edit scopes and the harness can
  isolate them safely.
- Never mark a package or milestone complete merely because code exists. Completion requires all
  specified tests, evidence, and reviews.
- When a milestone row is too large for the atomic limits in `PI_WORK_PACKAGE_STANDARD.md`, split it
  into suffixed tasks such as `M05-008a` and `M05-008b`. Record the split before implementation and
  preserve the original acceptance criterion across the children.
- Do not silently shrink scope. Record every deferred, excluded, or intentionally changed behavior
  in the scope ledger and active evidence file.

## Package loop

For every work package:

1. Confirm dependencies are complete in `plans/EXECUTION_STATE.md`.
2. Create or update its exact task specification using the template in
   `plans/PI_WORK_PACKAGE_STANDARD.md`.
3. Declare its permission class, writable/read-only paths, network/process needs, artifact bounds,
   and external-state effects using `plans/SCOPED_PERMISSIONS.md`.
4. Create the package branch from the active milestone integration branch.
5. Inspect the named Python source and tests read-only. Expand inspection only when necessary to
   understand a direct dependency.
6. Write a failing focused test or compatibility fixture first where practical.
7. Implement the smallest complete behavior. Do not add speculative abstractions or unrelated
   cleanup.
8. Run formatting, focused tests, affected-crate tests, lints, and any specified differential,
   property, fuzz, mutation, or benchmark check.
9. Write `plans/evidence/<package-id>.md` with commands, exact results, compatibility evidence,
   benchmark effect, unresolved differences, and source-oracle commit.
10. Run the required independent review tier. The implementer may not be the sole reviewer.
11. Fix every actionable finding and rerun affected checks.
12. Commit only the package's scoped changes.
13. Update `plans/EXECUTION_STATE.md`, including the next ready package.

Use Qwen 3.6 35B through Pi v0.84.1 as the default implementer. Use Qwen 27B only for proven
mechanical work. Use a frontier model for review tiers C and D, milestone exit reviews, repeated
failures, architecture decisions, timing, legality, payments, hidden information, schema migration,
training mathematics, security boundaries, unsafe code, and performance claims.

## Context-compaction protocol

Compact context regularly. Long context is not an execution record.

Perform a compaction checkpoint at the earliest of:

- completion of three atomic work packages;
- reaching approximately 50–60% of the harness context budget;
- finishing a large investigation, benchmark, differential campaign, or review;
- switching subsystem or milestone;
- before and after a milestone exit review;
- whenever tool output has made the conversation difficult to navigate.

Before compacting:

1. Finish or safely stop the current command; do not compact during an unknown mutation.
2. Update `plans/EXECUTION_STATE.md` with current milestone/package, status, last commit, exact tests,
   decisions, open findings, blockers, modified files, and next command.
3. Update the active evidence file. Evidence must not exist only in conversation.
4. Record `git status --short --branch` and confirm whether the tree is clean.
5. If the tree is intentionally dirty, list every changed path and why it is safe.
6. Write a compact handover summary using the format below.
7. Invoke the Pi harness's supported context-compaction mechanism. If no explicit compaction command
   is available, end the current agent session after persisting the handover and resume in a fresh
   session from the required reading order.

Handover format:

```text
Objective:
Oracle commit:
Active milestone/package:
Status and completed acceptance criteria:
Current branch and HEAD:
Working-tree state:
Tests last run and exact results:
Compatibility evidence:
Decisions made and rationale:
Open review findings or blockers:
Next exact action/command:
Files to read first after compaction:
```

After compacting, do not immediately continue from memory. Follow the required reading order, verify
Git state against the handover, and only then resume. If the handover and repository disagree, trust
the repository and investigate before changing files.

## Accuracy rules

- Legal actions are generated, not accepted by late rejection.
- Invalid or failed transitions are atomic.
- Deterministic behavior must not depend on hash-map iteration, thread scheduling, filesystem order,
  locale, or wall-clock time.
- Rules legality uses exact arithmetic. Floating-point tolerances are restricted to policy/training
  math and must be specified by tests.
- Hidden information is enforced through typed views and API boundaries, not convention.
- Preserve stable choice IDs and canonical projections.
- Preserve current implemented/partial/unimplemented registries exactly until an explicit later
  project changes scope.
- Never claim parity from aggregate outcomes alone. Use decision-boundary differential evidence.
- Never claim a speedup without the M00 protocol, the same machine/workload, raw measurements,
  variance, and passing semantic gates.
- Never turn a parser error, bridge refusal, worker crash, or incomplete game into an apparent success.
- Validate schema version, size limits, references, and checksums before mutating state.
- Keep checkpoint writes atomic and recoverable.

## Testing discipline

During a package, run the narrowest useful tests first, then the affected crate. Before merging a
milestone, run the entire workspace suite and every milestone-specific gate. Do not repeatedly rerun
a flaky test until it passes; diagnose and remove the nondeterminism.

A source Python test is not considered covered merely because a similarly named Rust test exists.
The scope/test ledger must link it to an assertion of the same behavior or to a reviewed exception.

Do not update golden fixtures simply to make a failure disappear. Regenerate a fixture only through
the versioned oracle process, inspect the semantic diff, and record why the new fixture is correct.

## Review and failure handling

- First failure: diagnose in the current Qwen context and retry once.
- Second failure of the same invariant: use a fresh Qwen 35B context for independent diagnosis.
- Third failure, architecture conflict, nondeterminism, or unexplained oracle mismatch: obtain a
  frontier-model diagnosis before further implementation.
- A reviewer reports findings; it does not silently rewrite critical code without preserving the
  review trail.
- Record rejected review suggestions and technical rationale in evidence.

If blocked, continue with another dependency-ready, non-overlapping package only when doing so cannot
hide or compound the blocker. Otherwise persist a full checkpoint and stop. Do not fabricate a user
decision or broaden authority.

## Git and filesystem safety

- Work only inside `D:\Projects\ti4-engine-rs`, except for read-only oracle inspection.
- Follow `plans/SCOPED_PERMISSIONS.md`; delegation never broadens those permissions.
- Preserve unrelated changes. Never use destructive reset or checkout commands.
- Use one branch and one focused commit per atomic package.
- Do not commit build products, large training outputs, private captures, or copied artifacts without
  the repository's artifact policy and a checksum manifest.
- Do not rewrite shared branch history.
- Before deletion or bulk movement, resolve and verify the absolute target is inside this repository.
- Keep secrets, access tokens, machine-specific paths, and personal TTS data out of Git.

## Autonomous decision policy

Proceed without asking for routine implementation choices when the answer follows from the oracle,
plans, tests, or established architecture. Prefer the smallest reversible decision and record it.

Stop and request authority only when a choice would materially change public behavior, accepted
compatibility, security posture, licensing, deployment scope, external systems, or destructive data
handling and the plans do not already decide it.

## Managed RPC operation

When Pi is launched through `tools/pi_rpc_bridge.py`, that controller is the sole owner of the Pi
session. Do not launch a TUI, `--continue`, `--resume`, print-mode job, or second RPC process against
the same session. Prompts, steering, status checks, aborts, and compaction may arrive through the
native Pi RPC queue. Treat them like operator instructions, while still enforcing this file and
`plans/SCOPED_PERMISSIONS.md`.

Normal autonomous work must be submitted as one bounded package through the controller's `/task`
endpoint. Make the first repository edit promptly, use at most one simpler retry after a failed
approach, run the package acceptance checks, report compactly, and stop. Never begin the next package
from the same prompt. The controller may abort work after the configured no-edit timeout, absolute
timeout, or tool-error limit. After an abort, the next prompt must be smaller than the failed one.

Monitoring uses `/summary` at checkpoints. Token-level `message_update` events, streaming tool-output
deltas, and full transcript reads are not part of normal supervision. See
`plans/PI_RPC_CONTROL.md` for the low-token policy and limits.

Before settling after a triggered work unit, update `plans/EXECUTION_STATE.md` and package evidence,
then report the exact branch, commit, checks, findings, and next safe action. Do not describe an
independent review as complete unless a different reviewing agent or model actually performed it.

## Milestone completion

At a milestone exit:

1. Close every work-package row or record an approved exception.
2. Run the full workspace suite and all milestone-specific campaigns.
3. Reconcile the scope, test, artifact, and known-difference ledgers.
4. Confirm the Python oracle is still unchanged.
5. Obtain and resolve the specified frontier review.
6. Write the milestone report and update `plans/EXECUTION_STATE.md`.
7. Compact context before beginning the next milestone.
