# Pi/Qwen work-package standard

## Models and reviews

- Default implementer: Qwen 3.6 35B through Pi v0.84.1.
- Qwen 27B: mechanical fixtures, renames, or repetitive tables only after a successful sampled review.
- Frontier model: architecture, critical logic, milestone exits, security, training mathematics, and performance validation.
- The implementer must not be the only reviewer of its own patch.

## Atomic package size

One package should normally touch one to five files, add 200–500 net lines, implement one behavior
cluster, and add four to twelve focused tests. Split work when it crosses crate boundaries, mixes
schemas with behavior, or cannot be reviewed from a single diff.

Each atomic package prompt must contain:

```text
ID and title
Milestone and dependencies
One-sentence objective
Exact Python source references
Exact Python test references
Allowed Rust edit paths
Permission class and scoped access declaration
Inputs and outputs
Invariants and compatibility class
Explicit non-goals
Tests to add
Commands to run
Expected evidence
Known traps
Definition of done
```

## Execution loop

1. Create `wp/mNN-NNN-description` from the milestone integration branch.
2. Read only the architecture notes and files named by the package, expanding only when blocked.
3. Add a failing focused test or fixture first where practical.
4. Implement the smallest complete behavior.
5. Run formatting, focused tests, affected-crate tests, and lints.
6. Record commands, results, compatibility evidence, and benchmark effects.
7. Commit one package.
8. Run an independent review pass over the diff and named invariants.
9. Fix all findings and rerun checks.
10. Merge only after the milestone integrator confirms edit-scope and dependency consistency.

## Escalation

- First failure: same Qwen context diagnoses and retries.
- Second failure: fresh Qwen 35B context independently diagnoses the invariant.
- Third failure, nondeterminism, or architecture conflict: frontier model takes over.
- A flaky test blocks merge; rerunning until green is not acceptance.

## Evidence file

Every package adds `plans/evidence/MNN-NNN.md` once implementation starts. It records commit, source
oracle commit, changed paths, test commands/results, differential fixtures, benchmark delta, reviewer,
findings, and resolution. Large raw outputs are referenced by hash rather than committed.

## Review tiers

| Tier | Examples | Required review |
|---|---|---|
| A | Docs, repetitive content fixtures | Independent Qwen review |
| B | Ordinary model/rule/policy code | Independent Qwen plus milestone integration tests |
| C | Timing, legality, payments, hidden info, schema migration, training math, bridge security | Frontier review |
| D | Unsafe code, cutover, claimed performance gate | Two independent frontier passes |

## Definition of done

A package is done only when its behavior is implemented, focused and affected tests pass, no TODO
stands in for scope, compatibility evidence exists, review findings are resolved, and the package is
committed without unrelated edits.
