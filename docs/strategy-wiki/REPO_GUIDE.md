# Find your way around the repository

This project combines a Rust TI4 rules engine, bots and learning tools, multiplayer services, a browser client and native review/replay applications. Use this map to find the code for a task; use the linked source and current execution evidence to determine what is implemented or accepted.

Source scope: inspected in the `rule-bot` checkout on 2026-10-15. Its crate layout differs from `main` and `DEVniko`; use the [technical wiki](../TECHNICAL_WIKI.md) for their architectural changes. This is a reading guide, not a claim that every module, milestone or run is complete.

The guide pages link to each other inside Obsidian. Source links such as `../../crates/...` point outside the vault: use them from a repository browser or code editor; they are not vault notes or source-code nodes in Obsidian's graph. The code remains in its existing repository location.

## Start with your task

| I want to… | Start here |
|---|---|
| Understand a rule or fix a game decision | [Engine and content](REPO_ENGINE.md) |
| Understand why a bot chose an action | [Bots, simulation and learning](REPO_BOTS.md) |
| Train or evaluate a policy | [Bots, simulation and learning](REPO_BOTS.md), then the linked training guide |
| Play locally or change the browser UI | [Online play and interfaces](REPO_INTERFACES.md) |
| Inspect a saved game or branch a decision | [Online play and interfaces](REPO_INTERFACES.md) |
| Find tests, scripts, plans or run evidence | [Tooling and project history](REPO_WORKFLOWS.md) |
| Read faction strategy or component text | [TI4 wiki index](README.md), [component index](components.md), [rule scope](RULES_REFERENCE.md) |
| Understand how agents maintain these pages | [Wiki procedure](SCHEMA.md), [maintenance log](LOG.md) |

## How the pieces fit

The [model crate](../../crates/ti4-model/src/lib.rs) defines typed state and views. The [content crate](../../crates/ti4-content/src/lib.rs) loads component records. The [engine](../../crates/ti4-engine/src/game.rs) applies game rules and presents choices through [the decision interface](../../crates/ti4-engine/src/choice.rs).

People and bots answer those choices through different front ends: [the multiplayer server](../../crates/ti4-server/src/lib.rs), [authored/learned policies](../../crates/ti4-policy/src/lib.rs), [the MLP bot](../../crates/ti4-mlp/src/bot.rs), or [the replayer's manual-seat gate](../../crates/ti4-replayer/src/decider.rs). [Simulation](../../crates/ti4-sim/src/lib.rs) and [training](../../crates/ti4-training/src/lib.rs) provide experiments around the game. [Review](../../crates/ti4-review/src/lib.rs) and [replay](../../crates/ti4-replayer/src/lib.rs) serve different inspection and interaction needs.

The `rule-bot` snapshot's Rust members are recorded in [its pinned Cargo.toml](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/Cargo.toml). The pages above route that checkout's crates by reader task, not by milestone. For the published branch's current manifest, use [main's Cargo.toml](../../Cargo.toml) and the [technical wiki's branch note](../TECHNICAL_WIKI.md#technical-wiki).

## Top-level map

| Area | What to look for |
|---|---|
| [crates/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates) | Rust engine, shared types, policies, services, native applications and support crates |
| [web/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/web) | React client, UI tests and reproducible screenshots |
| [docs/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/docs) | Explanatory guides, design discussions and this wiki |
| [plans/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/plans) | Contracts, workstreams, execution state, review history and evidence; start at [INDEX.md](../../plans/INDEX.md) |
| [tools/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/tools) | Python research, corpus, benchmark and agent-support tools |
| [scripts/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/scripts) | Operational launchers and experiment configurations; inspect side effects before running |
| [examples/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/examples) | Repository examples, including reviewer inputs |
| [fixtures/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/fixtures) | Shared fixture material; crate-specific tests also live with their crates |
| [deploy/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/deploy) | Deployment material; its presence is not authorization to publish |
| [.cargo/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/.cargo) | Cargo/build configuration |
| [.github/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/.github) | Repository automation configuration |
| [.pi/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/.pi) | Existing Pi integration material; follow the project coordination documents before using it |
| `out/`, `target/` | Local outputs and build products. These are not a clean-clone inventory or disposable scratch space; preserve retained data |

## Read status separately from structure

[EXECUTION_STATE.md](../../plans/EXECUTION_STATE.md) is the current handover point; [INDEX.md](../../plans/INDEX.md) routes to active workstreams and historical milestone plans. A module header, example checkpoint, old benchmark or source file's existence does not establish current correctness or best performance. Some crate headers still describe earlier implementation stages; follow concrete modules, tests and dated evidence instead of repeating those summaries as current status.

Related: [engine](REPO_ENGINE.md) · [bots](REPO_BOTS.md) · [interfaces](REPO_INTERFACES.md) · [workflows](REPO_WORKFLOWS.md).
