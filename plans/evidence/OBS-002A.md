# OBS-002a — decision producer and delivery audit evidence

## Identity and scope

- Branch: `wp/obs-002a-decision-delivery-audit`
- Base: `4276188` (`STAGE2_COMPLETE_DECISION_CONTRACT` plan)
- Specification: `plans/OBS-002A_DECISION_PRODUCER_DELIVERY_AUDIT.md`
- Permission: P1; no network, external state, destructive action, or external repository access.
- Behavior change: none. This package adds a source gate and a read-only empirical diagnostic.
- Review tier: C because the audited boundary decides whether the learned actor receives its
  seat-redacted observation.

## Static inventory

The integration test scans each engine module only up to its top-level `#[cfg(test)]`. Its lexical
inventory is 106 `Choice::new(` occurrences, 58 direct `.ask_seeing(` calls, 15 direct viewless
`.ask(` calls, and 41 `pending_choice(` definitions/calls across 26 production modules. These are
source-surface counts, not a claim that all constructors are reached in one game.

| module | choices | viewless asks | observed asks | pending refs |
|---|---:|---:|---:|---:|
| action_cards | 13 | 0 | 13 | 0 |
| agenda_effects | 3 | 0 | 3 | 0 |
| choice | 1 | 0 | 2 | 4 |
| combat | 8 | 0 | 4 | 2 |
| draft | 1 | 0 | 0 | 0 |
| exploration | 1 | 0 | 1 | 0 |
| faction_abilities | 4 | 0 | 4 | 0 |
| fleet | 1 | 0 | 1 | 0 |
| game | 5 | 4 | 9 | 18 |
| invasion | 8 | 2 | 3 | 3 |
| laws | 1 | 1 | 0 | 0 |
| objectives | 1 | 0 | 0 | 2 |
| production | 9 | 0 | 7 | 3 |
| reactions | 1 | 0 | 1 | 0 |
| relics | 7 | 7 | 0 | 0 |
| secrets | 1 | 0 | 1 | 0 |
| strategy | 4 | 0 | 0 | 1 |
| strategy_cards | 19 | 0 | 1 | 0 |
| tactical | 2 | 0 | 0 | 0 |
| technology | 5 | 0 | 5 | 0 |
| thunders_edge | 2 | 0 | 1 | 0 |
| timing | 2 | 1 | 2 | 0 |
| tokens | 1 | 0 | 0 | 2 |
| transactions | 2 | 0 | 0 | 2 |
| transit | 1 | 0 | 0 | 2 |
| vote | 3 | 0 | 0 | 2 |

Production modules contain no direct `.choose(` call outside the `choice.rs` implementation. Thus
all engine decisions still pass through `Table`; the defect is which of its two entry points is
used.

### The 15 viewless asks

None is a genuine setup/offline exception. All can alter a live game and all are migration work for
OBS-003c.

| source/function | count | decision |
|---|---:|---|
| `game::committee_formation` | 1 | choose an elected player |
| `game::minister_of_war` | 1 | retrieve a command token from a system |
| `game::imperial_arbiter` | 2 | choose the strategy card given and the opponent card taken |
| `invasion::dunlain_reaper` | 1 | deploy a mech or decline before paying |
| `invasion::apply_bombard_plan` | 1 | choose which coexisting defender takes bombardment hits |
| `laws::offer_discard` | 1 | discard a ministry law or decline |
| `relics::grant_chosen_technology` | 1 | choose a technology granted by a relic/effect |
| `relics::codex` | 1 call site | repeatedly choose a discarded action card or stop |
| `relics::titan_prototype` | 1 | choose which player may build |
| `relics::stellar_converter` | 1 | choose a planet to destroy |
| `relics::crown_of_emphidia_explore` | 1 | choose a controlled planet to explore or decline |
| `relics::offer_dominus_orb` | 1 | purge the relic or decline |
| `relics::neuraloop` | 1 | choose which relic to purge or decline |
| `timing::pick` | 1 | choose among eligible context-free timing abilities |

The corresponding `timing::pick_with_context` path is observed; the viewless `pick` remains used by
the context-free resolver API and must be classified/migrated rather than assumed test-only.

## Head/kind/subtype findings

The current router has 19 heads. Most routing is kind-based, but it still has prompt fallbacks for
secondary, movement, production, payment, trade, and combat; otherwise it returns `other`. `Choice`
has no typed subtype or source field, so the complete source/subtype matrix cannot be represented in
a runtime record today. OBS-003a must add that contract. This audit therefore records:

- source statically by the 26 modules above;
- head and kind set empirically through the same `decision_head` used by inference;
- prompt samples only as diagnostics, never as a stable subtype.

The fixed four-game/four-round census exercised 3,869 non-forced decisions. Aggregate head counts
were:

