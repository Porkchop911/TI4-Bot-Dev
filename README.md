# ti4-engine-rs

Rust engine and multiplayer server for Twilight Imperium 4. The historical
Python repository is read-only context, not an acceptance oracle.

Start here:

1. [`AGENTS.md`](AGENTS.md) — autonomous execution, review, safety, and context-compaction rules.
2. [`plans/SCOPED_PERMISSIONS.md`](plans/SCOPED_PERMISSIONS.md) — least-privilege authorization matrix.
3. [`plans/EXECUTION_STATE.md`](plans/EXECUTION_STATE.md) — durable resume point across sessions.
4. [`plans/MASTER_PLAN.md`](plans/MASTER_PLAN.md) — scope, architecture, gates, and order.
5. [`plans/PI_WORK_PACKAGE_STANDARD.md`](plans/PI_WORK_PACKAGE_STANDARD.md) — how Pi/Qwen tasks are written, implemented, reviewed, and accepted.
6. [`plans/INDEX.md`](plans/INDEX.md) — milestone subplans and dependencies.
7. [`plans/PI_RPC_CONTROL.md`](plans/PI_RPC_CONTROL.md) — bounded, low-token monitoring and control of the managed Pi session.

For local browser play, run `cargo run -p ti4-server --bin server` and
`npm run dev` from `web/`. Create a lobby in the browser, share its URL, join
from other tabs, mark all players ready, and start as host. See
[`web/README.md`](web/README.md) for the current player-session flow.

Stage-1 learning comparisons are documented in
[`docs/STAGE1_PARITY_COMPARISON.md`](docs/STAGE1_PARITY_COMPARISON.md). Use the gated parity runner,
not the legacy six-player curve, before making learning or performance claims.

The optimized Stage-1 and Stage-2 policy-gradient pipeline, production commands, checkpoint rules,
determinism guarantees, and measured throughput are documented in
[`docs/TRAINING_PIPELINE.md`](docs/TRAINING_PIPELINE.md).
