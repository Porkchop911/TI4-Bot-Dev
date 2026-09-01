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
| Action Phase | VERIFIED | 3.1-3.5 + notes: the three action types plus a pass; passing is legal only when nothing else can be done (the gate is tested before the windows open), passed players are skipped and cannot pass again, and the turn is `None` once every player has passed; note 2 (an after-window after *every* action) was **absent**, added 2026-09-01 — component actions ended through `advance_turn` and never opened it |
| Action Cards | **PARTIAL** | infrastructure exists; 142 of 142 cards have effects (engine remainder closed) |
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
| Combat | VERIFIED | 18 (LRR 18) cross-read with Space Combat 78: the round order, the simultaneous hits and the 50-round stop are in `combat.rs`; roles are anchored to the active player — seating order was **absent** as an anchor, added 2026-09-01 (`CombatWindow::new`); Winnu's dynamic dice are out of scope |
| Command Sheet | OK | `TokenPool`, `tactic_tokens` etc. in `state.rs` |
| Command Tokens | **PARTIAL** | 20.1-20.3, 20.6, 20.7a, 20.9 present; 20.4/20.4a (limited by tokens in reinforcements) not modelled — pools can grow without bound |
| Commodities | **PARTIAL** | exists; space-station +1 (rule 8) and convert (rule 12) absent |
| Component Action | VERIFIED | 22.1-22.4: six component sources, each costing the whole turn **unless** the play is cancelled while announced (22.4) or cannot be resolved (22.3) — both halves were **absent**, added 2026-09-01: `ACTION_COMPLETED` now fires after every component action, and a play cancelled by Sabotage re-offers the same turn with the card spent |
| Component Limitations | OK | `supply.rs`, LRR 31.4, with the fighter/infantry exemption correct |
| Construction | OK | stations excluded (rule 5) — phase 1 |
| Control | OK | station control is sole occupancy, reconciled per step — phase 1 |
| Cost | VERIFIED | 26 (LRR 26): costs are exact arithmetic (no floats anywhere in a legality check), an unpayable cost voids the effect after the play is announced (52.10, the card is spent either way), a combined bill is one transaction, and promissory notes are accepted as payment in the payment window that opens before any effect |
| Custodians Token | **PARTIAL** | 27.1-27.5 present; 27.2a (no removal without ground forces to commit) was **absent**, added 2026-08-31 |
| Deals | **PARTIAL** | 28.1's transactional half verified — adjacency and note-holding legality are enforced when offers are built; the offer itself, its binding and non-binding character and counters (28.2-28.4) are unrepresentable in a single-agent decision engine — a design boundary, recorded, not a defect |
| Defender | VERIFIED | 29.1-29.3 (LRR 29): the defender is the opponent of the combat the active player opened — the anchoring was **absent** (roles came from seating order), added 2026-09-01 with the Attacker fix; the two-sided combat window remains the structural boundary for N>2 |
| Deploy | VERIFIED | 20.1-20.5; the two in-scope DEPLOY mechs (Sol, Letnev) are implemented |
| Destroyed | VERIFIED | 31.1-31.2; removal by fleet supply or capacity does not fire destroyed triggers |
| Diplomacy | VERIFIED | both halves: opponents place a token in the chosen system, then ready two planets |
| Elimination | **ABSENT** | no code; harmless at a 4-round horizon |
| Entropic Scars | **ABSENT** | 9 rules, none implemented; anomaly tiles are in the corpus |
| Exhausted | VERIFIED | 34.1-34.5 + notes 1-2: exhaustion is a flag on technologies, planets, relics and leaders; the status phase readies all four kinds (81.6 confirmed against `status.rs`); planets exhaust in payment and never both at once (75.2); a not-Ready leader refuses and an exhausted technology cannot pay; planets ready at the end of the agenda phase; note 2's "your planets" is enforced by the `controlled_planets` filter, and the cards that reach into rivals' planets say so instead |
| Expedition | **PARTIAL** | all six slice costs exact and the claim guards hold (once per slice, once per turn — the action consumes the turn); the LRR "at the end of their turn" timing is modelled as a turn-consuming component action — no end-of-turn decision window exists in the driver, a design boundary recorded, not a defect; the sixth-slice Thunder's Edge placement is ABSENT as a board feature |
| Exploration | VERIFIED | 35.1-35.8b + notes: the permission clause (35) — a token is explored only by a DET owner or another game effect — was **wrong** (the token explored on *any* arrival) and the DET trigger itself was **absent**; both fixed 2026-09-02: arrivals now only announce, and the trigger fires in `close_tactical`, covering a fleet that moved in and one already parked on the token; 35.8a reshuffle is **open** (no exploration discard pile exists, and the 14-card POK frontier deck is thin against the ~28 planetless systems on the engine map, so exhaustion is reachable), the simultaneous-exploration order (35.3) is fixed-order, and 35.2c is vacuous in scope (no POK planet carries two traits) |
| Fighter Tokens | OK | intentionally uncapped, `supply.rs` documents why |
| Fleet Pool | VERIFIED | 37.1-37.6; Letnev's Armada lifts the cap after Fleet Regulations caps it |
| The Fracture | **ABSENT** | 15 rules, none implemented |
| Frontier Tokens | OK | station-only tiles take a token (rule 14) — phase 1 |
| Game Board | VERIFIED | 39.1-39.2 + notes: every placed tile is on the board, isolated ones included, and a system is on the rim iff one of its six hex neighbours is an empty board slot (`edge_systems`); the two in-scope consumers (Populate the Outer Rim, Control the Borderlands) read from it; the Creuss-home and wormhole-nexus clauses are N/A (ABSENT as board features), hyperlane edges are N/A (no hyperlane tiles placed), and the setup variants are out of scope — the engine builds its own deterministic spiral |
| Game Round | VERIFIED | 40.1-40.3 + notes: Strategy → Action (first player in initiative order) → Status → Agenda (only after the custodians are removed, 8.1) → RoundEnded; turns happen only in the Action phase, `active` is `None` everywhere else; all six transient flags are turn-scoped and none persists across a player turn; the nine-round cap is subsumed — games end at 10 VP or on objective-deck exhaustion (81.2), which lands around round 9; the sim's 50-round horizon is a safety net |
| Gravity Rift | VERIFIED | 41.1-41.5 all present in `movement.rs`/`transit.rs`, incl. the path-dependent +1 |
| Ground Combat | VERIFIED | 42.1-42.4; burst icons roll per die. Fragile now applies here (was space-only), and the Shield Paling mech lifts it |
| Ground Forces | OK | cannot be committed to stations (rule 5) — phase 1 |
| Hyperlanes | **PARTIAL** | re-confirmed in the tenth batch: the corpus carries the hyperlane tiles but the engine's map build never places one (the spiral is system tiles only), and line-based adjacency (44.1) is unmodelled — the standing 6.4/44 gap; the placement and alternate-setup clauses are N/A on the engine's own map |
| Imperial | VERIFIED | primary scores a public then Mecatol/secret; 45.4 counts hand *and* scored and returns an unscored one |
| Infantry Tokens | OK | as fighter tokens |
| Influence | VERIFIED | 47.1-47.3 + note: a planet's influence is the printed value (the corpus's own field — the "rightmost blue border" is how the card shows it), spending influence exhausts the planet (34.2), trade goods pay as influence through the same payment window (47.3), trade goods never vote (8.x), and the custodians' six-influence removal rides the same path — the invasion test funds the seat with trade goods |
| Initiative Order | VERIFIED | 48.1-48.3: the order is each player's lowest initiative card, ties by seating (the engine total-orders on seat index), and it drives the Action phase turn order, status-phase action-card draws, agenda votes and every initiative-referencing effect; 48.3 is vacuous at six players (all six holding the same number is impossible) and the code handles it anyway; the Naalu "0" token is out of scope |
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
| Promissory Notes | VERIFIED | 69.1-69.9; a seat can never hold its own faction's note, which is why 69.3 holds by construction. 69.10/69.11 need player elimination, which this engine has not |
| Purge | VERIFIED | 72.1-72.3; the Silver Flame purges before it branches, which is 72.3 |
| Readied | VERIFIED | 71.1-71.7; planets, technologies, relics, leaders and strategy cards all ready in the status phase |
| Reinforcements | **PARTIAL** | 70.1 supply limits present, and fighters/infantry correctly uncapped; note 1 (place from the board when the box is empty) is not offered — it only ever permits more |
| Relics | **PARTIAL** | 5 of 24 |
| Rerolls | VERIFIED | scoped 2026-08-31: the Thalnos cards were reaching space cannon, barrage and bombardment rolls, none of which is a combat round |
| Resources | VERIFIED | 75.1-75.3 |
| Ships | **PARTIAL** | 77.1-77.4; unit upgrades now apply (was: researched and never applied), so Fighter II's excess-fighters clause is the one part still unmodelled |
| Space Cannon | **PARTIAL** | offence, defence and the adjacency clause (PDS II, Indomitus) -- adjacency was **absent** until 2026-08-31 |
| Space Combat | VERIFIED | 78.1-78.9: barrage first round only, defender announces first, the round loops back to Announce Retreats |
| Space Dock | **PARTIAL** | 68.3 (one per planet) and the capture case of 68.4; the coexistence path can meet 68.4's condition without a control change and is not checked |
| Space Stations | **PARTIAL** | rules 2, 2a, 2b, 5, 7, 14 done (phase 1); 8, 10, 12 economy outstanding |
| Speaker | ? | |
| Status Phase | **PARTIAL** | all eight steps present; 81.5's second sentence (redistribute tokens already held) is **not** implemented — see `status.rs` |
| Strategic Action | VERIFIED | 82.1-82.4 and 82.6; 82.5 is a 3-4 player rule and out of scope at six |
| Strategy Card | VERIFIED | initiative order, exhaustion, and the secondary token gate (52.3 exempts Leadership) |
| Strategy Phase | **PARTIAL** | 91.1/91.2 present; 91.1a (the trade goods on a chosen card go to the chooser) was **absent** — the pile grew all game and nobody collected it. Fixed 2026-09-01 |
| Structures | OK | not placeable on stations (rule 5) — phase 1 |
| Supernova | VERIFIED | 86.1 bar and the Magmus Reactor exemption |
| Sustain Damage | **PARTIAL** | 15.1-15.6 present; 15.7 Non-Euclidean Shielding was **absent**, added 2026-08-31 |
| Synergy | **ABSENT** | 6 rules, none implemented; every breakthrough carries a synergy |
| System Tiles | ? | |
| Tactical Action | VERIFIED | all five steps in order; 89.1b gates activation and production runs whether or not anything moved |
| Technology | VERIFIED | 90.1-90.23: colours, faction restriction, prerequisites, specialties, unit upgrades have no colour |
| Technology (S.C.) | VERIFIED | with Technology; the secondary charges four resources and Jol-Nar substitutes the primary |
| Trade | VERIFIED | three trade goods, replenish, and chosen players replenish too |
| Trade Goods | VERIFIED | 93.1-93.9; votes never touch trade goods (93.4b) and a received commodity lands as a trade good (93.7) |
| Transactions | **PARTIAL** | station-to-station (rule 10) absent |
| Transport | **PARTIAL** | 95.5 (no pickup from your own command token) was **absent**, added 2026-08-31; 95.1 pickup *en route* is still origin-only |
| Units | VERIFIED | 96.1-96.4; every plastic count matches the box, and fighters/infantry are correctly uncapped tokens |
| Unit Upgrades | **PARTIAL** | 90.7/90.8 applied 2026-09-01 at research and at production; before that no upgraded unit ever reached the board |
| Victory Points | VERIFIED | 98.1-98.10: cap at 10, initiative tiebreak, a law's point survives the law |
| Warfare | **PARTIAL** | recall and pool gain present; "then redistribute your command tokens" was **absent**, added 2026-08-31 |
| Wormhole Nexus | **PARTIAL** | counted by one secret; not modelled as a board feature |
| Wormholes | VERIFIED | 101.1-101.4 and the notes; a system is never adjacent to itself, and PDS II fires through wormholes because `Galaxy::adjacent` already unions them |

