# Technical wiki maintenance log

Append dated entries with changed pages, source scope, checks and review state. This log covers the technical wiki in this checkout; the strategy vault has its own log.

## [2026-10-18] navigation | Create a checkout-specific technical wiki

- Pages: [index](TECHNICAL_WIKI.md), [engine](TECHNICAL_WIKI_ENGINE.md), [bots](TECHNICAL_WIKI_BOTS.md), [interfaces](TECHNICAL_WIKI_INTERFACES.md).
- Source scope: `wp/base-factions` at `1f3556d1`; [Cargo workspace](../Cargo.toml), inspected crate entry points and modules, [web README](../web/README.md), [web2 UI instructions](../web2/AGENTS.md), and [planning index](../plans/INDEX.md). The map routes to 22 workspace crates and labels stale handover status separately from source structure.
- Validation and independent review: pending.

## [2026-10-18] validation | Check technical wiki navigation

- Five wiki pages plus the root README: 143 local links checked, all targets exist; no trailing whitespace. All 22 Cargo workspace members appear in the wiki routes.
- Independent Tier A reviewer `/root/wiki_review` checked source fidelity and links. Four initial bad links were fixed: the fork test path, two host protocol paths and the absent `bridge/server.py`. The bridge dependency gap is now explicit in [interfaces](TECHNICAL_WIKI_INTERFACES.md). Reviewer recheck found no further actionable issue.
- These checks establish navigation and sampled source fidelity, not feature acceptance or full engine qualification.

## [2026-10-18] depth | Trace behavior and failure boundaries

- Pages: expanded [index](TECHNICAL_WIKI.md), [engine](TECHNICAL_WIKI_ENGINE.md), [bots](TECHNICAL_WIKI_BOTS.md), and [interfaces](TECHNICAL_WIKI_INTERFACES.md); added [invariants](TECHNICAL_WIKI_INVARIANTS.md) and [tests](TECHNICAL_WIKI_TESTS.md). Applied the upstream `/karpathy:wiki` source-first, concept-page, frontmatter and citation approach within the existing `docs/` folder.
- Source scope: `wp/base-factions` at `1f3556d1`. Read engine choice/game/snapshot code, content loader/validator, bot and MLP paths, simulation/training entry points, host protocol/projection, server tests, Wasm and both web manifests. Corrected an initial overstatement: the MLP API boundary tests cover inference refusal, not opponent-hand privacy.
- Validation: seven pages and 320 local links, including heading fragments, resolve; all 22 workspace members are mapped, concept pages have required frontmatter and cited paths, no pages are orphaned, and no trailing whitespace was found. No runtime or training test was run for documentation-only edits.
