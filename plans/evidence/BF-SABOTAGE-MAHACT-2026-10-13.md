# Sabotage targets and Mahact command tokens

## Scope and authority

Operator request, 2026-10-13: offer Sabotage after the targeted card chooses its targets;
check Mahact agent and Edict token handling; include Mahact's own token for Scepter when
Mahact has its own tokens in the fleet pool.

P1 local source/test/evidence changes in the supplied Rust checkout. No dependencies,
network, external writes, destructive actions, or live game interaction. Cargo artifacts
and validation logs stay under ignored `target/`. Existing branch: `codex/diplomacy-v1`,
starting HEAD `bc7a8e17`; changes remain uncommitted alongside preexisting work.

Normative source: operator's requested Sabotage timing and embedded PoK printed records
`abilities.json` (`edict`), `leaders.json` (`mahactagent`), and
`promissory_notes.json` (`scepter`). No historical Python code was used.

## Implementation

- A one-shot action-card effect gate opens the existing `ACTION_CARD_PLAYED` cancellation
  window after target selection and before effect mutation, payment, or dice rolls.
  Target IDs are attached to the reaction option. Multi-target selection stages targets
  before applying effects; forced singleton selections are recorded too. Opponent private
  responses, such as giving a note for Diplomatic Pressure, remain effect resolution.
  Cancelled cards are still discarded, and Sabotage cannot itself be sabotaged.
- Edict records opponents' tokens separately from Mahact's own redistributable pools.
  Both space and ground combat wins collect at most one token from each seated opponent,
  only when that opponent has a token in reinforcements. Foreign tokens increase fleet
  supply and consume the original owner's reinforcement supply. Fleet laws still apply.
- Jae Mir Kan substitutes the active player's board token for a secondary token payment
  and exhausts. The token returns to its owner's reinforcements except for Construction,
  which places that token in the system where the structure is placed. The printed agent
  does not collect the token into Mahact's fleet pool.
- Scepter is offered at strategy-phase start, chooses a non-home system holding the
  holder's units, places eligible issuers' reinforcement tokens, and returns the note.
  Mahact is an issuer only when its own fleet pool has tokens. The holder is excluded.
- Foreign token identities are public; serialization defaults the new field when loading
  older state and omits it when empty, preserving existing empty-state serialization.
- Updated the decision-delivery inventory for the new Scepter prompt. Also classified the
  already-present, preexisting dirty Ssruu activation-copy prompt after checking that its
  delivery uses the observed choice API; its implementation was preserved.

Touched source: action_cards, reactions, timing, choice, game, legendary (context field),
faction_abilities, fleet, combat, invasion, leaders (registry/context fields only atop
preexisting edits), promissory, strategy, strategy_cards, and model state. Regression
tests and the delivery inventory cover the affected contracts. No policy feature schema,
dependencies, or unrelated training configuration changed.

## Independent review

Separate agents implemented card checkpoints and Mahact, then reviewed the opposite
scope. Review findings resolved: Scepter initially tested all own pools instead of fleet
only; Reparations selected one target after the gate; several forced targets were absent
from the reaction payload; Construction needed the substituted token's owner. The card
agent's final audit accepted the corrected gates. Root reviewed token accounting, issuer
placement, call sites, nested source restoration, and consolidated checks.

## Validation

- `cargo test -p ti4-engine -p ti4-model --offline -j 1`: PASS, 1,349 engine unit tests,
  5 engine integration tests, 83 model tests, and 5 doctests (1,442 total).
- New tests cover cancellation after target choice, five-target cancellation with no
  goods placed, Edict deduplication/supply/redistribution, agent payment and Construction,
  Scepter fleet-only issuer and delivery/return, and old/new state serialization.
- `cargo clippy -p ti4-engine -p ti4-model --all-targets --offline -j 1`: completes;
  existing style/size/documentation warnings remain. Strict `-D warnings` is not green.
  New checkpoint and API-documentation warnings found during validation were fixed.
- Scoped Rust formatting and `git diff --check -- crates/ti4-engine crates/ti4-model` pass.
- An earlier inventory run exposed the unclassified new Scepter and preexisting Ssruu
  prompts; both observed delivery sites were inspected and registered, not bypassed.
- Earlier parallel builds hit a Windows linker sharing error and then a compiler memory
  allocation failure. Serial `-j 1` validation succeeded. No unrelated process was stopped.

Logs: `target/engine-rule-fixes-tests.log` and `target/engine-rule-fixes-clippy.log`.
Full workspace, live bridge, benchmarks, and learning campaigns were not run. Mahact
commander/Imperia and copied-agent expansion are outside this request.

Preexisting dirty edits in leaders, training script, proposal documents, and Ssruu evidence
were retained. Repository-wide whitespace checking also reports preexisting whitespace in
the training script; scoped checks pass. No commit or push was made.
