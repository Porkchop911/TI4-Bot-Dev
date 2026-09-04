# OBS-003h slice 2 — reaction and remaining content context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`; `OBS-003h` slice 1 (`576150a`).
- Objective: attach a stable `DecisionContext` to every remaining registered reaction/content
  `Choice` producer, without changing the choices themselves.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003h`); LRR 2, 22, 35,
  45.4, 81.5; and the embedded printed effect text for the named content.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/{exploration,faction_abilities,laws,reactions,relics,
  secrets,thunders_edge}.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Producers and context identity

| module | producer(s) | source | subtype(s) |
|---|---|---|---|
| exploration | instant-card reward choice | `Content(card)` | `{card}_choose_reward` |
| faction abilities | Orbital Drop, Peace Accords, Munitions Reserves | `FactionAbility(alias)` | `orbital_drop_choose_planet`, `orbital_drop_deploy_mech`, `peace_accords_annex`, `munitions_reserves_reroll` |
| laws | law discard offer | `Content(law)` | `offer_discard_law` |
| reactions | action-card reaction window | `Rule(22.1)` | `play_reaction_{relation}_{event}` |
| relics | technology, Codex, Titan Prototype, Stellar Converter, Crown, Dominus Orb, Neuraloop | `Content(relic)` | card-specific choice subtype |
| secrets | secret-objective hand-limit return | `Rule(45.4)` | `return_over_secret_hand_limit` |
| Thunder's Edge | expedition action/secret discards | `Content(thunders_edge)` | `expedition_discard_action_card`, `expedition_discard_secret` |

## Boundaries and acceptance

- No option IDs, labels, legal sets, mechanics, replay behavior, or policy features change.
- The relic technology helper accepts its content alias from each caller; exploration's Enigmatic
  Device passes its own alias rather than impersonating the relic.
- Add an end-to-end captured exploration choice regression. Check every in-scope `Choice` site for
  a contextualization, then run focused and affected-crate tests, strict Clippy, formatting, and
  diff checks. Tier C independent review is required before commit.
