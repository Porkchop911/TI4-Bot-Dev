# Action card UI audit

Branch `action-card-ui-audit` (off `consolidated-final-2026-10-14`, HEAD `5c3e4588`). Read-only research: no app or engine code was changed. Written 2026-10-14.

## Summary

- The deck has **142 physical cards = 107 distinct cards** (base 82, Codex 20, PoK 20, Thunder's Edge 20; `crates/ti4-content/content/action_cards.json`). Every one has an engine effect (`effect_for`, `crates/ti4-engine/src/action_cards.rs:6694`, 142 aliases).
- Engine surface is small. Cards ask questions only through the helper `pick`/`pick_detailed` (`action_cards.rs:4993`), which emits subtype `<alias>_pick_<kind>` (source `ActionCard(<alias>)`), through ~10 hand-built choices (Confusing, Disgrace, Reparations, Riders, Ghost Squad, Skilled Retreat, Silence, Crash Landing), and through shared helpers (casualty, reroll dice). About half the cards ask nothing at all: they set a flag/marker (`TransientFlags`, `last_sustain`, per-activation markers) and only exist to the client as a **reaction window** (`reaction_<when|after>_<EVENT>`, handled by `ReactionStatusBar`).
- Classification of the 107 cards: **(a) 59 fine, (b) 41 works but clumsy, (c) 7 need a specialised element**. The (a) group is mostly the planet/system/player/technology/strategy-card pickers built on 2026-10-07 (see `docs/UI_DECISION_COVERAGE.md`).
- The seven (c) cards: **Fire Team** (dice reroll), **Lucky Shot** (ship picker with no unit details), **Repeal Law** (raw law ids), **Insider Information** (engine no-op: the top-3 peek is not implemented at all), **Ghost Squad** (ground-force mover), **Lie in Wait** (hidden hands, raw labels), **Plagiarize** (broken tech tiles, verified).
- Biggest lever: not a card-specific widget but the **reaction prompt**: 26 reaction-only cards plus 11 "marker" cards (Flank Speed, Morale Boost, Nav Suite, ...) share one gap: the prompt says *that* a window opened but not *what playing would do*, and nothing shows a played card's lingering effect afterwards. Reaction windows are the most common card-related decision in the nightly traces (over 8,000 windows in 263 runs, against about 2,600 pick decisions).

## Method and what was verified

1. **Cards**: parsed `action_cards.json` (alias, name, phase, window, text, source).
2. **Engine**: read `effect_for` and each effect function (decisions and kinds), cross-checked with `docs/UI_DECISION_COVERAGE.md` (which enumerated 130 `DecisionContext::new` call sites).
3. **Web**: read the routing in `web/src/presentation/choiceModel.ts`, `PendingChoiceModal.tsx` (panel order), `cardOptions.ts`, `unitPick.ts`, `technologyPick.ts`, `politicsDecision.ts`, `systemFacts.ts`, `strategyGoods.ts`, `reactionModel.ts`, `turnBar.ts`.
4. **Verified by driving real data** (basis column `V`): I took one real decision per action-card subtype from the nightly traces (`nightly-reports/*/runs/*/trace/trace.jsonl`, 263 runs, 54 distinct card subtypes with real ids/kinds/labels/option shapes) and fed each through the real web routing and describe functions under vitest (scratch test, removed afterwards; nothing committed). That produced the "which component renders it" column for 54 subtypes and confirmed these concrete defects:
   - **Plagiarize**: option id is `<seat>|<tech>`; `describeTechnologyPickOption` treats the whole id as a tech id and renders the name `"Player Ebc4ae...|Amd"` with text `"Technology"` (verified).
   - **Lucky Shot**: `lucky_pick_ship` matches the unit-pick regex but the engine sends no `details.units`, so `describeUnitPickOption` returns `null`; options stay raw text `destroy player_2f37...'s cruiser in 31` (verified).
   - **Public Disgrace / Overrule / Strategize**: `isStrategyCardGrid` is false for them (Disgrace kind `strategy_card` with subtype `public_disgrace_choose_card`; Overrule/Strategize kind `strategy card` with a space), so they are radio lists; Overrule/Strategize labels carry raw seat ids (verified).
   - **Repeal Law**: ids and labels are raw law ids (`minister_exploration`), no law text (verified).
   - **Probe**: option ids like `83a120` are frontier tokens, not system ids, so `isSystemPickChoice` (needs a board lookup) cannot treat them as a system pick (inferred from id shape; board lookup not run).
   - **Fire Team**: `reroll die 1 of combat value 6 (shows 10)` as a radio option, one die at a time, no dice display.
5. **Not verified**: nothing was rendered in a browser (the nightly sweep owns the CPU; no Playwright or cargo runs). Planet-payload and `details` shapes are not in traces, so for planet picks and `details.units` I used the engine code; those rows are basis `R` (read the code). Cards that never asked a question in 263 bot runs (basis `R`) were classified from the engine function plus the reaction model only. `web2/` was not examined.
6. **Frequency caveat**: counts are decisions in bot games (nightly sweep), a proxy; human play weights reactions and combat cards more heavily than bots, which play every action card they are offered.

Classes: (a) generic or existing component is fine; (b) works, but clumsy or missing a consequence preview; (c) needs a specialised element. Element codes are defined in the next section.

## Specialised elements, grouped

| Code | Element | Serves (distinct cards) | Count |
|---|---|---|---:|
| A | Reaction prompt with effect preview ("what playing it would do now") | Sabotage, Direct Hit, Shields Holding, Maneuvering Jets, Reflective Shielding, Salvage (has it), Parley, Infiltrate, Deadly Plot, Counterstroke, Rout, Waylay, Intercept, Emergency Repairs, Disable, Bunker, Blitz, Fighter Prototype, Rally, Harness Energy, Hack Election, Distinguished Councilor, Political Stability, Coup d'Etat, Confounding Legal Text, Reverse Engineer | 26 |
| K | Active-effects strip (lingering card effects: who, what, until when) and skipped/marked notices | Flank Speed, Lost Star Chart, Morale Boost, Nav Suite, Solar Flare, War Machine, Master Plan, Puppets on a String, Extreme Duress, Crisis, Black Market Dealings | 11 |
| B | Board-anchored unit/ship/system picker (real names, icons, map highlight, consequence) | Lucky Shot, Plague, Upgrade, Exploration Probe, Rescue, Experimental Battlestation (+ Scuttle/Refit/Decoy already partly) | 6 |
| I | Multi-step unit pick with progress ("1 of 2, stop") | Scuttle, Refit Troops (built), Decoy Operation | 3 |
| F | Player picker with per-target consequence facts | Spy, Insubordination, Ancient Burial Sites, Assassinate Representative, Forward Supply Base, Confusing Legal Text, Plagiarize (fix) | 7 |
| E | Strategy-card grid by holder (reuse draft grid) | Public Disgrace, Overrule, Strategize | 3 |
| D | Law / agenda card pickers and top-of-deck peek | Repeal Law, Insider Information (engine work too), Veto, Archaeological Expedition (reveal 3) | 4 |
| C | Dice display and reroll picker | Fire Team, Courageous to the End, Scramble Frequency (shared with the never-seen combat reroll rows of the coverage doc) | 3 |
| J | Ground-force mover / coexistence prompt | Ghost Squad, Crash Landing, Exchange Program | 3 |
| H | Hidden-hand / fragment card picker | Lie in Wait, Seize Artifact | 2 |
| G | Number stepper | Bribery | 1 |

## Proposed components, in priority order

Effort: XS under half a day, S about a day, M two to four days, L over a week. "Eng" = engine change needed to feed the UI.

| # | Component | Covers | Why this rank | Effort |
|---|---|---|---|---|
| 0 | **Bug fixes** (do first): Plagiarize tile id split `seat|tech`; Lucky Shot unit details (`details.units` like Scuttle, or reuse its `unit_pick_details`); Disgrace/Overrule/Strategize grid routing and seat-id to name mapping | Plagiarize (39 asks), Lucky Shot, Disgrace (63), Overrule, Strategize | Plainly wrong or raw text today; each a few lines | XS each |
| 1 | **Reaction prompt with effect preview + active-effects strip (A, K)**. Extend `ReactionRow.effect` (currently only `salvageEffect`) with small pure client functions from public state: Direct Hit ("destroys <their Dreadnought> in <system>"), Shields Holding ("cancels 2 of your N hits"), Maneuvering Jets, Morale Boost ("+1 to all your rolls this round"), Counterstroke, Infiltrate, Parley. For K, expose the card flags the engine already keeps (`TransientFlags`, per-activation markers) in the view, and show them as chips on the turn bar / combat overlay until they expire | 37 cards | Most frequent decision type touching action cards (over 8,000 windows in 263 runs; Sabotage alone 597, Flank Speed 446, Morale Boost 227, Skilled Retreat 220) and the only one that affects every table | M (client) plus M (Eng for K) |
| 2 | **Dice display and reroll picker (C)**: show the roll (faces, hit threshold, hits), let the roller tick dice to reroll, show before/after. Same component serves the never-seen combat rerolls (Crown of Thalnos, Jol-Nar commander, War Funding, Kenara, Ixthian) | Fire Team, Courageous, Scramble + 6 non-card rows | Generic UI cannot show what a die rolled; high-stakes moments in combat | M |
| 3 | **Board-anchored picker upgrade (B/I)**: generalise `UnitPickParts` to all `*_pick_ship/infantry/unit/cruiser/rescue_from` kinds, add map highlight of the option's system, real names (no seat ids), consequence line (Plague: "N infantry there, expect about N/6"; Lucky Shot: "destroys <their Dreadnought>"; Upgrade: replaced by a Dreadnought) | Lucky Shot, Plague, Upgrade, Probe, Rescue, Experimental, Decoy | Plague asks are the 5th most frequent card decision (120); Lucky Shot and Rescue are fully raw | S to M (Eng: `details.units` in `lucky`, `decoy`, `upgrade`, frontier id to system mapping for Probe) |
| 4 | **Player picker consequence facts (F)**: add a per-card one-line fact per target (Spy: cards in hand; Insubordination: tactic tokens; ABS: cultural planets that exhaust; Seize: fragments held; Plagiarize: neighbour techs you lack; FSB: who gets +1) | 7 cards | Spy 100, Insub 52, FSB 36, ABS 17 asks; picker exists, facts are the gap (the coverage doc already noted "no standing shown") | S |
| 5 | **Law picker and top-of-deck peek (D)**: Repeal as law tiles with name and text (reuse agenda card block); **Insider Information must first be implemented in the engine** (effect is empty) and then show the top 3 agendas privately (peek only; the card does not reorder); Veto replacement banner on the agenda strip | Repeal Law, Insider Information, Veto, Arch. Expedition | Insider is a no-op: a correctness gap not just UI; Repeal 19 asks | M (Eng + web) |
| 6 | **Ground-force mover (J)**: reuse `InvasionLandingTray` styling to move N infantry between two planets with before/after counts; coexistence confirm for Exchange Program and Crash Landing | Ghost Squad, Crash Landing, Exchange Program | Rare (Ghost Squad never asked in 263 runs) but the generic list "move N infantry from A to B" is unusable at scale | M |
| 7 | **Hidden-hand picker (H)**: show the revealed hands (Lie in Wait) as card tiles grouped by player, fragment tiles with art for Seize | Lie in Wait, Seize | Rare (3 and 11 asks); Thunder's Edge/PoK only | S |
| 8 | **Number stepper (G)**: +/- control with total cost for Bribery (and reusable for vote goods) | Bribery | 20 asks; the list of 0..N radios is clumsy but works | XS |

Cheap shared wins that do not need a new component: put the card name and printed text in the header for every `ActionCard`-sourced decision (today only player/unit pickers do via `describePickCard`/`describeUnitPickCard`), and map every raw seat id in labels to the participant name (`useParticipantText`).

## Table of all 107 cards

Columns: card; copies in deck; source; timing (phase: printed window); engine decision (subtype and option kind); current web component; class; element code (`-` none needed); decisions seen in 263 nightly runs; basis (`V` verified by routing a real traced decision through the web code, `R` read from engine and web code only). Sorted by class (c, b, a), then frequency.

| Card | Copies | Source | Timing | Engine decision | Current UI | Class | Element | Seen | Basis |
|---|---:|---|---|---|---|---|---|---:|---|
| Plagiarize | 1 | codex1 | Action: Action | `plagiarize_pick_technology` (technology, id `seat\|tech`, label "<raw seat id>'s Antimass Deflectors") | TechnologyPickPanel tiles; names render as "Player Ebc4...\|Amd" with text "Technology" (VERIFIED bug: tile code reads the id as a tech id) | c | F | 39 | V |
| Repeal Law | 1 | base | Action: Action | `repeal_pick_repeal` (kind `repeal`, id = law id, label = raw id e.g. "minister_exploration") | generic list of raw law ids, no law text (VERIFIED) | c | D | 19 | V |
| Fire Team | 1 | base | Action: After your ground forces make combat rolls during a round of ground combat | `fire_team_reroll` (reroll_die per die + decline) | generic list "reroll die 1 of combat value 6 (shows 10)"; no dice shown | c | C | 18 | V |
| Lie in Wait | 1 | thunders_edge | Any: After 2 of your neighbors resolve a transaction | `lieinwait_pick_action_card` x2 (kind `action_card`, label "take <alias> from <raw seat id>") | generic list with raw alias + seat ids (hidden hands revealed only through these labels) | c | H | 3 | V |
| Lucky Shot | 1 | base | Action: Action | `lucky_pick_ship` (kind `ship`, ids `sys\|index`, label "destroy <raw seat id>'s cruiser in 31"); NO `details.units` | UnitPickPanel shows card text only; options are raw labels, no icons, no map highlight (VERIFIED: describeUnitPickOption returns null) | c | B | 2 | V |
| Ghost Squad | 1 | codex1 | Action: After another player commits units to land on a planet you control | `ghost_squad_move` (move groups, + decline), repeated | generic list "move N infantry from A to B" | c | J | - | R |
| Insider Information | 1 | codex1 | Agenda: After an agenda is revealed | NONE: effect body is empty (`insider_information` is a no-op). The "look at top 3" never reaches any client | card can be played; shows nothing | c | D | - | R |
| Plague | 1 | base | Action: Action | `plague_pick_planet` (planet); rolls dice internally | PlanetSelectionBar; infantry count/dice not shown | b | B | 120 | V |
| Spy | 1 | base | Action: Action | `spy_pick_player` (player) | player picker (faction, VP, TG) | b | F | 100 | V |
| Public Disgrace | 1 | base | Strategy: When another player chooses a strategy card during the strategy phase | `public_disgrace_choose_card` (strategy_card, label=name only) | generic radio list, NOT the strategy grid (VERIFIED: isStrategyCardGrid false) | b | E | 63 | V |
| Insubordination | 1 | base | Action: Action | `insub_pick_player` (player) | player picker | b | F | 52 | V |
| Direct Hit | 4 | base | Action: After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities | none (destroys `last_sustain` unit); reaction only | reaction window only; trigger sentence, no target shown | b | A | 50 | V |
| Forward Supply Base | 1 | codex1 | Action: After another player activates a system that contains your units | `fsb_pick_player` (player) | player picker | b | F | 36 | V |
| Exploration Probe | 1 | pok | Action: Action | `probe_pick_system` (kind `system`, ids like `83a120` = frontier tokens) | SystemPickParts only if the id is a system; frontier ids are not (VERIFIED against real ids: board lookup needs tile id) | b | B | 28 | V |
| Assassinate Representative | 1 | base | Agenda: After an agenda is revealed | `assassin_pick_player` (player) | player picker | b | F | 21 | V |
| Bribery | 1 | base | Agenda: After the speaker votes on an agenda | `bribery_pick_count` (count 0..n TG) | generic radio list "spend N trade goods" | b | G | 20 | V |
| Ancient Burial Sites | 1 | base | Agenda: At the start of the agenda phase | `abs_pick_player` (player) | player picker (PoliticsDecisionParts, card text) | b | F | 17 | V |
| Archaeological Expedition | 1 | pok | Action: Action | `arch_expedition_pick_planet` (planet) then reveals 3 explore cards, relic fragments | PlanetSelectionBar; revealed cards in log/toasts | b | D | 12 | V |
| Seize Artifact | 1 | pok | Action: Action | `seize_pick_player` then `seize_pick_fragment` (kind `fragment`, ids CULTURAL...) | player picker; fragment step generic list with raw type | b | H | 11 | V |
| Exchange Program | 1 | thunders_edge | Action: Action | `exchangeprogram_pick_player`, `_pick_planet`, then `exchange_program_answer` yes/no for the other player | player picker, PlanetSelectionBar, generic yes/no | b | J | 10 | V |
| Overrule | 1 | thunders_edge | Action: Action | `overrule_pick_strategy card` (kind `strategy card`) then the primary of that card (full sub-flow) | generic list with raw seat ids (VERIFIED) | b | E | 7 | V |
| Confusing Legal Text | 1 | base | Agenda: When you are elected as the outcome of an agenda | `confusing_legal_text_elect` (player, kind `elect`) | generic list; raw seat ids | b | F | 6 | V |
| Courageous to the End | 1 | base | Action: After 1 of your ships is destroyed during a space combat | `courageous_to_the_end_assign_casualty` x N (casualty), dice rolled inside | combat_casualty -> SpaceCombatOverlay hit panel; the 2 dice are not shown before the picks | b | C | 6 | V |
| Strategize | 4 | thunders_edge | Action: Action | `strategizeN_pick_strategy card` (kind `strategy card`, label "<raw seat id>'s Leadership") then the card's secondary sub-flow | generic list (VERIFIED not a grid), raw seat ids | b | E | 6 | V |
| Black Market Dealings | 1 | thunders_edge | Any: When you are negotiating a transaction | none: state flag/auto-resolve (marker; widens the transaction) | TradeDeskModal (items include action cards, secrets, fragments) | b | K | - | R |
| Crash Landing | 1 | thunders_edge | Action: When your last ship in the active system is destroyed | `crashlanding_choose_ground` then `crashlanding_choose_planet` (kinds `crashlanding_ground`, planet) | ground: generic list; planet: PlanetSelectionBar | b | J | - | R |
| Crisis | 1 | thunders_edge | Action: At the end of any players turn, if there are at least 2 players who have not passed | none: state flag/auto-resolve (skip next turn flag) | reaction window only; skipped seat gets no notice | b | K | - | R |
| Deadly Plot | 1 | pok | Agenda: During the agenda phase when an outcome would be resolved | none: state flag/auto-resolve (discard agenda, exhaust all) | reaction window only at resolution; consequence not stated | b | A | - | R |
| Decoy Operation | 1 | pok | Action: After another player activates a system that contains 1 or more of your structures | `decoy_pick_unit` x2 ("another or stop") + `decoy_pick_planet` (kind `unit`, `planet`) | generic list for units; planet via PlanetSelectionBar | b | I | - | R |
| Experimental Battlestation | 1 | base | Action: After the active player moves ships into the active system during a tactical action | pick of dock (`experimental_battlestation` space_cannon) + dice | generic list "dock in <sys>"; rolls only in log | b | B | - | R |
| Extreme Duress | 1 | thunders_edge | Action: At the start of another player's turn, if they have a readied strategy card | none: state flag/auto-resolve (marker; punishes on next action) | reaction window only; the marked player gets no visible warning | b | K | - | R |
| Flank Speed | 4 | base | Action: After you activate a system | none: state flag/auto-resolve (+1 move) | reaction window only; no active-effect indicator afterwards | b | K | - | R |
| Infiltrate | 1 | base | Action: When you gain control of a planet | none: state flag/auto-resolve (replaces all PDS/docks) | reaction window only; "what it would replace" not shown | b | A | - | R |
| Lost Star Chart | 1 | base | Action: After you activate a system | none: state flag/auto-resolve (adjacency flag) | reaction window only; no indicator after play | b | K | - | R |
| Maneuvering Jets | 4 | base | Action: Before you assign hits produced by another player's SPACE CANNON roll | none; cancels hit | reaction window only; fires inside hit assignment | b | A | - | R |
| Master Plan | 1 | codex1 | Action: After you perform an action | none: state flag/auto-resolve (extra action flag) | reaction window only then a second turn menu | b | K | - | R |
| Morale Boost | 4 | base | Action: At the start of a combat round | none: state flag/auto-resolve (+1 all combat rolls) | reaction window only; effect lasts a round with no indicator | b | K | - | R |
| Nav Suite | 1 | pok | Action: After you activate a system | none: state flag/auto-resolve | reaction window only | b | K | - | R |
| Parley | 1 | base | Action: After another player commits units to land on a planet you control | none: state flag/auto-resolve (returns last committed unit) | reaction window only during landing tray; unit returned not previewed | b | A | - | R |
| Puppets on a String | 1 | thunders_edge | Action: At the end of a player's turn, if you have passed | none: state flag/auto-resolve (PUPPET_ACTION flag) then a full turn menu | reaction window only + turn menu | b | K | - | R |
| Reflective Shielding | 1 | codex1 | Action: When one of your ships uses SUSTAIN DAMAGE during combat | none: state flag/auto-resolve (2 hits) | reaction window only during sustain | b | A | - | R |
| Rescue | 1 | thunders_edge | Action: After a player moves ships into a system that contains your ships | `rescue_pick_rescue_from` (kind `rescue_from`, one per source system) | generic list | b | B | - | R |
| Scramble Frequency | 1 | codex1 | Action: After another player makes a BOMBARDMENT, SPACE CANNON, or ANTI-FIGHTER BARRAGE roll | none: state flag/auto-resolve (rerolls all rolled dice; no pick) | reaction window only; the old/new rolls only in the log | b | C | - | R |
| Shields Holding | 4 | base | Action: Before you assign hits to your ships during a space combat | none; cancels up to 2 hits | reaction window only inside hit assignment | b | A | - | R |
| Solar Flare | 1 | codex1 | Action: After you activate a system | none: state flag/auto-resolve | reaction window only | b | K | - | R |
| Upgrade | 1 | base | Action: After you activate a system that contains 1 or more of your ships | `upgrade_pick_cruiser` (kind `cruiser`, one per cruiser) | generic list; not matched by UnitPick regex | b | B | - | R |
| Veto | 3 | base | Agenda: When an agenda is revealed | none; swaps in `agenda_veto_replacement` | reaction window only at agenda reveal; agenda strip should show the replacement | b | D | - | R |
| War Machine | 4 | codex1 | Action: When 1 or more of your units use PRODUCTION | none: state flag/auto-resolve (+4 production) | reaction window only while building; ProductionBuilder shows the new total? | b | K | - | R |
| Manipulate Investments | 1 | pok | Strategy: At the start of the strategy phase | `investments_pick_strategy_card` x5 (kind `strategy_card`) | strategy grid with goods (built 10-07) | a | - | 345 | V |
| Divert Funding | 1 | pok | Action: Action | `divert_funding_pick_technology` x2 (return, then research) | TechnologyPickPanel tiles (built 10-07) | a | - | 206 | V |
| Scuttle | 1 | pok | Action: Action | `scuttle_pick_ship` x2 (kind `ship`, + stop) | UnitPickPanel with icons, place, cost (details.units) | a | I | 200 | V |
| Refit Troops | 1 | pok | Action: Action | `refit_pick_infantry` x2 (kind `infantry`, + stop) | UnitPickPanel with icons, place, effect (details.units) | a | I | 188 | V |
| Diplomatic Pressure | 4 | pok | Agenda: When an agenda is revealed | `dpN_pick_player` + `dpN_pick_promissory note` (hand card tiles) | player picker + card tiles with note text | a | - | 181 | V |
| Signal Jamming | 1 | base | Action: Action | `jamming_pick_system` + `jamming_pick_player` (system, player) | SystemPickParts then player picker | a | - | 111 | V |
| Skilled Retreat | 4 | base | Action: At the start of a combat round | `skilled_retreat_choose_system` (system, kind `skilled_retreat`) | SystemPickParts + map | a | - | 97 | V |
| Frontline Deployment | 1 | base | Action: Action | `f_deployment_pick_planet` (planet) | PlanetSelectionBar + map | a | - | 91 | V |
| Mining Initiative | 1 | base | Action: Action | `mining_initiative_pick_planet` (planet) | PlanetSelectionBar | a | - | 87 | V |
| Cripple Defenses | 1 | base | Action: Action | `cripple_pick_planet` (planet) | PlanetSelectionBar + map | a | - | 82 | V |
| War Effort | 1 | base | Action: Action | `war_effort_pick_system` (system) | SystemPickParts | a | - | 74 | V |
| Uprising | 1 | base | Action: Action | `uprising_pick_planet` (planet) | PlanetSelectionBar | a | - | 59 | V |
| In The Silence Of Space | 1 | base | Action: After you activate a system | `silence_choose_system` (system, kind `silence`) | SystemPickParts + map | a | - | 45 | V |
| Focused Research | 1 | base | Action: Action | `f_researched_pick_technology` (technology) | TechnologyPickPanel tiles | a | - | 40 | V |
| Unstable Planet | 1 | base | Action: Action | `unstable_pick_planet` (planet) | PlanetSelectionBar | a | - | 31 | V |
| Unexpected Action | 1 | base | Action: Action | `unexpected_pick_recall` (system) | system picker (built 10-07) | a | - | 28 | V |
| Pirate Contract | 4 | thunders_edge | Action: Action | `piratecontract1_pick_system` (system) | SystemPickParts | a | - | 6 | V |
| Reveal Prototype | 1 | pok | Action: At the start of a combat | `reveal_prototype_pick_technology` (technology) | TechnologyPickPanel; payment after | a | - | 2 | V |
| Tactical Bombardment | 1 | base | Action: Action | `tactical_pick_system` (system) | SystemPickParts | a | - | 2 | V |
| Reactor Meltdown | 1 | base | Action: Action | `meltdown_pick_planet` (planet, space dock) | PlanetSelectionBar | a | - | 1 | V |
| Blitz | 1 | codex1 | Action: At the start of an invasion | none: state flag/auto-resolve (BOMBARDMENT 6 marker) | reaction window only | a | A | - | R |
| Brilliance | 1 | thunders_edge | Action: Action | `brilliance_pick_player`/planet ready pick | player picker / PlanetSelectionBar | a | - | - | R |
| Bunker | 1 | base | Action: At the start of an invasion | none: state flag/auto-resolve (invasion-scoped marker) | reaction window only (ReactionStatusBar) | a | A | - | R |
| Confounding Legal Text | 1 | pok | Agenda: When another player is elected as the outcome of an agenda | none: state flag/auto-resolve (override elected) | reaction window only | a | A | - | R |
| Construction Rider | 1 | base | Agenda: After an agenda is revealed | Rider: `predict` outcome list + agenda reaction | PredictOutcomePanel (built 10-07) | a | - | - | R |
| Counterstroke | 1 | codex1 | Action: After another player activates a system that contains 1 of your command tokens | none: state flag/auto-resolve (token to tactic pool) | reaction window only; token returned not previewed | a | A | - | R |
| Coup d'Etat | 1 | pok | Action: When another player would perform a strategic action | none: state flag/auto-resolve (cancels strategic action) | reaction window only at strategic action start | a | A | - | R |
| Diplomacy Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict + system pick after success | PredictOutcomePanel; system via SystemPickParts | a | - | - | R |
| Disable | 1 | base | Action: At the start of an invasion in a system that contains 1 or more of your opponents' PDS units | none: state flag/auto-resolve (invasion marker) | reaction window only | a | A | - | R |
| Distinguished Councilor | 1 | base | Agenda: After you cast votes on an outcome of an agenda | none: state flag/auto-resolve (+5 votes) | reaction window only; vote tally shown by ballot modal | a | A | - | R |
| Economic Initiative | 1 | base | Action: Action | none: state flag/auto-resolve (readies cultural); `details.action_card_effects` carries planets | turn bar row shows "readies N of M" (turnBar.ts planetsSummary) | a | - | - | R |
| Emergency Repairs | 1 | base | Action: At the start or end of a combat round | none: state flag/auto-resolve (repairs all) | reaction window only | a | A | - | R |
| Fighter Conscription | 1 | codex1 | Action: Action | none: state flag/auto-resolve (fighters in each valid system) | none needed; result visible on the map | a | - | - | R |
| Fighter Prototype | 1 | base | Action: At the start of the first round of a space combat | none: state flag/auto-resolve | reaction window only | a | A | - | R |
| Ghost Ship | 1 | base | Action: Action | `ghost_ship_pick_system` (system, wormhole) | SystemPickParts (if the board knows ids) | a | - | - | R |
| Hack Election | 1 | codex1 | Agenda: After an agenda is revealed | none: state flag/auto-resolve (vote order) | reaction window only at agenda reveal; ballot order shown by AgendaBallotModal | a | A | - | R |
| Harness Energy | 1 | codex1 | Action: After you activate an anomaly | none: state flag/auto-resolve | reaction window only | a | A | - | R |
| Imperial Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict | PredictOutcomePanel | a | - | - | R |
| Impersonation | 1 | codex1 | Action: Action | none: state flag/auto-resolve (pays 3 influence via payment drawer, draws secret) | PaymentBar/Drawer | a | - | - | R |
| Industrial Initiative | 1 | base | Action: Action | none: state flag/auto-resolve (gain TG) | none needed (menu row); gain shown in log | a | - | - | R |
| Intercept | 1 | base | Action: After your opponent declares a retreat during a space combat | none: state flag/auto-resolve | reaction window only | a | A | - | R |
| Leadership Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict | PredictOutcomePanel | a | - | - | R |
| Mercenary Contract | 1 | thunders_edge | Action: Action | `mercenary_contract_pick_planet` (planet) + payment | PlanetSelectionBar | a | - | - | R |
| Pirate Fleet | 1 | thunders_edge | Action: Action | `piratefleet_pick_system` (system) + payment | SystemPickParts | a | - | - | R |
| Political Stability | 1 | base | Status: When you would return your strategy card(s) during the status phase | none: state flag/auto-resolve | reaction window only | a | A | - | R |
| Politics Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict | PredictOutcomePanel | a | - | - | R |
| Rally | 1 | codex1 | Action: After you activate a system that contains another player's ships | none: state flag/auto-resolve (2 tokens to fleet pool) | reaction window only | a | A | - | R |
| Reparations | 1 | base | Action: After another player gains control of a planet you control | `reparations_exhaust` / `reparations_ready` (planet) | PlanetSelectionBar | a | - | - | R |
| Reverse Engineer | 1 | pok | Any: After another player discards an action card that has a component action | none: state flag/auto-resolve (takes discard) | reaction window only | a | A | - | R |
| Rise of a Messiah | 1 | base | Action: Action | none: state flag/auto-resolve (infantry on each planet) | none needed | a | - | - | R |
| Rout | 1 | pok | Action: At the start of the 'Announce Retreats' step of space combat, if you are the defender | none: state flag/auto-resolve | reaction window only | a | A | - | R |
| Sabotage | 4 | base | Any: When another player plays an action card other than 'Sabotage' | none; cancels the played card | reaction window only ("Play Sabotage" row, trigger sentence names the cancelled card) | a | A | - | R |
| Salvage | 1 | base | Action: After you win a space combat | none: state flag/auto-resolve (take commodities) | reaction window only with computed `effect` (salvageEffect) - the only reaction with a preview | a | A | - | R |
| Sanction | 1 | codex1 | Agenda: After an agenda is revealed | Rider predict | PredictOutcomePanel | a | - | - | R |
| Summit | 1 | base | Strategy: At the start of the strategy phase | 2x `gain_command_token` (pool) | CommandTokenPanel | a | - | - | R |
| Technology Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict + research after success | PredictOutcomePanel; TechnologyModal | a | - | - | R |
| Trade Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict | PredictOutcomePanel | a | - | - | R |
| Warfare Rider | 1 | base | Agenda: After an agenda is revealed | Rider predict + system pick | PredictOutcomePanel; SystemPickParts | a | - | - | R |
| Waylay | 1 | pok | Action: Before you roll dice for ANTI-FIGHTER BARRAGE | none: state flag/auto-resolve | reaction window only | a | A | - | R |

## Known gaps in existing docs

- `docs/UI_DECISION_COVERAGE.md` lists Plagiarize, Focus Research, Repeal and Bribery as generic fallback; since then `TechnologyPickPanel` took over the first two (Plagiarize now mis-renders, see above). It also lists `/disgrace-pick`, `/repeal-pick`, `/seize-pick`, `/bribery-count`, `/confusing-elect` as still generic; this audit agrees.
- `docs/REACTION_UI_PLAN.md` described the reaction prompt as a raw prompt with "Triggered by: <viewer>"; the current `reactionModel.ts` has trigger facts, window text and per-row `effect`, but `effect` is only filled for Salvage.
- No web code references active card effects (grep for flank speed, active effects, transient flags in `web/src` finds nothing relevant), so every marker card in element K leaves no trace on screen after it is played other than the event log.

## Implementation status (branch `action-card-ui-pickers`, off this branch)

Done: priority 0 (Plagiarize tiles, Lucky Shot unit details, strategy-card grid for Public Disgrace, Overrule and Strategize, Repeal Law text, Fire Team dice, Insider Information), 2 (dice and reroll picker), 3 (unit and system picks anchored on the map: Lucky Shot, Upgrade, Rescue, Experimental Battlestation, Plague's consequence line, Scuttle and Refit Troops share it), 4 (player-pick facts: Ancient Burial Sites, Forward Supply Base, Seize Artifact, Exchange Program; Insubordination's fact was never shown because the code looked for the alias `insubordination` while the card's alias is `insub`), 5 (Repeal Law and the Insider Information peek), 6 (Ghost Squad mover), 7 (Lie in Wait and Seize Artifact fragments), 8 (Bribery stepper). Screenshots: `web/e2e/screenshots/EA-action-card-pickers`.

Not done: Decoy Operation (needs per-unit details), the Veto replacement banner, Archaeological Expedition's reveal of 3, the coexistence confirm for Exchange Program and Crash Landing, Probe's frontier ids, and priority 1 (reaction preview and active-effects strip, another branch).

Save and replay compatibility: every engine addition is display-only (option payloads, `Choice::details`), which never enter a `DecisionRecord`, so prompt, option ids, order and context are unchanged and the V1 and V2 decision hashes stay as they were (`fingerprint::display_facts_on_a_question_never_reach_its_decision_hash`). Insider Information asks nothing, so it adds no decision; it records a private `faction_marks` row (`private:<seat>:insider:peek`), skipped from the serialized state when empty.
