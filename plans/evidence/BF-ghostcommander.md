# BF-ghostcommander — Sai Seravus effect

## Scope and authority

P1 faction-module package in `crates/ti4-engine/src/factions/ghost.rs`, inline tests, and this
evidence file. It builds on the separately reviewed `SHIP_MOVED.wormholes` route correction
(`dc9ea910`, `BF-ghostcommander-route.md`). No external state, network, folder, worktree or
shared-checkout game-loop edit. This package was built in the existing isolated checkout.

## Printed rule and implementation

Sai Seravus says: “After your ships move: For each ship that has a capacity value and moved
through 1 or more wormholes, you may place 1 fighter from your reinforcements with that ship if
you have unused capacity in the active system.” Unlock: “Have units in 3 systems that contain
alpha or beta wormholes.” The unlock hook already existed.

An `After SHIP_MOVED` listener records each qualifying capacity ship and its nonzero wormhole
route for the active Ghost seat. An `After MOVEMENT_FINISHED` listener computes the legal number
of fighters from that count, current unused system capacity (`fleet::standing`) and remaining
fighter plastic (`supply::remaining`). It offers 1 through that limit plus decline, then places
through the supply-checked `action_cards::place_units_counted`. Waiting until the movement window
closes prevents newly placed fighters from being picked up by a later ship in the same move.
The mark is private, activation-scoped and removed after resolution or when placement is
impossible. The choice occurs before placement, and a declined choice changes no units.

## Tests and checks

- `cargo test -p ti4-engine --lib factions::ghost::tests::sai_seravus -j 4`: 3 passed,
  including positive two-ship placement and negatives for locked commander, no wormhole,
  capacityless ship and full system capacity.
- `cargo test -p ti4-engine --lib sai_seravus_places_a_fighter_after_a_real_quantum_wormhole_move -j 4 -- --nocapture`:
  1 passed. A running `Game` performs a tactical action across a Ghost-only alpha-to-beta
  Quantum Entanglement link; the carrier and new fighter end in the active system.
- `cargo test -p ti4-engine -q --no-fail-fast -j 4`: 1,889 library tests passed,
  1 ignored; integration binaries 1/1, 4/4, 5/5; doctests passed. This was run before a
  documentation-only Clippy cleanup; the focused tests had already passed.
- `cargo test -p ti4-engine --lib print_faction_ledger -- --ignored --nocapture`: passed;
  `ghost 12/12 implemented`; twelve planned factions 123/145 claimed at this commit.
- `cargo clippy -p ti4-engine --all-targets -j 4`: passed. Two new `ghost.rs` documentation
  warnings were fixed, and a repeat showed no Ghost warning; unrelated existing warnings remain.
- `rustfmt --edition 2024 crates/ti4-engine/src/factions/ghost.rs` applied;
  `git diff --check` passed.

## Independent review and limits

A separate Terra reviewer inspected the code, timing, capacity/supply gates, choice legality,
mark lifecycle, no-Ghost behavior and live test; no actionable findings. This is a Terra review
under the operator's model ceiling, not the frontier review named in the plan. The Ghost asset
ledger is complete; the BF faction exit remains open because Phase 2 has not yet seated Ghost in
ti4-sim for its 200-game soak, and the plan's frontier exit review cannot be performed under the
current model ceiling.
