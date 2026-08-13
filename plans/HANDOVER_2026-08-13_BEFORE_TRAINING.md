# Handover — what is needed before real training can begin

> **Status, 2026-08-13 (later).** Five of the six blockers below are now closed:
> checkpoints and resume (M10-018, M10-020), parallel rollouts (M10-008),
> champion/learner promotion (M10-017), the evaluation harness with error bars
> (M10-015/016), and map variety (M10-002). **Item 6 — what Stage 1 is for — is the
> remaining one, and it is a judgement call rather than a defect.** The sections below
> are kept as written, because the acceptance criteria in them are what the work was
> checked against.

2026-08-13. Branch `wp/m08-007f-public-trade-good-reserves`, head `78fbb71`. Working tree clean.
Workspace green: 18 test binaries, 0 failures, `RUSTFLAGS="-D warnings" cargo clippy --workspace
--all-targets` clean, oracle tree untouched.

---

## Where this stands

**The learning loop is closed and it works.** A blank profile plays, its decisions are credited,
its weights move, and a fresh profile plays better than a blank one on games it never trained on.

Measured after 12 generations × 8 games (six seats, 5.4 s), evaluated on 40 held-out seeds — 240
seat-games:

| per seat | blank | trained |
|---|---|---|
| planets gained | 0.11 | **0.16** |
| victory points | 0.23 | **0.47** |
| scoreable positions held | 0.06 | 0.02 |

Reproduce with `cargo run -p ti4-training --example train --release`.

That is a smoke run, not training. It proves the pipeline carries a gradient end to end. It proves
nothing about whether a policy trained for hours plays well, and the gap between those two
statements is what this document is about.

### What exists

| Piece | Where | Package |
|---|---|---|
| Hashed factual features | `ti4-policy/src/features.rs` | M09-003 |
| Hash + head router + profile | `ti4-policy/src/learned.rs` | M09-001/002/006 |
| Inference, sampling, trajectory | `ti4-policy/src/inference.rs` | M09-004/013 |
| Progress snapshots | `ti4-policy/src/progress.rs` | M09-011/012 |
| Opening bar | `ti4-engine/src/opening.rs` | M09-011 |
| Reward and returns, both stages | `ti4-training/src/reward.rs` | M10-011 |
| Rollouts → episodes | `ti4-training/src/rollout.rs` | M10-012 |
| Centered REINFORCE update | `ti4-training/src/gradient.rs` | M10-013/014 |
| Generation loop | `ti4-training/src/stage1.rs` (`stage2` re-exports it) | M10-027/028 |

---

## Blocking, in the order I would do them

### 1. Checkpoints — a run cannot be saved, resumed, or shipped

`archive.rs` is `todo!()`. Every training run currently dies with the process, which caps training
at whatever fits in one invocation and makes "train overnight" impossible.

Needs: write a `Profile` to disk and read it back; a run manifest (plan, stage, generation index,
seeds consumed, telemetry per generation); resume from a checkpoint and continue seeds where the
last run stopped. `Profile` already round-trips through serde and has a test for it, so this is
plumbing rather than design.

**Acceptance:** a run stopped at generation N and resumed produces the same profile as an
uninterrupted run of 2N — the property that makes a long run trustworthy.

### 2. Parallel rollouts — 32 cores are idle

Currently one game at a time. `Rollout` is `Send` (checked), and each `play` builds its own
`Rc`s internally, so whole rollouts parallelise without touching the policy code — the pattern
`ti4-sim/src/run.rs` already uses with `std::thread::scope`.

Measured: **0.056 s/game** at the four-round horizon, six seats. One million games is 15.6 hours
single-threaded and well under an hour across 32 cores. Determinism must survive: collect results
by seed, not by completion, exactly as `ti4-sim::run` does.

**Acceptance:** the same plan trains the same profile at any worker count.

### 3. Champion/learner separation — self-play against yourself teaches you your own habits

Every seat currently trains from blank simultaneously and all six update together. A policy that
gets better at exploiting the other five copies of itself has no evidence it is better at TI4.

Needs: a frozen champion seated against a learner; promotion only when the learner beats it by a
margin that is not noise. `promotion.rs` is `todo!()`. M10-017 names the shape: stage-specific
acceptance, regression vetoes, champion isolation.

