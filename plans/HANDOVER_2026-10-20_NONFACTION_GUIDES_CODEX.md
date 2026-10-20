# Handover 2026-10-20 — transcribe and summarise the non-faction SCPT guides (for Codex)

```text
Objective and normative sources:
  Transcribe and summarise the SCPT episodes that are not faction guides but carry strategy the
  per-faction reward work needs. First consumer: plans/FACTION_REWARD_CONDITIONS_2026-10-20.md
  §5 Q1 — breakthrough advice for the factions whose only guide is a PoK-era "Super" guide.
  Sources: the SCPT RSS feed (https://spacecatspeaceturtles.podbean.com/feed/), the existing
  pipeline tools/scpt_strategy.py, docs/strategy-wiki/PIPELINE.md, SUMMARY_CONTRACT.md,
  FACTION_SUMMARY_WORK_ORDER.md (method rules apply to these episodes too).

Active workstream/package and acceptance status:
  Not started. Acceptance below.

Branch and HEAD; dirty paths and owners:
  Written on claude/faction-model-rewards-fac690 (worktree .claude/worktrees/faction-model-rewards-fac690).
  The operator assigns Codex its own checkout/branch; do not work in another session's tree.

Checks actually run and exact results; evidence:
  None for this package. Feed listing pulled 2026-10-20 (titles below).

Decisions, review debt, blockers, and limitations:
  - The pipeline today selects only the newest faction guide per faction (`discover`). Non-faction
    episodes need a selection path: an explicit episode list (preferred: no title heuristics),
    stored as its own catalog entry kind so faction pages are untouched.
  - Raw audio/transcripts stay in ignored out/strategy-wiki/; append new transcription versions,
    never overwrite. No new folders without asking the operator (AGENTS.md).
  - GPU: read plans/COMPUTE_QUEUE.md first and record the job there. Whisper on GPU was
    operator-authorised 2026-10-14 (PIPELINE.md); re-check VRAM and the queue before each batch.
    CPU fallback: up to 16 workers per AGENTS.md, record the worker count.
  - Twilight's Fall episodes are out of scope (different game mode, not in the engine).
  - Summaries are drafts; podcast opinion is not rules. Keep disagreements and conditions.

Next exact action/command; files to read first:
  Read AGENTS.md, plans/COMPUTE_QUEUE.md, docs/strategy-wiki/PIPELINE.md, SUMMARY_CONTRACT.md,
  tools/scpt_strategy.py (discover/transcribe), then the reward doc above.
```

## Episodes (priority order)

| Pri | Ep | Title | Why |
|---|---|---|---|
| 1 | **428** | Breakthroughs and Synergies RANKED (2025-10-29) | Fills §5 Q1: per-faction breakthrough value and synergy |
| 1 | 437 | 2025 Faction Tier List | Only partial notes exist (`crates/ti4-server/docs/podcast-transcripts/notes/437.md`); full transcript |
| 1 | 462 | Ultimate Guide to Scoring and Tempo | Partial notes only; horizon/tempo rules for the reward |
| 1 | 477 | Ultimate Guide to Winslaying and Point-Blocking | Partial notes only (0:19–1:22) |
| 2 | 455, 459 | Every Generic Technology Ranked (1, 2) | Tech-term weights (`tech:X@R`) |
| 2 | 453 | Ultimate Guide to Exploration | `explore`, relic/fragment terms |
| 2 | 447 | Every Relic RANKED | relic terms (Mahact, Naaz) |
| 2 | 443 | Coexistence Overview and Rules Questions | Deepwrought / Titans breakthrough terms |
| 2 | 439 | Thunder's Edge Planets and System Tiles | Fracture/Styx/legendary terms |
| 3 | 425 | Thunder's Edge Rulebook Analysis | Breakthrough unlock (expedition) rules |
| 3 | 460 | Top Ten Thunder's Edge Rules Questions | Rules caveats |
| 3 | 432 | Thunder's Edge New Action Cards Overview | Context |
| 3 | 465, 470, 474, 478 | Every Action Card Ranked (A–R) | Yssaril / Ral Nel `ac` terms |
| 3 | 451 | How to play Twilight Imperium in 2026 | General TE meta |
| 3 | 457 | Prelims Tournament Statistical Analysis | Descriptive faction data (not causal) |
| 3 | 399, 394 | Super Guide to the Strategy / Agenda Phase | Partial notes exist; full transcripts |
| 3 | 404, 408 | Playing against Yssaril / Mahact | Counter-play context |

## Deliverables and acceptance

1. Transcripts for every priority-1 and priority-2 episode, complete and checksum-verified, same
   provenance header as the faction guides. Priority 3 if time allows; list what was skipped.
2. One summary per episode (`docs/strategy-wiki/summaries/scpt-<ep>-<topic>-summary.md` plus
   JSON if the contract's validator can take a non-faction file; if it cannot, propose the schema
   change to the operator instead of bending the faction schema).
3. **For 428 specifically:** a table, one row per engine faction (33 ids, Keleres/Firmament
   shared as in the wiki), columns: breakthrough, hosts' rank/tier, what makes it pay off, unlock
   advice (which expedition cost, how early), conflicts, citation (timestamp + segment ids). Mark
   factions the episode does not discuss. This table is what the reward doc's §5 Q1 consumes.
4. Wiki rebuild (`tools/scpt_strategy.py wiki`) leaves faction pages unchanged except added links.
5. Evidence file `plans/evidence/NONFACTION-GUIDES-2026-10-xx.md`: commands, worker/device counts,
   durations, transcript checksums, skipped episodes, any pipeline code change and its review.
6. Pipeline code changes are Tier B (independent review by a different agent before commit).

Not in scope: changing reward code, training, or the faction summaries themselves.
