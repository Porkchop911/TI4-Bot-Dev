# M09-023 — Secret redaction in feature paths

**ID and title.** M09-023 — Secret redaction in feature paths.

**Milestone and dependencies.** M09; depends on accepted M08-019 and M09-018. Independent of
M09-022, though both land in `features.rs`.

**One-sentence objective.** Prove, across every feature set rather than one, that the acting seat
sees its own secrets and opponents expose public counts only — and emit the public count MLP plan
§5.2 requires, which did not exist.

**Exact normative references.** `docs/MLP_PLAN.md` revision 5 §5.2 (D6), §4.1. The typed
private-view boundary established by M09-021 F-M09-021-1 rounds 2–4 and AA1.

**Exact acceptance-test reference.** M09_LEARNED_POLICY row M09-023: "Acting seat sees own
secrets; opponents expose public counts only across every feature set."

**Review tier.** **C** — hidden information.

## The §5.2 mechanism no longer exists, and that is the right outcome

§5.2 prescribes a specific implementation:

> the feature path must take the acting player's id and build from `Observed::redacted_for(player)`,
> emitting for opponents only `opponent-secrets-held:<n>`

`Observed::redacted_for(viewer)` **was removed** during M09-021 F-M09-021-1 round 2, because it was
the same defect as the finding it sat next to: a public method taking a caller-chosen viewer and
returning that viewer's unredacted cards. Its replacement, `SeatObservation::held_state()`, was
removed in turn at round 4 — a capability that hands out a copy of the state is a state handle, and
the copy's redaction was defeatable by set complement over the unredacted `secret_deck`.

So the plan's prescribed mechanism is obsolete, and what replaced it is **stronger** than what §5.2
asked for. Building features "from a redacted view" means building them from a state copy with
markers substituted; the boundary now is that `Observed` **carries no private data of any seat at
all**, in any form, redacted or otherwise. There is no view to redact.

This package therefore delivers §5.2's *requirement* and not its *mechanism*, and says so rather
than quietly reinterpreting the plan. The half of §5.2 that is still outstanding is the emission:
`opponent-secrets-held:<n>` was specified and never built.

## Allowed Rust edit paths

`crates/ti4-policy/src/features.rs` only. No engine edits. No change to the legacy hashed
extractor's inputs — adding a fact there would silently change what existing schema-2 weights mean.

**Permission class.** P1.

## Invariants

1. **Counts, never identities.** Everything read about an opponent comes from `PublicSeat`, which
   carries counts and no card identity.
2. **Seat-anonymous.** The count keys the family and the value counts the opponents at that count.
   No opponent is named. A per-seat feature would be a board identity that means nothing in the
   next game — the same reason bare option ids are kept out of the explicit path.
3. **Explicit path only.** The legacy hashed extractor's bucket inputs are frozen.
4. **Bare on every option under every crossing mode**, plus crossed copies — the F-M09-021-2
   structure and the §4.1 contract.
5. **Legacy subvector unchanged.**

## Explicit non-goals

- No opponent action-card counts. §5.2 says opponents expose *only* the secrets count; adding more
  public counts is a plan change, not an implementation detail.
- No re-litigation of the M09-021 boundary. This package proves it holds across feature sets; it
  does not rebuild it.
- No change to what the acting seat may see about itself.

## Tests to add

1. `opponent_secrets_expose_counts_and_never_identities_in_any_feature_set` — three seats with
   known distinct holdings; for each seat, both the explicit path and the legacy hashed **name**
   path are extracted and no opponent's alias appears in either. Carries a non-vacuity check.
2. `opponent_secret_counts_are_a_seat_anonymous_distribution` — swapping which opponent holds which
   count leaves the facts identical (anonymity); changing the distribution changes them
   (sensitivity). The second half is what stops the first from being a statement about a constant.
3. `opponent_counts_survive_state_cross_none` — the §4.1 contract.

**On the non-vacuity check.** A held secret reaches the features by *alias* only once it is
satisfied; before that it contributes family-token progress. So test 1 does not assert that an
alias appears — it removes the acting seat's secret records and shows the feature set changes. An
earlier draft asserted the alias directly, and failed: worth recording, because it is exactly the
kind of assertion that would otherwise have been weakened until it passed.

## Commands to run

```
cargo test -p ti4-policy
cargo test --workspace
cargo clippy -p ti4-policy --all-targets
rustfmt --edition 2024 --check crates/ti4-policy/src/features.rs
git diff --check
```

## Known traps

- **The vacuous redaction proof.** "No opponent alias appears" is trivially true if no alias ever
  appears. Every such assertion needs the channel proven live first.
- **The signature argument.** The legacy path takes no secret records, so it *cannot* leak one.
  That is an argument about a signature; M08-019 Y1 is the standing lesson that those need a
  measurement beside them. Test 1 measures the output.
- **Scope creep into the plan.** Public counts for action cards are equally public and equally
  easy; §5.2 says "only", so they stay out until a plan revision says otherwise.

## Definition of done

`opponent-secrets-held:<n>` emitted in both namespaces; no opponent identity reachable in any
feature set, proven by measurement with non-vacuity; pins pass; workspace green; clippy and format
clean; evidence recorded; independent Tier-C review resolved.

**Authorship note.** Written and implemented by Claude Opus 5, who reviewed M08-017 through
M09-021 and cannot review this package. Tier C requires a frontier review; that seat is open.
