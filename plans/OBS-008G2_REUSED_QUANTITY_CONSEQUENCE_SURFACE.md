# OBS-008g2 — reused-quantity content consequence surface

## Package

Second preview slice of the `content` family. Where `OBS-008f2` added a new quantity, this batch
covers three subtypes across trade/reactions/action-cards whose consequence is exactly a quantity
the engine already models: Munitions Reserves spends trade goods, Peace Accords gains a controlled
planet, and Skilled Retreat moves a fleet — the same shapes `pay`, a new (till now unused)
`PlanetsControlled`, and `tactical`/`combat`'s own `ShipsInSystem` already carry.

- Dependencies: `OBS-008f2` (`6a5985b`, the `content` family's first preview reader),
  `OBS-007a`/`OBS-008b3` (the `ShipsInSystem`/`PlanetsControlled` quantities themselves).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008g` row), LRR 78.7
  (Skilled Retreat borrows the ordinary retreat rule), the Peace Accords and Munitions Reserves
  faction-ability texts.
- Writable paths: `crates/ti4-engine/src/faction_abilities.rs`, `crates/ti4-engine/src/action_cards.rs`,
  `crates/ti4-policy/src/features.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **`munitions_reserves_reroll`** (`space_combat_round_started`): the "reroll this round's
   misses" option previews the seat's trade-good pool falling by exactly `MUNITIONS_COST` (2).
   Declining previews no change (it carries no preview at all, matching every other decline this
   session left unpreviewed). This closes a real gap: the option's 2-trade-good cost previously
   reached no numeric feature at all, because the option's kind (`"ability"`) never routed through
   `payment_decision_features` (which requires kind `"pay"`).
2. **`peace_accords_annex`** (`strategy_resolved`): every candidate planet previews the seat's own
   controlled-planet count rising by exactly one — uniform across options, the same shape
   `OBS-008d2`'s token-gain preview used, because annexing is the same consequence whichever planet
   is chosen. First use of `Quantity::PlanetsControlled`, declared since `OBS-007a` but never wired
   to a producer until now.
3. **`skilled_retreat_choose_system`** (`skilled_retreat`): mirrors `OBS-008b3`'s `retreat_to`
   exactly — 78.7 (which this card borrows) moves the whole remaining fleet together, so every
   destination previews the seat's own ship count there rising by the fleet size leaving the active
   system.
4. **Policy.** `content_decision_features` (which began reading previews in `OBS-008f2`) gains three
   more quantity mappings: `TradeGoods` -> `content:trade-goods-*`, `PlanetsControlled` ->
   `content:planets-controlled-*`, `ShipsInSystem` -> `content:ships-*`. No new family, no
   vocabulary change — these are specific quantity names inside an already-registered family.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- `orbital_drop_choose_planet`/`crashlanding_choose_ground`/`crashlanding_choose_planet`/
  `silence_choose_system` stay unpreviewed: each is a pure identity choice (which planet, which
  unit kind, which origin) whose consequence does not vary in magnitude across options, or (for
  Silence) grants a permission rather than a quantity.
- `orbital_drop_deploy_mech` was checked and found already covered: its kind (`"place"`, mapped to
  `"produce"`) already routes it through `production_decision_features`, which already reads its
  `cost`/`count` payload.

## Tests and commands

- `faction_abilities.rs::obs008g2_munitions_reserves_previews_its_exact_trade_good_cost`: the
  reroll option previews `5 -> 3` trade goods.
- `faction_abilities.rs::obs008g2_peace_accords_previews_the_planet_count_gain`: the offered
  candidate previews the seat's controlled-planet count rising by exactly one.
- `action_cards.rs::obs008g2_skilled_retreat_previews_the_exact_arrival_count`: an occupied
  destination previews `1 -> 3`; an empty one previews `0 -> 2` — both reading the same
  two-ship fleet leaving the active system.
- `features.rs::obs008g2_reused_quantity_previews_reach_the_policy`: all three map to
  `content:{trade-goods,planets-controlled,ships}-*` and survive `projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

Three more `content` subtypes preview their exact consequence, reusing existing quantities; the
policy reads all three under the existing `content` family; no new family, no vocabulary change;
option identity and V1/V2 replay hashes unchanged; full checks pass; only scoped files committed.
