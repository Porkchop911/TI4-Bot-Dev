# Astra response — diplomacy, PPO throughput, and playing for points

2026-09-16. Reviewed the request, handovers, current dirty source on `codex/diplomacy-v1`
at `85d01517a196315a5062b706199513c7570f15c0`, reward/optimizer implementation, and prior
performance evidence. This is an advisory review, not the outstanding full Tier-C integration
review. No engine code, training process, checkpoint, or Git history was changed. No new games,
benchmarks, or tests were run; timing and behavioral results below come from the request.

Operator clarification: training and evaluation remain bounded to four rounds. Maximizing VP by
the end of round 4 is the intended playing-to-win objective. The recommendations below incorporate
that constraint; the earlier proposal to extend the horizon is withdrawn.

My order would be: repair the measurements, remove demonstrably useless contact entry, run a small
VP-focused experiment against frozen opponents, then optimize whichever rollout component actually
dominates. Keep typed bundles and promises. Defer new promise families and inference architecture
changes until those experiments have results.

1. **Correct four premises before spending another overnight run.**

   - **Rollouts already run in parallel.** `ppo_update.rs:1497–1557` creates worker-local actors and
     uses Rayon with a shared job cursor, then restores deterministic job order. The completion
     message at line 1900 saying “sequential” is stale. CPU inference does not imply sequential
     games. Verify actual worker count and binary provenance rather than budgeting a new
     parallelization project. Cross-game *batched inference* would still be a separate change.
   - **86% no-offer contacts does not mean 86% had no available offer.** Separate zero bundles and
     zero signals, signals only, and nonempty menus where the policy chose “make no offer.” Only
     the first category is an unambiguous dead menu. The measurement is also one round at
     temperature 0.5; it does not establish its frequency in four-round, temperature-2.5 training.
   - **The reported win rate is strict VP leadership at the horizon.** `crossplay_eval.rs:428–433`
     uses candidate VP minus the best opponent and counts `margin > 0`. Ties lose this metric;
     unfinished games are included. The negative self-play margin and sub-1/6 “win” null are
     therefore unsurprising. Keep the metric, label it accurately, and report any actual engine
     wins reached within four rounds separately. VP at round 4 remains the primary objective.
   - **The run does not demonstrate reward hacking by a particular set of flags.** Its flags are
     unauthenticated. “No demonstrated improvement on this diplomacy-off benchmark” is supported;
     “shaping caused the failure” and “diplomacy learning failed” are not established.

   Use the launcher for future runs, and save the trainer's resolved reward/settings object as well
   as requested arguments, binary/source hashes, pool and starting-bundle hashes. The example
   launcher config currently still contains `styx-bonus = 16`; copying it unchanged repeats that
   intended setting rather than providing a neutral control.

   The final-minus-start changes are −0.143 VP, −0.108 margin, and 0.0 percentage points of reported
   wins. Final-minus-null is −0.061 VP, −0.018 margin, and +2.8 points of strict leadership. Report
   paired uncertainty before interpreting these differences. Resample seed blocks with their
   rotations and candidate seats together; those observations are related. Also repeat promising
   training arms with independent training seeds. Reserve a final test set after selecting on
   validation. This follows the uncertainty concern documented by
   [Agarwal et al.](https://arxiv.org/abs/2108.13264); the particular blocking scheme is my
   recommendation for this harness.

2. **Keep the negotiation state machine; make entry content-aware.**

   The useful primitive is a bilateral offer containing immediate transfers and optional typed
   future obligations, followed by accept/counter/decline. A bounded contact is a reasonable
   implementation of that. There is no need to throw away settlement, replay, or the two-counter
   machinery because entry is wasteful.

   First measure and then suppress contacts with no admissible bundle, relevant signal, or
   negotiation-opening opportunity. Preserve signal-only contacts. Do not filter by the current
   policy's estimated attractiveness: that would remove its opportunity to learn new deals.
   If most empties are voluntary declines of nonempty menus, expose bounded factual summaries of
   available deal types at entry. If those still fail, experiment with choosing partner plus first
   proposal in one bounded menu. Avoid flattening every partner's full menu into the turn action
   space; it would create a new option-count problem.

   “Probing” is not a strong reason to preserve the current empty path. In
   `diplomacy/window.rs:230–233`, “make no offer” closes the window without a negotiation response
   from the other seat. It does not ask whether that seat would accept something. However, opening
   is **not generally side-effect-free**: `game.rs:2538–2585` emits `TRANSACTION_OPENED`, permits
   timing effects before generating bundles, and consumes the transaction allowance. Black Market
   can widen the menu. A prefilter must preserve those opportunities and derive availability
   without leaking another seat's private cards. This is a legality change needing focused review,
   not just deleting a redundant inference call.

   Instrument offered contacts → opened contacts → menu categories → proposals → responses →
   accepted deals → delivered benefits. Break it down by round, pair, temperature, and template.
   Include nested timing decisions and actual CPU time. Measure the gate on the real training
   workload before predicting a saving. The existing counts do not support “half the update is
   inside contacts” or “86% of the surcharge is removable”: diplomacy can also change subsequent
   play and game length.

3. **Give stage 2 an objective that matches promotion.**

   For the immediate question “can this policy beat five frozen benchmark seats?”, train that
   matchup directly in a bounded pilot. Rotate the learner seat and faction, freeze opponent
   weights, and put only learner-seat decisions into PPO. Mixing frozen opponent trajectories
   into the learner's ordinary on-policy batch would be wrong. Use matched seeds and a comparable
   learner-data budget; one learner seat per game produces much less trainable data than six-seat
   self-play. Once progress is demonstrated, include several archived opponents/self-play to
   reduce specialization to one benchmark.

   Start with a transparent control: VP weight 1, other shaping and penalties explicitly zero.
   Keep clearance and waste as reported diagnostics. Compare it with one arm retaining a modest
   waste penalty, rather than changing every coefficient at once. If the intended waste penalty
   of 10 actually reached the old run, one waste event cost roughly nine VP at weight 1.1—already
   more than typical four-round scoring. That deserves at least as much attention as Styx.

   Optimize own VP by the end of round 4 as the primary objective. Use paired margin and strict
   VP leadership as secondary competitive checks against the frozen benchmark. Margin alone
   can improve by suppressing opponents without raising the learner's VP, so it must not replace
   the stated objective. Select for a credible paired improvement in round-4 VP and report the
   competitive checks alongside it.

   **Set Styx bonus to zero in the production-objective control.** Likewise zero terminal Fracture
   holding bonuses. Their ordinary points and resources already have consequences. If exploratory
   curriculum reward is necessary, try a once-only entry bonus of 0.1 VP-equivalent, perhaps 0.3 as
   an upper pilot arm, then anneal it to zero. Those are experimental bounds, not validated
   coefficients. Test retention of useful behavior with all such bonuses off. A bonus of 16 is
   incompatible with treating one ordinary VP as approximately one unit of value.

   There is an additional mathematical distinction: `reward.rs:269–286,351–375` differences a
   potential containing VP, scoreable objectives, fleet, and technology. At default gamma 1 this
   telescopes to the final potential minus the initial potential, so unspent fleet and unscored
   opportunities **remain rewarded at the cutoff**. Calling that “potential shaping” does not
   make it invariant to the true VP objective. Policy-invariant shaping uses
   `gamma * Phi(next) - Phi(current)` with appropriate terminal boundary treatment; finite-horizon
   residual potentials matter. See [Ng, Harada and Russell](https://ai.stanford.edu/~ang/papers/shaping-icml99.pdf).
   Do not change gamma casually: with a per-decision clock, extra diplomacy choices would then
   change how heavily later game outcomes are discounted.

4. **Profile before batching; keep the present PPO batch settings initially.**

   Derived from the supplied timings, diplomacy-on processes about 9,376 decisions per rollout
   second versus 9,587 without it. Aggregate throughput differs by only about 2.2%; decision volume
   is the strongest first explanation for the doubled rollout time. These are whole-worker-pool
   rates, not per-decision CPU latency or proof that every decision costs the same.

   Use the existing `--diag <file>` instrumentation. It already records worker-level features,
   vocabulary, actor forward, critic features/forward, and record time, plus assembly and optimizer
   phases. Add attribution by decision kind where necessary. Serialized bytes are not a CPU
   profile: `window.rs:394–402` creates a JSON value for each bundle-bearing option, and
   `features.rs:1542–1546` deserializes it into a typed bundle. Repeated allocations, feature work,
   candidate construction, and journal/state copying remain plausible costs even with +7% bytes.
   Prior performance evidence also warns that the checkpointed journal is unbounded; a short
   random probe does not bound learned-policy memory use over four-round training games.

   Batch pending decisions **across independent games**, if forward time warrants it. Decisions
   from six seats in one game are generally causally dependent. Current `Decider::choose_seeing`
   calls are synchronous, including nested asks: a batching service needs bounded requests and
   partial-batch flushing, or a resumable engine interface. Waiting for a full batch when every
   worker is blocked would deadlock. Keep one frozen actor per rollout phase, per-seat RNG streams,
   stable option order, and deterministic result assembly. CPU batching is compatible with CPU
   inference; GPU inference would change the current specification. Prior batching experiments
   already failed bitwise logits, so revisit that evidence before expecting equivalence.

   Four epochs and minibatch 4096 are a reasonable baseline, not an established optimum. They
   produce 232 Adam steps for 237,220 decisions versus 120 for 119,832. Doubling decision count
   therefore also changes the number of updates to shared parameters, the decision-type mix,
   and the global advantage normalization—not merely elapsed time. This implementation uses
   Monte Carlo returns minus the rollout critic, not GAE. Gamma defaults to 1; there is no current
   exponential credit decay caused by extra contacts. More contact decisions can still dilute
   strategic examples and change shared-trunk learning. Inspect per-head sample counts, advantage
   distributions, critic error/explained variance, clipping, and finite log probabilities.

   PPO supports multiple minibatch epochs, but its paper does not prescribe the right count for
   this game: [Schulman et al.](https://arxiv.org/abs/1707.06347). After the rollout work, compare
   two versus four epochs by held-out improvement per hour. Do not treat `EpochStats::kl` as a
   true KL divergence: `ppo.rs:537` accumulates mean absolute sampled log-ratio. Add a clearly
   defined KL diagnostic before using a KL stopping rule; investigate the historical infinite
   log-ratio instead of treating it as harmless telemetry.

   Even eliminating the reported optimizer time entirely caps improvement at 33.8/25.3 = 1.34x.
   Halving rollout time gives about 1.60x; halving optimizer time gives about 1.14x, all under the
   simplifying assumption that the other component is unchanged. The printed “total” omits some
   update work, including freeze and reporting. Use diagnostic `wall_s` and whole-process elapsed
   time for claims. Use synchronized CUDA timings for attribution separately from throughput.

   Keep 96 games initially. It is 16 seed groups with six rotations, not 96 independent opening
   draws. Compare future batch sizes using games, decisions, learner decisions, Adam steps, and
   wall time together. A fixed decision budget can stabilize batch size, but arbitrary truncation
   needs explicit return/bootstrap semantics; it must not silently manufacture terminal states.

5. **Measure whether the Fracture helps score by round 4 before paying for visits.**

   Keep the four-round bound for training and evaluation. The policy should learn to turn
   opportunities into points within that bound. This PPO return has no continuation bootstrap
   beyond the cutoff: later commodity payments or delayed investments have no direct
   post-cutoff payoff under this objective. Count promises due after round 4 and time remaining
   after first feasible entry. A deal that delivers goods now for repayment after the evaluation
   ends can exploit the bounded objective; report that exposure explicitly. Do not assign a
   speculative continuation value or extend play to make a Fracture detour look worthwhile.
   Training hardcodes a 10,000-step bound in `play_one`; distinguish hitting that safety cap from
   normally completing four rounds.

   Rerun the census after fixing its measurement boundary. It calls
   `audit_game_with_deciders`, whose loop (`rollout.rs:1348–1352`) breaks on an engine error and
   later returns `Ok` with the partial state; reaching its step cap is also not an explicit error.
   These can masquerade as lack of Fracture opportunity. This finding does not establish that
   previous games actually failed. Report normal horizon completion, actual finish, error, and
   step-cap truncation separately, and reject contaminated comparisons. The audit helper also
   does not enable diplomacy, so the present census is a diplomacy-off measurement.

   Refine the funnel to: Fracture enters play → seat gets a later tactical opportunity → eligible
   activation offered → usable movement route/fleet exists → activation chosen → ships actually
   enter → invasion/control → resources or points realized. `tactical::activatable` enumerates
   systems without the player's token; it does not require a useful movement route. Thus “offered
   but not chosen” is not enough to diagnose poor valuation. Equally, “in play, never offered”
   requires an eligible later choice before it establishes a bug.

   Build a small diagnostic panel of legal saved states with accessible Fracture rewards and
   enough time remaining before round 4 ends, plus negative controls where entering is bad. Compare normal choice
   against a forced legal entry followed by the same continuation policy, over several matched
   continuations, all ending by round 4. A consistently favorable branch that the policy rejects
   implicates valuation or exploration; a branch that fails to pay off by round 4 can be correctly
   rejected under the intended objective. Fracture usage alone is not a success criterion. This is a
   policy-relative opportunity estimate, not an oracle proof of the best strategy. Check that
   policy features actually distinguish the route, destination, and obtainable reward.

6. **Retain the promise set provisionally; test decisions rather than uptake alone.**

   Trade, credit, paid non-aggression, votes, refresh, and already implemented targeted agents
   cover enough useful interactions to train. Finish fulfillment hooks before adding more
   templates. The next candidate I would investigate is a paid, system-specific
   `DoNotActivate`: the type and event judgement already exist, but the initial candidate
   generator does not construct it. This can describe a territorial concession more precisely
   than blanket non-aggression. Another later candidate is timing an owned strategy card before
   a specified public milestone, judged on actual play events. Both need relevance filters and
   explicit deadlines. Do not add “help with my objective” unless the exact observable obligation
   is defined. Keep agreement fulfillment distinct from whether it actually benefited the buyer.

   The requirement that both sides contain a term prevents syntactic gifts, not economic ones:
   a commodity debt that cannot be fulfilled or is almost never honored may be worthless. Track
   legal repayment opportunities, amounts recovered, and opportunity costs; do not infer repayment
   reliability from “3 of 12 payment options chosen.” Likewise, template take rates over repeated
   menu appearances are not independent acceptability trials.

   Zero refusals among 57 selected deals is not by itself a model failure. Those are deals another
   policy chose to propose, and free-to-break promises can make acceptance rational over a short
   horizon. Test controlled beneficial, costly, and dominated offers, with the same counter
   alternatives. Inspect logits and the complete accept/counter/decline split. If clearly harmful
   immediate exchanges are still accepted, check information representation before reward tuning.
   Current diplomacy features aggregate many distinct assets into scalar values; the commodity
   coefficient 0.2 is a heuristic opportunity-cost assumption, not something the conversion rule
   proves. Preserve explicit asset kind/quantity and contextual value where needed, rather than
   assuming one aggregate number conveys the deal's strategic consequences.

   Keep the directional relationship matrix through one controlled ablation. Its storage for six
   seats is small, and removing it now saves little of the measured work. At inference, mask or
   permute relationship features to test immediate dependence. Then compare retrained models
   with and without them to test usefulness; masking alone creates a distribution shift. Use
   repeat counterparties with different breach histories but matched current material positions.
   Keep the matrix if it improves later partner selection, repayment/risk calibration, or held-out
   outcomes. Do not reward high trust or cooperation directly. If compact breach/fulfillment
   history does as well, simplify the four scores while retaining the authenticated event record.

The next measurements I would request, in order:

| Measurement | Decision it resolves |
|---|---|
| Matched four-round T=2.5 contact funnel plus existing `--diag`, repeated for several seed blocks | How much a safe entry filter can remove; which CPU component to optimize |
| Per-game paired round-4 VP, margin, leadership, truncation counts, and diplomacy on/off | Whether a candidate scores more by round 4 against the frozen benchmark |
| Explicitly logged VP-only and modest-waste pilots against frozen opponents, with equal wall budgets | Whether shaping and changing opponents are obscuring learnable VP gains |
| Corrected Fracture funnel plus accessible-state positive/negative controls, bounded to round 4 | Whether the policy misses profitable opportunities or correctly rejects detours that cannot pay off in time |

Defer more promise families, relationship-score tuning, large terminal bonuses, and a new batching
architecture until these results identify a reason to spend on them. Judge the eventual change by
paired playing-strength improvement per wall-clock hour, with failures and unfinished games visible.
