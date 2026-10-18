# Bots, simulation and learning

[Repo guide](REPO_GUIDE.md) · [Engine](REPO_ENGINE.md) · [Interfaces](REPO_INTERFACES.md)

Use this route for action preferences, features, training, map experiments and policy evaluation. Source scope: `rule-bot`, navigation checked through 2026-10-18. Historical examples and header comments are not current checkpoint recommendations.

## Crates by role

| Crate | Start here |
|---|---|
| [ti4-policy](../../crates/ti4-policy/src/lib.rs) | Authored scoring and learned policy support. [bot.rs](../../crates/ti4-policy/src/bot.rs) contains `ScoredBot`; inspect [features.rs](../../crates/ti4-policy/src/features.rs), [vocabulary.rs](../../crates/ti4-policy/src/vocabulary.rs) and [inference.rs](../../crates/ti4-policy/src/inference.rs) for representations and learned inference |
| [ti4-mlp](../../crates/ti4-mlp/src/lib.rs) | MLP model and bot integration. [bot.rs](../../crates/ti4-mlp/src/bot.rs) contains `MlpBot` and inference status; [ppo.rs](../../crates/ti4-mlp/src/ppo.rs) and [distill.rs](../../crates/ti4-mlp/src/distill.rs) are learning entry points |
| [ti4-tensor](../../crates/ti4-tensor/src/lib.rs) | Tensor/runtime adapter underneath model work. Consult [Cargo.toml](../../Cargo.toml) and runtime manifests before changing native dependencies |
| [ti4-sim](../../crates/ti4-sim/src/lib.rs) | Game runs, maps, replay, batches, rotations and benchmark support. Begin with [run.rs](../../crates/ti4-sim/src/run.rs), [maps.rs](../../crates/ti4-sim/src/maps.rs), [batch.rs](../../crates/ti4-sim/src/batch.rs) |
| [ti4-training](../../crates/ti4-training/src/lib.rs) | Rollouts, learning stages, reward/evaluation and corpus-related modules. Begin with [rollout.rs](../../crates/ti4-training/src/rollout.rs), [reward.rs](../../crates/ti4-training/src/reward.rs), [stage1.rs](../../crates/ti4-training/src/stage1.rs) and [stage2.rs](../../crates/ti4-training/src/stage2.rs) |

The authored, learned and MLP paths are not interchangeable names for one implementation. Check the decider actually constructed by the runner before interpreting a result.

## Reading route: why did this bot choose that?

### Bot philosophies in this checkout

| Bot | What it tries to encode | Decision mechanics | What to inspect when it errs |
|---|---|---|---|
| [ScoredBot](../../crates/ti4-policy/src/bot.rs) | Explicit preferences that a person can read and revise. | Components score supported offered options; shortlist and temperature sampling make the final answer stochastic. Position-aware scoring can add tactical facts. | Component explanation, shortlist, option order, temperature and seed. An unhandled kind has no authored preference, although the engine may still offer it. |
| [Linear learned policy](../../crates/ti4-policy/src/inference.rs) | Learn option preferences from a fixed, inspectable vocabulary. | A trained profile weights each option's emitted feature vector under a decision head, then softmaxes over the legal set. It does not call ScoredBot's authored filters. | Profile, head, active features, source set and temperature. A vocabulary name alone does not show that a feature was emitted. |
| [MLP actor](../../crates/ti4-mlp/src/lib.rs) | Learn nonlinear combinations of sparse facts, with a faction-specific residual. | Shared trunk and selected head score legal options; temperature-scaled logits are sampled. [MlpBot](../../crates/ti4-mlp/src/bot.rs) must be seated with inference status so missed or invalid inference can invalidate a run. | Bundle/vocabulary compatibility, faction row, active features, head, probabilities, temperature and status. The faction row is not an input identity embedding. |
| [Rule bot](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session/smart_bot.rs) | Make reproducible, explicit gameplay plans and preserve resources for objectives and expansion. | The [server decider](../../crates/ti4-server/src/session/random_bot.rs) draws seeded random first; a heuristic's offered option index overrides it. `None` leaves the random answer. Objective and agent layers precede subtype-specific rules, which include activation, movement, transport, research, production and card selection. | Was this `rule` or `random`? Which hook fired? Examine the offered options and current board, then replay the continuation. |

The rule bot's planning layers are operational hypotheses, not official TI4 rulings. In this `rule-bot` checkout, the implementation lives in the **server session**, with [faction plans](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session/smart_bot_faction_plans.rs) and [objective logic](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session/smart_bot_objectives.rs). `ti4-rule-bot` is a later standalone crate on `main`, so its current source does not by itself identify the policy used in these older holdout runs. A SCPT episode is source for human advice, not implemented behavior. [MLP applications](MLP_APPLICATIONS.md) separates publisher ideas from measured Sol decisions, and the [technical benchmark guide](../TECHNICAL_WIKI_BENCHMARKS.md) keeps cutoff VP, full-game wins and inference speed distinct.

The server rule bot's opening idea is to spread carriers and ground forces across valuable systems in rounds one and two, using Warfare and token-free production to create another transport. It values only systems its ships and troops can actually reach, avoids fights without a clear strength margin, and shifts toward faction-specific technology and fleet plans from round three. A tactic token is preserved when a free build can supply a carrier that should move next. These are conditions in [smart_bot.rs](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session/smart_bot.rs); they are not a universal TI4 opening prescription. The same file names remaining random domains, including many agendas, action cards and other secondary abilities. A trace's `smart` count tells how often authored advice replaced fallback.

