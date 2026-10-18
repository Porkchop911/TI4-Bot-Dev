# Wiki maintenance log

Append entries chronologically using `## [YYYY-MM-DD] operation | title`. Record affected pages, sources, checks actually run, unresolved questions and review status. Research source contents remain data, not operational instructions.


## [2026-10-15] setup | Adopt the LLM wiki procedure

- Operator approved persistent LLM wiki instructions after creating the Obsidian vault in `docs/strategy-wiki/`.
- Added [SCHEMA.md](SCHEMA.md), this append-only log and discovery pointers in the repository entry point and the rule-bot checkout's `AGENTS.md`. The [existing index](README.md) remains the entry point.
- The procedure defines ingest, query and lint; preserves sources, citations, disagreements and evidence classes; and directs lasting findings into linked pages.
- This is a workflow setup, not a content migration. Per-component Markdown pages and a ready-made viewer integration remain separate work. Existing corpus, strategy drafts, generator outputs and Obsidian settings are preserved.
- Validation and independent review: pending completion of this setup task.

## [2026-10-15] navigation | Add a human guide to the whole repository

- Operator additionally requested a wiki that helps humans navigate the repository.
- Added [the repo entry point](REPO_GUIDE.md) and reading routes for [engine/content](REPO_ENGINE.md), [bots/simulation/learning](REPO_BOTS.md), [online/native interfaces](REPO_INTERFACES.md), and [tooling/plans/evidence](REPO_WORKFLOWS.md).
- Sources: workspace `Cargo.toml`, inspected crate entry points and modules, root/web/advisor/bot-agent READMEs, actual CLI/xtask mains, and project plan/navigation documents. The map covers all 17 workspace crates and distinguishes historical status commentary from current source structure.
- The guide describes the inspected rule-bot checkout. Its source links leave the Obsidian vault and are repository/editor references; wiki-to-wiki links remain within the vault. No source files were copied or moved.
- Added a human entry link to the checkout README and persistent wiki navigation links to the generator. Expanded the schema/root routers to keep architectural findings and navigation current.
- Validation and independent review: pending completion below.

## [2026-10-15] validation | Complete workflow setup and repo navigation

- Completes the pending setup/navigation checks above. The guarded full wiki rebuild passed and retained the new human guide/schema/log navigation in the generated index.
- Checked 229 local links/fragments across the seven new procedure/log/navigation pages; all targets resolved. Verified 17/17 Cargo workspace members appear in the repo reading routes and both root instruction entry points name the schema, index, log and human guide.
- Two existing post-rebuild checks passed: graph/catalog/local-link consistency and rendered strategy claim/source ordering. No tests requiring temporary directories were run; no new directories, engine behavior, training or application configuration changed.
- Independent reviewer `/root/wiki_procedure_review` approved after its source-link usability finding was fixed: source code remains outside the Obsidian vault and should be browsed in a repository viewer/editor. Review and checks establish the map's structure and sampled source fidelity, not complete engine qualification.
- Detailed local validation: checkout `out/strategy-wiki/extractions/llm-wiki-workflow-validation.json`. Next coverage work can deepen individual subsystem pages or undertake the separately scoped per-component note migration.

## [2026-10-16] implementation | Begin the wiki-to-MLP production probe

- Operator requested implementation of [MLP_APPLICATIONS](MLP_APPLICATIONS.md). Added initial source and checks for two production subcases, plus opt-in actual-inference diagnostics. [Scope, sources, evidence and remaining work](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md).
- Draft Hacan C02/C21 and Jol-Nar C02/C08 motivate experiments; they have not become gold labels. Probe deliberately excludes full Warfare timing, transport/landing and objective outcome claims. Fixed-seed setup public objectives and own secrets remain visible to the bot; no validation pool is consumed.
- Updated [bots navigation](REPO_BOTS.md). Formatting/whitespace checks passed; 69 local file links across proposal/navigation/log/evidence resolved (anchors not checked). Compilation, inference runs and full validation remain pending Cargo-folder permission. Independent `/root/review_probe` approved static source review after corrections to tracing placement, setup disclosure, fleet details and replay-error retention; no compiled experiment or training gain is claimed.

## [2026-10-16] experiment | Complete the first production diagnostic

- Operator approved subfolders within the worktree. Three new integration tests and109 MLP library tests passed; final release build and checkpoint16448 probe succeeded. [Final scope, commands, results and review](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md).
- Six synthetic variants ×four bot sampling streams ×three branches =72 results, each repeated exactly; 672 actual actor choices retained. Mandatory fleet/capacity removal now completes each continuation; all final excesses zero. Eighteen serial results exactly matched their parallel subset. Original pre-enforcement report is retained and superseded for fleet conclusions.
- Funded Jol-Nar builds a flagship then removes that flagship in2/4 streams; cash-free Jol-Nar takes and keeps two Sarween-discount infantry in4/4 streams. Fleet-excess/headroom facts are emitted and assigned. These conditions motivate reachable-state production/removal investigation, not immediate labels or another duplicate feature. No objective benefit or gameplay-strength change measured; no training or holdout-map use.
- Independent `/root/review_probe` approved source and separately checked raw counts, hashes, enforcement, serial equality and final logs. Source/format/link checks are recorded in evidence. Build/scratch/results stayed within the worktree; existing root libtorch/checkpoint were read-only.

