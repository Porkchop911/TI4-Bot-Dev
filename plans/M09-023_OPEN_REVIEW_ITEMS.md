# M09-023 open review items

## Independent Tier-C review of `662e27c` (2026-08-25)

**Verdict: accepted for the M09-023 delta.** No blocking finding.

The new `opponent-secrets-held:<n>` family is derived exclusively from
`PublicSeat.secret_objectives_held`; no private identity enters the builder. The count-distribution
representation is seat-anonymous and sensitive to actual count changes. Bare facts survive
`StateCross::None`, and the crossed namespace remains available for linear schemas. The legacy
extractor is unchanged.

The broader feature flow is also structurally sound: live own-secret records originate from the
accepted bound `SeatObservation`; the legacy path receives no private records; offline callers that
compute records directly already possess the complete `GameState`. The package does not reopen an
arbitrary-viewer capability.

### Open-item dispositions

- **O-M09-023-1 accepted as LOW:** the alias-absence output test covers one fixture/choice kind,
  but the structural boundary plus the `StateCross::None` and existing M09-021 tests cover the
  invariant independently. A future metamorphic full-vector sweep would strengthen defence in
  depth but is not required to accept this delta.
- **O-M09-023-2 accepted:** emitting the zero-held bucket is a coherent factual representation;
  its value remains nonzero whenever the bucket exists, so it does not conflict with zero-skip.
- **O-M09-023-3 accepted/deferred:** no performance claim is made; M09-029 owns the end-to-end
  throughput gate.

### Independent checks

- `cargo test -p ti4-policy opponent_secret` — **3/0**.
- `cargo test -p ti4-policy opponent_counts_survive_state_cross_none` — **1/0**.
- legacy-subvector pin — **1/0**; inventory pin — **1/0**.
- full `cargo test -p ti4-policy --lib` — **135/0**.
- `cargo clippy -p ti4-policy --all-targets` — no policy warning; one documented pre-existing
  engine warning at `game.rs:1260`.
- scoped rustfmt and `git diff --check` — clean.

M09-023's code delta is Tier-C accepted. Because it is stacked on the still-open M09-022 branch,
integration and M09-024 remain blocked until F-M09-022-1 is corrected and the resulting combined
frontier is rechecked for overlap.
