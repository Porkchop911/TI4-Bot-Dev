# R01 structured events and compressed replay format

**Date:** 2026-09-11
**Baseline:** `37b236bd9ef2f2f8f6ea9f47d1623865e181bf07`
**Scope:** offline `ti4-review` only; no TTS or bridge path inspected or changed
**Permission:** R01 P1 source/test work plus its already-authorized bounded local smoke artifact

## Result

- Review-session v3 records a reviewer-owned normalized copy of each finalized timing event: event
  id, type, payload and cancellation state. Legacy string event names remain beside it because the
  engine does not yet emit a typed event for every mutation.
- V2 sessions deserialize with empty structured-event fields, upgrade to v3 in memory, and rebuild
  action summaries from their existing frames.
- Completed and in-progress active-player periods share the same summary path. An interrupted action
  is visibly marked in progress rather than being omitted or described as complete.
- Native and self-contained HTML views expose structured payloads, active system, pending stage and
  current agenda vote/prediction state.
- New GUI saves and autosaves default to `.ti4review.json.zst`. Plain `.ti4review.json` remains
  supported for loading and explicit saves.

## Verification

`cargo test -p ti4-review`

- 28 passed, 0 failed after compressed-session and suffixed cancellation coverage was added.

`cargo fmt --package ti4-review -- --check` and
`cargo clippy -p ti4-review --all-targets --no-deps -- -D warnings` both completed cleanly.
The existing 632,478,006-byte, 3,357-frame v2 completed review still validates in about 15.4
seconds; reconstruction is deliberately skipped when an old artifact already carries completed
summaries, avoiding a measured regression to about 44.5 seconds.

Current policy smoke:

```text
checkpoint: out/vponly-fixed-20260910/checkpoints/checkpoint-7612/slots.json
map pool: out/pools/full_np8_12_final.json
seed/rotation/temperature: 42 / 0 / 0.01
command: one complete action
result: 13 frames, 12 steps, 11 decisions, 1 action, target reached, still in progress
structured events: 13
```

The equivalent v3 artifacts measured:

```text
plain JSON: 1,123,917 bytes
zstd:         21,228 bytes
```

The compressed artifact validated and rendered to self-contained HTML. The embedded JavaScript
parsed successfully and contained structured-event rendering.

## Remaining limits

- Typed events still do not cover every physical mutation. Summary generation therefore uses
  structured events for causality, decision payloads for selected intent, and state deltas only for
  unexplained visible changes. It must not label an unexplained delta with an invented cause.
- Compression substantially reduces disk/write volume but does not remove full `GameState` clones
  held per live frame. Periodic full checkpoints plus typed deltas remain the correct deeper storage
  redesign if memory or serialization latency remains material.
- The complete Wormhole Nexus lifecycle remains engine-owned. The reviewer displays the face found
  in state and does not invent transitions.
- Independent R01 review remains waived by the operator. Formatting, tests and lint gates remain
  required.