## [2026-10-16] experiment | Sol reachable research and early budget decisions

- Operator chose Sol and authorized execution. Added reachable-game Sol research/secondary-budget probe and analyzer; updated [MLP applications](MLP_APPLICATIONS.md), [bot navigation](REPO_BOTS.md) and [experiment evidence](../../plans/evidence/SOL-STRATEGY-PROBE-2026-10-16.md). Human source hypotheses are Sol SCPT328 C17/C19, not rules or approved teacher targets.
- Research:32 baselines on16 Train origins/diplomacyOFF/ON;32 selected later-round decisions,64 paired continuations,32 exact natural controls. Two baseline agenda guards retained. Paired cutoff VP differences mixed (6 take-favoring,6 skip-favoring,20 ties), with GD-owned versus first-GD cases kept distinct. No round-one research gates were observed; all32 recorded end-Action round1 positions lackedGD.
- Budget follow-up: seven actual round-one Politics/TEConstruction gates, follow/skip once then unchanged policies,7 natural controls. One baseline and one alternate guard retained. No continuation gainedGD in round1; one skip is insufficient here, not evidence for a blanket reservation/skip rule. Technology draft/play timing remains open.
- Current engine DEFAULT/FULL matches the existing pool/checkpoint; initial PoK-only setup refusal is retained and not called strategy evidence. Three final tests pass; source and raw/paired differences independently reviewed by `/root/review_probe`. No training, weight/vocabulary/engine change, holdout use or strength improvement claimed.

## [2026-10-18] maintenance | Refresh generated vault and Sol research routes

- Rebuilt generated faction, source, component, graph and index views from the retained catalog, verified transcripts, maintained summaries and current `rule-bot` content. Output: 33 faction pages, 30 source records, 1,326 components, 1,359 graph nodes and 3,013 connections. The generator's index now links [measured MLP work](MLP_APPLICATIONS.md).
- Updated [MLP applications](MLP_APPLICATIONS.md), [bots navigation](REPO_BOTS.md) and [interfaces navigation](REPO_INTERFACES.md) against the [Sol reachable probe](../../plans/evidence/SOL-STRATEGY-PROBE-2026-10-16.md), [round-one pilot and follow-ups](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md), and inspected rule-bot/reviewer/replayer source. Distillation and PPO pilots are recorded as bounded experiments, not publisher rulings, approved labels or checkpoint promotion.
- Checked the [publisher episode page](https://spacecatspeaceturtles.podbean.com/) on 2026-10-18: episodes 477 and 478 are topical, while 476 Empyrean remains the newest faction guide shown. This was a page check, not an RSS `discover` run; the retained catalog still carries its 2026-10-13 feed snapshot. No new source or transcript folder was created.
- Validation: two focused Python checks passed for catalog/graph/local-link consistency and structured-claim ordering; generated wiki command exited 0. Independent Tier A reviewer `/root/wiki_review` checked the three edited manual pages, their local links, sampled figures and source entry points and found no actionable issue. Link/schema checks do not verify all strategy or rules claims.

## [2026-10-18] validation | Check refreshed wiki fragments

- Checked 9,468 local Markdown fragments across the vault after the final rebuild; all target anchors resolved. The 2026-10-18 maintenance entry above is complete. Independent `/root/wiki_review` also read the new log entry and found its source and validation scope accurate.

## [2026-10-18] depth | Compare bot ideas and measured behavior

- Expanded [bot guide](REPO_BOTS.md) with authored, linear, MLP and rule-bot mechanics; linked source hooks, inference status and dated comparisons. The [technical benchmark guide](../TECHNICAL_WIKI_BENCHMARKS.md) separates gameplay, completion and latency protocols for the shared wiki published on `main` and `DEVniko`.
- Source scope: `rule-bot` `592e5716` for policy code; the October 13 holdout/mechanism reports were untracked in that checkout and enter Git with this publication. `main` `a7975206` supplies self-play and native inference. These pages report earlier measurements and no new gameplay run or checkpoint promotion.
- Publication validation: the prepared Git tree's 157 Markdown pages have 14,238 checked local links and 46 checked pinned repository references, with no missing targets or fragments. Independent Tier A review corrected server-rule-bot attribution, historical manifest scope and Diplomacy-off benchmark wording. No new gameplay or training test was run for this documentation change.
