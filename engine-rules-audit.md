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
| Abilities | VERIFIED | 52.1-52.18 + notes: the resolver's round-robin, one ability each before the next pass, When→resolve→After inside an event, initiative order from the active player in the action phase and from the speaker in strategy/agenda; "before" windows (52.18) are the When window of the event they precede, and 52.10 is honoured literally — an unpayable cost voids the effect, but the card was already played and is discarded (`focused_research_charges_nothing_when_it_cannot_pay`) |
| Action Cards | **PARTIAL** | infrastructure exists; 0 of 142 cards have effects |
| Action Phase | ? | `phase.rs`, `game.rs` |
| Active Player | VERIFIED | 4.1-4.6: the turn advances in initiative order skipping passed players and is `None` once all have passed; strategy, status and agenda have no active player; the attacker is the active seat in space combat; combat windows and space-cannon hits start with the active player; transactions are offered only from the active seat. The Mahact's Benediction note is out of scope |
| Active System | VERIFIED | 5.1-5.4 + notes: activation spends a tactic token and excludes systems holding one of the player's tokens (5.2; rivals' tokens are no obstacle, 5.3); the active system lasts for the action and is cleared when the tactical action ends (and on Minister of Peace's early end); nebulae are entered only as the active system; component and strategic actions have no active system and fire no activation trigger; `SYSTEM_ACTIVATED` is emitted only for genuine (free) tactical actions |
| Adjacency | **PARTIAL** | 6.0-6.3 + notes verified: tile-edge contact, wormhole pairs (with the law-driven nexus/Creuss layout properties), a system is never adjacent to itself, unit and planet adjacency through their containing system; **6.4 (LRR 44) is not modelled** — hyperlane line adjacency: the corpus carries 108 hyperlane tiles but no line-pattern data, and the engine's map build never places one, see the open item |
| Agenda Card | **PARTIAL** | 34 of 63 registered |
| Agenda Phase | **PARTIAL** | 8.1-8.21 + notes 1-5, 7 verified: custodians gate, two agenda reveals, voting clockwise from the player left of the speaker, full planet influence (space stations, the Triad and the Oceans count), one outcome per voter, trade goods never vote, abstention is legal, extra votes ride on the outcome voted, the speaker's tie-break is not a vote, a law stays and a directive is discarded, predictions are paid after resolution, only planets are readied; **note 6 is absent** (no transactions are offered in the agenda phase), the "Elect Scored Secret Objective" and "Elect Strategy Card" agendas are discarded rather than voted, and Checks and Balances (Against) readies the first three planets instead of letting the player choose, see the open items |
| Anomalies | VERIFIED | 9.1-9.5 + notes: the four types are independent flags, so one tile can be two anomalies (tile 117 is both, base map), anomalies may contain planets, and a wormhole does not make a system an anomaly; the entropic-scar flag is present (the scar rules live under Entropic Scars, **ABSENT**) |
| Anti-Fighter Barrage | VERIFIED | 78.3: simultaneous, first round only, fighters only, excess discarded |
| Asteroid Field | VERIFIED | 11.1 bar and the Antimass Deflectors exemption |
| Attach | VERIFIED | 12.1-12.3; attachments follow the planet through a control change and are purged with it |
| Attacker | VERIFIED | 13: during combat the active player is the attacker (a tactical action starts combat, `combat.rs`), the opponent is the defender, and the attacker has the first opportunity in every combat timing window. The Mahact's Benediction note is out of scope |
| Blockaded | **PARTIAL** | coexisting structures always blockaded (rule 4) — phase 2; base rule unverified |
| Bombardment | **PARTIAL** | hits grouped per unit, no spillover (7.2) — phase 2; 7/7.1 choice outstanding |
| Breakthroughs | **PARTIAL** | 2 of 31 have effects; breakthrough roll (rule 3) absent |
| Capacity | **PARTIAL** | 16.3 fixed 2026-08-31 (ground forces were never excess); 16.3c end-of-combat removal still missing, see `fleet.rs` |
| Capture | OUT OF SCOPE | every capture effect in the corpus is Vuil'raith Cabal, which is not one of the six trained factions |
| Coexistence | **PARTIAL** | rules 2, 3.1, 3.2, 4, 5, 6, 7.2, 9-13 done; only 7/7.1 bombardment target choice outstanding |
| Combat | ? | `combat.rs`, 3110 lines |
| Command Sheet | OK | `TokenPool`, `tactic_tokens` etc. in `state.rs` |
| Command Tokens | **PARTIAL** | 20.1-20.3, 20.6, 20.7a, 20.9 present; 20.4/20.4a (limited by tokens in reinforcements) not modelled — pools can grow without bound |
| Commodities | **PARTIAL** | exists; space-station +1 (rule 8) and convert (rule 12) absent |
| Component Action | ? | |
| Component Limitations | OK | `supply.rs`, LRR 31.4, with the fighter/infantry exemption correct |
| Construction | OK | stations excluded (rule 5) — phase 1 |
| Control | OK | station control is sole occupancy, reconciled per step — phase 1 |
| Cost | ? | |
| Custodians Token | **PARTIAL** | 27.1-27.5 present; 27.2a (no removal without ground forces to commit) was **absent**, added 2026-08-31 |
| Deals | ? | |
| Defender | ? | |
| Deploy | VERIFIED | 20.1-20.5; the two in-scope DEPLOY mechs (Sol, Letnev) are implemented |
| Destroyed | VERIFIED | 31.1-31.2; removal by fleet supply or capacity does not fire destroyed triggers |
| Diplomacy | VERIFIED | both halves: opponents place a token in the chosen system, then ready two planets |
| Elimination | **ABSENT** | no code; harmless at a 4-round horizon |
| Entropic Scars | **ABSENT** | 9 rules, none implemented; anomaly tiles are in the corpus |
| Exhausted | ? | |
| Expedition | ? | `thunders_edge.rs`, 6 slices |
| Exploration | ? | 71 of 80 cards |
| Fighter Tokens | OK | intentionally uncapped, `supply.rs` documents why |
| Fleet Pool | VERIFIED | 37.1-37.6; Letnev's Armada lifts the cap after Fleet Regulations caps it |
| The Fracture | **ABSENT** | 15 rules, none implemented |
| Frontier Tokens | OK | station-only tiles take a token (rule 14) — phase 1 |
| Game Board | ? | |
| Game Round | ? | |
| Gravity Rift | VERIFIED | 41.1-41.5 all present in `movement.rs`/`transit.rs`, incl. the path-dependent +1 |
| Ground Combat | VERIFIED | 42.1-42.4; burst icons roll per die. Fragile now applies here (was space-only), and the Shield Paling mech lifts it |
| Ground Forces | OK | cannot be committed to stations (rule 5) — phase 1 |
| Hyperlanes | ? | |
| Imperial | VERIFIED | primary scores a public then Mecatol/secret; 45.4 counts hand *and* scored and returns an unscored one |
| Infantry Tokens | OK | as fighter tokens |
| Influence | ? | |
| Initiative Order | ? | |
| Invasion | OK | stations excluded; coexistence combat chain 9-12 implemented |
| Leader Sheet | ? | |
| Leaders | **PARTIAL** | 3 unimplemented across the six trained factions |
| Leadership | VERIFIED | three tokens then three influence each, and the secondary spends no strategy token (52.3) |
| Legendary Planets | **PARTIAL** | counted for objectives; no legendary ability is implemented |
| Mecatol Rex | ? | |
| Mechs | **PARTIAL** | all six in-scope mech abilities implemented 2026-08-31; they were counted by nothing before, see `unimplemented_mechs` |
| Modifiers | ? | |
| Move | VERIFIED | with Movement |
| Movement | VERIFIED | 58.4b/c/e, path length, transport; the active-system exception is tested |
| Nebula | VERIFIED | 59.1-59.4 in `movement.rs`; 59.5 defender +1 was **absent**, added 2026-08-31 |
| Neighbors | **PARTIAL** | adjacency only; station-to-station transactions (rule 10) absent |
| Neutral Units | **ABSENT** | 9 rules, none implemented |
| Objective Cards | **PARTIAL** | 30 of 40 public registered; stations excluded and coexisters counted (rule 13) — phases 1-2 |
| Opponent | ? | |
| PDS | **PARTIAL** | PDS II's adjacent-system clause was **absent**, added 2026-08-31 |
| Planets | OK | stations are not planets for landing, scoring or the opening bar — phase 1 |
| Planetary Shield | **PARTIAL** | 63.1 and 63.3 present; 63.2 (the shield stops Harrow) was **absent**, added 2026-08-31 |
| Politics | VERIFIED | speaker, two action cards, and the top two agendas reordered |
| Producing Units | **PARTIAL** | 68.10 (no ships in a blockaded system) was **absent** from the production path, added 2026-08-31; 68.3b (produce one of a pair, pay full) not offered |
| Production | **PARTIAL** | 68.1.3 combined bill fixed 2026-08-31; see Producing Units |
| Promissory Notes | ? | |
| Purge | ? | |
| Readied | ? | |
| Reinforcements | ? | |
| Relics | **PARTIAL** | 5 of 24 |
| Rerolls | VERIFIED | scoped 2026-08-31: the Thalnos cards were reaching space cannon, barrage and bombardment rolls, none of which is a combat round |
| Resources | ? | |
| Ships | ? | |
| Space Cannon | **PARTIAL** | offence, defence and the adjacency clause (PDS II, Indomitus) -- adjacency was **absent** until 2026-08-31 |
| Space Combat | VERIFIED | 78.1-78.9: barrage first round only, defender announces first, the round loops back to Announce Retreats |
| Space Dock | ? | |
| Space Stations | **PARTIAL** | rules 2, 2a, 2b, 5, 7, 14 done (phase 1); 8, 10, 12 economy outstanding |
| Speaker | ? | |
| Status Phase | **PARTIAL** | all eight steps present; 81.5's second sentence (redistribute tokens already held) is **not** implemented — see `status.rs` |
| Strategic Action | ? | |
| Strategy Card | VERIFIED | initiative order, exhaustion, and the secondary token gate (52.3 exempts Leadership) |
| Strategy Phase | ? | |
| Structures | OK | not placeable on stations (rule 5) — phase 1 |
| Supernova | VERIFIED | 86.1 bar and the Magmus Reactor exemption |
| Sustain Damage | **PARTIAL** | 15.1-15.6 present; 15.7 Non-Euclidean Shielding was **absent**, added 2026-08-31 |
| Synergy | **ABSENT** | 6 rules, none implemented; every breakthrough carries a synergy |
| System Tiles | ? | |
| Tactical Action | ? | `tactical.rs` |
| Technology | VERIFIED | 90.1-90.23: colours, faction restriction, prerequisites, specialties, unit upgrades have no colour |
| Technology (S.C.) | VERIFIED | with Technology; the secondary charges four resources and Jol-Nar substitutes the primary |
| Trade | VERIFIED | three trade goods, replenish, and chosen players replenish too |
| Trade Goods | ? | |
| Transactions | **PARTIAL** | station-to-station (rule 10) absent |
| Transport | **PARTIAL** | 95.5 (no pickup from your own command token) was **absent**, added 2026-08-31; 95.1 pickup *en route* is still origin-only |
| Units | ? | |
| Unit Upgrades | VERIFIED | with Technology (90.7-90.10) |
| Victory Points | VERIFIED | 98.1-98.10: cap at 10, initiative tiebreak, a law's point survives the law |
| Warfare | **PARTIAL** | recall and pool gain present; "then redistribute your command tokens" was **absent**, added 2026-08-31 |
| Wormhole Nexus | **PARTIAL** | counted by one secret; not modelled as a board feature |
| Wormholes | VERIFIED | 101.1-101.4 and the notes; a system is never adjacent to itself, and PDS II fires through wormholes because `Galaxy::adjacent` already unions them |

Totals after phases 1-2: **0 wrong**, **5 absent**, **15 partial**, **10 verified correct**,
**32 unverified** (was 79).

## Phase 9 verification, 2026-08-31

Twenty-three topics moved off the *unverified* list by fetching their rules text and reading it against
the code — pass 1 of the method above, the only pass that establishes correctness.

**Twelve defects in forty-five in-scope topics.** That is close to the base rate this audit warned
about, and it is the reason the remaining 56 rows should still be read as "not checked" rather than
"probably fine":

| Rule | Defect | Status |
|---|---|---|
| 16.3 | fighters and ground forces share one capacity total; ground forces were never counted as excess | fixed |
| 16.3c | excess removed at the *end* of combat — enforcement runs before combat and never after | **open**, see `fleet.rs` |
| 59.5 | the nebula defender's +1 to each combat roll | added |
| 15.7 | Non-Euclidean Shielding cancels two hits, not one | added |
| 63.2 | a planetary shield stops L1Z1X's Harrow | added |
| 95.5 | no pickup from a system holding your own command token | added |
| 68.10 | no ship production in a system holding another player's ships | added |
| Thalnos scope | both cards say "during each combat round"; they reached space cannon, barrage and bombardment | fixed |
| Fragile | Jol-Nar's -1 applied in space only; `combat_modifier`'s `context` parameter had one caller and it passed "space" | fixed |
| PDS II / Indomitus | "SPACE CANNON against ships in adjacent systems" — `space_cannon_offense` read only the activated system | added |
| mech abilities | four of six in-scope mechs unimplemented, and counted by no coverage helper because the ability is printed on the unit | added |
| 27.2a | the custodians token — and its victory point — could be taken with six influence and no ground forces to commit | fixed |
| Warfare | "then, the active player can redistribute their command tokens" — the recall was implemented, the redistribution was not | added |
| 81.5 | the status phase's own redistribution of tokens already held | **open**, see `status.rs` |
| 6.4 / 44.1-44.2 | hyperlane line adjacency — a hyperlane tile connects by its printed line and the line itself is not a system; the corpus carries no line-pattern data and the engine's map build never places a hyperlane tile, so the gap is dormant in engine play | **open** |
| 8 note 6 | a player may transact once with each other player during each agenda; the engine offers no transactions in the agenda phase at all | **open** |
| 8.15-8.18 | "Elect Scored Secret Objective" and "Elect Strategy Card" have outcomes the engine cannot vote on; the agenda is discarded without a vote | **open** |
| 8.4 / Checks and Balances | "each player readies only 3 of their planets" — which three is the player's choice; the engine readies the first three in a fixed order | **open** |

Clean on inspection against their rules text: Abilities, Active Player, Active System, Anomalies,
Attacker, Anti-Fighter Barrage, Asteroid Field, Fleet Pool, Gravity Rift, Space Cannon, Supernova.
Capture is Cabal-only and therefore out of scope.

Two simplifications recorded rather than hidden: 95.1 allows pickup from each system a ship moves
*through* and this engine offers only the origin (a narrower offer, not an illegal one); and 68.3b
lets a player produce one unit of a two-for-one pair and pay the full cost, which is not offered.

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
