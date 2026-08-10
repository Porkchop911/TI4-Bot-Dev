# ti4-engine-rs

This directory is reserved for the isolated Rust rewrite of `ti4-engine`.

No production code has been started. The planning baseline is the Python repository at
`D:\Projects\ti4-engine`, branch `codex/fully-learned-policy`, commit `37061c5`.
That repository is an immutable behavioral oracle for the migration.

Start here:

1. [`AGENTS.md`](AGENTS.md) — autonomous execution, review, safety, and context-compaction rules.
2. [`plans/SCOPED_PERMISSIONS.md`](plans/SCOPED_PERMISSIONS.md) — least-privilege authorization matrix.
3. [`plans/EXECUTION_STATE.md`](plans/EXECUTION_STATE.md) — durable resume point across sessions.
4. [`plans/MASTER_PLAN.md`](plans/MASTER_PLAN.md) — scope, architecture, gates, and order.
5. [`plans/PI_WORK_PACKAGE_STANDARD.md`](plans/PI_WORK_PACKAGE_STANDARD.md) — how Pi/Qwen tasks are written, implemented, reviewed, and accepted.
6. [`plans/INDEX.md`](plans/INDEX.md) — milestone subplans and dependencies.
7. [`plans/PI_RPC_CONTROL.md`](plans/PI_RPC_CONTROL.md) — bounded, low-token monitoring and control of the managed Pi session.

Implementation must not begin beyond M0 until M0 has frozen the compatibility corpus and
remeasured the Python baseline.
