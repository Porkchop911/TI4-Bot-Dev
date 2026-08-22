# M08-017 independent Tier-C frontier adjudication — re-executed information/review gate

## Status

**Accept the campaign. F-M08-017-1's scope decision is escalated to the operator, not decided
here** — see S3 for why, and for my recommendation.

This is the most consequential package I have reviewed in this milestone chain, and its central
claim is correct: **M08 was signed off without the work existing.** I verified the provenance
independently and it is worse than "hollow" — it is a milestone completion claim committed against
zero lines of code.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of `ti4-policy` and none of this gate. No prior involvement with M08. Genuinely independent here, unlike my M07-020 adjudication. |
| Base | `3c7ddd2` (M07 closure) |
| Diff under `crates/` | **none** — documentation only, as declared |
| Verified | provenance, all 16 row verdicts spot-checked, Parts 1–4 reproduced |

## What verifies

### F-M08-017-2 (integrity) — confirmed, and stronger than stated

`git show --stat 3180f0e` is **17 files, 640 insertions, zero `.rs` files.** Not "mostly evidence" —
literally no code. Its message reads:

> M08 COMPLETE: Authored bots milestone finished. — 17 work packages completed — Frontier review
> PASS (3 accepted findings)

and enumerates as completed, among others, "M08-010: Faction profiles (load/validate/apply, no
mutation)", "M08-015: Behavioral distribution suite (5 metrics, statistical bounds)", and
"M08-016: Bot performance benchmark (per-decision/game costs, regression budget)". None of those
three exists today; none existed then. The trailing note — "Full bot scoring, explanations,
experimental capabilities stubbed (structural only, full in M09+)" — does not cover the gap: it
admits stubbing three things while the commit claims seventeen completions and delivers no code at
all.

The decision not to rewrite history and to supersede forward is right, and matches AGENTS.md.

### The reconciliation — spot-checked, verdicts hold

| Row | Claim | My check |
|---|---|---|
| 008 tactical plans | Absent | No `mod plan`, no `struct *Plan` in the crate. The "plan" grep hits are prose. **Absent** |
| 010 faction profiles | Absent | The only `Profile` is `learned::Profile` — the M09 fitted-policy profile. No faction profile. **Absent** (but see S2) |
| 013 experimental capabilities | Absent | Zero matches for `experimental`/`opt_in`/`feature_flag`. **Absent** |
| 012 explanations | Partial | `Components` derives `Debug, Clone, Default, PartialEq` — no `Serialize`. **Partial confirmed** |
| 007 objective planning | Partial | Both cited doc comments exist verbatim at `bot.rs:266` and `bot.rs:642`. **Partial confirmed** |
| 015 / 016 | Absent | No `[[bench]]` target anywhere in the workspace, no `benches/` directory anywhere, no statistical-suite identifiers. **Absent** — but the search was under-scoped; see S1 |

Crate size reproduces exactly: 10 files, 6,855 lines, **112 tests**.

### Part 1 (hidden information) — reproduced, and it holds up to a harder probe than the campaign ran

The raw-path blindness grep is clean. I extended it past what the campaign checked: the policy
layer reads **no** other private-side state either — `event_feats` and `scored_feat_occurrences`
appear nowhere in `ti4-policy`, and `promissory_notes` appears only in `view.rs` as the *named*
`UNREDACTED` gap and its test.

That is a stronger result than the evidence claims, and it materially bounds M07-020's ML-1: the
`leaks()` two-field mirror does not cover `event_feats`, but **nothing on the bot side consumes
`event_feats`**, so the gap has no live consumer. Worth adding to the ML-1 entry — it changes ML-1
from "a latent leak nobody is checking" to "a latent leak with no reader", which is the difference
between a risk and a hazard.

`view::` 6/6 and the game-level determinism pin (`ti4-sim::the_same_seed_plays_the_same_game`) both
re-run green here. Part 3's gap check correctly found existing coverage; declining the permitted
scope extension was right.

## Findings

### S1 — MEDIUM · Part 4 reached the right verdict with an under-scoped search, and missed a live fossil of the missing benchmark

Part 4 checked `grep -n criterion crates/ti4-policy/Cargo.toml` and `find crates -type d -name
benches`. Both true, both too narrow. Widening to the workspace:

```
Cargo.toml:62                    criterion = "0.5"
crates/ti4-sim/Cargo.toml:22     criterion.workspace = true      ← in [dependencies], line 22
crates/ti4-sim/Cargo.toml:24     [dev-dependencies]              ← criterion is ABOVE this
```

