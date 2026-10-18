# Technical wiki maintenance log


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

## [2026-10-18] depth | Explain bot designs and benchmark limits

- Expanded [bot guide](TECHNICAL_WIKI_BOTS.md) with ScoredBot, linear policy, MLP and rule-bot philosophies, mechanics and failure traces; added [benchmark guide](TECHNICAL_WIKI_BENCHMARKS.md). The [index](TECHNICAL_WIKI.md) now distinguishes the `wp/base-factions` architecture snapshot from `main`/`DEVniko` inference and self-play paths.
- Source scope: `wp/base-factions` `1f3556d1` for authored bot implementation; `main` `a7975206` for self-play and native inference; `rule-bot` `592e5716` for the server smart-bot code baseline. The October 13 holdout and mechanism reports were untracked in that checkout and enter Git with this publication; the measurements are reproduced from those files, not rerun. A public TI4 autonomous-player benchmark search found no comparable leaderboard; no best-in-class claim is made.
- Publication validation: 157 Markdown pages, 14,238 local links and 46 unique pinned repository source references checked in the prepared Git tree; no missing targets or fragments. Changed manual pages pass whitespace check. Independent Tier A review corrected branch provenance, historical manifest scope, Diplomacy-off conditioning and source-link targets. Documentation evidence was checked; no new benchmark or engine test was run.

Append dated entries with changed pages, source scope, checks and review state. This log covers the technical wiki in this checkout; the strategy vault has its own log.

## [2026-10-18] depth | Explain the battle and trade arenas

- Added [arena guide](TECHNICAL_WIKI_ARENAS.md) and linked it from the [index](TECHNICAL_WIKI.md) and [bot guide](TECHNICAL_WIKI_BOTS.md). Source scope: `main` `a7975206` code for battle simulation/prediction and trade value/teacher paths; dated [arena predictor](../plans/evidence/ARENA_002_PREDICTOR_2026-09-17.md) and [wide-roster review](../plans/evidence/BF-22B-REVIEW-2026-10-11.md) for measurements and open gates.
- Arena fit and teacher accuracy are kept separate from full-game strength. Publication validation: 158 Markdown pages and 14,266 local links/fragments checked, zero issues; changed pages pass whitespace check. Independent Tier A reviewer matched the cited predictor and teacher figures to dated reports and found no actionable issue. No new arena or gameplay run was performed.

Append dated entries with changed pages, source scope, checks and review state. This log covers the technical wiki in this checkout; the strategy vault has its own log.
