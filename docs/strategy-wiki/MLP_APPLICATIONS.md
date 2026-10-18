# Using the strategy wiki to improve MLP bots

Working proposal, updated 2026-10-18. [Twelve source-linked case families](MLP_CASE_CATALOG.json) are specified for Hacan, Sol, Jol-Nar, L1Z1X, Letnev and Xxcha. They remain unlabelled proposals. Synthetic production, reachable Sol decisions, controlled round-one overrides and limited training pilots have been measured; none makes podcast advice an approved training label.

Implemented [strategy_probe](../../crates/ti4-mlp/examples/strategy_probe.rs): synthetic Hacan money/capacity contrasts and Jol-Nar infantry/cash contrasts, actual actor inputs/probabilities, unchanged-MLP versus infantry-first/stop-first continuations, and mandatory fleet/capacity enforcement. **72/72 branch results replayed exactly**; the corresponding 18-result serial subset matched the parallel run. Three integration tests and 109 MLP library tests passed. [Evidence, final results and independent review](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md). This first slice covers production/enforcement only; Warfare timing, transport, landing, defensive quality, scoring and the remaining families are pending. These are diagnostics, not approved action labels.

The unchanged model successfully takes two free infantry in the cash-free Jol-Nar case. In the funded shortage case it builds a flagship plus three infantry, then removes one ship for fleet supply; in two of four sampling streams that ship is the newly built flagship. The relevant fleet-excess/headroom facts were emitted and mapped to assigned vocabulary slots. This suggests investigating production/removal coordination in reachable student games before adding features or teaching a fixed infantry-first rule. Cash-rich synthetic cases do not establish how often this happens in ordinary play. The complete source-linked case curriculum remains open.

## Recommendation

### Current operator focus: Sol only (2026-10-16)

The operator selected Sol for its more direct decision priorities. Implemented
[sol_strategy_probe.rs](../../crates/ti4-mlp/examples/sol_strategy_probe.rs) and
[analysis](../../tools/analyze_sol_strategy.py) now replay **reachable games**, with no
artificial money or units, through the round-four cutoff. [Sol experiment evidence](../../plans/evidence/SOL-STRATEGY-PROBE-2026-10-16.md)
records source claims C17/C19, exact protocol, guards and independent review. Uses verified
Train-role maps and the current DEFAULT/FULL engine scope; the guide's PoK-era compatibility
remains unresolved.

- Research:16 origins ×diplomacyOFF/ON,32 selected decisions in rounds2–4,64 paired
  continuations and32 exact natural-action replay controls. Two baseline agenda guards are
  retained; no paired continuation failed. Cutoff VP favors take in6, skip in6, ties in20.
  Fifteen roots already owned Gravity Drive, seventeen did not. These dependent cases do not
  support a blanket research preference.
- All32 baselines reached end-Action round1 without Gravity Drive; Sol chose Leadership
  in all32. A separate four-origin budget test skips/follows one actual early non-Technology
  secondary while Sol still has4 resources/2 strategy tokens. Seven matched decisions,
  seven exact natural controls; one alternate later hits an agenda guard. No continuation
  obtains round-one Gravity Drive. One early skip alone therefore does not supply the
  acquisition plan; Technology draft/play timing needs investigation before a reserve label.

These initial probes changed no weights. Subsequent Sol distillation and PPO pilots are recorded below; they did not approve a source-derived label or promote a checkpoint. Existing faction-wide proposals remain future options.

### Measured Sol follow-ups (2026-10-16)

[Round-one pilot and follow-ups](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md) use the six-faction, diplomacy-ON Train setup and round-four cutoff. After the timing-window fix, the 40-origin confirmation completed 1,960/1,960 games with 40/40 determinism checks: forcing Trade instead of Sol's natural Leadership lost 0.79 VP (95% origin-level interval −1.13 to −0.46); forced Military Support sales changed VP by +0.06 (−0.19 to +0.33). The source's Trade advice is conditional strategy advice, and these outcomes describe this MLP in this setup.

A carrier/fighter production override and a spend-before-pass override made more units without improving cutoff VP. In the same evidence, replacing **all** Sol decisions with the rule bot gained 0.89 VP (+0.54 to +1.24), while isolated tactical and production substitutions hurt; the research-only substitution gained 0.09 (+0.01 to +0.17). The coherent rule policy and its individual parts therefore have different measured effects.

Behaviour cloning and three rounds of student-state DAgger did not reproduce the whole-rule gain. In a two-arm, 50-update PPO pilot, the DAgger-started student changed Sol VP by −0.03 (−0.37 to +0.31) against base, while a base-checkpoint start gained 0.56 (+0.22 to +0.89). The six-faction roster switch is a confound, and these local pilots are neither a general strength claim nor checkpoint promotion. The [evidence](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md) records setup, exclusions and source limits.

