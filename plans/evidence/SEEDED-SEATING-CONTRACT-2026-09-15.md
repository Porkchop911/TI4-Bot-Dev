# Repository-wide seeded seating contract

Date: 2026-09-15

Every game-producing path now assigns factions through
`ti4_training::rollout::seated_faction(factions, seed, rotation, seat)`. The function draws one
deterministic permutation from the existing game seed and cyclically shifts that same permutation
for rotations. Reviewer, rollout, teacher/vocabulary corpora, training entry points, clearance and
cross-play evaluation, PPO, policy gates, cost tools, probes, reports, and offline corpus capture
use this contract.

The legacy `set_seat_scramble` API remains as a no-op for source compatibility. It cannot disable
seeded seating, and `seat_scramble()` always reports true. Stage-1 and Stage-2 manifests therefore
record scrambling as enabled without requiring an opt-in flag.

Verification:

- All targets in `ti4-training`, `ti4-mlp`, and `ti4-review` compile in release mode.
- `the_shared_seating_contract_cannot_fall_back_to_fixed_rotation` passes and checks several seeds,
  all rotations, and all seats against the shared permutation function, even after requesting the
  legacy false setting.
- `faction_order_is_seeded_reproducible_and_rotated` passes in the reviewer.
- Static inventory finds no remaining direct fixed cyclic faction assignment in game producers.
- Strict library Clippy is blocked by three pre-existing `ti4-training` findings in `ppo.rs` and
  `stage1.rs`; the affected-target compile reports only pre-existing example warnings.

Unrelated in-progress offline-corpus edits already present in the working tree were preserved.
