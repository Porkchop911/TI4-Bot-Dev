# Tooling, verification and project history

[Repo guide](REPO_GUIDE.md) · [Engine](REPO_ENGINE.md) · [Bots](REPO_BOTS.md) · [Interfaces](REPO_INTERFACES.md)

Use this route to find an operational entry point or assess evidence. Source scope: `rule-bot`, inspected 2026-10-15. Commands and scripts are not automatically authorized to run.

## Plans versus evidence

| Need | Read |
|---|---|
| Current handover, active work and open reviews | [EXECUTION_STATE.md](../../plans/EXECUTION_STATE.md) |
| Workstream and milestone navigation | [INDEX.md](../../plans/INDEX.md), then the named plan |
| Intended architecture and acceptance contracts | [MASTER_PLAN.md](../../plans/MASTER_PLAN.md), the relevant milestone/workstream specification |
| What an experiment or validation actually established | Dated files under [evidence/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/plans/evidence) and [reviews/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/plans/reviews), plus the referenced artifacts |
| Historical context | [archive/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/plans/archive); date and scope matter |
| Permissions and working agreements | [SCOPED_PERMISSIONS.md](../../plans/SCOPED_PERMISSIONS.md), [AGENTS.md](../../AGENTS.md) |

An old plan is not a status report, a build is not an independent review, and a benchmark aggregate does not establish which decision caused a result. Follow links from claims to exact sources/checks.

## Remaining workspace crates

| Crate | Navigation and limitation |
|---|---|
| [ti4-legacy](../../crates/ti4-legacy/src/lib.rs) | Legacy artifact, replay, checkpoint and state-conversion-related modules. Inspect the specific [source file](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-legacy/src) and tests rather than inferring completion from its historical header |
| [ti4-cli](../../crates/ti4-cli/src/main.rs) | The inspected main is a version-printing stub, not a universal launcher for engine tools. Locate operational binaries/examples in the owning crate |
| [xtask](../../crates/xtask/src/main.rs) | The inspected main is a version-printing stub. Do not assume a fixture/validation command exists just because an xtask crate is present |

Together with [engine](REPO_ENGINE.md), [bots](REPO_BOTS.md) and [interfaces](REPO_INTERFACES.md), this covers the workspace members declared by [Cargo.toml](../../Cargo.toml).

## Finding a check

Rust tests are both inline in modules and under each owning crate's `tests/`. Start with the affected subsystem and read its manifest/features before selecting the narrow check. [Workspace configuration](../../Cargo.toml) defines runtime dependencies and profiles; some paths require native libtorch. Follow the repository's test and review requirements rather than launching every expensive suite for a documentation edit.

Browser checks are routed through [web/package.json](../../web/package.json), the [web README](../../web/README.md), [web/e2e](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/web/e2e) and the [screenshot guide](../../web/e2e/screenshots/README.md). For state copying, the [fork-equivalence test](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/tests/fork_equivalence.rs) is a concrete entry point; for broader rules behavior, use the owning crate's actual tests and package contract.

## Tools and retained outputs

- [scripts/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/scripts) contains launchers and experiment configurations. For training, begin with [PPO_TRAINING_HOWTO.md](../../plans/PPO_TRAINING_HOWTO.md), then inspect the specific script for filesystem, process and compute effects.
- [tools/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/tools) contains research/corpus/benchmark support. The wiki itself is maintained by [scpt_strategy.py](../../tools/scpt_strategy.py), [scpt_wiki.py](../../tools/scpt_wiki.py), their [focused tests](../../tools/test_scpt_strategy.py) and the [graph template](../../tools/scpt_graph.html); see [PIPELINE.md](PIPELINE.md).
- [examples/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/examples) and [fixtures/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/fixtures) contain repository inputs. Local ignored `out/` data is not guaranteed to exist in another clone and may be the only retained copy of a run or checkpoint.
- [deploy/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/deploy) contains deployment material. Deploying or changing live services requires the relevant explicit authorization.
- The [historical-reference policy](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/AGENTS.md#mission-and-sources) makes the old Python checkout read-only context; it is not an acceptance oracle and is not a scratch directory.

## Keeping this map useful

Follow [SCHEMA.md](SCHEMA.md) when a task changes a documented module responsibility or discovers a reusable architectural fact. Update the affected route, cite current source, retain unresolved differences and append [LOG.md](LOG.md). This map helps a person reach evidence; it does not replace that evidence.
