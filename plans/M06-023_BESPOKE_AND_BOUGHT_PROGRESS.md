# M06-023 — Remaining position and exact bought-cost objective progress

## Status

Implementation is in progress. M06-022 and M06-021a are accepted.

| Field | Value |
|---|---|
| Milestone | M06 — General rules |
| Depends | accepted M06-022 |
| Permission class | P1 |
| Review tier | C — exact payment semantics |
| Compatibility | accepted Rust predicates/payment planner; Python parity not applicable |

## Objective

Complete progress exposure for the six remaining public predicates, all seventeen remaining
position-based secret predicates, and ten bought objectives, using exact integer counts and the
existing disjoint payment planner rather than heuristics or independently summed currencies.

## Normative sources

- Accepted legality and payment paths in `crates/ti4-engine/src/objectives.rs`, `payment.rs`, and
  `production.rs`.
- `docs/MLP_PLAN.md` §5.1 and decisions D5/D17.
- The typed progress contract accepted in M06-022.

## Scoped access

```text
Writable paths:
  crates/ti4-engine/src/objectives.rs
  crates/ti4-engine/src/secrets.rs
  plans/M06-023_BESPOKE_AND_BOUGHT_PROGRESS.md
  plans/M06-023_OPEN_REVIEW_ITEMS.md
  plans/evidence/M06-023.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-engine/src/payment.rs
  crates/ti4-engine/src/production.rs
Network/process needs: bounded Cargo format/test/lint/property commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

## Six exact count paths

| Alias | Family count | Threshold | Unavailable without map |
|---|---|---:|---|
| `conquer` | controlled rival-home planets | 1 | no |
| `engineer_marvel` | systems holding own flagship or war sun | 1 | no |
| `supremacy` | those systems that are a rival home or Mecatol | 1 | no |
| `intimidate` | systems with own ships adjacent to Mecatol | 2 | yes |
| `push_boundaries` | distinct neighbours controlling fewer planets | 2 | yes |
| `distant_lands` | distinct rival home reaches containing a controlled planet | 2 | yes |

Each existing boolean predicate must derive from its count and threshold. Distinct-system,
distinct-neighbour, and distinct-rival reductions are preserved exactly; duplicate units or planets
must not inflate those families.

`intimidate`, `push_boundaries`, and `distant_lands` require a galaxy. Their progress is unavailable
without one; absence of a map is not factual zero and must not be collapsed into unmet progress.

## Seventeen remaining secret position paths

These are the complete registered-secret residue after M06-022's ten counting aliases and
M06-021a's thirteen occurrence aliases. None remains implicit or boolean-only:

| Alias | Exact progress | Threshold | Map required |
|---|---|---:|---|
| `csl` | rival space-dock systems containing an own ship | 1 | no |
| `dhw` | held relic fragments | 2 | no |
| `fsn` | held action cards | 5 | no |
| `sb` | distinct rival note issuers held | 1 | no |
| `lsc` | own ship systems adjacent to at least one anomaly | 3 | yes |
| `te` | own ship systems adjacent to a rival home | 1 | yes |
| `fc` | other players currently reached as neighbours | number of other players | yes |
| `dfat` | own units in the wormhole nexus | 1 | no |
| `btgk` | alpha/beta wormhole kinds occupied by own ships | 2 | no |
| `ans` | owned faction technologies | 2 | no |
| `dp` | laws in play | 3 | no |
| `syc` | systems where controlled planets are shared with a rival | 1 | no |
| `pem` | greatest own PRODUCTION capacity in one system | 8 | no |
| `sai` | controlled legendary planets | 1 | no |
| `ose` | ships in Mecatol while controlling a Mecatol planet; otherwise zero | 3 | no |
| `eh` | combined influence of controlled planets | 12 | no |
| `hrm` | combined resources of controlled planets | 12 | no |

`fc` is unavailable when no galaxy is supplied. At a one-player table it is also unavailable rather
than manufacturing a zero threshold; every returned progress value has a non-zero threshold. The
three map-dependent secret families (`lsc`, `te`, `fc`) and three map-dependent public families must
all preserve unavailable separately from unmet.

## Ten bought-cost paths

The aliases and targets are the existing `cost_of` table:

| Alias | Cost family | Target |
|---|---|---:|
| `monument` | resources | 8 |
| `golden_age` | resources | 16 |
| `sway_council` | influence | 8 |
| `manipulate_law` | influence | 16 |
| `trade_routes` | trade goods | 5 |
| `centralize_trade` | trade goods | 10 |
| `lead` | command tokens | 3 |
| `galvanize` | command tokens | 6 |
| `amass_wealth` | `AllThree` | 3 |
| `vast_reserves` | `AllThree` | 6 |

For target `n`, report the greatest integer `k` in `0..=n` for which `can_afford` accepts the same
`Cost` variant scaled to `k`. The target remains `n`; later normalization is `k / n`. In particular:

- resource/influence progress delegates to `payment::affordable` through `can_afford`;
- `AllThree(k)` delegates to the existing disjoint resource/influence plans plus `k` trade goods;
- no component-wise minimum, raw currency sum, fractional payment, or greedy approximation is valid;
- progress queries never call `pay_for` and never exhaust planets or spend goods/tokens;
- zero/negative targets are rejected by the typed mapping/content validation and never normalized;
- final `satisfied`/offer legality remains exactly `can_afford(original_cost)`.

## Non-goals

Do not alter payment selection or spending order, feature normalization/names, vocabulary, policy
code, objective prices, score values, registries, or hidden-information behavior. M09-021 owns
feature emission and aggregation.

## Tests to add

- Below/equal/above threshold plus duplicate-deduplication tests for all six count paths.
- Table-driven mapping for all ten bought aliases and their exact family/target.
- Exhaustive small-state properties: greatest affordable `k` is affordable, `k + 1` is not when
  `k < n`, and progress completion equals `can_afford(original_cost)`.
- Disjoint-payment traps where one planet cannot fund both halves of `AllThree`; trade-goods
  substitution and mixed exhausted/ready planet cases; token-pool splits.
- Querying every count/cost progress path preserves structural state equality.
- Table-driven family/parameter/threshold coverage for all seventeen secret residues, with
  below/equal/above and maximum/distinct boundaries for each family.
- No-map tests for all six map-dependent public/secret paths, plus `fc`'s empty-opponent boundary.
- Reconcile all 40 secrets exactly: ten M06-022 counting, thirteen M06-021a occurrence, and
  seventeen M06-023 position paths, with no alias absent or duplicated.
- Existing payment, affordability, objective scoring, affected-crate, and workspace suites pass.

## Commands and evidence

Run scoped `rustfmt`, focused tests, bounded exhaustive/property tests, `cargo test -p ti4-engine`,
`cargo test --workspace`, engine Clippy, and `git diff --check`. Evidence must record exact cases,
the absence of mutation, independent Tier-C reviewer/findings, and no performance/Python claim.

## Definition of done

M06-022 is accepted; all 33 aliases expose exact typed progress; legality is derived from or proven
equivalent to the same count/payment path; exhaustive small-state and regression suites pass; the
independent Tier-C review is resolved; evidence is complete; and only scoped paths are committed.
