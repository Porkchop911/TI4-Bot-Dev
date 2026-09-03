# OBS-003b — replay and decision-hash migration

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-003a`.
- Objective: bump the canonical decision-hash contract so a typed context can be bound, keep every
  existing replay readable and byte-identical, and pin which context fields participate.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-003b;
  `crates/ti4-engine/src/fingerprint.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/{fingerprint,choice}.rs`, this specification, evidence.
- Network, external state, destructive actions: none.

## The contract

`CanonicalHashVersion::V2` is added. `DecisionRecord` gains
`context: Option<DecisionContext>`, `#[serde(default, skip_serializing_if = "Option::is_none")]`.

- **V1 hashes the record with any context stripped.** Not "usually equal" — stripped, by
  `DecisionRecord::without_context`, so the answer does not depend on whether a caller happened to
  attach one. An old replay keeps its digest, and a new record can still be fingerprinted the old
  way to compare against an old trace.
- **V2 binds the context.**
- `skip_serializing_if` means a record with no context serialises to exactly the bytes it always
  did, so the V1 golden value is unchanged rather than recomputed.

## Participating fields

`V2_CONTEXT_FIELDS` names the eight identity fields and `V2_CONTEXT_QUANTITIES` names `outstanding`.
A test asserts their union equals `DecisionContext::visibility()`'s keys, so adding a context field
without deciding its fingerprint role fails rather than being swept in by `derive(Serialize)`.

`outstanding` participates deliberately. It is what distinguishes "pay three influence" asked with
one influence of credit from the same sentence asked with none, and a replay that could not tell
those apart would not be a replay. `actor` participates although `DecisionRecord::player` already
carries it, so the fingerprint is checkable from the context alone.

## Invariants and non-goals

- No producer supplies a context yet; `DecisionLog::record` writes `None`. Populating producers is
  OBS-003d–h.
- No legal option set, option id, or prompt changes.
- The V1 golden digest `7214f574…` is unchanged and asserted from a JSON fixture that predates the
  field, not only from a struct literal.

## Tests and commands

- `cargo test -p ti4-engine --lib fingerprint`
- `cargo test -p ti4-engine`, `cargo test -p ti4-training`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`
- `cargo fmt -p ti4-engine -- --check`

## Definition of done

An old replay JSON without the field deserialises, defaults to no context, and keeps its digest; V1
cannot distinguish records that differ only in context while V2 can; changing `subtype` or
`outstanding` changes the V2 digest; the participating-field set is pinned against the visibility
table; checks pass; no producer or option set changed.
