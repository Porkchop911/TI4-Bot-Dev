# Evidence — OBS-008c2a production-limit surface

## Scope and provenance

- Branch: `wp/obs-008c2a-production-limit-surface`.
- Base: `4bdf247` (`OBS-008c1`).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`,
  `plans/OBS-008C2A_PRODUCTION_LIMIT_SURFACE.md`, LRR 68.1/68.1a/68.3b, and the existing
  Bellum Gloriosum helpers in `crates/ti4-engine/src/breakthroughs.rs`.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, dependency, or generated
  committed artifact.

## Changed paths

- `crates/ti4-engine/src/preview.rs`
- `crates/ti4-engine/src/production.rs`
- `crates/ti4-policy/src/features.rs`
- `plans/OBS-008C2A_PRODUCTION_LIMIT_SURFACE.md`
- `plans/evidence/OBS-008C2A.md`
- `plans/EXECUTION_STATE.md`

The two pre-existing untracked review samples were neither read nor staged.

## Result

- Each production-unit selection has typed Rule 68 context, system target, and a
  `ProductionCapacity` constraint containing the original limit and currently consumed amount.
- Each offered build reports the actual bill, printed bill, batch count/yield, carried credit,
  credit used, amount owed, and production capacity spent. IDs, labels, legal set, payment flow,
  and state application are unchanged.
- Each build carries an analytic preview for remaining production and the per-use Bellum Gloriosum
  allowance. It mirrors placement arithmetic directly; it does not clone or apply game state.
- The policy exposes bounded, transferable `production` facts for the context, marginal values,
  and known preview deltas. Zero/unknown facts are not fabricated.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs008c2a` | 2 passed |
| `cargo test -p ti4-policy --lib obs008c2a` | 2 passed |
| `cargo test -p ti4-engine --quiet` | 1,141 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 197 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign, 328.21 s |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings` | passed |
| `git diff --check` | passed |

`cargo fmt --check` reports pre-existing formatting drift in unrelated `ti4-content`, `ti4-model`,
and `ti4-sim` files. Direct checking of the modified source files also reaches pre-existing drift
elsewhere in `production.rs`; no formatter diff touches this package's added hunks. No unrelated
formatting was written.

The policy suite is intentionally split so the long nested-window campaign stays observable: 196
ordinary tests plus its one 102-game campaign test cover all 198 policy library tests.

## Counterfactual and analytic-agreement evidence

- The engine test applies a fighter choice and compares both previewed quantities to the completed
  production window.
- A Sol capacity ship opens exactly the previewed allowance; the subsequent fighter batch consumes
  it without reducing the production limit, exactly as previewed.
- Otherwise-identical policy choices independently vary used capacity, carried credit, and allowance
  aftermath; each changes its named feature. Used capacity also survives MLP projection and is
  admitted by the production family.

## Independent Tier-C review

Reviewer: independent frontier review agent `observation_review`.

Initial verdict: changes required, two P2 acceptance-test gaps. The context test established only
the initial zero-consumed limit, and policy tests established only a certain preview. Resolution:
after completing a build, the engine test now asserts the next choice's `paid == limit - remaining`;
a second policy test covers absent, unknown, and unavailable previews and confirms that each lacks
numeric remaining/allowance aftermath while the latter two retain distinct markers.

Recheck verdict: **PASS / APPROVED**. The continuation assertion proves the full limit stays stable
while `paid` and `remaining` reflect a completed build; absent, unknown, and unavailable previews
have no numeric production-limit or allowance aftermath, while unknown/unavailable retain their
distinct markers. No remaining mechanics, arithmetic, hidden-information, replay, serde, identity,
performance, or test-adequacy finding remains.

## Non-goals retained

Fleet-supply and transport consequences of the later placement choice remain `OBS-008c2b`. This
slice does not alter payment legality or execution, introduce a vocabulary artifact, republish a
bundle, or migrate prompt-free choices.
