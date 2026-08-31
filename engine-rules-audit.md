# Engine vs. rules audit

All 109 rule topics from [tirules2.com](https://tirules2.com/), checked against
`crates/ti4-engine`. Written 2026-08-28.

## Method, and what this audit does and does not establish

Three independent passes:

1. **Rules text.** The rule index and full text for the topics judged most at risk were fetched
   verbatim and compared line by line with the code that implements them.
2. **Coverage functions.** Most content areas already carry an `unimplemented()`/`registered_aliases()`
   pair beside the registry they report on. Nothing gathered them; `cargo run -p ti4-engine --example
   coverage_report` now does.
3. **Topic-to-module mapping.** Every one of the 109 topics was grepped against the engine source to
   separate "no code exists" from "code exists".

**The limits matter.** Pass 2 measures *registration*, not correctness: `unimplemented` means "no
requirement function is registered", so a registered-but-wrong rule counts as implemented. Pass 3
proves only that code mentioning a topic exists. Only pass 1 establishes correctness, and it was run
on a minority of topics. Rows below marked *unverified* mean exactly that — code exists, and this
audit did not check it against the rules text. **They are not claims of correctness.**

The one topic that got the full treatment before this audit, space stations, went from "looks
implemented" to five wrong rules and four missing ones. That is the base rate to keep in mind for
every *unverified* row.

## Headline

The engine loads **all** Thunder's Edge content — `DEFAULT = FULL` — while implementing almost none
of Thunder's Edge's mechanics. Five of its core systems have **no code at all**, and the content they
govern is on the board during every training game.

| Thunder's Edge mechanic | Content loaded | Engine |
|---|---|---|
| Space stations | 4 stations, on 4 tiles | wrong: see `plans/evidence/SPACE_STATIONS_AUDIT.md` |
| Coexistence | — | **absent** |
| Entropic Scars | anomaly tiles present | **absent** |
| The Fracture | — | **absent** |
| Synergy | 31 breakthroughs carry one | **absent** |
| Neutral Units | — | **absent** (only unrelated uses of the word) |
| Breakthroughs | 31 cards | 2 have effects (`xxchabt`, `letnevbt`); 29 are inert ids |
| Expedition | 6 slices | implemented |

Thunder's Edge content actually loaded: 31 breakthroughs, 49 planets, 36 systems, 21 units, 8 tokens.

Galactic events are deliberately out of scope. The corpus carries all 20 and the engine implements
none, but they are an optional variant rather than a mechanic the base expansion assumes, so their
absence is a choice about what game is being played rather than a rules deviation. They are excluded
from every count below.

**The cheapest correct action available is to stop loading Thunder's Edge** — train on `POK` instead
of `DEFAULT` — unless the intent is to model the expansion. Every unimplemented mechanic in the table
above is currently a silent rules deviation rather than an absent feature, because the content that
triggers it is present.

## Content coverage

`cargo run --release -p ti4-engine --example coverage_report`

| area | as first reported | corrected | now |
|---|---|---|---|
| action cards | 0 of 142 (0.0%) | **34 of 142 (23.9%)** | 34 of 142 |
| public objectives | 30 of 40 (75.0%) | **40 of 40 (100%)** | 40 of 40 |
| agendas | 34 of 63 (54.0%) | — | **45 of 63 (71.4%)** |
| relics | 5 of 24 (20.8%) | — | **9 of 24 (37.5%)** |
| laws | 36 unenforced | — | **20 unenforced** |
| reaction windows | 11 unsupported | — | **6 unsupported** |
| exploration cards | 71 of 80 (88.8%) | — | 71 of 80 |
| secret objectives | 40 of 40 (100%) | — | 40 of 40 |
| faction abilities | 19 of 73 (26.0%) | 14 of 14 for the six factions | 14 of 14 |
| leaders | 3 unimplemented (the six) | — | 3 unimplemented |

### Two of those first figures were wrong, and both were my own measurement

This is the failure this audit warns about in its own method section, committed by the audit.

**Action cards were never 0.** `action_cards::unimplemented` returned *every* card unconditionally
— it never consulted `effect_for`, which covers 34 aliases. Its doc comment said "every action card
is currently unimplemented", which was true when written and stopped being true as effects landed.
A coverage function that cannot improve is worse than none, because it reads as evidence. Fixed to
consult `effect_for`; its test asserted only `len() > 50`, which held whether the function worked or
not, and now asserts the invariant per alias.

**Public objectives were never 30 of 40.** The ten "spend N" cards — Erect a Monument, Sway the
Council, Hold Vast Reserves and the rest — are scored through `cost_of`/`bought_progress`, not
through `registered_aliases`. `scoreable_on` accepts either family; counting one list reported ten
working cards as missing.

Both corrections move the number the same way, and neither changes any code that plays the game.
The lesson is the one already written above: **coverage numbers measure registration, and a
registration list can be looking at the wrong register.**

## The 109 topics

Status key: **OK** verified against rules text · **WRONG** verified defect · **PARTIAL** known gap ·
**?** code exists, unverified · **ABSENT** no code.

| Topic | Status | Note |
|---|---|---|
| Abilities | ? | `faction_abilities.rs`; 26% of printed abilities registered |
| Action Cards | **PARTIAL** | infrastructure exists; 0 of 142 cards have effects |
| Action Phase | ? | `phase.rs`, `game.rs` |
| Active Player | ? | |
| Active System | ? | |
| Adjacency | ? | `galaxy.rs` axial hex + wormholes |
| Agenda Card | **PARTIAL** | 34 of 63 registered |
| Agenda Phase | ? | |
| Anomalies | ? | asteroid/nebula/supernova/rift present; **entropic scars absent** |
| Anti-Fighter Barrage | ? | `combat.rs` |
| Asteroid Field | VERIFIED | 11.1 bar and the Antimass Deflectors exemption |
| Attach | ? | |
| Attacker | ? | |
| Blockaded | **PARTIAL** | coexisting structures always blockaded (rule 4) — phase 2; base rule unverified |
| Bombardment | **PARTIAL** | hits grouped per unit, no spillover (7.2) — phase 2; 7/7.1 choice outstanding |
| Breakthroughs | **PARTIAL** | 2 of 31 have effects; breakthrough roll (rule 3) absent |
| Capacity | **PARTIAL** | 16.3 fixed 2026-08-31 (ground forces were never excess); 16.3c end-of-combat removal still missing, see `fleet.rs` |
| Capture | ? | |
| Coexistence | **PARTIAL** | rules 2, 3.1, 3.2, 4, 5, 6, 7.2, 9-13 done; only 7/7.1 bombardment target choice outstanding |
| Combat | ? | `combat.rs`, 3110 lines |
| Command Sheet | OK | `TokenPool`, `tactic_tokens` etc. in `state.rs` |
| Command Tokens | ? | |
| Commodities | **PARTIAL** | exists; space-station +1 (rule 8) and convert (rule 12) absent |
| Component Action | ? | |
| Component Limitations | OK | `supply.rs`, LRR 31.4, with the fighter/infantry exemption correct |
| Construction | OK | stations excluded (rule 5) — phase 1 |
| Control | OK | station control is sole occupancy, reconciled per step — phase 1 |
| Cost | ? | |
| Custodians Token | ? | `invasion.rs` keeps Mecatol off the table until removed |
| Deals | ? | |
| Defender | ? | |
| Deploy | ? | |
| Destroyed | ? | |
| Diplomacy | ? | `strategy_cards.rs` |
| Elimination | **ABSENT** | no code; harmless at a 4-round horizon |
| Entropic Scars | **ABSENT** | 9 rules, none implemented; anomaly tiles are in the corpus |
| Exhausted | ? | |
| Expedition | ? | `thunders_edge.rs`, 6 slices |
| Exploration | ? | 71 of 80 cards |
| Fighter Tokens | OK | intentionally uncapped, `supply.rs` documents why |
| Fleet Pool | ? | `fleet.rs` |
| The Fracture | **ABSENT** | 15 rules, none implemented |
| Frontier Tokens | OK | station-only tiles take a token (rule 14) — phase 1 |
| Game Board | ? | |
| Game Round | ? | |
| Gravity Rift | VERIFIED | 41.1-41.5 all present in `movement.rs`/`transit.rs`, incl. the path-dependent +1 |
| Ground Combat | ? | `invasion.rs`; coexistence combat rules 9–12 absent |
| Ground Forces | OK | cannot be committed to stations (rule 5) — phase 1 |
| Hyperlanes | ? | |
| Imperial | ? | |
| Infantry Tokens | OK | as fighter tokens |
| Influence | ? | |
| Initiative Order | ? | |
| Invasion | OK | stations excluded; coexistence combat chain 9-12 implemented |
| Leader Sheet | ? | |
| Leaders | **PARTIAL** | 3 unimplemented across the six trained factions |
| Leadership | ? | |
| Legendary Planets | **PARTIAL** | counted for objectives; no legendary ability is implemented |
| Mecatol Rex | ? | |
| Mechs | ? | |
| Modifiers | ? | |
| Move | ? | `movement.rs`, `transit.rs` |
| Movement | ? | |
| Nebula | VERIFIED | 59.1-59.4 in `movement.rs`; 59.5 defender +1 was **absent**, added 2026-08-31 |
| Neighbors | **PARTIAL** | adjacency only; station-to-station transactions (rule 10) absent |
| Neutral Units | **ABSENT** | 9 rules, none implemented |
| Objective Cards | **PARTIAL** | 30 of 40 public registered; stations excluded and coexisters counted (rule 13) — phases 1-2 |
| Opponent | ? | |
| PDS | ? | |
| Planets | OK | stations are not planets for landing, scoring or the opening bar — phase 1 |
| Planetary Shield | ? | `invasion.rs`, incl. L1Z1X commander override |
| Politics | ? | |
| Producing Units | ? | |
| Production | ? | `production.rs` |
| Promissory Notes | ? | |
| Purge | ? | |
| Readied | ? | |
| Reinforcements | ? | |
| Relics | **PARTIAL** | 5 of 24 |
| Rerolls | VERIFIED | scoped 2026-08-31: the Thalnos cards were reaching space cannon, barrage and bombardment rolls, none of which is a combat round |
| Resources | ? | |
| Ships | ? | |
| Space Cannon | ? | `combat.rs`; two reaction windows unsupported |
| Space Combat | ? | |
| Space Dock | ? | |
| Space Stations | **PARTIAL** | rules 2, 2a, 2b, 5, 7, 14 done (phase 1); 8, 10, 12 economy outstanding |
| Speaker | ? | |
| Status Phase | ? | `status.rs` |
| Strategic Action | ? | |
| Strategy Card | ? | |
| Strategy Phase | ? | |
| Structures | OK | not placeable on stations (rule 5) — phase 1 |
| Supernova | VERIFIED | 86.1 bar and the Magmus Reactor exemption |
| Sustain Damage | ? | applied inside hit assignment; 3 reaction windows unsupported |
| Synergy | **ABSENT** | 6 rules, none implemented; every breakthrough carries a synergy |
| System Tiles | ? | |
| Tactical Action | ? | `tactical.rs` |
| Technology | ? | |
| Technology (S.C.) | ? | |
| Trade | ? | |
| Trade Goods | ? | |
| Transactions | **PARTIAL** | station-to-station (rule 10) absent |
| Transport | ? | `transit.rs` |
| Units | ? | |
| Unit Upgrades | ? | |
| Victory Points | ? | |
| Warfare | ? | |
| Wormhole Nexus | **PARTIAL** | counted by one secret; not modelled as a board feature |
| Wormholes | ? | |

Totals after phases 1-2: **0 wrong**, **5 absent**, **15 partial**, **10 verified correct**,
**79 unverified**.

Originally **8 wrong**, **6 absent**, **11 partial**, **4 verified**, **80 unverified**. Every
verified defect is fixed; coexistence moved from absent to partial. Progress against
`plans/ENGINE_COMPLETION_PLAN.md` is tracked there, not here.

Of the six absent topics, five are Thunder's Edge (Coexistence, Entropic Scars, The Fracture,
Synergy, Neutral Units) and one is base-game Elimination, which cannot arise at a four-round
horizon.

