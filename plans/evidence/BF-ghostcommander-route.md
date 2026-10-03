# BF-ghostcommander-route — wormhole hops in SHIP_MOVED

## Scope and authority

P1 change in the existing `game.rs` only, plus this evidence file. This package corrects the
`wormholes` payload supplied to timing listeners; it does not implement or claim Sai Seravus.
No network or external-state changes. The existing isolated in-repository worktree was reused.
The other session's uncommitted `game.rs` income and relic-gain edits in the shared checkout were
not touched.

## Rule and behavior

Sai Seravus says: “For each ship that has a capacity value and moved through 1 or more wormholes,
you may place 1 fighter from your reinforcements with that ship if you have unused capacity in
the active system.” The route event must report a wormhole hop before that effect can be offered.
The previous payload counted only non-hex hops whose endpoints shared a wormhole kind. It missed
Quantum Entanglement's alpha-to-beta link for the Ghost player. The route now counts non-hex
adjacency supplied by the map's wormholes, excluding unrelated `extra_links`, and also counts the
Ghost faction's alpha/beta Quantum Entanglement link even when ordinary wormholes are disabled.
The active map already receives token, ion-storm, Nexus, and flagship wormholes through
`laws::apply_to_galaxy`. A geometric hex move still counts as zero wormhole hops. The common case
with no ability links makes no map clone.

## Red/green and validation

- Focused test first: compilation failed because `wormhole_hops` did not exist.
- `cargo test -p ti4-engine --lib ship_moved_wormholes -j 4`: 2 passed. Cases: Ghost alpha to beta,
  Sol negative, unrelated ability link negative, real token wormhole, Travel Ban, Hil Colish delta,
  and ordinary adjacent-hex movement.
- `cargo test -p ti4-engine -q --no-fail-fast -j 4`: 1,886 library tests passed, 1 ignored;
  integration binaries 1/1, 4/4, 5/5; doctests passed. This was run before the final
  Travel Ban assertions, after which the focused 2/2 tests passed.
- `cargo clippy -p ti4-engine --all-targets -j 4`: passed; no new `game.rs` warning. Existing
  warnings in other files and the prior unfulfilled lint expectation in `game.rs` remain.
- `rustfmt --edition 2024 crates/ti4-engine/src/game.rs`: applied; `git diff --check` passed.
- Clean 30-seed six-faction `cargo run --release -p ti4-sim --example rebaseline_behavior -j 4`
  with the pinned absolute LIBTORCH: all ten observed points match the `c5632448` values recorded
  in `BF-CODEX-2026-10-03.md`, including `share_SHIP_MOVED 0.046439` and `vp_pace 0.472222`.
  Five points are outside the retired v45 bounds, exactly as before this route correction; no
  behavior bounds were changed.

## Limit

The commander effect remains unimplemented and unclaimed. This package only makes its event
payload accurate for the tested movement links. Invalid-choice atomicity of after-move timing
windows is a separate shared `note_arrival` concern (`emit_typed` currently ignores errors).

## Independent review

A separate Terra reviewer inspected the patch, galaxy/law and Ghost-link paths, and event payload. No actionable findings. This is a Terra review under the operator's model ceiling, not a frontier review.
