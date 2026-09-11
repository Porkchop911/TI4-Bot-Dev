# R01 current-engine reviewer integration

Date: 2026-09-11

The current reviewer UI was transplanted onto integration commit `e1ee387`. The earlier reviewer
checkout diverged before current engine commits including `d151134` (Fracture systems become legal
tactical activation targets once in play) and `f6013db` (continued expedition-slice availability).

Scope was limited to `ti4-review`, its Cargo dependency entry, and the timing resolver's read-only
finalized-event journal. No TTS or bridge path was changed.

Verification:

- `cargo test --release -p ti4-review`: 28 passed.
- Focused engine regression
  `fracture::tests::a_fracture_system_can_be_activated_once_the_fracture_is_in_play`: passed.
- `cargo clippy --release -p ti4-review --all-targets --no-deps -- -D warnings`: passed.
- Focused reviewer and timing formatting checks: passed. The workspace-wide engine formatting check
  still reports pre-existing differences in unrelated current-engine files; they were not rewritten.
- Rebuilt the canonical `target/release/ti4-review.exe`.

## Empty-system follow-up

The native reviewer formerly gated selected-system information on a dynamic `state.board` entry.
Empty printed systems therefore showed no metadata. The same assumption excluded technology
specialties in empty map systems from Fracture ingress placement. The reviewer now always shows
the selected tile's static metadata, and ingress candidates enumerate the actual galaxy plus
dynamic board additions. A focused regression removes a specialty system from `state.board` and
proves that its planet remains eligible.