## Unsupported reaction windows

Eleven windows cannot fire. Ten are one structural cause: combat resolves in whole steps, so there is
no moment between rolling and assigning, no single named destroyed unit, and sustain is chosen inside
hit assignment rather than announced. The eleventh is that the strategy phase emits no start event.
This is a design boundary rather than a bug, but it silently disables every card and ability that
triggers on those windows.

## Effect on training

The space-station defects are the only ones measured. On 4,320 held-out seat-games, **6.2% of cleared
openings depend on a space station taken by a move rule 5 forbids**; clearance falls from 95.3% to
89.4% once stations stop counting as planets. Every opening-clearance number recorded in this project
is inflated by roughly six points.

The absent Thunder's Edge mechanics do not currently distort the opening measurement — they are
mid-game systems and the opening is one round — but they do mean four-round Stage-2 games are played
under rules that do not exist, with 29 inert breakthroughs on the table and no synergy behind them.

## Priority

1. **Decide the source set.** Training on `POK` removes five absent-mechanic deviations at a stroke.
   If Thunder's Edge is wanted, items 2–4 are prerequisites, not enhancements.
2. **Space stations** — the six fixes listed in `plans/evidence/SPACE_STATIONS_AUDIT.md`. This is the
   only defect with a measured cost, and it invalidates existing clearance figures.
3. **Coexistence**, if Thunder's Edge stays: it changes invasion, bombardment, blockade and control,
   all of which the opening policy exercises every game.
4. **Breakthrough effects and synergy**, if Thunder's Edge stays: 29 of 31 cards are inert.
5. **Action cards** — 0 of 142. The largest single content gap, though a self-consistent one: no
   player has them, so no player is disadvantaged.
6. Verify the 76 unverified topics against rules text, worst-first: Invasion, Movement, Transport,
   Production, Space Combat, Control, Blockaded, Anomalies.
