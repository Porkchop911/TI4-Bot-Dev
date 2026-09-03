# Stage 2 actor observation surface

## Package

- ID/title: Stage2-OBS-001 — complete public game-state surface for actor decisions
- Milestone/dependencies: M09 learned-policy observation boundary and M10 Stage 2 training;
  depends on M09-021 through M09-027b and the current engine state at `b77e18b`.
- Objective: add the high-value, legally observable global, self, opponent, board, timing, economy,
  and strategy-card facts that the MLP actor needs to distinguish materially different decisions.

## Contract

- Normative sources: `docs/MLP_PLAN.md` §§4–5; `plans/M09_LEARNED_POLICY.md` rows 021–027b;
  the typed public/private boundary documented by `ti4_engine::choice::{Observed,SeatObservation}`.
- Acceptance references: focused observation and projection tests in `choice.rs` and
  `projection.rs`; existing M09 hidden-information and critic-invariance tests.
- Writable paths: this specification, `crates/ti4-engine/src/choice.rs`,
  `crates/ti4-policy/src/projection.rs`, package evidence, and the durable execution-state entry.
- Read-only external paths: none.
- Permission: P1 for source/tests/docs. The operator-requested local annotated safe-point tag is a
  bounded P2 action; it names existing commit `b77e18b`, is not pushed, and changes no working file.
- Network/processes: no network or server. Bounded Cargo build/test/lint processes only.
- Generated artifacts: Cargo target outputs only; no training corpus or bundle regeneration.
- Destructive/external actions: none; no remote Git mutation.

## Invariants and compatibility

- The actor receives only public table state plus the already engine-bound acting seat's private
  objective facts. Opponent card identities remain unreachable.
- Feature names describe transferable properties, not player, planet, or system identities.
- The legacy schema-4 extractor remains unchanged. The richer surface is MLP-projection-only under
  the existing `seat-state` family.
- Existing vocabulary columns retain their meaning. New names use the existing family OOV on old
  bundles and receive distinct columns when the next vocabulary is discovered/published.
- Feature construction and ordering are deterministic.
- Non-goals: recurrence/history, an omniscient centralized critic, graph/entity architecture,
  learned embeddings for arbitrary cards, authored valuations, and outcome lookahead.

## Required surface

1. Global/timing: phase, pending tactical step, active/speaker relation, initiative position,
   passed/active counts, active-system presence, and custodians state.
2. Self: VP, public hand counts, passed state, ready/exhausted planets, exact currently spendable
   resources/influence, board footprint/unit count, scored count, scoreable public/secret count,
   fleet-supply headroom, and held/used strategy cards.
3. Opponents: public aggregate VP/economy/token/card/board standing, passed count, leader gap, and
   public strategy-card used/ready state without seat identities.
4. Board/relationships: occupied/contested systems, own/enemy units in the active system, Mecatol
   control/custodians, faceup promissory counts, and Support-for-the-Throne relationships.

## Tests and commands

- Tests prove representative state mutations change the expected named facts.
- Tests prove opponent private hand contents do not change the surface while public counts do.
- Tests prove no emitted name contains player/system/planet identity from the fixture.
- Run `cargo fmt --check`, focused policy/engine tests, affected-crate tests, and strict Clippy.

## Definition of done

The surface above is implemented through the typed observation boundary, focused and affected
checks pass, hidden-information invariants remain green, evidence records exact results, and only
package-owned files are committed. Independent Tier-C review remains a separate acceptance gate.
