# OBS-008b2 — reroll-die stochastic surface

## Package

- Milestone: Stage 2 complete decision contract, second slice of `OBS-008b`.
- Dependencies: `OBS-008b1` (`aab65bc`), `OBS-003d` (the `reroll_die` typed context), `OBS-007c`
  (`preview::stochastic::hit_count_preview`, whose own spec deliberately left it unattached: "no
  producer attaches this yet — that is OBS-008b's job").
- Objective: give every reroll-die option its unit/face/threshold identity and the exact d10 hit
  distribution rerolling it would produce, and read the result into the policy — connecting
  OBS-007c's stochastic preview foundation to a real producer for the first time.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008b` row),
  `plans/OBS-007C_STOCHASTIC_PREVIEW_FOUNDATION.md`, LRR 78.3 (reroll) and 78.13 (the hits-on-N
  d10 convention).
- Acceptance references: focused `obs008b2` tests in `combat.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/combat.rs`, `crates/ti4-engine/src/preview.rs`,
  `crates/ti4-policy/src/features.rs`, `crates/ti4-policy/src/projection.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `preview::Quantity` gains `Hits`: the number of hits one roll (or one die) produces, before
   any casualty is assigned. What a hit count implies downstream is producer-specific; this names
   only the count itself, matching the doctrine `hit_count_preview`'s own doc comment states.
2. `combat::choose_reroll_dice` computes each die's current hit status — its face plus whatever
   per-die adjustment (Thalnos's +1) still sits on that position, the same arithmetic
   `RerollEntry::hits` sums over a whole entry — and attaches:
   - to the reroll option: `unit`, `face`, and `hits_on` payload, plus
     `stochastic::hit_count_preview(1, hits_on, …)`, a `Hits` delta from the current status to
     each possible redraw outcome. A fresh d10 draw replaces both the face and its per-die
     adjustment (`RerollEntry::deltas`'s own doc: "the adjustment dies with the die"), so the
     redraw is exactly the uniform, unmodified threshold roll the helper already models.
   - to the decline option: `unit` payload and a `Preview::certain` fact that the die's status is
     unchanged — a certain zero-chance fact, not the reroll's distribution.
   - Both facts are omitted (no preview at all) when `entry.hits_on` is `None`: a roll with no
     recorded threshold has no known consequence to state.
3. `tactical_decision_features`'s pattern is mirrored by a new `combat_decision_features`, guarded
   on subtype `reroll_die`: `combat:subtype:reroll_die`, `combat:option-count`, and from the
   preview — `Certain` reads before/after/change like the tactical surface; `Chanced` reads its
   exact expected value via `Preview::expected`, as `combat:hits-expected`, rather than the whole
   per-case breakdown (that remains future work, not this slice's).
4. `combat` joins `EXPLICIT_FIXED_FAMILIES`, `projection::FAMILY_ROLES` (`Transferable`), and the
   vocabulary registry as its own new family: `OOV_REGISTRY_VERSION` 7 → 8, `OOV_FAMILIES_V8`
   appends `combat`, new pinned fingerprint, one-version-back inference window moves to v7 (v6 is
   now refused exactly as v2-v5).

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes, so no V2
  decision-hash movement.
- No change to reroll legality or application (`apply_reroll_dice` untouched).
- The preview claims only the redrawn die's own hit status, never a downstream casualty, control
  change, or combat outcome — the same boundary OBS-008a4 drew for ground commitment.
- `Preview::expected` is the only distributional detail read into features this slice; the full
  per-case breakdown of a `Chanced` outcome is deliberately not exposed as features yet.

## Tests and commands

- `combat.rs`: an `obs008b2` test proving a reroll option (hitting on 6+, currently a miss) names
  its unit/face/hits_on and previews the exact `Chanced` distribution (5/10 hit, 5/10 miss,
  `out_of` 10) with `Hits` deltas from the current status; the decline option previews a certain,
  unchanged fact instead.
- `features.rs`: an `obs008b2` test proving `combat:subtype:reroll_die`, `combat:option-count`,
  and `combat:hits-expected` (the exact 0.5 expectation) reach the policy from a `Chanced`
  preview, that a `Certain` preview (hitting on 1) reads before/after/change instead, and that the
  facts survive the MLP projection.
- `cargo test -p ti4-engine --lib obs008b2`, `cargo test -p ti4-policy --lib obs008b2`,
  `cargo test -p ti4-policy --lib vocabulary`, `cargo test -p ti4-policy --lib projection`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not preview a downstream casualty or control change from a reroll; only the die's own hit
  status.
- Do not attach a preview when `hits_on` is `None`.
- Do not forget that a redraw clears the die's per-die adjustment; the post-reroll distribution is
  the unmodified threshold roll, not one still carrying the old face's Thalnos bonus.

## Definition of done

Every reroll option names the unit, face, and threshold it concerns and previews the exact d10
hit distribution a redraw would produce; declining previews the unchanged current status; the
policy reads both under a new, approved `combat` family; option identity and V1/V2 replay hashes
are unchanged; focused and full checks pass; only scoped files are committed.