| head | decisions | head | decisions |
|---|---:|---|---:|
| ability | 112 | activation | 247 |
| agenda | 90 | cargo | 462 |
| combat | 48 | development | 54 |
| exploration | 21 | landing | 169 |
| movement | 266 | other | 389 |
| payment | 179 | production | 215 |
| scoring | 58 | secondary | 355 |
| strategy | 108 | tokens | 297 |
| trade | 221 | turn | 578 |

`transit` was not exercised. Static coverage keeps it in scope; its absence is not an exception.
The run observed zero viewless decisions because the 15 sites are conditional/rare. This is the
important static-versus-dynamic result: a green ordinary rollout does not prove that every actor is
seat-bound.

Representative ambiguities exposed by the census:

- the `other` head carried objective, planet, unit, infantry, player, prediction, promissory-note,
  ship, silence, strategy-card, and 318 command-token redistribution decisions;
- the same `secondary/strategy` row included command-token purchase and several unrelated strategy
  card secondaries;
- prompts currently distinguish several of these, confirming that typed source/subtype must land
  before prompt-derived model input can be removed;
- no empirical path can identify the producer module from `Choice`, so source attribution currently
  exists only in the static half of the audit.

## Verification

- `cargo test -p ti4-engine --test decision_delivery_inventory -- --nocapture`: 3 passed.
- `cargo run -p ti4-training --example decision_surface --release -- --games 4 --rounds 4`:
  completed 4 games/4 rounds, 3,869 non-forced choices, 0 dynamically exercised viewless choices;
  deterministic grouped census printed for 18 heads.
- `cargo test -p ti4-engine`: 1,108 unit + 3 integration + 5 doc tests passed.
- `cargo test -p ti4-training`: 133 unit tests passed; no doc tests. Existing warnings remain in
  `seat_advantage.rs` and `vp_sources.rs`, neither touched by this package.
- `cargo clippy -p ti4-engine --test decision_delivery_inventory -- -D warnings`: passed.
- strict Clippy for the training example is blocked before reaching the example by three pre-existing
  library findings in `ppo.rs` and `stage1.rs`. A capped run reaches the example; after correcting
  its one `collapsible_str_replace` finding, it reports only those three pre-existing library
  warnings.
- targeted `rustfmt --check` and `git diff --check`: passed.

## Review

Independent Tier-C review: **approved**, with two non-blocking notes.

### What was verified, not taken on trust

**The static inventory was recounted independently.** A separate scan of `crates/ti4-engine/src`,
cutting each module at its first top-level `#[cfg(test)]`, reproduces the evidence table exactly:
106 `Choice::new(`, 15 `.ask(`, 58 `.ask_seeing(`, 41 `pending_choice(`, and `.choose(` appearing
only inside `choice.rs`. Every per-module row matches. The claim that all engine decisions still
pass through `Table` holds.

**The gate was proved sensitive by mutation**, which is the check that decides whether this package
is worth anything. A compiling `.ask(` call was added to a real function in `tokens.rs`, inserted
INSIDE the production region rather than appended after the module's `#[cfg(test)]`:

```text
Site { module: "tokens.rs", function: "obs002a_review_probe", operation: AskViewless }: 1
```

`every_producer_and_delivery_site_matches_the_reviewed_registry` failed on it, attributing the
correct module, the correct enclosing function, and the correct classification. The probe was
reverted and the suite returned to green. Note for anyone repeating this: appending the probe to the
end of the file proves nothing, because the scanner correctly stops at `#[cfg(test)]` — the first
attempt in this review made exactly that mistake and produced a false pass.

**Checks re-run here:** inventory 4/4, engine 1,108 + 4 + 5, training 133, and the census example at
`--games 4 --rounds 4`, which produces the documented delivery/head/kind table.

### Notes

1. The evidence and `EXECUTION_STATE.md` say "inventory 4/4"; there are four tests. Stale by one.
2. `all_fifteen_viewless_asks_remain_explicit_migration_work` sums the `VIEWLESS_ASKS` constant, so
   in isolation it cannot fail — it is a ratchet, not a measurement. Its force is real but indirect:
   a new viewless ask trips the scan equality, the registry must then be edited to restore it, and
   that edit trips the 15. Worth one comment line saying so, or an assertion over the scanned
   `AskViewless` total, so a later reader does not mistake it for a direct check of the engine.

### Boundary judgement

The audit's central claim — that none of the 15 viewless asks is a genuine setup/offline exception
and all are migration work for OBS-003c — is supported by the per-site table, and each named site is
a live in-game decision that can alter the position. Approving the boundary on that basis.

The disclosed limitation is correctly stated and worth keeping visible: four deterministic games
exercised 3,869 non-forced choices and reached none of the rare viewless sites. The census cannot
substitute for the static gate, which is precisely why the static gate carries the contract.
