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

## F-M09-022-1 correction — alternate-store regression (implementer, 2026-08-25)

The review is right, and the finding is against the package's own predeclared acceptance test. The
evidence recorded the gap honestly as O-M09-022-1, but recording a gap is not the same as closing
one, and "resolves through the active store" was left resting on code inspection — the exact
standard M08-019 Y1 established as insufficient.

### What was added

`ability_facts_use_the_active_content_store`. A second corpus is built through
`ContentStore::from_dir`, one decomposition field is changed in it, and the emitted vector is
asserted to follow the store the observation carries.

- **Fixture:** the embedded corpus directory (30 files, **992 KiB**) copied into the OS temp
  directory under `ti4-m09-022-store-<pid>-<value>`; `factions.json` rewritten so `sol`'s
  `commodities` reads **9** against the corpus's real **4**.
- **Cleanup:** an `EditedCorpus` guard with a `Drop` implementation that removes the directory,
  so it is cleaned on panic as well as on success (`Drop` runs while unwinding). Verified after the
  gate run: **0** directories matching the fixture name remain. Nothing is generated into the repo
  and nothing is committed.
- **Both directions asserted:** the alternate arm reports 9, the embedded arm reports 4, and the
  two are asserted to disagree — so the test cannot pass by both arms reading the same store.
- **Non-degeneracy:** the alternate store must produce abilities at all, and the set of facts
  differing between the two stores must be **exactly** `["faction-commodities"]`. An empty or
  broken alternate corpus fails both. This is what stops the assertion passing for the wrong
  reason.

### Falsification check — the test was proved to catch the defect it exists for

A passing test is not evidence until it can fail. `ability_facts` was temporarily mutated to
`let content = ti4_content::ContentStore::embedded();` — the exact defect the finding names — and
the focused set re-run:

```
test features::tests::ability_facts_use_the_active_content_store ... FAILED
    assertion `left == right` failed: the emitted fact did not follow the alternate store
test result: FAILED. 2 passed; 1 failed; 0 ignored; 133 filtered out
```

Two points in that output. The new test **fails** on the mutant, so it detects the defect. And the
other two `ability_facts_*` tests — including `ability_facts_follow_the_active_source_scope` —
**pass** on the mutant, which confirms the review's central claim: the source-scope test covers a
genuinely different half of the invariant and could never have caught this. The mutation was
reverted and the file re-formatted; the suite is green on the reverted tree.

### Gates after the correction

```
cargo test -p ti4-policy --lib      136 passed, 0 failed   (135 before)
cargo test --workspace             1379 passed, 0 failed   (1378 before)
cargo clippy -p ti4-policy --all-targets
    no warning in ti4-policy; one pre-existing ti4-engine warning
rustfmt --edition 2024 --check crates/ti4-policy/src/features.rs   clean
git diff --check                   clean
temp fixture directories remaining after the run   0
```

### Disposition

**O-M09-022-1 is closed** — the store half of invariant 2 is now proven by a test that has been
shown to fail without the property. O-M09-022-2 (per-choice fleet parsing) remains accepted and
deferred to M09-029, per the review.

Requesting the narrow independent Tier-B recheck. M09-022 remains open until it lands; M09-023 is
already accepted for its delta, and M09-024 unblocks once the combined frontier passes its overlap
recheck.

## Narrow independent Tier-B recheck of `b444f52` (2026-08-25)

**Verdict: accepted. F-M09-022-1 is resolved and closed.**

The new regression loads a separately copied and modified corpus through
`ContentStore::from_dir`, changes Sol's commodity decomposition from the embedded value 4 to 9,
and proves the emitted feature follows each observation's active store in both directions. Its
non-degeneracy assertions prove the alternate store remains functional and that exactly the edited
feature differs. The recorded mutation check directly falsifies the embedded-store defect while
leaving the separate source-scope test green, confirming that both tests are necessary.

Independent checks: active-store **1/0**; active-source-scope **1/0**; M09-023 opponent overlap
**3/0**; legacy-subvector pin **1/0**; inventory pin **1/0**; scoped Clippy adds no policy warning;
rustfmt and `git diff --check` clean; **0** matching temporary fixture directories remain.

M09-022 is complete and independently accepted. The previously accepted M09-023 delta remains
valid on the combined frontier. M09-024 is dependency-ready.
