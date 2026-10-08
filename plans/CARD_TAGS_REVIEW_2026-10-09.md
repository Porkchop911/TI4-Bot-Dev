# Card tags - first-pass review (2026-10-09)

Status: draft-unreviewed. Artifact: `plans/CARD_TAGS_DRAFT_2026-10-09.json` (taxonomy_version 1). Labelled by two Haiku helpers (split: abilities/technologies/units/leaders/breakthroughs vs everything else), taxonomy and merge by Sonnet 5.5. No LLM server used. Mechanically validated (see end); semantics NOT human-reviewed.

## Counts

Cards: 896; labels: 2532; mean 2.83 labels/card; cards with no tags: 8; taxonomy tags: 133.

| kind | cards | labels | labels/card |
|---|---|---|---|
| ability | 73 | 166 | 2.27 |
| technology | 102 | 291 | 2.85 |
| unit | 97 | 205 | 2.11 |
| leader | 103 | 416 | 4.04 |
| promissory | 40 | 155 | 3.88 |
| breakthrough | 31 | 100 | 3.23 |
| action_card | 142 | 376 | 2.65 |
| agenda | 63 | 200 | 3.17 |
| public_objective | 40 | 53 | 1.32 |
| secret_objective | 40 | 61 | 1.52 |
| relic | 24 | 95 | 3.96 |
| exploration | 80 | 177 | 2.21 |
| galactic_event | 20 | 92 | 4.60 |
| legendary_planet | 19 | 76 | 4.00 |
| strategy_card | 12 | 53 | 4.42 |
| plot | 10 | 16 | 1.60 |

Labels-per-card histogram: 0:8, 1:110, 2:277, 3:267, 4:141, 5:60, 6:29, 7:3, 8:1

## Tag frequency (cards using tag) and definitions

