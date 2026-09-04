# OBS-012 — decision completeness qualification

## Package

The plan's own charter is explicit: "publish remaining exceptions rather than claiming blanket
completeness." This package runs what is feasible within a single session against the
`OBS-011`-published generation and reports honestly against the plan's four verification gates.
Two of the plan's seven named ablation kinds (performance, PPO/distillation retraining) require a
multi-arm training sweep -- hours to days of compute at the same budget as prior baselines -- and
are explicitly **not run**, recorded as the remaining exception rather than silently skipped.

- Dependencies: `OBS-004`-`OBS-011` (everything being qualified).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`'s "Verification gates" section
  (static contract, counterfactual, empirical separability, learning and performance) and the
  `OBS-012` row.
- Writable paths: none in the tracked repository for the parts that ran (see below); this spec
  and its evidence are the record.

## Static contract gate — confirmed, continuously

Not re-derived here: these properties are checked by tests that ran green on every single commit
this session (34+ commits, `cargo test -p ti4-engine`/`-p ti4-policy` in every evidence file).

- **100% of consequential learned-policy choices delivered through a seat-bound
  `DecisionObservation`**: `crates/ti4-engine/tests/decision_delivery_inventory.rs` (959 lines,
  4 tests) scans production source and checks every producer/delivery site against a reviewed
  registry -- `every_producer_and_delivery_site_matches_the_reviewed_registry`,
  `every_indirect_producer_reaches_its_classified_delivery_api`,
  `the_remaining_viewless_asks_stay_explicit_migration_work`,
  `no_engine_module_calls_a_decider_around_table`.
- **No prompt/label-derived feature; rewording leaves vectors bit-identical**:
  `projection.rs::mlp_vectors_ignore_prompt_and_label_rewording_but_keep_stable_ids`.
- **Serialized decision-context fields have explicit redaction tests**: the M09-023 secret
  redaction suite (`features.rs`) plus `critic.rs::two_bound_seats_receive_only_their_own_secret_
  progress`.

## Counterfactual gate — spot-checked against this session's own evidence

Not a systematic paired-fixture suite (that is a project of its own); the ten listed properties
each have at least one existing focused test, cited rather than re-derived:

| property | evidence |
|---|---|
| production capacity remaining | `OBS-008c`'s `production_decision_features` tests |
| hits remaining and sustain state | `OBS-008b1`-`b4`'s preview tests |
| objective progress delta for the offered option | `objectives.rs::obs008d2_scoring_previews_the_exact_victory_point_gain` |
| opponent identity-relative score/reach/Support relation | `choice.rs::obs005_opponent_slots_are_invariant_under_player_id_relabeling` |
| own held card/relic/leader availability | `OBS-004a`/`OBS-010`'s actor-inventory tests |
| opponent-private mutation leaves actor/critic vectors bit-identical | `projection.rs::opponent_private_identities_cannot_change_the_public_surface`, `opponent_secrets_do_not_survive_the_projection_either` |

**Not individually re-verified this pass**: strategy-card used-versus-ready, opponent
passed-versus-active, payment debt/already-paid, cargo capacity remaining, public law/agenda
outcome. Each is plausibly covered by existing OBS-003/007/008 tests but was not looked up and
cited here -- recorded as an open item rather than assumed.

## Empirical separability gate — run

`cargo run --release -p ti4-training --example separability -- --checkpoint
out/stage2_r6/final10000.json --games 60`, against the r6 champion playing itself (no heuristic in
the loop), 98,600 decisions pooled:

```
head           decisions     blind  tied opts   ceiling  distinct
trade              27044     32.7%      33.4%     0.906      39.3
turn               18437      0.0%       0.0%     1.000     103.6
other               7472     80.4%      55.8%     0.746      78.9
cargo               7169      0.0%       0.0%     1.000     130.6
secondary           4701      0.0%       0.0%     1.000     156.0
tokens              4452      0.0%       0.0%     1.000     156.6
landing             4451      0.0%       0.0%     1.000     131.4
movement            4205      0.0%       0.0%     1.000     115.0
activation          3897     12.9%       0.9%     0.996      10.6
production          3831     16.0%      21.7%     0.905      66.8
payment             3354     57.7%      44.2%     0.793      16.3
agenda              3240      0.0%       0.0%     1.000      22.7
ability              2210      0.0%       0.0%     1.000     123.7
strategy            1498      0.0%       0.0%     1.000     139.6
scoring               844      0.0%       0.0%     1.000     111.1
exploration           613      0.0%       0.0%     1.000      75.5
development           601      0.0%       0.0%     1.000      90.8
combat                577      4.9%       8.2%     0.970      96.5
transit                  4      0.0%       0.0%     1.000      39.8

