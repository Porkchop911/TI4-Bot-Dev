# Request for evaluation: is this approach capable of competent play?

Date 2026-08-18. Written for an outside reader. Concepts and evidence, not code.

---

## 1. What the system is

A Twilight Imperium 4 engine (Rust, ported from a Python reference that acts as a correctness
oracle) plus a self-play learner.

**The policy.** Every decision the game asks is routed to one of ~14 **heads** — `activation`,
`movement`, `landing`, `production`, `strategy` (the card draft), `secondary`, `turn`, `payment`,
and so on. Each head owns a weight vector. A decision is scored by extracting a sparse named
feature vector per *legal option*, taking a dot product with the head's weights, and sampling from
a softmax over the resulting scores.

Three properties matter for what follows:

- **It is linear.** The score is a weighted sum of features. It cannot form a product of two
  features unless that product is supplied as a feature.
- **Features are hand-specified and named**, e.g. `target:resources`, `target:own-distance`,
  `option:carrier`, `state-option:yes:trade_goods`. Weights are keyed by name, so checkpoints are
  portable and human-readable. About 47,000 weights per faction.
- **There are six independent policies**, one per faction, trained simultaneously in the same
  games. They are opponents to each other.

**The training.** Self-play policy gradient. Each update plays a batch of 96 games (16 map seeds ×
6 seat rotations), computes an undiscounted suffix-sum return per decision, centres it against a
batch baseline, and steps. Recently also PPO (clipped surrogate, 4 epochs per retained batch).

**The curriculum.** Two stages:

- **Stage 1** — a one-round "opening". Objective: a bounded per-seat *clearance* flag, awarded for
  gaining 3 planets, holding 3 systems, and gaining 1 unit.
- **Stage 2** — a four-round horizon. Objective: victory points.

Stage 2 resumes from Stage-1 weights.

## 2. Where it actually is

| | |
|---|---|
| Stage-1 clearance, held-out boards | **0.83** |
| Stage-2 victory points per seat, 4 rounds | **2.1 – 2.3** |
| Previous champion (same measurement) | 1.98 |
| Best artefact ever produced by this project — a Python heuristic evolved over a hand-written evaluator | **2.90** |

**A competent human reaches 8–10 VP by round 4, and 8 is not unusual.**

So the learner is at roughly a quarter of competent play, and *the best thing this project has ever
produced, by any method, is at about a third.* That gap — not the difference between arms — is the
subject of this request.

Supporting detail: across 3,600 seats, 3.2% score nothing, 40.2% score exactly 2, and 6.4% reach 4.
VP accrues near-linearly at about half a point per round. Public objectives are *scoreable* on only
0.06–0.28 of decisions and secret objectives on 0.00–0.01 — so seats take whatever falls out of
ordinary expansion and essentially never construct an objective's preconditions.

## 3. What has been established, so the evaluator can skip re-deriving it

These are measured, most with disjoint across-seed ranges on held-out panels.

**On the optimiser**

- PPO is **3.3–4.0× more sample-efficient than REINFORCE** when training from blank weights, and it
  is the only configuration that passes 0.75 clearance at Stage 1.
- The same PPO arm is a **measured null (−0.02 VP)** when resuming an already-converged policy.
  Reconciled as: optimiser choice matters where the policy is far from converged, not when
  polishing.
- A **round-bucketed baseline** — centring each round's returns against that round's own mean
  rather than one mean per head — is worth **+1.29 table VP**, the largest single learning result
  obtained. Rationale: a suffix-sum return is systematically larger early in an episode than late,
  and one mean per head leaves that trend in the advantage and treats it as signal.
- Discounting the return (γ=0.97) is **harmful** at Stage 2 (−0.68 VP) and neutral at Stage 1.
- Raising the learning rate 4× is indistinguishable from baseline. **Step size is not the
  constraint.**

**On the representation** — this is where most of the found defects live

- In a linear softmax, a feature carrying **the same name and value on every option of a choice is
  provably inert**: it adds one constant to every logit and cancels. Consequence: for a *binary*
  decision, no feature describing the game state can ever influence the choice, because the state
  is identical under both options. Until recently, "should I use this strategy card's secondary?"
  was answered from the card's identity alone — eight distinct inputs, eight fixed probabilities,
  no reference to whether the seat could afford it.
- Fixed by crossing state with option identity (`state-option:yes:trade_goods`), raising the
  head's distinguishable situations from 8 to 808. State now reaches 98% of decisions, up from 84%.
- **Interactions must be hand-supplied.** The activation head ranked systems identically whether the
  seat had one command token or five, because a linear model cannot form `tokens × distance`. Three
  bounded interaction features were added by hand. There is no mechanism by which the learner could
  have discovered these itself.
- A guard against memorising board identities turned out to be **accidental**: option ids were
  filtered for being all-digits, which caught system ids (numeric) and missed planet names
  (alphabetic). Every planet's name was a feature on payment decisions.

**On the experimental setup itself** — three separate confounds were found and fixed *in this
session*, which is itself evidence about how much of the earlier record is trustworthy

- Faction-to-seat assignment was a **cyclic rotation**, so the offset between any two factions never
  changed. Draft precedence between a given pair was fixed at 16.7%–83.3% (fair for only 6 of 30
  ordered pairs), and **board neighbours never varied** — one faction bordered the same two
  factions in every game ever played. Now randomised per seed.