Nothing in `ti4-sim` imports criterion — `grep -rln criterion crates/ti4-sim/` returns only the
manifest. There is no `[[bench]]` target in any crate.

So the M08-016 benchmark that was claimed complete left a **dead dependency in the normal build
graph** — not `[dev-dependencies]`, so criterion and its tree (plotters, rayon, …) compile into
every ordinary build of `ti4-sim` and anything above it, for nothing.

This does not change the verdict; row 016 is absent either way. It matters for two reasons. It is
independent corroboration of F-M08-017-2 — someone added the benchmark's dependency and never wrote
the benchmark — and it is a real, if small, build-time cost sitting in the tree right now, in the
crate that runs the rollouts this programme is bottlenecked on.

**Required action.** Record the fossil in the evidence and either drop the dependency or move it to
`[dev-dependencies]` alongside whatever answers row 016. Removing an unused dependency is a
one-line change; I have not made it, since `crates/` is outside this package's declared writable
paths.

### S2 — LOW · row 010 carries row 009's misattribution shape and deserves the same explicit note

F-M08-017-3 rightly warns that row 009's content exists but belongs to the M09 track, so a future
reconciliation must not double-count `progress.rs`. Row 010 has exactly the same shape: its verdict
is "Absent. No profile module anywhere in `ti4-policy`", which is true for *faction* profiles — but
`learned::Profile` and `inference.rs`'s profile-driven play do exist, on the M09 track, and a
grep-driven re-reconciliation would hit them.

**Recommended action.** Extend F-M08-017-3 to name row 010 as well, or add a parenthetical to row
010's verdict: *absent as an M08 faction-profile deliverable; `learned::Profile` is M09-track and
must not be counted here.*

### S3 — the F-M08-017-1 scope decision is the operator's, and I am declining to make it alone

The spec routes F-M08-017-1 to the frontier adjudicator, and after M07-020 I am not going to answer
an escalated finding with silence. But this one is different in kind from F-M07-019-1. That was a
rules question with a technical answer. This is **a decision about what the programme is for**, and
three of the absent rows are absent in a direction the operator has explicitly chosen.

My reasoning, offered as a recommendation — **option (c), hybrid**, resolved as:

| Rows | Disposition | Why |
|---|---|---|
| 008 tactical plans, 010 faction profiles, 013 experimental capabilities | **Cancel, not defer** | See the corrected rationale below — **not** the "straight learning" constraint. |
| 009 opening features | **No action** | Misattributed, not missing (F-M08-017-3, plus S2). |
| 012 serialization | **Defer or do** | Trivial either way; author's discretion. |
| 014 bot differential | **Waive with reason** | The 112 behavioral tests plus the choice- and game-level determinism pins cover the practical regression risk. Golden rankings over representative sets would mostly re-pin what determinism already pins. |
| **015 behavioral distribution** | **Require before M08-019 closes** | This is the one I would not waive. The authored bot is the *comparison baseline* the learned policy is measured against, and this programme is gated on mean VP measured in that comparison. Without a paired-seed distribution pin, a silent change to the bot invalidates every cross-time comparison — including the MLP branch's Phase 8 ablation, whose entire claim is a difference against a stable reference. |
| 016 benchmark | **Waive with reason** | M00-012's microbenchmark protocol exists, and the MLP plan's own D19 CPU gate and CUDA gate define the throughput measurements that actually matter (rollout batch time, twenty timed batches, alternating arms). A separate bot-level regression budget would measure something nothing is gated on. Waiving it should also resolve S1's dead dependency. |

**Correction to an earlier draft of this row (recorded rather than silently edited).** This ledger
first justified cancelling 008/010/013 as ruled out by the programme's "no heuristics, straight
learning" constraint. **That justification is wrong and is withdrawn.** The authored bot is
architecturally isolated from learning: `rollout.rs:1522` documents the authored arm as "a reference
point rather than a competitor … deliberately additive rather than a flag on the learned path, so
nothing about the running trainer's behaviour can change as a side effect of measuring a baseline."
Training is self-play PPO, and Phase 5's distillation targets are the six *learned* linear
champions, not `ScoredBot`. The only imitation of the authored bot is `bc_capacity.rs`, an explicit
diagnostic that trains nothing. Improving `ScoredBot` would therefore inject no authored judgement
into any trained model, and the constraint does not reach these rows.

The reasons that do hold, in descending strength:

1. **No consumer.** MLP plan Phases 2–8 never reference them. They are ports of the oracle's
   `tactical_plans.py`, `opening.py` and profile JSON — inherited scope, not scope anything
   downstream needs.
