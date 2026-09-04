# Evidence — OBS-008c2b placement fleet and transport consequence surface

## Scope and provenance

- Branch: `wp/obs-008c2b-placement-consequence-surface`.
- Base: `3c5262e` (`OBS-008c2a` plus the milestone handover).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008c`, representation rules
  5 and 6), `plans/OBS-008C2B_PLACEMENT_CONSEQUENCE_SURFACE.md`, LRR 16.2/16.3/16.3a,
  37.1/37.1a/37.3, 68.2/68.4, 31.4, and the Fighter II implementation already in
  `crates/ti4-engine/src/fleet.rs`.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, dependency, or generated
  committed artifact.

## Changed paths

- `crates/ti4-engine/src/fleet.rs`
- `crates/ti4-engine/src/production.rs`
- `crates/ti4-engine/tests/decision_delivery_inventory.rs`
- `crates/ti4-policy/src/features.rs`
- `plans/OBS-008C2B_PLACEMENT_CONSEQUENCE_SURFACE.md`
- `plans/evidence/OBS-008C2B.md`
- `plans/EXECUTION_STATE.md`

The two pre-existing untracked review samples were neither read nor staged.

## Result

- `fleet::standing` is now the single arithmetic behind fleet supply and transport.
  `over_supply`, `over_capacity` and `fighters_over_capacity` are expressed through it, and it
  optionally takes an `Arrival` — a unit not yet placed, in the space area or on a planet.
- A production placement choice carries `DecisionContext { source: Rule("68"), subtype:
  "place_unit", target: system }` with a `FleetSupply` and a `TransportCapacity` constraint holding
  the limit the system offers and what is already charged against it.
- Every placement option reports its destination, the batch, the units that will actually arrive
  under 31.4, the capacity it consumes, the headroom and free capacity it leaves, and how many units
  the end-of-turn enforcement would then remove. Each carries an analytic `Preview` of
  `FleetSupplyHeadroom` and `CapacityFree`.
- A build option whose unit has exactly one legal destination extends its `OBS-008c2a` preview with
  the same two quantities. This is what makes a ship's fleet-supply consequence visible at all: a
  ship never reaches a placement question, because its destination is forced and the choice settles
  without asking.
- A build option with an open destination states `placement_pending` and emits no fleet or transport
  quantity.
- Option IDs, labels, the legal set, placement legality, payment application and the point at which
  enforcement runs are unchanged.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs008c2b` | 4 passed |
