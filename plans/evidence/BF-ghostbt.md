# BF-ghostbt — Ghost Particle Synthesis

## Scope and permissions

- Permission class: P1.
- Writable paths used: `crates/ti4-engine/src/factions/ghost.rs` and this evidence file only.
- Read-only inputs: `AGENTS.md`, `plans/BF_AGENT_GUIDE.md`, `plans/evidence/BF-ghost.md`,
  `crates/ti4-content/content/breakthroughs.json`, the faction-hook contracts, and production
  call sites.
- Network, external state, generated artifacts, destructive actions, and commits: none.

## Printed rule and implementation

`ghostbt` (Particle Synthesis, Thunder's Edge) says: “Each wormhole in a system that contains
your ships gains PRODUCTION 1 as if it were a unit you control. Reduce the combined cost of units
you produce in systems that contain wormholes by 1 for each wormhole in that system.”

`ghost.rs` claims `ghostbt` and registers both `EconomyHooks::extra_production` and
`EconomyHooks::production_cost_reduction`. Both require that the player holds `ghostbt`; only the
PRODUCTION clause also requires one of that player's actual ships in the space area.

The counter deliberately does not use `hooks_movement::wormholes_at`, because its `BTreeSet` of
kinds would collapse a printed Alpha plus an Alpha token. It counts each physical source:

- printed tile wormholes from the embedded DEFAULT corpus;
- every entry in `GameState::wormhole_tokens` at the system;
- the ion-storm wormhole side;
- both new wormholes when the Nexus is unlocked, in addition to its printed Gamma; and
- Hil Colish's Delta from the module's existing extra-wormhole route.

The discount hook lacks content/source arguments, so it uses the same embedded DEFAULT corpus
assumption as `hooks_movement::wormholes_at`; production is built with DEFAULT in this package.

## Tests

New inline tests in `factions::ghost`:

- `particle_synthesis_counts_physical_wormholes_for_live_production_capacity` grants the card,
  uses a printed Alpha plus Alpha and Beta tokens, and verifies the live
  `production::capacity` result is 3. It also covers a token, ion storm, and Hil Colish Delta;
  and the unlocked Nexus's printed Gamma plus Alpha/Beta.
- `particle_synthesis_needs_its_card_and_a_ship_and_reduces_a_live_production_bill` verifies the
  normal cruiser cost remains 2 without the card through `ProductionWindow`, that an infantry is
  not a ship, then verifies one printed Alpha adds capacity and makes the live cruiser bill 1.
- Existing `a_game_without_a_creuss_seat_is_unchanged_and_offered_nothing` remains green, covering
  the six-faction/no-Ghost regression route.

## Validation

| Command | Result |
| --- | --- |
| `rustfmt --edition 2024 crates/ti4-engine/src/factions/ghost.rs` | passed |
| `cargo test -p ti4-engine --lib factions::ghost` | 24 passed, 0 failed |
| `cargo clippy -p ti4-engine --all-targets` | no warnings in `ghost.rs`; 18 pre-existing warnings in other files/test targets |
| `cargo test -p ti4-engine --lib print_faction_ledger -- --ignored --nocapture` | passed; ledger reports `ghost 11/12 implemented` (only `ghostcommander` remains) |

## Unresolved issues

None for Particle Synthesis. Sai Seravus's commander effect remains separately unimplemented.

## Integration check and independent review

- `cargo test -p ti4-engine -q --no-fail-fast -j 4`: 1,884 library tests passed, 1 ignored; integration binaries 1/1, 4/4, 5/5; doctests passed.
- Independent Terra review of the diff, printed text, economy-hook contract and `ProductionWindow`: no actionable findings. The reviewer confirmed the discount is a use-scoped combined-bill pool and the hooks do not mutate state. This is a Terra review under the operator's model ceiling, not a frontier review.