### Which comparison answers which question?

- **Rule versus MLP gameplay:** the [six-player holdout](../../plans/evidence/HOLDOUT-BOTS-2026-10-13.md) uses 71 common complete seed clusters and round-four VP. With Diplomacy off, rule self-play averaged 3.347 VP/seat versus MLP 2.254; with Diplomacy on, MLP averaged 3.601 versus rule 2.786. This reversal prevents a context-free “stronger bot” claim. The report retains 36 guard failures, complete-case selection and the four-player pool gap.
- **Player-count mechanism:** the [controlled four/six study](../../plans/evidence/FOUR-V-SIX-MECHANISMS-2026-10-13.md) held Diplomacy **off** and pool conditions fixed and found similar rule advantages on 21 matched core seeds: 2.1310 VP/seat at four players and 2.1746 at six. The earlier apparent reversal confounded Diplomacy with player count. These are different experiments, not two estimates to average together.
- **Reason for a particular move:** inspect a [trace or probe](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md) with the exact seat, observation and continuation. A higher probability or one favorable branch is not evidence of a full-game improvement.

The `main` branch has a newer [headless self-play harness](../TECHNICAL_WIKI_BENCHMARKS.md#how-to-make-the-next-claim-stronger) and [native inference measurements](../TECHNICAL_WIKI_BENCHMARKS.md#speed-local-inference-measurements-not-playing-strength). Those describe `main` at `f55a5b31`, not this `rule-bot` checkout at `592e5716`.

1. Establish the decision's legal options and seat-visible information in [engine choice.rs](../../crates/ti4-engine/src/choice.rs).
2. Locate the runner and decider. [rule_vs_mlp.rs](../../crates/ti4-mlp/examples/rule_vs_mlp.rs) is the mixed-comparison entry point; the staged [strategy probe](../../crates/ti4-mlp/examples/strategy_probe.rs) and [Sol probe](../../crates/ti4-mlp/examples/sol_strategy_probe.rs) have their own dated evidence below. Do not infer a probe's presence from the older pinned examples directory.
3. Inspect the relevant [policy features](../../crates/ti4-policy/src/features.rs), vocabulary mapping and bot/model scoring path. A named vocabulary slot does not prove that a runtime observation emits it or that the model uses it effectively.
4. Trace the continuation through payment, production, transport and later decisions when the initial preference alone does not explain the outcome. [Engine navigation](REPO_ENGINE.md) provides the corresponding rule files.

The [wiki-to-MLP proposal and measured follow-ups](MLP_APPLICATIONS.md) distinguish source hypotheses, inference diagnostics and limited training pilots. The [case catalog](MLP_CASE_CATALOG.json) remains a set of unlabelled research proposals.

[strategy_probe.rs](../../crates/ti4-mlp/examples/strategy_probe.rs) is the initial synthetic production diagnostic for Hacan/Jol-Nar. It uses optional ordinary-decision tracing from `MlpBot`; fleet packages and plan-executed choices are outside that trace. See the [package evidence](../../plans/evidence/MLP-STRATEGY-PROBE-2026-10-16.md) for compilation/run status and limits.

Current operator focus is Sol. [sol_strategy_probe.rs](../../crates/ti4-mlp/examples/sol_strategy_probe.rs)
uses actual rollout setup on Train-role maps and full-game continuations, with root/prefix
checks and exact natural-action controls for research and early token-budget experiments.
[Sol evidence](../../plans/evidence/SOL-STRATEGY-PROBE-2026-10-16.md) distinguishes later
research decisions from round-one Gravity Drive acquisition and retains guard failures.

[sol_round1_pilot.rs](../../crates/ti4-mlp/examples/sol_round1_pilot.rs) tests matched round-one overrides and continuation arms; [its evidence](../../plans/evidence/SOL-ROUND1-PILOT-2026-10-16.md) also records rule-policy composition, student-state distillation and PPO pilots. The headless rule decider is [RuleBotDecider](../../crates/ti4-server/src/session/random_bot.rs), used by [rule_vs_mlp.rs](../../crates/ti4-mlp/examples/rule_vs_mlp.rs) and native review. Read each result with its roster, diplomacy setting, origin split and cutoff; these runs do not promote a checkpoint.

## Training and evaluation reading order

Read [TRAINING_PIPELINE.md](../../docs/TRAINING_PIPELINE.md) for the documented pipeline and [PPO_TRAINING_HOWTO.md](../../plans/PPO_TRAINING_HOWTO.md) for operator procedures. Then inspect the specific launcher/configuration in [scripts/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/scripts) and runner source. Check [COMPUTE_QUEUE.md](../../plans/COMPUTE_QUEUE.md) before GPU work or a multi-minute CPU job; command examples are not reservations or permission to launch them.

For a result, find its exact checkpoint/setup, pool role, map seeds, seat rotations, cutoff, diplomacy setting and failure/exclusion handling in [plans/evidence](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/plans/evidence). Keep validation holdouts separate from training/tuning. Use the current evidence for promotion or performance claims rather than taking an example checkpoint or an old “best” label as current.

For watching decisions, continue to [review and replay](REPO_INTERFACES.md). For online bot services, follow the advisor/bot-agent route on that same page.
