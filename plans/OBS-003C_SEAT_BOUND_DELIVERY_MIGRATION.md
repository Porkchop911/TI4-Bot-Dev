# OBS-003c — seat-bound delivery migration

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-002a` and `OBS-003a`.
- Objective: route every consequential live choice through seat-bound delivery, so the acting seat
  receives its redacted observation, and leave a closed registry for any genuine viewless exception.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-003c;
  `plans/evidence/OBS-002A.md`, which named all fifteen viewless asks and classified none of them as
  a genuine exception.

## Slicing

Fifteen sites across five modules, each needing `content`, `sources` and a galaxy threaded to the
point of the ask. That exceeds one reviewable change, so it is taken by module. The audit gate is
the ledger: `VIEWLESS_ASKS` shrinks slice by slice and the count assertion moves with it, so partial
progress is visible and cannot be mistaken for completion.

| slice | module | sites | state |
|---|---|---:|---|
| 1 | `relics.rs` | 6 | **done** |
| 2 | `game.rs` | 4 | pending |
| 3 | `invasion.rs` | 2 | pending |
| 4 | `laws.rs`, `timing.rs`, `relics::neuraloop` | 3 | pending |

## Slice 1 — relics

`grant_chosen_technology`, `codex`, `titan_prototype`, `stellar_converter`,
`crown_of_emphidia_explore` and `offer_dominus_orb` now deliver through
`ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))`.

`codex` and `offer_dominus_orb` had no content or sources at all and now take them.
`grant_chosen_technology`, `titan_prototype` and `crown_of_emphidia_explore` took content and
sources but no galaxy. `stellar_converter` already held everything and only needed the call swapped;
it shadows its own `galaxy` with the unwrapped value, so the ask passes `Some(galaxy)`.

Callers updated: `exploration::perform_action` gained a galaxy and passes it on, and three
`game.rs` sites now pass `self.galaxy.as_ref()` — the field rather than the `galaxy()` accessor,
because the accessor borrows all of `self` and the calls also take `&mut self.state`.

`relics::neuraloop` is deliberately not migrated in this slice: it takes neither a player nor
content, and its choice names a seat indirectly, so it belongs with the remaining stragglers rather
than being forced into a relics-shaped change.

## Invariants and non-goals

- No legal option set, option id, prompt or reward changes. Delivery only.
- No producer populates a `DecisionContext` yet; that is OBS-003d–h.
- `DecisionLog::record` still writes `None`, so replay digests are unchanged.

## Tests and commands

- `cargo test -p ti4-engine --test decision_delivery_inventory`
- `cargo test -p ti4-engine`, `cargo test -p ti4-training`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`

## Definition of done

Every slice landed, `VIEWLESS_ASKS` empty or holding only entries a reviewer has accepted as genuine
setup/offline exceptions, the scanned viewless total agreeing with the registry, and the suites and
strict Clippy green at each slice.