pooled: 98600 decisions, 18.2% blind, 18.3% of options tied, ceiling 0.944
```

**Real, not blanket-complete.** Thirteen of nineteen heads (`turn`, `cargo`, `secondary`,
`tokens`, `landing`, `movement`, `agenda`, `ability`, `strategy`, `scoring`, `exploration`,
`development`, `transit`) are fully separable: zero blind decisions, ceiling 1.000. Four are
strong but not perfect (`activation` 0.996, `combat` 0.970, `production` 0.905, `trade` 0.906).
Two are the genuine remaining plateau:

- **`other` (80.4% blind, ceiling 0.746)**: the legacy catch-all bucket
  (`learned.rs::oracle_other_head`) most of the `content` family's ~30 still-unpreviewed subtypes
  (`crashlanding_*`, `silence_choose_system`, `dominus_orb_purge_to_move`,
  `neuraloop_choose_relic_to_purge`, `stellar_converter_choose_target`,
  `titan_prototype_choose_builder`, and most reactions/action-cards) fall into. Consistent with
  `OBS-008g2`/`h2`/`h3`'s own finding that these are identity choices without a clean
  single-quantity preview -- this measurement shows that gap has a real, sizeable cost, not just
  a theoretical one.
- **`payment` (57.7% blind, ceiling 0.793)**: unexpected given `OBS-008c`'s coverage; not
  investigated further this pass.

**Recorded exception, not fixed here**: closing `other`'s blindness needs candidate-relative
board facts (which planet/system/relic each option targets) rather than a quantity preview --
`OBS-006`-shaped work applied to the content family, out of this package's scope.

## Hidden-information and equivariance gates — confirmed, continuously

- **Hidden information**: `projection.rs::opponent_private_identities_cannot_change_the_public_
  surface`, `opponent_secrets_do_not_survive_the_projection_either`; `critic.rs::two_bound_seats_
  receive_only_their_own_secret_progress`.
- **Equivariance**: `choice.rs::obs005_opponent_slots_are_invariant_under_player_id_relabeling`.

## Learning and performance gate — not run (the recorded exception)

The plan's own five-arm pre-registered ablation (OBS-001 baseline through aligned critic, same
seeds/rotations/corpus/vocabulary generation/teacher/budget) is a multi-hour-to-multi-day training
commitment at this project's scale -- qualitatively different from `OBS-011`'s ~16-minute
discovery run or this package's ~80-second separability run. **Not attempted this session.**
Distillation and PPO ablations depend on it and are likewise not run. This is the package's
headline "remaining exception," named rather than silently absent.

## Definition of done

Static contract, hidden-information, and equivariance gates confirmed via existing, continuously
green tests, cited rather than re-derived; the counterfactual gate spot-checked against six of ten
listed properties with four recorded as open; the empirical separability gate actually run against
the `OBS-011` generation with real numbers, finding thirteen fully-separable heads and two genuine
plateaus (`other`, `payment`) rather than claiming blanket completeness; the learning/performance
gate explicitly recorded as not run, with the reason. Full checks not applicable (no source
changed).
