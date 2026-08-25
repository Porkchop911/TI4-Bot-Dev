# M09-022 open review items

## Independent Tier-B review of `26ad269` (2026-08-25)

**Verdict: changes required.** The emitted decomposition, 33-seat separation, source-scope
handling, deterministic dual-namespace delivery, and legacy pins are sound. One package-owned
acceptance test is missing.

### F-M09-022-1 — MEDIUM: active content store is not proven by the required regression

The package specification explicitly requires `ability_facts_use_the_active_content_domain`: load
a second `ContentStore` from a directory and prove emitted facts follow that store rather than the
embedded corpus. It also names the embedded-store trap as a known recurring defect and makes
“active content domain proven by test” part of the definition of done.

The implementation correctly calls `seen.content()` and the source-scope test proves
`seen.sources()`, but `ability_facts_follow_the_active_source_scope` uses the embedded store in both
arms. The evidence openly records that the store half is argued from code inspection rather than
tested. That does not meet this package's predeclared acceptance criterion, particularly after the
M08-019 Y1 precedent established that this exact inference is insufficient regression evidence.

**Required:** construct a bounded valid alternate store through `ContentStore::from_dir`, change a
faction decomposition field in that store, and assert the emitted vector follows the alternate
record while the embedded vector does not. Keep the existing source-scope test; the two tests cover
different halves of the invariant. Record the generated fixture bounds and cleanup mechanism.

### Accepted observations

- **O-M09-022-2 accepted:** per-choice fleet parsing is not a package gate and is deferred to the
  M09-029 end-to-end throughput campaign. No performance claim is made here.
- The `neutral` exclusion predicate and 34-record/33-seat reconciliation match the current corpus.
- All 33 selectable seats have distinct emitted decomposition vectors; the Keleres collision and
  its resolution are non-vacuously pinned.
- Bare facts survive `StateCross::None`; crossed copies remain available to linear schemas.
- The legacy subvector and closed-family inventory pins pass.

### Independent checks

- `cargo test -p ti4-policy ability_` — **3/0**.
- selectable-seat predicate — **1/0**; Keleres separation — **1/0**.
- legacy-subvector pin — **1/0**; inventory pin — **1/0**.
- full `cargo test -p ti4-policy --lib` on combined HEAD — **135/0**.
- `cargo clippy -p ti4-policy --all-targets` — no policy warning; one documented pre-existing
  engine warning at `game.rs:1260`.
- scoped rustfmt and `git diff --check` — clean.

**Next exact action:** add the alternate-store regression, rerun focused/policy/workspace gates,
update evidence, and request a narrow independent Tier-B recheck. M09-022 and dependent M09-024
remain open.
