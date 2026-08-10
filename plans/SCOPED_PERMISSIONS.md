# Scoped permissions

## Purpose

These permissions define what an autonomous implementation agent is authorized to do while executing
the Rust migration. They are least-privilege operating rules, not a grant of general control over the
machine or external services.

The boundaries apply to the primary agent, Pi/Qwen implementers, reviewers, spawned subprocesses,
scripts, and tools. Delegating an action does not broaden permission.

## Scope map

| Scope | Path/system | Access | Conditions |
|---|---|---|---|
| Rust rewrite | `D:\Projects\ti4-engine-rs` | Read/write | Only migration work, tests, generated local evidence, Git branches and commits |
| Python oracle | `D:\Projects\ti4-engine` | Read-only | Must remain at verified commit; commands must not create caches, outputs, lock changes, or tracked/untracked files |
| Temporary files | OS temp or a temp directory under the Rust repo | Read/write/delete | Use task-specific directories; validate exact path before recursive cleanup |
| Cargo caches/toolchains | Standard Rust/Cargo locations | Read/write through normal tooling | Only dependencies/toolchains needed by the pinned workspace; do not manually delete shared caches |
| Other `D:\Projects` repositories | Any sibling other than the two above | No write; no routine read | Inspect only after explicit user authority or when a plan names a precise read-only dependency |
| User profile/documents | Outside standard tool caches | No access | Do not search for credentials, captures, saves, or unrelated configuration |
| TTS installation/live session | Local application/process | No mutation by default | Live patching, commands, or play require explicit cutover/test authority in M11/M13 |
| Remote services | GitHub, registries, APIs, model services | Limited | See network and external-state rules below |

## Permission classes

### P0 — Always allowed, read-only

- Read files inside the Rust repository.
- Inspect the exact Python oracle files named by the active package.
- Inspect Git status, log, diff, branches, tags, object hashes, and configuration without mutation.
- Run read-only discovery and diagnostic commands.
- Compare checksums, schemas, fixtures, test output, and benchmark reports.
- Read installed tool help and version information.

### P1 — Allowed inside the Rust repository

- Create and edit source, tests, fixtures, plans, evidence, and configuration required by the active package.
- Format migration-owned files.
- Build, test, lint, benchmark, fuzz, mutate, and document the Rust workspace.
- Create package branches following the plan.
- Stage and commit only active-package files after checks and review pass.
- Generate bounded test and benchmark output under ignored repository directories.
- Create task-specific temporary directories and remove only those verified directories afterward.
- Add or update dependencies required by the active package, subject to the dependency rules below.

### P2 — Allowed only when the active plan/package explicitly requires it

- Download Rust crates, pinned tools, vulnerability databases, or official documentation.
- Install the pinned Rust toolchain or approved development component.
- Start bounded local test servers or worker processes.
- Generate large fuzz, simulation, training, Parquet, map-pool, or benchmark artifacts.
- Convert copied legacy artifacts.
- Bind a local bridge test server to loopback.
- Create tags or release-candidate packaging during M13.
- Perform recoverable bulk movement or deletion of migration-owned generated output.

P2 actions must be recorded in package evidence, including target, reason, and result. Long-running
processes need a timeout, cancellation path, output location, and resource bound.

### P3 — Requires explicit user authorization

- Push branches, tags, releases, or packages to any remote.
- Open, merge, close, or modify pull requests/issues.
- Deploy, publish, or switch a real workload.
- Patch a real TTS save, control a live TTS session, or send live bridge commands.
- Read or migrate private/user production artifacts not already copied into an approved fixture area.
- Access credentials, secrets, private registries, cloud resources, or paid external APIs beyond the
  already authorized Pi/model execution.
- Write outside `D:\Projects\ti4-engine-rs` except normal pinned tool/cache installation.
- Change the accepted compatibility policy, supported platform, licensing position, security posture,
  or workload cutover criteria.
- Delete or rewrite data that is not reproducibly generated migration output.

Authorization must name the target and action. Permission for one target does not generalize.

### P4 — Forbidden

