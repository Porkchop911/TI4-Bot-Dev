# R01 implementation evidence — offline graphical viewer

## Scope

Operator-directed one-pass implementation of the independent R01 review application. The operator
explicitly waived R01 package sequencing and independent reviews. The migration milestones, M10
training acceptance, TTS bridge, engine state, and historical Python reference are unaffected.

## Delivered

- New `ti4-review` workspace crate and `ti4-review` CLI.
- Strict canonical JSON bundle validation before render: size bounds, duplicate keys, closed object
  shapes, checksums, state/timeline references, bounded collections, IDs, NFC text, and terminal
  status.
- Self-contained local HTML/SVG output with a hex board, systems/planets/unit labels, player cards,
  timeline controls, and frame facts. No web server, external assets, or network request is used.
- `example`, `validate`, and `render` CLI commands, including a sample bundle for immediate use.

## Commands and exact results

```text
cargo fmt
cargo clippy -p ti4-review --all-targets -- -D warnings
Finished `dev` profile ...

cargo test -p ti4-review
3 passed; 0 failed

cargo run -p ti4-review -- example target/r01-smoke/sample.ti4review.json
cargo run -p ti4-review -- validate target/r01-smoke/sample.ti4review.json
valid: target/r01-smoke/sample.ti4review.json
cargo run -p ti4-review -- render target/r01-smoke/sample.ti4review.json target/r01-smoke/sample.html
```

The resulting sample bundle was 1,858 bytes and its self-contained viewer page was 5,047 bytes.

## Deliberate boundary

This is a finished offline viewer for valid ReviewBundle input, not an engine capture adapter.
`ti4-sim` replay remains a stub, so no claim is made that a current simulation can yet generate a
complete safe review timeline. The viewer's crate has no dependency on `ti4-engine`, `ti4-sim`,
`ti4-training`, or `ti4-bridge`.