**Acceptance:** promotion refuses a learner that is merely different, on a fixed evaluation set
with a stated confidence.

### 4. An evaluation harness with error bars

The comparison above is hand-rolled in an example and reports point estimates with no variance.
240 seat-games of a noisy quantity is enough to see a doubling and nowhere near enough to see a
5% gain. Every future "is this better" question needs a real answer.

Needs: fixed held-out seed blocks, paired comparison against a named baseline, mean **and**
spread, and a minimum detectable effect stated up front.

**Acceptance:** re-running the same evaluation twice gives the same numbers, and the harness
reports what size of difference it could have detected.

### 5. Map variety — every game is played on one board

`build_board` places Mecatol, filler, and six homes in fixed positions from a deterministic filler
list. Every game in every rollout is the same map. A policy trained on it will learn that map, and
nothing will report that it has.

M10-002 to M10-006 are the map pool packages; `ti4-sim/src/maps.rs` is `todo!()`. Note this was
already the source of one silent defect: homes were seated one tile apart for the whole project
until somebody looked (fixed in `64826b3`).

**Acceptance:** a batch draws maps from a pool by seed, and the same seed draws the same map.

### 6. Decide what Stage 1 is for

Stage 1's signal is sparse from blank weights: 82 of 96 episodes credit every decision alike,
because uniform-random play gains 0.01 planets a seat in round one and nothing clears the opening
bar. It is not *absent* — returns are centered per `(seat, head)` across a whole generation, so the
pooled spread is non-zero and weights do move — but it is thin.

Three options, and this is a judgement call rather than a defect to fix:
- bootstrap Stage 1 from the authored bot's play, so the opening facts have something to vary;
- skip Stage 1 and train at Stage 2, whose shaping moves on nearly every seat;
- densify the Stage-1 potential.

I would train at Stage 2, because it has signal today and needs nothing new. Recorded rather than
decided.

---

## Not blocking, but it shapes what gets learned

- **Action cards: 34 of 122 implemented.** A policy will be trained against a deck it will never
  meet in a full game. Unimplemented cards are *unavailable*, never silently free, so this is a
  fidelity gap rather than a correctness one.
- Relics 5/17, agenda effects 34/63, secret objectives 27/40, reaction windows 65/93.
- Three leaders: `xxchahero` (unimplemented in the oracle too), `jolnaragent` and
  `jolnarcommander` (need the oracle's event-ability path, where an ability sees and modifies the
  triggering event).
- Six faction abilities blocked; 11 reaction windows blocked on combat interrupt points.
- Thunder's Edge: 1,085 oracle lines, no Rust.

Run `cargo run -p ti4-sim --example ledger --release` for the current numbers rather than trusting
this list.

## Also open

- **M00-013, the Python performance baseline, has never been run.** It is the standing "next exact
  action" in `EXECUTION_STATE.md`. Without it there is no answer to whether the Rust port is
  faster than the thing it replaces, which is a stated reason it exists.
- No throughput budget anywhere. Scored play drifted 4.5× slower without anything noticing.
- M11 bridge, M12 legacy replay, M13 cutover: untouched. The CLI prints a version string.

---

## Conventions that are load-bearing here

- **Measure, do not recall.** Four claims in this session were wrong and each was caught by
  running something: an action-card count off by 35×, four leaders described as needing machinery
  the oracle already had, "Stage 1 has no gradient" (it has a sparse one), and a batch of games
  that was secretly one deck dealt a hundred times.
- **Mutation-check every rule.** Break it, watch a *named* test fail, restore it. In this session
  alone that caught: a centering test masked by shared option buckets, an ownership test that
  passed with every seat reading the first seat's trajectory, a "no authored score" test that
  asserted `!shape.is_empty()`, and a sampling test that assumed arithmetic the hashing trick does
  not do.
- **Golden corpora are generated by *calling* the oracle, not by reading it.** Reading it got four
  head-routing entries wrong, every one backwards from its name.
- The Python oracle at `D:\Projects\ti4-engine` is **read-only**. Reads go through
  `PYTHONDONTWRITEBYTECODE=1`; check `git status` in it afterwards.
- `git add -A` sweeps `.worktrees/` gitlinks into a commit. Prefer `git add crates plans`.