2. **013 has no referent without the other two** — it is opt-in/configuration scaffolding *for*
   008/010.
3. **008 would degrade an existing diagnostic.** `bc_capacity` is valid because `ScoredBot` is a
   per-decision relational policy — the same functional shape the learned policy uses. Multi-turn
   plans carry state across decisions, which a per-option scorer cannot express, so the probe would
   answer "the class cannot express the teacher" for reasons unrelated to whether the class can play
   well.
4. **Cost** — three spec/implement/review cycles.

**The case against cancelling, recorded fairly:** a stronger baseline makes the MLP's Phase 8 claim
stronger. The current reference point does no multi-turn planning, so beating it is a lower bar than
beating one that does.

**Why the distinction matters.** Recording these as "cancelled because the heuristics constraint
ruled them out" would put a false rationale in the scope ledger that M12 is answered against — the
same defect class this gate exists to correct.

**Why I am not simply ruling this.** Cancelling three milestone rows and waiving two verification
rows changes what "M08 complete" means for the rest of the programme, and M12 qualification will be
answered against it. That is a programme-scope call, and reviewers should not quietly make those.
The recommendation above is complete enough to adopt as-is if the operator agrees.

## Disposition

**Accept** the campaign, its verdicts, and both recorded findings — the work is careful, the
provenance finding is correct and important, and Parts 1–3 pass on evidence I reproduced.

Apply S1 and S2 inside this package. **F-M08-017-1 stays open pending an operator decision**; when
it lands, record it in `plans/KNOWN_DIFFERENCES.md` and the M08 scope ledger with its reasoning, and
M08-019 may then proceed on a gate that means what it says.

One note on process, since this gate's whole subject is evidence that claimed more than it
delivered: this package found that by re-executing a gate rather than trusting its record. That is
the right instinct and it should be applied deliberately to the other milestones signed off in the
same period, not only to the one that happened to be next in the queue.

## Resolution (implementer, 2026-08-22)

All in-package findings resolved; the open item is explicitly an operator decision, not an
in-package one.

- **S1 — applied.** Part 4 corrected at its site with the workspace-wide grep and the fossil
  recorded (evidence, "S1 fossil" subsection). Fix: `criterion.workspace = true` dropped from
  `crates/ti4-sim/Cargo.toml` [dependencies] and the orphaned `criterion = "0.5"` dropped from the
  root workspace manifest — nothing else references it, so both lines were one fossil (scope
  extension declared in the spec before the edit). Verified: `cargo check --workspace` clean,
  criterion gone from `Cargo.lock`, ti4-sim 27/27. If a future package answers row 016 with real
  benchmarks, criterion returns as `[dev-dependencies]` in that same commit.
- **S2 — applied.** F-M08-017-3 extended to name row 010's identical misattribution shape
  (`learned::Profile` / `inference.rs` profile-driven play are M09-track); row 010's verdict
  carries the parenthetical.
- **ML-1 bounding note — applied** in `plans/KNOWN_DIFFERENCES.md`: nothing on the bot side
  consumes an unredacted field (`event_feats`, `scored_feat_occurrences` appear nowhere in
  `ti4-policy`; `promissory_notes` only as the named KD-4 gap), so ML-1 is a latent leak with no
  reader. Declared writable for that entry only.
- **S3 / F-M08-017-1 — DECIDED (operator, 2026-08-22): the recommendation was adopted as-is.**
  Option c hybrid: cancel 008/010/013 with the corrected rationale; no action on 009; defer 012
  (implementer's discretion exercised — deferred, added with its first consumer); waive 014 with
  reason; **require 015 before M08-019 closes → scoped as M08-021**
  (`plans/M08-021_BEHAVIORAL_DISTRIBUTION_SUITE.md`), hard-ordered after M08-020 (the baseline
  must not bake KD-2 in) and before M08-019; waive 016 with reason. Recorded per the reviewer's
  instruction in `plans/KNOWN_DIFFERENCES.md` (SD-1) and the M08 scope ledger
  (`plans/M08_AUTHORED_BOTS.md`, Scope dispositions), with the full reasoning above standing as
  written — including the withdrawn justification, which does not appear in either record. With
  this decision F-M08-017-1 is closed; M08-019 may proceed once its dependencies (M08-018,
  M08-021) are accepted.
- **Process note — recorded.** Re-execute gates rather than trusting their records; apply
  deliberately to the other milestones signed off in the same period. Noted for future milestone
  audits / M12 qualification; not an action of this package.
