# M06-022 — Counting-family objective progress

## Preparation status

This exact package specification was prepared before M06-021a acceptance. M06-021a2b is now
accepted, so M06-022 is dependency-ready; implementation has not yet started.

| Field | Value |
|---|---|
| Milestone | M06 — General rules |
| Depends | accepted M06-021a2b |
| Permission class | P1 |
| Review tier | B |
| Compatibility | accepted Rust scoring legality; Python parity not applicable |

## Objective

Expose exact, typed progress for the existing public and secret counting-family requirements while
making every affected `satisfied` result derive from the same count and threshold, preserving all
current scoring legality.

## Normative sources

- Accepted predicates and focused tests in `crates/ti4-engine/src/objectives.rs` and `secrets.rs`.
- `docs/MLP_PLAN.md` §5.1 and decisions D5/D17 for family identity and later normalization.
- `plans/M06_GENERAL_RULES.md` M06-022 row. M06-023, not this package, owns six formerly bespoke
  public counts and ten exact bought-cost progress paths.

## Scoped access

```text
Writable paths:
  crates/ti4-engine/src/objectives.rs
  crates/ti4-engine/src/secrets.rs
  plans/M06-022_COUNTING_OBJECTIVE_PROGRESS.md
  plans/evidence/M06-022.md
  plans/EXECUTION_STATE.md
Read-only external paths: none
Network/process needs: bounded Cargo format/test/lint commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

## Required API and invariants

- Add a public, deterministic family identifier that retains parameters needed to make unlike
  counts unlike (for example colour prerequisite, planet trait, or unit base type).
- Expose `have` and a non-zero `threshold` through one typed requirement-progress result. A
  map-dependent count that cannot be evaluated without a galaxy returns unavailable rather than
  inventing a factual zero; its `satisfied` result remains false.
- Derive affected legality as `have >= threshold` from that result. Do not keep a parallel boolean
  implementation that can drift from progress.
- Counts use exact integer arithmetic and deterministic iteration/reduction. Maximum-in-one-place
  families report the maximum single qualifying trait/system/planet/colour count, not a total.
- Querying progress is read-only and cannot exhaust, spend, reveal, or reorder state.
- Unknown/unregistered aliases remain unavailable and unscoreable. Public and secret registries,
  stable choice IDs, scoring order, redaction, and persisted state are unchanged.
- This package exposes raw counts only. Clipped ratios, feature names/aggregation, vocabulary, and
  policy wiring belong to M09-021; bespoke and bought-cost counts belong to M06-023.

## Families in scope

Public: `non_home`, `on_the_rim`, `same_trait`, `tech_specialties`, `unit_upgrades`,
`colours(per_colour)`, `structure_count`, `structures_away`, `fleet_in_one_system`,
`planetless_systems`, `attached_planets`, and `in_notable_systems`.

Secret: `ground_forces_on_one_planet`, `mechs_on_distinct_planets`,
`planets_of_trait(trait)`, `same_colour_technologies`, `ships_in_systems`, and
`units(base_type)`.

The public aliases are the 24 existing family-backed objectives from `outer_rim` through
`become_legend`, excluding the six M06-023 counts. The secret aliases are `eap`, `fwm`, `gamf`,
`ctr`, `faa`, `mrm`, `mp`, `mlp`, `mtm`, and `otf`.

## Tests to add

- Table-driven alias-to-family/parameter/threshold assertions for every in-scope alias.
- Below/equal/above-threshold cases for every family, including maximum-not-total behavior.
- Dual-trait, colour, distinct-planet, non-fighter fleet, attachment, and map-absence boundaries.
- For every in-scope alias and generated position: `satisfied == progress.map(|p| p.have >=
  p.threshold).unwrap_or(false)`.
- Progress queries preserve structural state equality; unknown aliases return unavailable.
- Existing objective and secret scoring suites remain unchanged and green.

## Commands and evidence

Run scoped `rustfmt`, focused progress tests, `cargo test -p ti4-engine`, `cargo test --workspace`,
engine Clippy, and `git diff --check`. Evidence records exact family coverage, command results,
reviewer/findings, and that no performance claim or historical Python comparison was made.

## Definition of done

The dependency is accepted; all 34 in-scope aliases have one typed progress mapping; affected
legality derives from the progress path; boundary/property/state-preservation tests and affected
suites pass; an independent Tier-B review is resolved; evidence is complete; and only scoped paths
are committed.