For each new source-derived case, first measure the unchanged model in a **conditional decision curriculum**. The wiki supplies hypotheses, conditions, counterexamples and component relationships. The engine establishes legality and what a branch actually accomplishes. Only then can an action preference be considered for training.

Further work should test how resources, command tokens and transport become objective points, including downstream choices after production. Keep diplomacy as a separate specialist track. The Sol pilots show why a policy or teacher must be evaluated as a coherent sequence rather than inferred from one local preference.

## What the evidence changes

The holdout benchmark is a six-player, round-four comparison on the same 71 complete seed clusters, with guard failures retained. It uses the runner's InScope roster; its aggregate means are not a baseline for a new six-faction-only pilot.

| Setting | MLP self-play VP | Rule self-play VP | MLP mixed VP | Rule mixed VP |
|---|---:|---:|---:|---:|
| Diplomacy OFF | 2.254 | 3.347 | 1.840 | 3.465 |
| Diplomacy ON | 3.601 | 2.786 | 3.749 | 3.207 |

This motivates investigating the OFF weakness and protecting the ON strength. It does not show which decision causes either result. Blind imitation of the rule bot could damage an environment where the MLP currently scores better. The earlier apparent four-versus-six contrast did not hold diplomacy fixed; four-player holdout evaluation remains pending. [Holdout results and uncertainty](../../plans/evidence/HOLDOUT-BOTS-2026-10-13.md), [protocol investigation](../../plans/evidence/FOUR-V-SIX-MECHANISMS-2026-10-13.md).

The baseline checkpoint-16448 manifest matched the benchmark SHA256 at inspection. It describes schema 12, 15 heads including diplomacy, width 256 and two residual blocks. Its vocabulary has 39,667 allocated slots, including 115 `objective-progress:` slots, 80 `objective-met:` slots, 67 `objective-need:` slots, target progress/completion gains, transport facts and leader inventory. Vocabulary presence does not establish runtime emission, trained weights or useful sensitivity. Missing legacy `state-option:` slots is not automatically a defect in a nonlinear MLP.

The encoder already computes progress through engine requirement functions and action-specific counterfactual progress. In particular, a target gain can imagine planet acquisition or generic ship presence; it does not prove that the actual fleet, cargo, landing and combat sequence reaches that imagined state. Probe the complete decision chain before diagnosing a missing objective feature. [Encoder](../../crates/ti4-policy/src/features.rs), [actor logits](../../crates/ti4-mlp/src/lib.rs). The earlier read-only checkpoint/vocabulary inspection is saved in `out/strategy-wiki/extractions/mlp-strategy-input-review.json`; that inspection alone made no inference claim.

## First case families

Each family needs positive and contrasting states, source compatibility checks, actual legal alternatives and a recorded continuation. The catalog retains exact episode citations, source conditions and pinned faction-profile references.

| Faction | Main conversion case | Second case |
|---|---|---|
| Hacan | Money versus a binding production cap and defensive screen | Mobility/QDN opportunity versus low-value extra-income research |
| Sol | Additional research versus an expansion build | Useful Politics secondary versus the remaining token/card budget |
| Jol-Nar | Repair the two-infantry opening through timely production, loading and landing | Research Agreement benefit versus the technology given away |
| L1Z1X | Feasible structure capture and Assimilate timing | Strong fleet versus insufficient ground forces or command tokens |
| Letnev | Ship resources versus influence/action constraints | Armada fleet legality versus retaining tactic tokens |
| Xxcha | Hero spending power versus unchanged printed production | Eligible Imperial score and secret access versus merely drawing a card |

The Jol-Nar deal case belongs to diplomacy ON. Breakthrough-dependent lines and other era-sensitive mechanisms remain excluded until matched to the actual setup and implemented engine behavior. Mechanize progress, Xxcha printed values and similar known compression traps are source/rules validation controls; they are not evidence that the current MLP makes those mistakes.

## Diagnose before choosing the intervention

At every probed choice, retain the legal option IDs, own-seat observation, raw factual vectors, mapped vocabulary slots, model logits/probabilities, chosen action, source case ID and inference status. Replaying a probe must reproduce the same choice. No full-state hidden information or future deck/objective knowledge may enter an actor target.