| `cargo test -p ti4-policy --lib obs008c2b` | 2 passed |
| `cargo test -p ti4-engine --quiet` | 1,145 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 199 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign in 305.41 s |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings` | passed |
| `cargo fmt -p ti4-engine -p ti4-policy -- --check` | one pre-existing `OBS-008c2a` hunk in `production.rs`; every hunk this package added is clean |
| `git diff --check` | passed |

The `production.rs:3465` formatting drift is inside a committed `OBS-008c2a` test and was left
alone rather than reformatted as unrelated code.

## Analytic-agreement and counterfactual evidence

- `fleet.rs`: in a Fighter II position that engages both limits at once, `standing` reproduces
  `over_capacity`, `fighters_over_capacity` and `over_supply` exactly. Then the guarantee every
  preview rests on: asking about an `Arrival` gives the *same `Standing`* as placing that unit for
  real and asking again — proved separately for a space-area arrival and for a structure landing on
  a planet, where it changes the answer only through fighter support.
- `production.rs`: a forced ship destination is previewed, then applied, then compared against the
  `fleet::standing` the enforcement itself consults. The stated arrival count is compared against
  the units that actually arrived.
- `production.rs`: with Saar's Floating Factory in the space area and an ordinary dock on a planet,
  an infantry has two destinations. Both context constraints match the real standing; the space
  option consumes two capacity and the planet option none; the space option's previewed capacity is
  then compared against the position the real placement reached.
- `features.rs`: two placements identical in id, kind, unit and batch, differing only in aftermath,
  produce different features; the crowded one reports units removed and twice the capacity loss.
  Survives MLP projection and is admitted by the transferable `production` family.
- `features.rs`: an open destination emits `placement_pending` and no fleet or transport quantity,
  while a forced destination in the same choice emits both.

## Decisions made and rationale

- **One arithmetic, not two.** The preview could have been written as its own calculation. Then it
  would be a second implementation of Rules 16 and 37, free to drift from the one that removes
  units at the end of the turn. Rewriting `over_supply`/`over_capacity`/`fighters_over_capacity` to
  go through `standing` makes agreement structural rather than tested-for.
- **Fleet supply and transport are answered together** because Fighter II couples them: a ground
  force placed in a space area can push fighters out of capacity and onto the fleet pool. Answering
  either alone gets that case wrong.
- **Headroom is signed, free capacity is not.** Production places units first and the limits are
  enforced afterwards, so a negative headroom is a position a seat genuinely reaches and its
  magnitude is what will be removed. Free capacity has no such reading — what does not fit is
  excess, and the fleet-pool shift means it is not simply the negation of the slack.
- **The build option, not only the placement option.** A ship's destination is forced, so no
  placement question is ever asked and the consequence would otherwise never be stated. Extending
  the build preview only when the destination is determined keeps that truthful.
- **`placement_pending` rather than a zero.** Representation rule 6. A related limitation surfaced
  while testing and is recorded below.
- **Component limitation feeds the arithmetic.** `place` clamps the batch to what the box holds, so
  a preview computed from the purchased batch could state a consequence the box cannot deliver.
  See the correction below for what this does and does not fix today.

## Corrections and residuals

- **`OBS-008c2a`'s preview used the purchased batch where `place` uses the clamped one.** Now both
  use the clamped count. This is **not a live defect**: every unit offered as a two-for-one pair
  (fighter, infantry) is uncapped by 31.4, so the two counts cannot currently differ. It is a guard
  against a future capped pair, and it removes an agreement that held only by accident.
- **A sparse feature vector drops an exact zero**, so "no room left" and "not asked" arrive
  identically. This is inherited from `add_named`, not introduced here, and it applies to every
  numeric feature in the crate. Closing it generally needs an offset encoding across the whole
  feature set, which is out of scope. For this package's facts it is closed cheaply by
  `production:destination-known`, which distinguishes an answered zero from an unanswered quantity;
  both branches are asserted in the policy tests.
- **One planet destination is still not separated from another.** Where a placement offers two
  planets, this package's facts are identical for both — the fleet and transport aftermath genuinely
  is. Separating them needs control, defence and adjacency facts owned by `OBS-006`/`OBS-008a`.
  Recorded here as a deliberate exclusion rather than silently shrunk scope.
- **`fleet::enforce_everywhere`'s doc comment said it was not called from the game loop.** It has
  been called from `Game::advance_turn` since the fleet-supply fix, which is exactly what makes a
  placement's aftermath a consequence rather than a position nobody looks at. Corrected, because
  this package's truthfulness claim rests on it.
- **A new producer, `production.rs::placement_choice`,** was added to the `OBS-002a` reviewed
  registry, and `pending_choice`'s count in that registry dropped from 3 to 2, because the placement
  arm was extracted into its own method. The delivery classification is unchanged
  (`ObservedVia("game.rs::step_aftermath")`), and the indirect-reachability test confirms it.

## Performance

The 102-game deterministic campaign ran in **305.41 s**, against 328.21 s recorded for `OBS-008c2a`.
Folding the 31.4 component-limit question — previously asked once as an offer guard and once again
for the count — into a single call removed one full-board walk per offered unit.

**No speedup is claimed.** These are single runs on one machine without the M00 protocol, variance,
or a semantic gate. The measurement supports only the negative statement it is here for: adding
per-option fleet and transport previews produced no measurable regression.

## Independent review

**OUTSTANDING.** This package is Tier C (legality-adjacent arithmetic and a shared-helper
refactor) and has not been independently reviewed. The implementer is the sole reader of this diff
so far, which the work-package standard does not accept as done. No review verdict is recorded here
because none has happened.

## Non-goals retained

No change to fleet-supply, capacity, component-limitation or placement legality; no change to when
enforcement runs; no vocabulary generation, prompt-free migration, or bundle republish; no
transport or fleet-supply mechanics change.
