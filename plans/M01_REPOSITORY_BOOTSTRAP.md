# M01 — Repository bootstrap

## Goal

Create a reproducible, Windows-first Rust workspace safe for unattended agent implementation.

## Work packages

| ID | Package | Depends | Deliverable and acceptance test |
|---|---|---|---|
| M01-001 | Initialize Git repository | M00 | New independent history, default branch, ignores, line-ending policy; no linkage that permits writes to Python. |
| M01-002 | Pin toolchain | 001 | `rust-toolchain.toml` and documented components; fresh install builds. |
| M01-003 | Workspace skeleton | 002 | Create the ten crates from the master plan with dependency direction enforced by compilation. |
| M01-004 | Workspace policy | 003 | Shared edition, lints, profiles, feature policy, error/log conventions, and `unsafe` denial. |
| M01-005 | Formatting and lint commands | 004 | `cargo fmt --check` and strict workspace `clippy` pass. |
| M01-006 | Windows CI | 003–005 | Build, unit tests, docs, fmt, and clippy on a clean Windows runner. |
| M01-007 | Security CI | 003 | Dependency advisory, license, provenance, and duplicate-dependency checks; intentional exceptions expire. |
| M01-008 | Coverage and mutation harness | 003 | Commands and report locations exist; seeded sample test proves each tool works. |
| M01-009 | Benchmark harness | 003 | Criterion or equivalent produces versioned JSON with environment metadata and baseline comparison. |
| M01-010 | Fixture plumbing | M00,003 | Configurable read-only oracle/fixture paths; CI uses copied fixtures, never absolute developer paths. |
| M01-011 | CLI version command | 003 | Prints binary, Git, schema, content, and RNG versions with snapshot test. |
| M01-012 | Agent instructions | 004 | Package/evidence/branch rules are discoverable from repository root. |
| M01-013 | Fresh-checkout proof | 006–012 | Frontier reviewer follows documented bootstrap on a clean checkout and obtains green checks. |

## Exit gate

The empty workspace is reproducible on Windows, policy checks are automated, and no implementation
package can silently bypass lint, test, security, or evidence requirements.