- The "held-out" evaluation panel shared **all** of its board arrangements with training. The map
  pool advertises 8,192 arrangements but contains only 2,222 distinct boards. Now partitioned by
  distinct board.
- Per-faction results were confounded by the above: one faction's apparent weakness was traced to
  losing a specific strategy-card draft in exactly the two seats where the fixed rotation made it
  draft late, with a worthless fallback card.

**On self-play structure**

- **Profiles do not transfer between tables.** Taking each faction's best profile across seeds and
  combining them measured *worse* (0.816) than simply taking the best single seed intact (0.821).
  Every faction dropped when moved into a table it had not trained against. A profile's score is
  earned against its own five opponents.
- **Seed variance exceeds the effects being measured.** From *identical zero starting weights*, and
  after all runs had seen the identical set of training boards, one faction's clearance landed at
  0.34 / 0.53 / 0.98 across three seeds. The three runs had converged on three different strategy
  cards. Nothing distinguished them but the order games arrived in.

## 4. The question, stated plainly

**Is a linear policy over hand-specified sparse features, trained by self-play policy gradient on a
scalar episode return, capable of competent play in a game of this complexity — and if not, what is
the binding constraint?**

The specific worry is that the last two months of work have been *correctly executed* optimisation
of a formulation that cannot reach the target, and that every result above is a local improvement
inside a box whose ceiling is far below the goal.

## 5. Candidate explanations, in the order the author currently believes them

**(a) The environment may not permit the target.** This should be checked first because it would
invalidate the entire framing. A competent human scores 8 VP by round 4 in *real* TI4. It has not
been verified that this engine can. Objectives are awarded through a scoring path; agenda effects
and some action cards also grant VP; but a search did not find a victory point tied to removing the
custodians from Mecatol Rex, which is a standard early source. If the simulated game's scoring
surface is narrower than the real game's, then 2.9 VP may be near *the environment's* ceiling and
the learner is not the problem. **Every other question below is conditional on this one.**

**(b) The reward is too sparse and too distal for the behaviour required.** Scoring an objective
requires constructing preconditions over several rounds. The signal is a scalar at the end of a
~250-decision episode. Objectives are scoreable on 6–28% of decisions and secrets on ~1%, so the
learner rarely even observes the event it is supposed to be steering toward. Nothing in the feature
set describes *what a revealed objective requires*, so a seat cannot represent "this action moves me
toward that objective" even in principle.

**(c) The policy class cannot express the strategy.** Every useful interaction has had to be
hand-supplied. The three found so far were found by inspecting failures one at a time. This does not
scale, and it means the representation encodes the designer's hypotheses rather than learning them.

**(d) The curriculum's proxy is weakly coupled to the real objective.** Stage 1 optimises clearance.
Measured: faction VP is nearly flat (2.00–2.30) while faction clearance spans 0.67–0.92. A large
Stage-1 gain buys little Stage-2 gain. Stage 1 may be teaching land-grabbing at the expense of
objective play.

**(e) Self-play with six independent co-adapting policies is the wrong structure.** Opponents are
non-stationary, scores do not transfer between tables, and the six policies collectively find
degenerate equilibria — at one point they partitioned the six strategy cards between them and
committed to one each with ≥93% probability, which is coordination rather than play.

**(f) The optimisation landscape is too rough for the seed budget.** See the 0.34/0.53/0.98 result.
Three seeds cannot distinguish a real effect from basin luck, and most conclusions in the record
were drawn from three.

## 6. What would be most useful from an evaluator

1. **Sanity-check (a) first.** Is the target achievable in this environment at all? What is the
   cheapest way to establish an upper bound on VP at a four-round horizon, independent of any policy
   — e.g. a search or an oracle that plays only for VP?
2. **Is the linear-over-named-features policy class defensible here**, or is a learned
   representation (even a small MLP over the same inputs) a precondition rather than an
   optimisation? The counter-argument is that named features keep checkpoints auditable and preserve
   parity with a Python oracle; how much is that worth giving up?
3. **How should objective-directed behaviour be learned without hand-authored heuristics?** A hard
   project constraint forbids heuristic teachers or authored evaluators as training targets. Options
   considered but not tried: features describing revealed objectives' requirements and the seat's
   distance from them; a learned value function; search-based targets (expert iteration).
4. **Is the two-stage curriculum sound**, given the measured weak coupling? Alternatives: train
   directly on VP at a longer horizon, or replace clearance with a denser objective-linked proxy.
5. **Is six independent per-faction policies right**, versus a single shared policy conditioned on
   faction? Sharing would multiply effective data by six and remove the transfer problem, at the
   cost of faction-specific strategy.
6. **How should the seed-variance problem be handled** — more seeds, variance reduction, population
   methods, or is high seed variance itself a symptom of (c)?

## 7. Constraints any proposal must respect

- **No hand-written heuristic evaluators or teachers** as training targets. This rules out
  behaviour-cloning from the existing Python heuristic champion.
- **CPU only.** 32 logical cores. GPU was investigated; the workload is allocation-bound rather than
  compute-bound and `target-cpu=native` bought 1%.
- **Simulation dominates cost.** Roughly 90% of compute is playing games, ~10% is the policy update.
  Stage-1 games run at 282/s, Stage-2 games at 67/s.
- **Parity with the Python oracle** is maintained for engine behaviour and is a correctness gate;
  the learner is free, the rules are not.
- Checkpoints are name-keyed JSON and are expected to stay human-readable and portable.