| Finding | Response |
|---|---|
| Required action/mechanic is absent or incorrect | Engine/capability work; do not train around it |
| A material fact is not emitted or collapses with its contrasting state | Minimal factual encoding/vocabulary change, with migration and regression checks |
| The fact is visible but the preferred legal option ranks poorly | Supervised preference/distribution training on validated cases |
| The initial choice is sensible but loading, landing, payment or production later fails | Train and evaluate the responsible downstream heads and full sequence |
| Model chooses plausible alternatives and both branches work | Preserve multiple acceptable actions; do not force a podcast preference |
| Source advice fails legality, compatibility or controlled branch checks | Reject or qualify the case rather than promote it as gold |

Measure acceptable-action ranking and probability mass, ranking reversals between positive/contrast states, actual objective preparation/scoring, resource/token waste, and the first stage at which a branch fails. A lower imitation loss is not evidence of stronger gameplay.

## Routes, in order

1. **Validated case supervision, then student-state coverage.** Build a public-observation teacher for one conversion family. Preserve multiple acceptable actions; any soft target distribution needs an explicit tested construction, not invented confidence. Add cases from student-generated mistakes after the initial pilot so it covers states the MLP actually visits. Keep ordinary play in the mixture and balance contributions across factions and decision families.
2. **Improve a rule/planner teacher where the cases justify it.** The existing rule bot is a comparator and candidate teacher, not a universal oracle. Adopt source-informed priorities only after positive/contrast checks and paired games. Then collect useful preferences or trajectories for the MLP.
3. **Add factual inputs only for demonstrated gaps.** Distinguish encoder absence, unknown vocabulary slots, untrained rows and weak model use. A wiki graph edge suggests an interaction to test; it is not sufficient justification for a new feature.

The existing distillation loss accepts teacher distributions and balances per-faction means. The original corpus uses linear champions, and `distill_mlp` uses an MLP teacher with shared vocabulary. A rule/planner teacher needs a collector/target adapter and replay validation; the [Sol pilot](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md) used a dedicated collection route rather than a drop-in `distill_mlp` option. Preserve unrelated vocabulary rows when fine-tuning and establish which heads/trunk may move; freezing readouts alone does not isolate a shared-trunk change. [Distillation](../../crates/ti4-mlp/src/distill.rs), [MLP-teacher collector](../../crates/ti4-mlp/examples/distill_mlp.rs).

`trade_teacher` already demonstrates a head-focused public-information planner and contrastive bad offers; adapt the design rather than its deal-value objective blindly. `economy_probe` already examines critic sensitivity and production choices. Positive-corpus replay preserves action IDs and rejects wasted-activation trajectories, but filtering only model successes can omit important failure states and does not independently establish a source recommendation. [Trade teacher](../../crates/ti4-mlp/examples/trade_teacher.rs), [economy probe](../../crates/ti4-mlp/examples/economy_probe.rs), [positive corpus](../../crates/ti4-mlp/examples/build_positive_corpus.rs).

Broad reward shaping comes later: a reward for money, ships, tech or territorial spread can reinforce an expensive proxy that never becomes a point. Start with interpretable decisions and verified continuations.

## Pilot and adoption evidence

The initial [synthetic production probe](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md) and [reachable Sol probe](../../plans/evidence/SOL-STRATEGY-PROBE-2026-10-16.md) established decision and continuation diagnostics. The [round-one follow-up](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md) tested overrides, rule-policy composition and small distillation/PPO pilots. Remaining case families need legal positive and contrasting states, source compatibility checks, unchanged-checkpoint decisions, and controlled continuations before any label is approved.

Never train or tune on the validation holdout seeds already inspected here. Keep related state variants together when splitting a diagnostic/training corpus by origin seed. A future source-informed training comparison needs an equal-compute/data ordinary-play control and an ablation that isolates the intervention.

Gameplay acceptance keeps the existing round-four cutoff, fixed setup/checkpoint references, matched maps, seat rotations and both mixed swaps. Report all-MLP self-play and mixed rule/MLP games with diplomacy OFF and ON separately. Cluster uncertainty by map seed; report faction breakdowns, objective categories and guard/inference exclusions. Require an OFF improvement without materially sacrificing ON performance, with the material-regression threshold set before looking at new results. Existing guard failures and informative exclusions remain explicit; no incomplete game becomes zero VP or a win.

Four-player games remain pending until a compatible four-home holdout pool is available. Longer-horizon source recommendations need a separate longer-game evaluation and cannot be accepted from round-four standings.

[Wiki](README.md) · [Combined guides](FACTION_SUMMARIES.md) · [Source comparisons](source_comparisons.json) · [Case catalog](MLP_CASE_CATALOG.json) · [Summary contract](SUMMARY_CONTRACT.md)