| tag | cards | definition |
|---|---|---|
| timing-reaction | 271 | Reaction triggered by another player's action or event. |
| timing-action | 162 | Triggered as an ACTION (taken on your turn as the action). |
| exhaust | 90 | Card must be exhausted to use. |
| timing-combat | 78 | Triggers during or around a combat. |
| leader-unlock-condition | 70 | Leader that has an unlock requirement (commanders/heroes). |
| place-units-free | 58 | Places units from reinforcements without paying (not via production). |
| purge | 55 | Card is purged (removed from the game) after use. |
| destroy-units | 51 | Destroys units outright (not via combat hits). magnitude = number destroyed if stated. |
| gain-trade-goods | 47 | Gain trade goods. magnitude = amount if fixed. |
| ground-force-effect | 42 | Directly affects infantry/ground forces (placing, killing, boosting, moving). |
| requires-units-present | 42 | Effect conditional on having particular units in a system or on a planet. |
| deal-enforcement | 41 | Binding promises or commitments between players (promissory pledges, forced compliance, ceasefire). |
| elect-outcome | 36 | Effect of electing a player/planet/etc. and what they gain or lose. |
| gain-relic | 34 | Gain a relic or relic fragment. |
| hero-one-shot | 33 | Powerful effect followed by purging (hero). |
| timing-end-turn | 32 | Triggers at the end of a turn, round, or phase. |
| draw-action-card | 29 | Draw action cards. magnitude = N. |
| gain-command-token | 28 | Gain command tokens. magnitude = N. |
| gain-commodities | 26 | Gain or replenish commodities. magnitude = amount if fixed. |
| relocate-units | 26 | Moves units outside a normal move (teleport/swap/redeploy, move out of turn, transfer between systems). |
| timing-passive | 26 | Static always-on effect with no trigger. |
| ship-effect | 25 | Directly affects ships (placing, moving, killing, boosting). |
| combat-roll-bonus | 24 | Adds a positive modifier to combat rolls (space or ground) of your units or a chosen player's units. magnitude = the +N. |
| strategy-pool-use | 24 | Spends or interacts with tokens from the strategy pool as a cost or resource. |
| game-structure-change | 23 | Alters game structure/rules (e.g. galactic event modifying rules). |
| research-free | 23 | Gain or research a technology outside of normal research, or free/discounted research. |
| vote-restrict | 23 | Prevents voting or restricts how votes can be cast. |
| wormhole | 23 | Interacts with wormholes (counting, treating as wormhole, creating wormhole tokens). |
| mech-effect | 21 | Directly affects mechs (deploy, boost, revive, mech ability). |
| place-command-token | 21 | Place command tokens on the board or on cards. |
| obj-units-in-systems | 20 | Objective: units/ships in specified systems. |
| remove-command-token | 20 | Remove command tokens from board or from players' pools. |
| resources-bonus | 20 | Increases resources of planets or available resources. magnitude = N. |
| spend-trade-goods | 20 | Requires spending trade goods as a cost to use an effect. magnitude = cost. |
| move-bonus | 19 | Adds to the move value of ships (or sets it). magnitude = +N. |
| produce-units | 19 | Produces units outside normal production timing, or grants a production action. |
| cancel-hit | 18 | Cancels or ignores hits produced against your units, or prevents a unit from being destroyed. |
| gain-attachment | 18 | Attaches tokens/attachments to planets or systems. |
| gain-victory-point | 18 | Directly grants or removes victory points. magnitude = VP. |
| obj-control-planets | 18 | Objective: control a number/type of planets. |
| ready-planets | 18 | Readies exhausted planets. |
| law-permanent | 17 | A law/persistent rule change that remains in play. |
| negate-ability | 17 | Disables, cancels or nullifies another card, unit ability, technology, or faction ability. |
| bombardment | 16 | A unit has, grants, or enhances BOMBARDMENT. |
| exhaust-planets | 16 | Exhausts planets (yours or others'). |
| space-cannon | 16 | A unit has, grants, or enhances SPACE CANNON (including offense/defense variants and ranged use). |
| agenda-predict | 15 | Predict or choose an agenda outcome. |
| convert-commodities | 15 | Convert commodities to trade goods or modify commodity conversion. |
| strategy-card-manipulation | 15 | Exchange, take, change or restrict strategy cards or their abilities/initiative. |
| timing-status-phase | 15 | Triggers in the status phase. |
| discard-opponent-card | 14 | Make another player discard cards, or take/steal cards from their hand. |
| production-bonus | 14 | Increases or adds PRODUCTION capacity / units produced. magnitude = N. |
| requires-home-system | 14 | Effect tied to your home system, home planets, or another player's home. |
| requires-planets-controlled | 14 | Effect conditional on controlling planets/planet types/count. |
| sustain-damage | 14 | A unit has or gains the SUSTAIN DAMAGE keyword. |
| unit-replacement | 14 | Replaces a unit with a different (usually better/other) unit from reinforcements. |
| cost-reduction | 13 | Reduces the cost of units, technologies or other purchases. magnitude = amount. |
| influence-bonus | 13 | Increases influence of planets or available influence. magnitude = N. |
| place-structure | 13 | Places or builds structures (space dock, PDS) or changes where they may be placed. |
| prerequisite-skip | 13 | Ignore technology prerequisites. magnitude = N. |
| requires-neighbor | 13 | Effect targets or requires neighbors. |
| share-ability | 13 | Lets you (or others) use another player's card/ability/unit as if your own (alliance, commander sharing). |
| structure-effect | 13 | Directly affects structures (docks, PDS). |
| anomaly-movement | 12 | Ships may enter, move through, or are protected in anomalies (asteroid field, nebula, supernova, gravity rift). |
| extra-dice | 12 | Lets units roll additional dice in combat or another roll. magnitude = number of extra dice. |
| gain-promissory | 12 | Gain, take, steal, or give promissory notes. |
| post-combat-trigger | 12 | Triggers an effect after a combat round or on winning/losing combat. |
| space-cannon-immunity | 12 | Units or players are protected from SPACE CANNON hits, or the effect is reduced. |
| timing-strategy-phase | 12 | Triggers in the strategy phase. |
| adjacency-grant | 11 | Treats systems/units as adjacent that normally are not, or creates adjacency. |
| anti-fighter-barrage | 11 | A unit has or enhances ANTI-FIGHTER BARRAGE. |
| capacity-bonus | 11 | Increases ship capacity or lets units not count against capacity. magnitude = N. |
| explore | 11 | Explore planets/frontiers or draw exploration cards. |
| no-token-cost | 11 | Allows an action/secondary ability without spending a command token. |
| tech-specialty-use | 11 | Interacts with tech specialties, technology colors, or counts technologies owned. |
| transaction | 11 | Alters transactions: who can trade, what can be traded, or timing. |
| gain-secret-objective | 10 | Draw, gain, or swap secret objectives. |
| look-at-hidden-info | 10 | Look at hidden cards, objectives, or decks (hand, secret objective, top of deck). |
| obj-spend | 10 | Objective: spend resources, influence, trade goods or tokens. |
| repair-units | 10 | Repairs damaged units (removes damage markers). |
| tech-theft | 10 | Copy, take, or use another player's technology. |
| timing-agenda-phase | 10 | Triggers in the agenda phase. |
| unit-revival | 10 | A destroyed unit returns to play, is placed back from reinforcements, or is replaced after destruction. |
| combat-avoidance | 9 | Prevents or skips a combat, or prevents movement into a system to avoid combat. |
| extra-activation | 9 | Allows an additional activation, tactical action, or taking another action/turn. |
| faction-start-setup | 9 | Modifies setup, starting units, starting technologies, or components at game start. |
| fighter-effect | 9 | Directly affects fighters (placing, capacity, barrage). |
| obj-combat | 9 | Objective: win combats, destroy units, or kill units. |
| production-unit | 9 | A unit has the PRODUCTION keyword. |
| retreat-control | 9 | Grants, forces, or restricts retreats / retreat destinations. |
| timing-start-turn | 9 | Triggers at the start of a turn, round, or phase beginning. |
| vote-bonus | 9 | Gives additional votes or changes voting power. magnitude = N. |
| agenda-manipulation | 8 | Look at, reorder, reveal or otherwise manipulate the agenda deck / revealed agendas. |
| capture-units | 8 | Captures enemy units into your own capture area / holds units for ransom. |
| ignore-planetary-shield | 8 | Units ignore or remove PLANETARY SHIELD. |
| move-through-ships | 8 | Ships may move through or ignore other players' ships / blockades. |
| puppet-control | 8 | Controls another player's decisions, cards, or components (puppet). |
| ready-card | 8 | Ready exhausted cards, leaders or components. |
| reroll | 8 | Allows re-rolling dice (combat, exploration, or other rolls). |
| agenda-cancel | 7 | Cancel, discard, redo or replace an agenda or its outcome. |
| assign-hits-control | 7 | Lets a player choose which units take hits, or forces/limits how hits are assigned. |
| gain-planet | 7 | Take control of a planet or gain a planet card. |
| initiative-change | 7 | Changes initiative order or turn order. |
| obj-planet-traits | 7 | Objective: planet traits (cultural/hazardous/industrial) or planet attributes. |
| planetary-trait-bonus | 7 | Effect keyed to planet trait or attribute (legendary, cultural, hazardous, industrial). |
| requires-adjacent-units | 7 | Effect conditional on units being adjacent / in or near a system. |
| score-objective-help | 7 | Helps score, ignore requirements of, or gives conditions to score objectives. |
| give-trade-goods | 6 | Give trade goods/commodities to others (including as a cost). |
| move-without-transport | 6 | A unit may move without being carried, or be carried in unusual ways. |
| obj-own-technologies | 6 | Objective: own technologies/upgrades/colors. |
| obj-structures | 6 | Objective: structures/space docks/PDS on planets. |
| pre-combat-hits | 6 | Produces hits at the start of / before combat rolls (ambush, assault cannon, first strike), or destroys units at combat start. |
| restrict-activation | 6 | Prevents players from activating systems or limits where tactical actions can occur. |
| restrict-movement | 6 | Prevents or limits movement of ships/units (e.g. cannot move into a system). |
| war-sun-effect | 6 | Directly affects war suns or flagships. |
| fleet-limit-bonus | 5 | Raises the fleet pool / non-fighter ship limit per system, or exempts units from it. |
| frontier-token | 5 | Concerns frontier tokens or exploration discovery. |
| obj-hold-systems | 5 | Objective: occupy Mecatol Rex, home systems, or special systems. |
| redistribute-tokens | 5 | Reallocate command tokens between pools. |
| trade-goods-as-resources | 5 | Trade goods count as resources/influence or are worth more when spent. |
| combat-roll-penalty | 4 | Applies a negative modifier to combat/bombardment/space cannon die rolls, usually the opponent's. magnitude = N (positive number). |
| obj-tokens-cards | 4 | Objective: possess tokens, cards, relics, or trade goods. |
| obj-wormholes-anomalies | 4 | Objective: wormholes, anomalies, or system types. |
| once-per-action | 4 | Usable only once per action/combat/round/phase/turn. |
| obj-ownership-count | 3 | Objective: own a numeric count of components (units, planets, cards). |
| planetary-shield | 3 | A unit has/grants PLANETARY SHIELD or prevents bombardment. |
| steal-trade-goods | 3 | Take trade goods or commodities from another player. |
| sustain-deny | 3 | Prevents units from using SUSTAIN DAMAGE or from being repaired. |
| combat-stat-change | 2 | Changes a unit's combat value/hit threshold (e.g. combat value improved or set to a number). |
| commodity-value-bonus | 2 | Changes your commodity value / maximum commodities. magnitude = N. |
| protect-planet | 2 | Prevents planets from being taken, invaded, exhausted or having units removed. |
| requires-more-than-opponent | 2 | Effect conditional on having more/less of something than an opponent. |
| sustain-grant | 2 | Gives other units/ships the ability to sustain damage, or cancel a hit by sustaining. |

## Conventions and ambiguities resolved

- Text labelled = the concatenation of window/trigger and effect segments (joined by " | " in the helpers' view); leaders also include an "UNLOCK: ..." segment. Evidence quotes never cross segments.
- Unit rows: only units with a non-empty `ability` field (97 of 136). Stat-only units are not rows. Stat-line upgrade techs/units get only keyword tags (sustain-damage, bombardment, anti-fighter-barrage, space-cannon, planetary-shield, production-unit); capacity numbers get none.
- Kinds: `public_objective` / `secret_objective` instead of one `objective` kind (the kind carries the public/secret distinction, so no obj-public/obj-secret tags). Added kinds beyond the brief: exploration, galactic_event, legendary_planet, strategy_card, plot (genericcards.json).
- Leader ids are the content ids (e.g. `arborecagent`); commander/hero unlock text is tagged `leader-unlock-condition` (70 leaders); heroes carry `hero-one-shot` + `purge` when the text says so.
- Timing tags are coarse: `timing-reaction` (271 cards) is the default for any "when/after/before X" trigger; `timing-action` is for ACTION: abilities. Expect it to carry little signal on its own.
- `deal-enforcement` is applied to all 40 promissory notes (plus 1 action card) as the "this card is a pledge/commitment" marker, so it is effectively a kind marker; the more specific effect tags carry the content. Operator may prefer to drop it.
- `gain-relic` is applied to 25 exploration cards because they are relic fragments (cultural/hazardous/industrial/unknown); `elect-outcome` to 33 agendas with elect-type outcomes.
- Agendas: For and Against effects are both tagged on the same card (the card does not record which side a tag belongs to; the evidence quote shows it).
- Magnitude is recorded only when the text states a number and the tag definition mentions magnitude; otherwise null. Some helper magnitudes may be missing where text spells numbers in words.
- Singleton pruning: 3 tags used by fewer than 2 cards were removed from the taxonomy and their labels dropped (damage-units, obj-gain-resources, take-promissory), leaving those cards with fewer tags. Taxonomy designed with 136 tags, 133 after pruning (final count in the Counts section).
- 18 tags were proposed by helpers for uncovered effects and NOT adopted (each is a near-singleton): research-restriction, no-influence-cost, extra-hit-production, action-card-restriction, breach-placement, anti-fighter-barrage-immunity, movement-exemption, resource-influence-swap, combat-hit-production, end-turn-skip, restrict-action-cards, draw-public-objective, plot-placement, vote-order, speaker-token-change, obj-elected, obj-neighbors, obj-passing. Several (action-card restriction x2, speaker token x2, end-turn-skip x2) would qualify at 2 cards - candidates for taxonomy v2. Those cards were labelled with the nearest existing tag.
- Skipped files are listed in the JSON `skipped` array (units without ability text, non-legendary planets, attachments, combat_modifiers, factions, structural/metadata files).

## Spot-check table (40 cards)

Selection: per kind, one card containing a rare tag (used by <=6 cards) plus random others (seed 20261009).

| kind | id | faction | card text (shortened) | tags (magnitude) : evidence |
|---|---|---|---|---|
| ability | pillage | mentak | After 1 of your neighbors gains trade goods or resolves a transaction \| If they have 3 or more trade goods, you may take 1 of their trade goods or commodities. | **steal-trade-goods**: "you may take 1 of their trade goods or commodities"<br>**requires-neighbor**: "After 1 of your neighbors gains trade goods"<br>**timing-reaction**: "After 1 of your neighbors gains trade goods or resolves a transaction" |
| ability | phoenixstandard | bastion | At the end of combat \| you may galvanize 1 of your units that participated. | **timing-combat**: "At the end of combat"<br>**gain-attachment**: "you may galvanize 1 of your units" |
| ability | foresight | naalu | After another player moves ships into a system that contains 1 or more of your ships \| You may place 1 token from your strategy pool in an adjacent system that does not contain another player's ships; move your ships from the a... | **timing-reaction**: "After another player moves ships into a system"<br>**strategy-pool-use**: "place 1 token from your strategy pool"<br>**relocate-units**: "move your ships from the active system" |
| ability | devour | cabal | Capture your opponent's non-structure units that are destroyed during combat. | **capture-units**: "Capture your opponent's non-structure units that are destroyed during combat"<br>**timing-combat**: "that are destroyed during combat" |
| ability | assimilate | l1z1x | When you gain control of a planet \| Replace each PDS and space dock that is on that planet with a matching unit from your reinforcements. | **unit-replacement**: "Replace each PDS and space dock that is on that planet"<br>**timing-reaction**: "When you gain control of a planet" |
| technology | pds2 | - | PLANETARY SHIELD, SPACE CANNON 5 You may use this unit's SPACE CANNON against ships that are adjacent to this unit's system. | **planetary-shield**: "PLANETARY SHIELD"<br>**space-cannon**: "SPACE CANNON 5" |
| technology | nekroc4r | nekro | ACTION: Exhaust this card to place 1 PDS on a planet you control. ACTION: Exhaust this card to repair all of your damaged units. ACTION: Exhaust this card and discard 1 action card to draw 1 action card. | **timing-action**: "ACTION: Exhaust this card to place 1 PDS"<br>**place-structure**: "place 1 PDS on a planet you control"<br>**repair-units**: "repair all of your damaged units"<br>**draw-action-card** (1): "draw 1 action card"<br>**exhaust**: "Exhaust this card to repair all of your damaged units" |
| technology | asn | keleres | Once per action, when you resolve a unit's PRODUCTION ability, you may resolve another of your unit's PRODUCTION abilities in any system. | **once-per-action**: "Once per action"<br>**produce-units**: "you may resolve another of your unit's PRODUCTION abilities in any system"<br>**timing-reaction**: "when you resolve a unit's PRODUCTION ability" |
| technology | so | mentak | After you win or lose a space combat, gain 1 trade good; if you won the combat, you may also produce 1 ship in that system of any ship type that was destroyed during the combat. | **gain-trade-goods** (1): "gain 1 trade good"<br>**post-combat-trigger**: "After you win or lose a space combat"<br>**produce-units**: "you may also produce 1 ship in that system" |
| unit | winnu_flagship | winnu | When this unit makes a combat roll, it rolls a number of dice (hit on a 7) equal to the number of your opponent's non-fighter ships in this system. | **extra-dice**: "it rolls a number of dice (hit on a 7) equal to the number of your opponent's non-fighter ships"<br>**combat-stat-change**: "hit on a 7" |
| unit | l1z1x_dreadnought2 | l1z1x | This unit cannot be destroyed by "Direct Hit" action cards. | **cancel-hit**: "This unit cannot be destroyed by" |
| unit | arborec_infantry2 | arborec | After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system. | **unit-revival**: "place each unit that is on this card on a planet you control in your home system"<br>**timing-reaction**: "After this unit is destroyed" |
| unit | sardakk_dreadnought2 | sardakk | This unit cannot be destroyed by "Direct Hit" action cards. After a round of space combat, you may destroy this unit to destroy up to 2 ships in the system. | **cancel-hit**: "This unit cannot be destroyed by"<br>**destroy-units** (2): "destroy up to 2 ships in the system"<br>**timing-combat**: "After a round of space combat" |
| leader | letnevhero | letnev | ACTION: \| Place this card near the game board. the number of non-fighter ships you can have in systems is not limited by laws or by the number of command tokens in your fleet pool during this game round. At the end of that game... | **timing-action**: "ACTION:"<br>**fleet-limit-bonus**: "the number of non-fighter ships you can have in systems is not limited"<br>**purge**: "purge this card"<br>**hero-one-shot**: "purge this card"<br>**leader-unlock-condition**: "Have 3 scored objectives." |
| leader | cabalcommander | cabal | When you produce fighter or infantry units: \| Up to 2 of those units do not count against your PRODUCTION limit. \| UNLOCK: Have units in 3 gravity rifts. | **timing-reaction**: "When you produce fighter or infantry units"<br>**production-bonus** (2): "Up to 2 of those units do not count against your PRODUCTION limit"<br>**leader-unlock-condition**: "Have units in 3 gravity rifts" |
| leader | muaatcommander | muaat | After you spend a token from your strategy pool: \| You may gain 1 trade good. \| UNLOCK: Produce a war sun. | **timing-reaction**: "After you spend a token from your strategy pool"<br>**strategy-pool-use**: "spend a token from your strategy pool"<br>**gain-trade-goods** (1): "You may gain 1 trade good"<br>**leader-unlock-condition**: "Produce a war sun" |
| leader | kelerescommander | keleres | After you perform a component action: \| You may perform an additional action. \| UNLOCK: Spend 1 trade good after you play an action card that has a component action | **timing-reaction**: "After you perform a component action"<br>**extra-activation**: "You may perform an additional action"<br>**leader-unlock-condition**: "Spend 1 trade good after you play an action card that has a component action" |
| leader | mentakhero | mentak | At the start of a space combat that you are participating in: \| You may purge this card. if you do, for each other player's ship that is destroyed during this combat, place 1 ship of that type from your reinforcements in the ac... | **timing-combat**: "At the start of a space combat that you are participating in"<br>**purge**: "You may purge this card"<br>**hero-one-shot**: "You may purge this card"<br>**place-units-free**: "place 1 ship of that type from your reinforcements in the active system"<br>**leader-unlock-condition**: "Have 3 scored objectives." |
| promissory | tekklar | sardakk | At the start of an invasion combat: Apply +1 to the result of each of your unit's combat rolls during this combat. If your opponent is the N'orr player, apply -1 to the result of each of their unit's combat rolls during this co... | **combat-roll-bonus** (1): "Apply +1 to the result of each of your unit's combat rolls"<br>**combat-roll-penalty** (1): "apply -1 to the result of each of their unit's combat rolls"<br>**timing-combat**: "At the start of an invasion combat"<br>**deal-enforcement**: "return this card to the N'orr player" |
| promissory | favor | xxcha | When an agenda is revealed: Remove 1 token from the Xxcha player's strategy pool and return it to their reinforcements. Then, discard the revealed agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda... | **strategy-pool-use**: "Remove 1 token from the Xxcha player's strategy pool"<br>**agenda-cancel**: "discard the revealed agenda"<br>**agenda-manipulation**: "reveal 1 agenda from the top of the deck"<br>**timing-reaction**: "When an agenda is revealed"<br>**deal-enforcement**: "return this card to the Xxcha player" |
| breakthrough | nekrobt | nekro | When you would gain another player's technology using one of your faction abilities, you may instead place one of your "Z" assimilator tokens on that player's faction sheet. Your flagship gains the text abilities of that factio... | **tech-theft**: "gain another player's technology using one of your faction abilities"<br>**timing-reaction**: "When you would gain another player's technology"<br>**share-ability**: "Your flagship gains the text abilities of that faction's flagship" |
| breakthrough | obsidianbt | obsidian | Place 1 trade good from the supply onto this card each time you win a combat against a puppeted player. At the start of the status phase, gain all trade goods on this card, then gain an equal number of trade goods from the supply. | **timing-status-phase**: "At the start of the status phase"<br>**gain-trade-goods**: "gain all trade goods on this card"<br>**post-combat-trigger**: "each time you win a combat against a puppeted player"<br>**puppet-control**: "against a puppeted player" |
| action_card | bunker | - | At the start of an invasion \| During this invasion, apply -4 to the result of each BOMBARDMENT roll against planets you control. | **combat-roll-penalty** (4): "apply -4 to the result of each BOMBARDMENT roll"<br>**timing-combat**: "At the start of an invasion" |
| action_card | piratecontract2 | - | Action \| Place 1 neutral destroyer in a non-home system that contains no non-neutral ships. | **ship-effect**: "Place 1 neutral destroyer"<br>**place-units-free**: "Place 1 neutral destroyer in a non-home system"<br>**timing-action**: "Action" |
| action_card | seize | - | Action \| Choose 1 of your neighbors that has 1 or more relic fragments. That player must give you 1 relic fragment of your choice. | **gain-relic**: "That player must give you 1 relic fragment of your choice"<br>**requires-neighbor**: "Choose 1 of your neighbors"<br>**timing-action**: "Action" |
| agenda | minister_peace | - | The elected player gains this card. After a player activates a system that contains 1 or more of a different player's units, the owner of this card may discard this card - immediately end the active player's turn. | **elect-outcome**: "The elected player gains this card"<br>**timing-reaction**: "After a player activates a system that contains 1 or more of a different player's units"<br>**restrict-activation**: "immediately end the active player's turn" |
| agenda | defense_act | - | For: Each player can have any number of PDS units on planets they control. \| Against: Each player destroys 1 of their PDS units. | **law-permanent**: "Each player can have any number of PDS units on planets they control"<br>**structure-effect**: "Each player destroys 1 of their PDS units"<br>**destroy-units** (1): "Each player destroys 1 of their PDS units" |
| agenda | schematics | - | For: If any player owns a war sun technology, all players may ignore all prerequisites on war sun technologies. All war suns lose SUSTAIN DAMAGE. \| Against: Each player that owns a war sun technology discards all of their actio... | **law-permanent**: "all players may ignore all prerequisites on war sun technologies"<br>**prerequisite-skip**: "all players may ignore all prerequisites on war sun technologies"<br>**war-sun-effect**: "All war suns lose SUSTAIN DAMAGE"<br>**sustain-deny**: "All war suns lose SUSTAIN DAMAGE"<br>**discard-opponent-card**: "discards all of their action cards" |
| public_objective | push_boundaries | - | Control more planets than each of 2 of your neighbors. | **obj-control-planets**: "Control more planets than each of 2 of your neighbors"<br>**requires-more-than-opponent**: "Control more planets than each of 2 of your neighbors"<br>**requires-neighbor**: "each of 2 of your neighbors" |
| public_objective | lost_outposts | - | Control 2 planets that have attachments. | **obj-control-planets**: "Control 2 planets that have attachments" |
| secret_objective | otf | - | Have 9 or more ground forces on a planet that does not contain 1 of your space docks. | **obj-ownership-count**: "Have 9 or more ground forces on a planet"<br>**ground-force-effect**: "Have 9 or more ground forces on a planet" |
| secret_objective | mtm | - | Have 1 mech on each of 4 planets. | **obj-units-in-systems**: "Have 1 mech on each of 4 planets"<br>**mech-effect**: "Have 1 mech on each of 4 planets" |
| relic | dominusorb | - | Before you move units during a tactical action, you may purge this card to move and transport units that are in systems that contain 1 of your command tokens. | **move-without-transport**: "move and transport units"<br>**timing-action**: "Before you move units during a tactical action"<br>**purge**: "you may purge this card" |
| relic | quantumcore | - | When you gain this card, gain your breakthrough. You have SYNERGY for all technology types. | **timing-reaction**: "When you gain this card"<br>**tech-specialty-use**: "You have SYNERGY for all technology types" |
| exploration | frln3 | - | You may produce 1 unit in this system. You may spend influence as if it were resources to produce this unit. | **produce-units**: "You may produce 1 unit in this system"<br>**trade-goods-as-resources**: "You may spend influence as if it were resources" |
| exploration | lc1 | - | Draw 2 action cards. | **draw-action-card** (2): "Draw 2 action cards" |
| galactic_event | advent_war_sun | - | - At the end of setup, all players other than the Embers of Muaat player gain the War Sun unit upgrade technology. - The Embers of Muaat player purges their faction specific promissory note, unlocks their commander, and places ... | **research-free**: "gain the War Sun unit upgrade technology"<br>**war-sun-effect**: "gain the War Sun unit upgrade technology"<br>**faction-start-setup**: "At the end of setup"<br>**leader-unlock-condition**: "unlocks their commander"<br>**purge**: "purges their faction specific promissory note" |
| legendary_planet | mrte | - | The Galactic Council \| You may exhaust this card at the end of your turn and discard 1 secret objective to draw 1 secret objective. | **exhaust**: "You may exhaust this card at the end of your turn"<br>**gain-secret-objective**: "discard 1 secret objective to draw 1 secret objective"<br>**timing-end-turn**: "at the end of your turn" |
| strategy_card | te6warfare | - | PRIMARY: Perform a tactical action in any system without placing a command token, even if the system already has your command token in it: that system still counts as being activated. You may redistribute your command tokens be... | **timing-action**: "PRIMARY: Perform a tactical action in any system"<br>**extra-activation**: "Perform a tactical action in any system without placing a command token"<br>**no-token-cost**: "without placing a command token"<br>**redistribute-tokens**: "You may redistribute your command tokens before and after this action"<br>**strategy-pool-use**: "Spend 1 token from your strategy pool"<br>**produce-units**: "use the Production abilities of the units in your home system" |
| plot | siphon | - | When the puppeted player gains commodities, you gain an equal number of trade goods. | **puppet-control**: "When the puppeted player gains commodities"<br>**gain-trade-goods**: "you gain an equal number of trade goods"<br>**timing-reaction**: "When the puppeted player gains commodities" |


## Coordinator review (2026-10-09)

Independent validation (a separate script from the labeller's): 896 cards, 133 tags, 2,532 labels;
every tag in the taxonomy and used by at least 2 cards; no duplicate (kind, id). Findings:
- 7 ids are shared by two kinds (galvanize: ability/public_objective, sar: technology/secret_objective,
  pds2: technology/unit, ds: technology/exploration, crisis: action_card/agenda, emelpar:
  relic/legendary_planet, mirage: exploration/legendary_planet). **Integration must key cards by
  (kind, id).**
- 12 evidence quotes, all on strategy cards ("PRIMARY: ..."), are not plain substrings of the stored
  text (formatting in the content record); the effects themselves are right.
- At least one inverted label: ability `hubris` (Mahact) is tagged gain-promissory, but the text
  restricts receiving the Alliance note. Expect others of this kind; that is what the spot-check is for.
- Low-signal tags to consider dropping or splitting before integration: timing-reaction (271 cards),
  deal-enforcement (on every promissory note).

## Second review (Sonnet, 2026-10-09)

Reviewer: Sonnet 5.5 (a different agent from the labellers; no Haiku helpers used). Every one of the 896 cards was read against its source text in `crates/ti4-content/content/*.json` (text rebuilt per card as window | effect | UNLOCK; strategy cards as PRIMARY/SECONDARY segments), followed by keyword cross-checks for missed `purge`/`relic`/timing tags. Ambiguous labels were left alone; only clear mismatches with the tag definition were changed. Labels were NOT re-derived from scratch.

### Counts

| item | count |
|---|---|
| labels changed (tag swapped) | 40 |
| labels removed | 11 |
| labels added | 15 |
| evidence-only fixes (tag kept) | 13 (the 12 strategy-card "PRIMARY:" quotes + technology `lgf`) |
| magnitude-only fixes | 1 (`cripple` destroy-units 1 -> null, it destroys EACH PDS) |
| tags added to taxonomy | 6 (direct-hit-immunity, action-card-restriction, movement-exemption, influence-as-resources, end-turn-skip, reveal-public-objective) |
| tag definitions revised | 3 (cancel-hit, space-cannon-immunity, trade-goods-as-resources) |
| result | 896 cards, 2,536 labels (2532 - 11 + 15), 139 tags |

Cards with no tags: still 8 (covert agenda, 5 "mutated" plots, `cr2`, `cv2`).

### Taxonomy changes

| tag | cards | definition |
|---|---|---|
| direct-hit-immunity (new) | 6 | Unit cannot be destroyed by "Direct Hit" action cards (immunity to one destruction effect; not a hit cancellation). |
| action-card-restriction (new) | 4 | Prevents players from playing action cards, or restricts when/which action cards may be played. |
| movement-exemption (new) | 3 | Lifts a normal movement/activation restriction (e.g. move out of or activate systems that contain your command tokens). |
| influence-as-resources (new) | 5 | Influence may be spent as if it were resources (or resources as influence, or both values combined). |
| end-turn-skip (new) | 6 | Ends or skips a player's turn. |
| reveal-public-objective (new) | 3 | Draws, reveals, replaces or creates a public objective. |
| cancel-hit (revised) | 12 | Now "Cancels or ignores hits produced against your units." (dropped "or prevents a unit from being destroyed"; Direct Hit immunity has its own tag). |
| space-cannon-immunity (revised) | 12 | Now also covers immunity to ANTI-FIGHTER BARRAGE (`naalu_mech_omega`; a separate tag would have had 1 card). |
| trade-goods-as-resources (revised) | 2 | Now also covers commodities spent as trade goods (`keleresagent`); with the exploration cards moved out only `mc` and `keleresagent` use it. |

Evidence-quote rule used for validation: every quote must be a substring of ONE segment of the card text, with no "PRIMARY:"/"SECONDARY:" prefix.

### Every change (kind, id, before -> after, reason)

| kind | id | change | reason |
|---|---|---|---|
| ability | slipstream | timing-action → timing-passive | "During your tactical actions" is a standing modifier, not an ACTION |
| ability | ambush | extra-dice → (removed) | rolls separate pre-combat dice for cruisers/destroyers, not additional dice for units |
| ability | telepathic | place-command-token → (removed) | places the Naalu "0" initiative token, not a command token |
| ability | orbital_drop | (none) → ground-force-effect | places infantry (consistent with mitosis) |
| ability | blood_ties | no-token-cost → (removed) | saves influence cost of the custodians token, not a command token |
| ability | hubris | gain-promissory → (removed) | text purges Alliance and forbids receiving it; nothing is gained |
| technology | lgf | evidence fixed | evidence not a substring (case) |
| technology | pa | timing-action → timing-passive | "During the action phase" is a standing permission, not an ACTION |
| technology | tp | timing-action → timing-passive | standing restriction, not an ACTION |
| technology | tp | restrict-activation → action-card-restriction | restricts playing action cards (not activation) |
| technology | sar | (none) → mech-effect | both effects concern mechs |
| technology | dn2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| technology | sdn2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| technology | exo2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| unit | dreadnought2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| unit | l1z1x_dreadnought2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| unit | sardakk_dreadnought2 | cancel-hit → direct-hit-immunity | immunity to Direct Hit action cards, not cancelling a hit |
| unit | crimson_mech | timing-action → timing-passive | DEPLOY during commit step is not an ACTION |
| leader | nomadagentthundarian | assign-hits-control → reroll | cancels hit assignment and returns to Roll Dice step = reroll |
| leader | mahactcommander | timing-action → timing-passive | "During your tactical actions" is a standing ability |
| leader | mahactcommander | (none) → movement-exemption | lifts the normal restriction on activating systems that hold your tokens |
| leader | nomadhero | restrict-movement → movement-exemption | grants movement out of command-token systems (inverted: was a restriction tag) |
| leader | jolnarhero | (none) → research-free | gains replacement technologies outside research |
| leader | xxchahero | resources-bonus → influence-as-resources | combines resource and influence so each counts as both |
| breakthrough | xxchabt | resources-bonus → influence-as-resources | spend influence as resources and vice versa |
| breakthrough | hacanbt | place-command-token → gain-command-token | moves reinforcements token into fleet pool = gain a token |
| breakthrough | saarbt | extra-dice → destroy-units | rolls dice equal to resources spent to hit ground forces; no additional dice |
| promissory | <color>_ta | give-trade-goods → steal-trade-goods | you receive the other player's commodities |
| promissory | gift | place-command-token → (removed) | places the Naalu "0" initiative token, not a command token |
| promissory | terraform | (none) → gain-attachment | card attaches to a planet |
| promissory | blackops | (none) → puppet-control | puts a control token on a plot (puppeting the receiver) |
| promissory | raisethestandard | repair-units → gain-attachment | galvanizes a unit; no repair |
| action_card | abs | (none) → planetary-trait-bonus | effect keyed to cultural planets (as industrial_initiative/unstable) |
| action_card | economic_initiative | (none) → planetary-trait-bonus | effect keyed to cultural planets |
| action_card | courageous | extra-dice → (removed) | rolls 2 separate dice once, not additional dice for units |
| action_card | plague | extra-dice → (removed) | rolls one die per infantry as the whole effect, not additional dice |
| action_card | cripple | destroy-units magnitude 1 → None | destroys EACH PDS; magnitude 1 is wrong |
| action_card | repeal | law-permanent → agenda-cancel | removes a law from play; law-permanent means a law that stays in play |
| action_card | crisis | restrict-activation → end-turn-skip | skips the next player's turn; no activation restriction |
| agenda | incentive | gain-secret-objective → reveal-public-objective | reveals public objectives (stage I/II), draws no secret objective |
| agenda | classified | (none) → reveal-public-objective | turns a secret objective into a public one |
| agenda | abolishment | law-permanent → agenda-cancel | discards a law from play; does not itself stay in play |
| agenda | constitution | law-permanent → agenda-cancel | For discards all laws from play |
| agenda | minister_peace | restrict-activation → end-turn-skip | discarding it ends the active player's turn; not an activation restriction |
| agenda | minister_sciences | timing-action → timing-reaction | triggers when resolving Technology; not an ACTION |
| agenda | minister_war | timing-action → timing-reaction | used after performing an action; not itself an ACTION |
| agenda | censure | restrict-activation → action-card-restriction | "cannot play action cards", not an activation restriction |
| secret_objective | otf | obj-ownership-count → obj-units-in-systems | ground forces on one planet = units in location (cf. mtm) |
| secret_objective | pe | timing-end-turn → (removed) | passing order, not an end-of-turn trigger |
| relic | neuraloop | gain-secret-objective → reveal-public-objective | replaces a revealed public objective with a random one; draws no secret objective |
| relic | dominusorb | timing-action → timing-reaction | "Before you move units during a tactical action" is a trigger inside a tactical action, not an ACTION |
| relic | dominusorb | move-without-transport → movement-exemption | lets units leave command-token systems; units still need normal transport |
| exploration | frln1 | trade-goods-as-resources → influence-as-resources | spends INFLUENCE as resources; trade goods are not involved |
| exploration | frln2 | trade-goods-as-resources → influence-as-resources | spends INFLUENCE as resources; trade goods are not involved |
| exploration | frln3 | trade-goods-as-resources → influence-as-resources | spends INFLUENCE as resources; trade goods are not involved |
| promissory | <color>_ps | (none) → action-card-restriction | target cannot play action cards |
| galactic_event | stellar_atomics | (none) → action-card-restriction | players without a marker cannot play action cards |
| technology | nf | timing-end-turn → end-turn-skip | effect is to end the player's turn immediately; not an end-of-turn trigger |
| unit | mahact_mech | timing-end-turn → end-turn-skip | spends the token to end the player's turn |
| leader | mahactcommander | timing-end-turn → end-turn-skip | activating a token system ends your turn |
| action_card | coup | (none) → end-turn-skip | ends the player's turn and cancels the strategic action |
| agenda | regulations | fleet-limit-bonus → (removed) | For caps the fleet pool at 4 (a restriction, not a bonus) |
| galactic_event | dangerous_wilds | produce-units → (removed) | replenishing neutral units is not player production |
| galactic_event | age_exploration | (none) → gain-relic | changes relic-fragment requirements |
| legendary_planet | garbozia | discard-opponent-card → draw-action-card | takes action cards from the discard pile to play; no opponent hand is affected |
| relic | thesilverflame | (none) → purge | card is purged on use |
| relic | bookoflatvinia | (none) → purge | card is purged on use |
| agenda | incentive | timing-agenda-phase → (removed) | text has no agenda-phase trigger; it only reveals an objective |
| strategy_card | base2 | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | base4 | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok1leadership | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok2diplomacy | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok3politics | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok4construction | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok5trade | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok6warfare | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok7technology | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | pok8imperial | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | te4construction | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |
| strategy_card | te6warfare | timing-action: quote prefix "PRIMARY:" removed | quote was not a substring of the stored text |

### Recommendation on low-signal tags (NOT applied)

- **deal-enforcement** (41 cards: all 40 promissory notes + `exchangeprogram`): drop. It equals `kind == promissory` (plus one card), so it adds no information, and it is wrong for `terraform`, which has no return clause. If the "goes back to its owner when X" mechanic matters, replace it with a precise tag such as `returns-to-owner`, set only where the text says "return this card to the ... player" (36 of the 40 notes); that also separates standing notes (`convoys`, `pop`, `an`) from one-shot notes.
- **timing-reaction** (274 cards, 31% of all cards; never alone, it always sits next to an effect tag): split rather than drop. It lumps "When/After/Before <trigger>" of very different kinds. Suggested split by trigger: `trigger-own-event` (you gain/produce/research/activate), `trigger-other-player-action` (another player moves/activates/plays a card), `trigger-agenda-reveal`, `trigger-unit-destroyed`. The phase/turn `timing-*` tags already cover phase triggers. If a split is too costly, keep the tag but treat it as a weak feature (it is a near-constant prior on action_card/leader/ability kinds: 69 of 142 action cards, 48 of 103 leaders). Do not drop it without the split, because "reacts to another player" is real information for the policy.
- Also weak (for information): `timing-passive` is applied inconsistently (26 cards; many standing abilities have no timing tag). `timing-action` is now reserved for true ACTION: abilities (it had been used for "During your tactical actions" modifiers).

### Cards I am unsure about (left as is)

- agenda `covert`: the content record has empty `text1`/`text2` (the rule text sits in the `target` field), so it stays untagged because no quote can be taken from the stored text. Intended tags: agenda-manipulation + look-at-hidden-info. Fix the content record or the text builder first.
- `jolnar_flagship` (combat-stat-change): "each 9 or 10 produces 2 additional hits" is an extra-hit rule, not a combat-value change; no tag fits (earlier proposed: combat-hit-production).
- `htp` (resources-bonus): swaps resource and influence values; no exact tag.
- `regulations`: the For clause is a fleet-pool cap, the Against clause adds fleet tokens; fleet-limit-bonus was removed, place-command-token kept.
- `edict`: redistribute-tokens is applied to a restriction ("cannot be redistributed").
- `cr2`, `cv2` (stat-only upgrades) stay empty by convention.
- promissory `terraform` keeps deal-enforcement although it has no return clause (covered by the recommendation above).

### Validation results (script run after the final write)

- JSON parses; file rewritten with the same layout (indent 1, CRLF line endings, no trailing newline); `taxonomy_version`, `status` and key order unchanged (new tags appended at the end of `tags`).
- Cards: 896; duplicate (kind, id) pairs: 0.
- Every label tag is in the taxonomy: yes (0 unknown). No card carries the same tag twice.
- Every taxonomy tag is used by at least 2 cards: yes (139 tags).
- Every evidence quote is a substring of one segment of its card text rebuilt from the content files: yes (0 failures; was 13).
- Labels: 2,536; tags: 139.