Totals after phase 9, tenth batch: **0 wrong**, **5 absent**, **33 partial**, **41 verified
correct**, **11 ok**, **1 out of scope**, **18 unverified** (was 79 at the audit's start; the
topic table is the source of truth).

## Phase 9 verification, 2026-08-31

Twenty-three topics moved off the *unverified* list by fetching their rules text and reading it against
the code — pass 1 of the method above, the only pass that establishes correctness.

**Twenty-seven defects in the topics checked so far: nineteen fixed, eight open.** That is close to the
base rate this audit warned about, and it is the reason the remaining unverified rows should still
be read as "not checked" rather than "probably fine":

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
| LRR 3 note 2 / 22.4 | the `ACTION_COMPLETED` after-window never opened after a component action — all six component branches advanced the turn directly, so Master Plan and every other "after you perform an action" trigger slept through faction, technology, expedition, action-card, device and relic actions | fixed |
| LRR 22.4 | a component action cancelled while announced (Sabotage in its WHEN window) still consumed the turn: the card was spent and the player lost the action the rules say was not used | fixed |
| LRR 13 / 29.1 | combat roles came from seating order, not from the active player — a combat opened by the seat seated behind an opponent in the system rolled first on the wrong side, took the nebula bonus on the wrong side, and announced retreats in the wrong order | fixed |
| LRR 49 | the post-combat invasion gate tested whether the seating-first survivor was the activator instead of whether the activator was among the survivors — a seated-second activator who outlasted the combat never got its invasion step | fixed |
| LRR 35 | a frontier token was explored on *any* arrival — the rule allows it only to a player who owns the Dark Energy Tap or is allowed by another game effect; `note_arrival` now only announces `SHIP_MOVED` | fixed |
| LRR 35 / DET | the Dark Energy Tap trigger — "after you perform a tactical action in a system that contains a frontier token, if you have 1 or more ships in that system, explore that token" — had no code at all; it now fires in `close_tactical`, covering a fleet that moved in and one already parked on the token | fixed |
| DET retreat | the holder's fleet may retreat into adjacent systems holding no other players' units even without own units or a controlled planet there — modelled as the union with 78.7c, because the technology waives only the own-presence clause (and its "units" is stricter than 78.7c's "ships") | fixed |
| LRR 35.8a | exploration decks are never reshuffled from their discard — the engine has no exploration discard pile at all, and the fourteen-card POK frontier deck is thin against the ~28 planetless systems on the engine map, so exhaustion is reachable in engine play | **open** |
| LRR 35.3 | a player exploring several planets simultaneously chooses the order; the engine resolves them in a fixed order without asking | **open** |

Clean on inspection against their rules text: Abilities, Active Player, Active System, Anomalies,
Attacker, Anti-Fighter Barrage, Asteroid Field, Fleet Pool, Gravity Rift, Space Cannon, Supernova.
Capture is Cabal-only and therefore out of scope.

**Ninth batch (2026-09-01): Action Phase, Combat, Component Action, Cost, Deals, Defender and
Exhausted** — the four defects above, all in paths a four-round game takes every time it acts or
fights, plus the batch's one design boundary:

The two component-action defects share a root cause: the six component branches each advanced the
turn by hand instead of ending it through the same `finish_action` every other action uses, so the
after-window never fired and a cancelled play consumed the action it cancelled. One test pair pins
both halves — an after-window card fires after a component action, and a play cancelled by
Sabotage re-offers the same turn with the card spent and the same `turn_seq`.

The combat-role defect had no test at all until `the_active_player_is_the_attacker_whoever_is_
seated_where`, and the invasion gate needed a combat that ends with *both* fleets present — fifty
rounds of an activator that never misses against a fleet that never hits — before the
seating-order reading became observably wrong. `the_activator_may_invade_when_seated_second_and_
still_holding` drives exactly that through the game driver and fails on the old gate in the
seated-second arm only.

Deals is the batch's one partial, and it is structural: the engine models the transactional half
of LRR 28 — adjacency and note-holding legality when offers are built — while the offer itself,
its binding and non-binding character, and counters (28.2-28.4) cannot be represented in a
single-agent decision engine, where every player is scripted by the same decider. Recorded as a
design boundary, not a defect.

Exhausted came back clean against 34.1-34.5 and its two notes: the status phase readies
technologies, planets, relics *and* leaders (81.6's four kinds, confirmed against `status.rs`),
planets exhaust in payment and never both at once, not-Ready leaders refuse and exhausted
technologies cannot pay, planets ready at the end of the agenda phase, and note 2 holds through
the `controlled_planets` filter — the cards that reach into rivals' planets name it instead.

The batch changes engine behaviour (the combat pair, and the new event emissions after component
actions), so the behavior baseline moved v26 -> v27: `share_SPACE_COMBAT_RESOLVED`
[0.0056, 0.0064] -> [0.0051, 0.0059], `faction_differentiation` [0.696, 1.185] ->
[0.452, 1.047], `vp_pace` [0.392, 0.451] -> [0.406, 0.460]. Raw old/new values are in
`plans/evidence/M08-021.md`. Two of the v26 metric bounds were already breached before the
re-baseline, which is how the gate caught the batch's effect rather than the batch catching
itself.

**Tenth batch (2026-09-02): Expedition, Exploration, Game Board, Game Round, Hyperlanes,
Influence and Initiative Order** — three defects, all on the Dark Energy Tap / frontier path, all
fixed:

LRR 35's permission clause — a token is explored only "if they own the Dark Energy Tap
technology or if another game effect allows them to" — had no gate at all: `note_arrival` explored
on *any* arrival, and the engine contained zero references to the Dark Energy Tap. The fix is two
halves: arrivals now only announce, and the DET trigger fires in `close_tactical`, the single
convergence point every tactical action ends through — which is what makes a fleet already parked
on the token explore when its owner acts there, and keeps the move that lands a fleet from
exploring on its own. The third defect is DET's retreat relaxation, modelled as the *union* with
78.7c rather than a replacement: the technology waives only the own-presence clause, and its
"units" is stricter than 78.7c's "ships", so an enemy garrison still bars a DET destination. One
test pair pins the arrival/trigger halves, two pin the retreat half, and a control test pins the
78.4c "not asked when there is nowhere to go" behaviour for non-holders.

The other topics came back clean or re-confirmed as known gaps: Game Board (39), Game Round (40 —
the nine-round cap subsumed by the 10-VP and deck-exhaustion endings), Influence (47) and
Initiative Order (48) verified. Expedition is PARTIAL — the six slice costs are exact and the claim
guards hold, but the LRR "at the end of their turn" timing is a turn-consuming component action
(no end-of-turn decision window exists in the driver), and the sixth-slice Thunder's Edge placement
is ABSENT. Hyperlanes stays on the open list as the 6.4/44 gap, and the batch adds two open
items: the missing exploration-deck reshuffle (35.8a — now high-reachability, a fourteen-card POK
deck against the ~28 planetless systems on the engine map) and the unasked simultaneous-exploration
order (35.3).

The fixes change POK game behaviour (frontier tokens no longer hand a draw to any arrival; DET
holders gain the trigger and the retreat option), so the behavior baseline moved v27 -> v28
without any bound being breached: every `now` value stayed inside the v27 intervals, and
`faction_differentiation`'s interval shifted the most, [0.452, 1.047] -> [0.490, 1.071]. Raw
old/new values are in `plans/evidence/M08-021.md`. One downstream effect: the policy campaign's
non-vacuity clause (a mid-window scorer re-offer, ~3% of games) lost coverage — its hand-picked
seeds no longer trigger the rare event under the shifted trajectories — so the seeds were
re-verified under the fixed engine (see the campaign comment in `bot.rs`).

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