- Modify, stage, commit, clean, reset, rebase, or generate files in the Python oracle.
- Disable, weaken, delete, or bypass tests, evidence, reviews, security checks, or milestone gates to
  make progress appear green.
- Fabricate benchmark, test, review, parity, or completion evidence.
- Exfiltrate secrets, private data, TTS captures, credentials, or unrelated source.
- Run destructive commands against a workspace root, user directory, drive root, unresolved variable,
  wildcard-expanded broad target, or shared tool cache.
- Force-push, rewrite shared history, or delete remote branches/tags.
- Bind the bridge to a non-loopback interface without an approved security design and explicit authority.
- Execute downloaded scripts/binaries whose provenance and checksum have not been reviewed.
- Use the Python oracle as a writable scratch directory, benchmark-output location, or test temp root.
- Treat reviewer or model access as authority to mutate external state.

## Git permissions

Allowed locally:

- `git status`, `log`, `diff`, `show`, `grep`, `rev-parse`, and similar inspection
- create/switch package and milestone branches
- stage exact active-package paths
- create focused package commits
- create M13 local tags when its package is active

Not allowed without explicit user authority:

- any push or remote mutation
- force operations
- history rewriting after a package is shared or integrated
- destructive reset/checkout/clean
- staging unrelated user changes

Before committing, inspect both `git status --short` and the staged diff. A package commit must not
contain another package's work unless the integration plan explicitly combines them.

## Dependency and network permissions

Network use is restricted to the active task. Prefer official Rust documentation, crate registries,
upstream repositories, and security advisory sources. Record the reason and source when adding a new
dependency or tool.

A dependency may be added only when:

- the standard library or existing dependency is materially insufficient;
- its license is compatible;
- its provenance and maintenance status are acceptable;
- its features are minimized;
- the lockfile diff is reviewed;
- advisories are checked;
- the package evidence records why it is needed.

Do not browse, download, or query unrelated material. Do not upload source, artifacts, fixtures, logs,
or benchmark data except through the already authorized model context needed for implementation and
review. Redact private or machine-specific data before model review.

## Process and resource permissions

- Use bounded worker counts and timeouts appropriate to the active package.
- Do not leave training, fuzzing, bridge, web-server, or worker processes running after the package.
- Before starting a server, verify the port and bind only to loopback.
- Before starting a costly campaign, run a small smoke sample and estimate disk/time use.
- Store large outputs only in an ignored, named directory with a manifest and cleanup policy.
- Do not monopolize all machine resources indefinitely; preserve an operator margin unless the user
  explicitly authorizes an exclusive benchmark run.
- Do not terminate unrelated processes. Identify ownership before stopping any process.

## Destructive-action protocol

For an allowed deletion or overwrite:

1. Resolve the absolute path.
2. Verify it is inside the Rust repository's approved generated-output area or a task-specific temp directory.
3. List the exact targets and confirm they are reproducible or backed up.
4. Prefer atomic replacement, archival, or trash over permanent deletion.
5. Perform the action within one shell and without broad globs or unresolved variables.
6. Record material deletion and recovery status in evidence.

If any check fails, do not perform the action.

## Permission check before every package

The package specification must include:

```text
Permission class required: P0 / P1 / P2 / P3
Writable paths:
Read-only external paths:
Network access:
Processes/ports:
Expected generated artifacts and maximum size:
Destructive actions:
External-state changes:
```

If the package needs P3, stop before that action and request narrowly scoped authorization. Continue
other safe work only when it cannot invalidate the pending decision.

## Enforcement plan

These rules are immediately binding on agents. During M01, add practical enforcement where supported:

- run the implementation harness with `D:\Projects\ti4-engine-rs` as its only writable workspace;
- mount or expose the Python oracle read-only;
- disable remote Git writes by default;
- restrict bridge tests to loopback;
- direct test temp/cache/output paths into the Rust repository;
- cap subprocess runtime, worker count, request size, and artifact directories;
- add an oracle-cleanliness and hash check before and after applicable packages.

If tool-level permissions cannot express a boundary, the agent must still obey the documented rule
and record the limitation in M01 evidence.

