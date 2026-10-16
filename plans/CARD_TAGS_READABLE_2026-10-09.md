# Card effect tags — readable dump (draft, 2026-10-09)

Generated from plans/CARD_TAGS_DRAFT_2026-10-09.json (896 cards, 139 tags). Format per card: **name** (`id`, faction) — card text — tags with (magnitude); quoted reasons are in the JSON.

## Contents

- [ability](#ability) (73)
- [technology](#technology) (102)
- [unit](#unit) (97)
- [leader](#leader) (103)
- [breakthrough](#breakthrough) (31)
- [promissory](#promissory) (40)
- [action_card](#action-card) (142)
- [agenda](#agenda) (63)
- [public_objective](#public-objective) (40)
- [secret_objective](#secret-objective) (40)
- [relic](#relic) (24)
- [exploration](#exploration) (80)
- [galactic_event](#galactic-event) (20)
- [legendary_planet](#legendary-planet) (19)
- [strategy_card](#strategy-card) (12)
- [plot](#plot) (10)
- [Tag glossary](#tag-glossary)

## ability

**Mitosis** (`mitosis`, arborec)  
> At the start of the status phase | Place 1 infantry from your reinforcements on any planet you control. | Your space docks cannot produce infantry  
Tags: `place-units-free`, `timing-status-phase`, `ground-force-effect`

**Raid Formation** (`raid_formation`, argent)  
> When 1 or more of your units uses ANTI-FIGHTER BARRAGE | for each hit produced in excess of your opponent's fighters, choose 1 of your opponent's ships that has SUSTAIN DAMAGE to become damaged.  
Tags: `anti-fighter-barrage`, `timing-reaction`

**Zeal** (`zeal`, argent)  
> When you cast at least 1 vote | cast 1 additional vote for each player in the game including you. | You always vote first during the agenda phase  
Tags: `vote-bonus`, `timing-passive`

**Galvanize** (`galvanize`, bastion)  
> When a game effect instructs a player to galvanize a unit | they place a galvanize token beneath it if it does not have one.  | Galvanized units roll 1 additional die for combat rolls and unit abilities.  
Tags: `extra-dice` (1), `gain-attachment`, `timing-reaction`

**Liberate** (`liberate`, bastion)  
> When you gain control of a planet | ready that planet if it contains a number of your infantry equal to or greater than that planet's resource value; otherwise, place 1 infantry on that planet.  
Tags: `ready-planets`, `place-units-free`, `timing-reaction`, `requires-units-present`

**Phoenix Standard** (`phoenixstandard`, bastion)  
> At the end of combat | you may galvanize 1 of your units that participated.  
Tags: `timing-combat`, `gain-attachment`

**Amalgamation** (`amalgamation`, cabal)  
> When you produce a unit | You may return 1 captured unit of that type to produce that unit without spending resources.  
Tags: `place-units-free`, `capture-units`, `timing-reaction`

**Devour** (`devour`, cabal)  
> Capture your opponent's non-structure units that are destroyed during combat.  
Tags: `capture-units`, `timing-combat`

**Riftmeld** (`riftmeld`, cabal)  
> When you research a unit upgrade technology | You may return 1 captured unit of that type to ignore all of the technology's prerequisites.  
Tags: `prerequisite-skip`, `capture-units`, `timing-reaction`

**Incursion** (`incursion`, crimson)  
> When you activate a system that contains a breach, you may flip that breach; systems that contain active breaches are adjacent. at the end of the status phase, any player with ships in a system that contain an active breach may remove that breach.  
Tags: `adjacency-grant`, `timing-reaction`, `timing-end-turn`

**Sorrow** (`sorrow`, crimson)  
> When you create the game board, place the Sorrow (tile 94) where your home system would normally be placed, then place a inactive breach there. The Sorrow is not a home system. Then, place your home system (tile 118) in your play area.  
Tags: `faction-start-setup`

**Sundered** (`sundered`, crimson)  
> You cannot use wormholes other than epsilon wormholes. Other players' units that move or are placed into your home system are destroyed.  
Tags: `wormhole`, `destroy-units`, `requires-home-system`

**Oceanbound** (`oceanbound`, deepwrought)  
> When your units begin coexisting on a planet | gain an ocean card and ready it. | Any time you have more ocean cards than there are planets that contain your coexisting units, discard ocean cards until you do not.  
Tags: `explore`, `ready-card`, `timing-reaction`

**Research Team** (`researchteam`, deepwrought)  
> When ground forces are committed | if your units on that planet are not already coexisting, you may choose for your units to coexist.  
Tags: `combat-avoidance`, `timing-reaction`

**Aetherpassage** (`aetherpassage`, empyrean)  
> After a player activates a system | You may allow that player to move their ships through systems that contain your ships.  
Tags: `move-through-ships`, `timing-reaction`

**Dark Whispers** (`dark_whispers`, empyrean)  
> During setup, take the additional Empyrean faction promissory note; you have 2 faction promissory notes  
Tags: `faction-start-setup`, `gain-promissory`

**Voidborn** (`voidborn`, empyrean)  
> Nebulae do not affect your ships' movement  
Tags: `anomaly-movement`

**Plots Within Plots** (`plotsplots`, firmament)  
> When you score another player's secret objective | do not gain a victory point; instead, place a facedown plot card into your play area with that player's control token on it. | You can score secret objectives already scored by other players if you fulfill their requirements; this does not count against your secret objective limit or the number you can score in a round.  
Tags: `score-objective-help`, `timing-reaction`

**Puppets of the Blade** (`puppetsoftheblade`, firmament)  
> If you have at least 1 plot card in your play area, gain the following ability: | ACTION: Purge The Firmament's faction sheet, leaders, planet cards, and promissory note. Then, gain all of the faction components for The Obsidian.  
Tags: `timing-action`, `purge`, `game-structure-change`

**Creuss Gate** (`creuss_gate`, ghost)  
> When you create the game board, place the Creuss Gate (tile 17) where your home system would normally be placed. The Creuss Gate system is not a home system. Then, place your home system (tile 51) in your play area.  
Tags: `faction-start-setup`

**Quantum Entanglement** (`quantum_entanglement`, ghost)  
> You treat all systems that contain either an alpha or beta wormhole as adjacent to each other. Game effects cannot prevent you from using this ability.  
Tags: `adjacency-grant`, `wormhole`

**Slipstream** (`slipstream`, ghost)  
> During your tactical actions | Apply +1 to the move value of each of your ships that starts its movement in your home system or in a system that contains either an alpha or beta wormhole.  
Tags: `move-bonus` (1), `timing-passive`, `wormhole`

**Arbiters** (`arbiters`, hacan)  
> When you are negotiating a transaction, action cards can be exchanged as part of that transaction.  
Tags: `transaction`

**Guild Ships** (`guild_ships`, hacan)  
> You can negotiate transactions with players who are not your neighbor.  
Tags: `transaction`, `requires-neighbor`

**Masters of Trade** (`master_of_trade`, hacan)  
> You do not have to spend a command token to resolve the secondary ability of the "Trade" strategy card.  
Tags: `no-token-cost`

**Analytical** (`analytical`, jolnar)  
> When you research a technology that is not a unit upgrade technology | You may ignore 1 prerequisite.  
Tags: `prerequisite-skip` (1)

**Brilliant** (`brilliant`, jolnar)  
> When you spend a command token to resolve the secondary ability of the "Technology" strategy card | You may resolve the primary ability instead.  
Tags: `strategy-card-manipulation`

**Fragile** (`fragile`, jolnar)  
> Apply -1 to the result of each of your unit's combat rolls.  
Tags: `combat-roll-penalty` (1)

**Council Patronage** (`council_patronage`, keleres)  
> At the start of the strategy phase | Replenish your commodities, then gain 1 trade good.  
Tags: `gain-commodities`, `gain-trade-goods` (1), `timing-strategy-phase`

**Law's Order** (`laws_order`, keleres)  
> At the start of any player's turn | You may spend 1 trade good or 1 commodity to treat all laws as blank until the end of that turn.  
Tags: `spend-trade-goods` (1), `timing-start-turn`, `negate-ability`

**The Tribuni** (`the_tribuni`, keleres)  
> During setup, choose an unplayed faction from among the Mentak, the Xxcha and The Argent Flight; take that faction's home system, command tokens and control markers. Additionally, take the Keleres Hero that corresponds to that faction  
Tags: `faction-start-setup`

**Assimilate** (`assimilate`, l1z1x)  
> When you gain control of a planet | Replace each PDS and space dock that is on that planet with a matching unit from your reinforcements.  
Tags: `unit-replacement`, `timing-reaction`

**Harrow** (`harrow`, l1z1x)  
> At the end of each round of ground combat | Your ships in the active system may use their BOMBARDMENT abilities against your opponent's ground forces on the planet.  
Tags: `bombardment`, `timing-combat`

**Armada** (`armada`, letnev)  
> The maximum number of non-fighter ships you can have in each system is equal to 2 more than the number of tokens in your fleet pool  
Tags: `fleet-limit-bonus`, `timing-passive`

**Munitions Reserves** (`munitions`, letnev)  
> At the start of each round of space combat | You may spend 2 trade goods to re-roll any number of your dice during that combat round.  
Tags: `reroll`, `spend-trade-goods` (2), `timing-combat`

**Edict** (`edict`, mahact)  
> When you win a combat | Place 1 command token from your opponent's reinforcements in your fleet pool if it does not already contain 1 of that player's tokens. | Other player's tokens in your fleet pool increase your fleet limit but cannot be redistributed.  
Tags: `fleet-limit-bonus`, `redistribute-tokens`, `place-command-token`, `timing-reaction`

**Hubris** (`hubris`, mahact)  
> During setup, purge your "Alliance" promissory note. Other players cannot give you their 'Alliance" promissory note.  
Tags: `purge`

**Imperia** (`imperia`, mahact)  
> While another player's command token is in your fleet pool, you can use the ability of that player's commander, if it is unlocked.  
Tags: `share-ability`, `timing-passive`

**Ambush** (`ambush`, mentak)  
> At the start of a space combat | You may roll 1 die for each of up to 2 of your cruisers or destroyers in the system. For each result equal to or greater than that ship's combat value, produce 1 hit; your opponent must assign it to 1 of their ships.  
Tags: `pre-combat-hits`, `timing-combat`

**Pillage** (`pillage`, mentak)  
> After 1 of your neighbors gains trade goods or resolves a transaction | If they have 3 or more trade goods, you may take 1 of their trade goods or commodities.  
Tags: `steal-trade-goods`, `requires-neighbor`, `timing-reaction`

**Gashlai Physiology** (`gashlai_physiology`, muaat)  
> Your ships can move through supernovas  
Tags: `anomaly-movement`

**Star Forge** (`star_forge`, muaat)  
> ACTION | Spend 1 token from your strategy pool to place either 2 fighters or 1 destroyer from your reinforcements in a system that contains 1 or more of your war suns.  
Tags: `timing-action`, `strategy-pool-use`, `place-units-free`, `requires-units-present`

**Foresight** (`foresight`, naalu)  
> After another player moves ships into a system that contains 1 or more of your ships | You may place 1 token from your strategy pool in an adjacent system that does not contain another player's ships; move your ships from the active system into that system.  
Tags: `timing-reaction`, `strategy-pool-use`, `relocate-units`

**Telepathic** (`telepathic`, naalu)  
> At the end of the strategy phase | Place the Naalu "0" token on your strategy card; you are first in initiative order.  
Tags: `timing-end-turn`, `initiative-change`

**Distant Suns** (`distant_suns`, naaz)  
> When you explore a planet that contains 1 of your mechs | You may draw 1 additional card; choose 1 to resolve and discard the rest.  
Tags: `explore`, `requires-units-present`

**Fabrication** (`fabrication`, naaz)  
> ACTION | Either purge 2 of your relic fragments of the same type to gain 1 relic; or purge 1 of your relic fragments to gain 1 command token.  
Tags: `timing-action`, `purge`, `gain-relic`, `gain-command-token` (1)

**Galactic Threat** (`galactic_threat`, nekro)  
> Once per agenda phase, after an agenda is revealed | You may predict aloud the outcome of that agenda. If your prediction is correct, gain 1 technology that is owned by a player who voted how you predicted. | You cannot vote on agendas  
Tags: `vote-restrict`, `once-per-action`, `agenda-predict`, `tech-theft`

**Propagation** (`propagation`, nekro)  
> When you would research a technology | Gain 3 command tokens instead. | You cannot research technology  
Tags: `gain-command-token` (3), `timing-reaction`

**Technological Singularity** (`technological_singularity`, nekro)  
> Once per combat, after 1 of your opponent's units is destroyed | You may gain 1 technology that is owned by that player.  
Tags: `tech-theft`, `once-per-action`, `timing-reaction`

**Future Sight** (`future_sight`, nomad)  
> During the Agenda phase, after an outcome that you voted for or predicted is resolved | Gain 1 trade good  
Tags: `gain-trade-goods` (1), `timing-agenda-phase`

**The Company** (`the_company`, nomad)  
> During setup, take the 2 additional Nomad faction agents and place them next to your faction sheet; you have 3 agents  
Tags: `faction-start-setup`

**The Blade's Orchestra** (`bladesorchestra`, obsidian)  
> When this faction comes into play | flip your home system, double-sided faction components, and all of your in-play plot cards. Then, ready Cronos Hollow and Tallin Hollow if you control them.  
Tags: `timing-reaction`, `game-structure-change`, `ready-planets`, `requires-planets-controlled`

**Marionettes** (`marionettes`, obsidian)  
> The player or players whose control tokens are on each plot card are the puppeted players for that plot.  
Tags: `puppet-control`, `timing-passive`

**Nocturne** (`nocturne`, obsidian)  
> This faction cannot be chosen during setup.  
Tags: `faction-start-setup`

**Miniaturization** (`miniaturization`, ralnel)  
> At the end of your tactical actions | you may place your structures that are in space areas onto planets you control in their respective systems. | Your structures can be transported by any ship; this does not require or count against capacity. While your structures are in the space area, they cannot use their unit abilities.  
Tags: `move-without-transport`, `place-structure`, `timing-end-turn`

**Survival Instinct** (`survivalinstinct`, ralnel)  
> After a player activates a system that contains your ships | you may move up to 2 of your ships into the active system from adjacent systems that do not contain your command tokens.  
Tags: `relocate-units`, `timing-reaction`

**Nomadic** (`nomadic`, saar)  
> You can score objectives even if you do not control the planets in your home system.  
Tags: `score-objective-help`, `requires-home-system`

**Scavenge** (`scavenge`, saar)  
> After you gain control of a planet | Gain 1 trade good.  
Tags: `gain-trade-goods` (1), `timing-reaction`

**Unrelenting** (`unrelenting`, sardakk)  
> Apply +1 to the result of each of your unit's combat rolls.  
Tags: `combat-roll-bonus` (1)

**Orbital Drop** (`orbital_drop`, sol)  
> ACTION | Spend 1 token from your strategy pool to place 2 infantry from your reinforcements on 1 planet you control.  
Tags: `timing-action`, `strategy-pool-use`, `place-units-free`, `ground-force-effect`

**Versatile** (`versatile`, sol)  
> When you gain command tokens during the status phase | Gain 1 additional command token.  
Tags: `gain-command-token` (1), `timing-status-phase`

**Awaken** (`awaken`, titans)  
> After you activate a system that contains 1 or more of your sleeper tokens | You may replace each of those tokens with 1 PDS from your reinforcements.  
Tags: `place-structure`, `timing-reaction`

**Coalescence** (`coalescence`, titans)  
> If your flagship or your AWAKEN faction ability places your units into the same space area or onto the same planet as another player's units, your units must participate in combat during "Space Combat" or "Ground Combat" steps.  
Tags: `timing-combat`, `requires-units-present`

**Terragenesis** (`terragenesis`, titans)  
> After you explore a planet that does not have a sleeper token | You may place or move 1 sleeper token onto that planet.  
Tags: `gain-attachment`, `timing-reaction`

**Blood Ties** (`blood_ties`, winnu)  
> You do not have to spend influence to remove the custodians token from Mecatol Rex.  
Tags: *(none)*

**Reclamation** (`reclamation`, winnu)  
> After you resolve a tactical action during which you gained control of Mecatol Rex | You may place 1 PDS and 1 space dock from your reinforcements on Mecatol Rex.  
Tags: `place-structure`, `timing-reaction`

**Peace Accords** (`peace_accords`, xxcha)  
> After you resolve the primary or secondary ability of the "Diplomacy" strategy card | You may gain control of 1 planet other than Mecatol Rex that does not contain any units and is in a system that is adjacent to a planet you control.  
Tags: `gain-planet`, `timing-reaction`, `requires-planets-controlled`

**Quash** (`quash`, xxcha)  
> When an agenda is revealed | You may spend 1 token from your strategy pool to discard that agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda instead.  
Tags: `agenda-cancel`, `agenda-manipulation`, `strategy-pool-use`, `timing-reaction`

**Devotion** (`devotion`, yin)  
> After each space combat round | You may destroy 1 of your cruisers or destroyers in the active system to produce 1 hit and assign it to 1 of your opponent's ships in that system.  
Tags: `destroy-units` (1), `assign-hits-control`, `timing-combat`

**Indoctrination** (`indoctrination`, yin)  
> At the start of a ground combat | You may spend 2 influence to replace 1 of your opponent's participating infantry with 1 infantry from your reinforcements.  
Tags: `unit-replacement`, `ground-force-effect`, `timing-combat`

**Crafty** (`crafty`, yssaril)  
> You can have any number of action cards in your hand. Game effects cannot prevent you from using this ability.  
Tags: `timing-passive`

**Scheming** (`scheming`, yssaril)  
> When you draw 1 or more action cards | Draw 1 additional action card. Then, choose and discard 1 action card from your hand.  
Tags: `draw-action-card` (1), `timing-reaction`

**Stall Tactics** (`stall_tactics`, yssaril)  
> ACTION | Discard 1 action card from your hand  
Tags: `timing-action`

## technology

**AI Development Algorithm** (`aida`)  
> When you research a unit upgrade technology, you may exhaust this card to ignore any 1 prerequisite. When 1 or more of your units use PRODUCTION, you may exhaust this card to reduce the combined cost of the produced units by the number of unit upgrade technologies that you own.  
Tags: `prerequisite-skip` (1), `cost-reduction`, `exhaust`, `timing-reaction`

**Antimass Deflectors** (`amd`)  
> Your ships can move into and through asteroid fields. When other players' units use SPACE CANNON against your units, apply -1 to the result of each die roll.  
Tags: `anomaly-movement`, `combat-roll-penalty` (1), `space-cannon-immunity`

**Assault Cannon** (`asc`)  
> At the start of a space combat in a system that contains 3 or more of your non-fighter ships, your opponent must destroy 1 of their non-fighter ships.  
Tags: `destroy-units` (1), `timing-combat`, `requires-units-present`

**Bio-Stims** (`bs`)  
> You may exhaust this card at the end of your turn to ready 1 of your planets that has a technology specialty or 1 of your other technologies.  
Tags: `exhaust`, `ready-planets`, `ready-card`, `timing-end-turn`

**Cruiser II** (`cr2`)  
> Cost 2, Combat 6, Move 3, Capacity 1  
Tags: *(none)*

**Carrier II** (`cv2`)  
> Cost 3, Combat 9, Move 2, Capacity 6  
Tags: *(none)*

**Duranium Armor** (`da`)  
> During each combat round, after you assign hits to your units, repair 1 of your damaged units that did not use SUSTAIN DAMAGE during this combat round.  
Tags: `repair-units`, `timing-combat`

**Destroyer II** (`dd2`)  
> Cost 1, Combat 8, Move 2 ANTI-FIGHTER BARRAGE 6(x3)  
Tags: `anti-fighter-barrage`

**Dark Energy Tap** (`det`)  
> After you perform a tactical action in a system that contains a frontier token, if you have 1 or more ships in that system, explore that token. Your ships can retreat into adjacent systems that do not contain other players' units, even if you do not have units or control planets in that system.  
Tags: `explore`, `frontier-token`, `retreat-control`, `timing-reaction`

**Dreadnought II** (`dn2`)  
> Cost 4, Combat 5, Move 2, Capacity 1 SUSTAIN DAMAGE, BOMBARDMENT 5 This unit cannot be destroyed by "Direct Hit" action cards.  
Tags: `sustain-damage`, `bombardment`, `direct-hit-immunity`

**Dacxive Animators** (`dxa`)  
> After you win a ground combat, you may place 1 infantry from your reinforcements on that planet.  
Tags: `place-units-free`, `post-combat-trigger`

**Fighter II** (`ff2`)  
> Cost 1(x2), Combat 8, Move 2 This unit may move without being transported. Fighters in excess of your ships' capacity count against your fleet pool.  
Tags: `move-without-transport`

**Fleet Logistics** (`fl`)  
> During each of your turns of the action phase, you may perform 2 actions instead of 1.  
Tags: `extra-activation`, `timing-passive`

**Gravity Drive** (`gd`)  
> After you activate a system, apply +1 to the move value of 1 of your ships during this tactical action.  
Tags: `move-bonus` (1), `timing-reaction`

**Graviton Laser System** (`gls`)  
> You may exhaust this card before 1 or more of your units uses SPACE CANNON; hits produced by those units must be assigned to non-fighter ships if able.  
Tags: `exhaust`, `assign-hits-control`, `space-cannon`

**Hyper Metabolism** (`hm`)  
> During the status phase, gain 3 command tokens instead of 2.  
Tags: `gain-command-token` (3), `timing-status-phase`

**Integrated Economy** (`ie`)  
> After you gain control of a planet, you may produce any number of units on that planet that have a combined cost equal to or less than that planet's resource value.  
Tags: `produce-units`, `timing-reaction`

**Infantry II** (`inf2`)  
> Cost 1(x2), Combat 7 After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `unit-revival`, `timing-reaction`

**Light/Wave Deflector** (`lwd`)  
> Your ships can move through systems that contain other players' ships.  
Tags: `move-through-ships`

**Magen Defense Grid ΩΩ** (`md`)  
> When any player activates a system that contains 1 or more of your structures, place 1 infantry from your reinforcements with each of those structures. At the start of ground combat on a planet that contains 1 or more of your structures, produce 1 hit and assign it to 1 of your opponent's ground forces.  
Tags: `place-units-free`, `pre-combat-hits`, `timing-reaction`, `timing-combat`

**Magen Defense Grid** (`md_base`)  
> You may exhaust this card at the start of a round of ground combat on a planet that contains 1 or more of your units that have PLANETARY SHIELD; your opponent cannot make combat rolls this combat round.  
Tags: `exhaust`, `planetary-shield`, `combat-avoidance`, `timing-combat`

**Magen Defense Grid Ω** (`md_c1`)  
> At the start of ground combat on a planet that contains 1 or more of your structures, produce 1 hit and assign it to 1 of your opponent's ground forces.  
Tags: `pre-combat-hits`, `timing-combat`, `requires-units-present`

**Neural Motivator** (`nm`)  
> During the status phase, draw 2 action cards instead of 1.  
Tags: `draw-action-card` (2), `timing-status-phase`

**Psychoarchaeology** (`pa`)  
> You can use technology specialties on planets you control without exhausting them, even if those planets are exhausted. During the action phase, you can exhaust planets you control that have technology specialties to gain 1 trade good.  
Tags: `tech-specialty-use`, `exhaust-planets`, `gain-trade-goods` (1), `timing-passive`

**PDS II** (`pds2`)  
> PLANETARY SHIELD, SPACE CANNON 5 You may use this unit's SPACE CANNON against ships that are adjacent to this unit's system.  
Tags: `planetary-shield`, `space-cannon`

**Predictive Intelligence** (`pi`)  
> At the end of your turn, you may exhaust this card to redistribute your command tokens. When you cast votes during the agenda phase, you may cast 3 additional votes; if you do, and the outcome you voted for is not resolved, exhaust this card.  
Tags: `redistribute-tokens`, `vote-bonus` (3), `exhaust`, `timing-end-turn`

**Plasma Scoring** (`ps`)  
> When 1 or more of your units use BOMBARDMENT or SPACE CANNON, 1 of those units may roll 1 additional die.  
Tags: `extra-dice` (1), `bombardment`, `space-cannon`, `timing-reaction`

**Self-Assembly Routines** (`sar`)  
> After 1 or more of your units use PRODUCTION, you may exhaust this card to place 1 mech from your reinforcements on a planet you control in that system. After 1 of your mechs is destroyed, gain 1 trade good.  
Tags: `place-units-free`, `gain-trade-goods` (1), `exhaust`, `timing-reaction`, `mech-effect`

**Space Dock II** (`sd2`)  
> PRODUCTION X This unit's PRODUCTION value is equal to 4 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships' capacity.  
Tags: `production-unit`, `production-bonus` (4), `capacity-bonus` (3)

**Scanlink Drone Network** (`sdn`)  
> When you activate a system, you may explore 1 planet in that system which contains 1 or more of your units.  
Tags: `explore`, `timing-reaction`, `requires-units-present`

**Sling Relay** (`sr`)  
> ACTION: Exhaust this card to produce 1 ship in any system that contains one of your space docks.  
Tags: `timing-action`, `exhaust`, `produce-units`, `requires-units-present`

**Sarween Tools** (`st`)  
> When 1 or more of your units use PRODUCTION, reduce the combined cost of the produced units by 1.  
Tags: `cost-reduction` (1), `timing-reaction`

**Transit Diodes** (`td`)  
> You may exhaust this card at the start of your turn during the action phase; remove up to 4 of your ground forces from the game board and place them on 1 or more planets you control.  
Tags: `relocate-units`, `exhaust`, `timing-start-turn`

**War Sun** (`ws`)  
> Cost 12, Combat 3(x3), Move 2, Capacity 6 SUSTAIN DAMAGE, BOMBARDMENT 3(x3)  Other players' units in this system lose PLANETARY SHIELD.  
Tags: `sustain-damage`, `bombardment`, `ignore-planetary-shield`

**X-89 Bacterial Weapon Ω** (`x89`)  
> After 1 or more of your units use BOMBARDMENT against a planet, if at least 1 of your opponent's infantry was destroyed, you may destroy all of your opponent's infantry on that planet.  
Tags: `bombardment`, `destroy-units`, `timing-reaction`

**X-89 Bacterial Weapon** (`x89_base`)  
> ACTION: Exhaust this card and choose 1 planet in a system that contains 1 or more of your ships that have BOMBARDMENT; destroy all infantry on that planet  
Tags: `timing-action`, `exhaust`, `bombardment`, `destroy-units`

**X-89 Bacterial Weapon ΩΩ** (`x89c4`)  
> Double the hits produced by your units' BOMBARDMENT and ground combat rolls. Exhaust each planet you use BOMBARDMENT against.  
Tags: `bombardment`, `exhaust-planets`, `timing-passive`

**Bioplasmosis** (`bio`, arborec)  
> At the end of the status phase, you may remove any number of infantry from planets you control and place them on 1 or more planets you control in the same or adjacent systems.  
Tags: `timing-end-turn`, `relocate-units`

**Letani Warrior II** (`lw2`, arborec)  
> Cost 1(x2), Combat 7 PRODUCTION 2 After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `production-unit`, `unit-revival`, `timing-reaction`

**Aerie Hololattice** (`ah`, argent)  
> Other players cannot move ships through systems that contain your structures. Each planet that contains 1 or more of your structures gains the PRODUCTION 1 ability as if it were a unit.  
Tags: `restrict-movement`, `production-unit`, `structure-effect`

**Strike Wing Alpha II** (`swa2`, argent)  
> Cost 1, Combat 7, Move 2, Capacity 1 ANTI-FIGHTER BARRAGE 6(x3) When this unit uses ANTI-FIGHTER BARRAGE, each result of 9 or 10 also destroys 1 of your opponents infantry in the space area of the active system.  
Tags: `anti-fighter-barrage`, `destroy-units` (1)

**4X41C "Helios" V2** (`helios2`, bastion)  
> This unit's PRODUCTION value is equal to 4 more than the resource value of this planet. The resource value of this planet is increased by 2.  Up to 3 fighters in this system do not count against your ships' capacity.  
Tags: `production-unit`, `production-bonus` (4), `resources-bonus` (2), `capacity-bonus` (3)

**Proxima Targeting VI** (`proxima`, bastion)  
> Cancel 1 hit produced by BOMBARDMENT rolls made against your ground forces for each of your galvanized units present.  At the start of a round of ground combat, you may resolve BOMBARDMENT 8 (x3) against your opponent's ground forces; if you do, make an identical roll against your ground forces.  
Tags: `cancel-hit`, `bombardment`, `timing-combat`

**Dimensional Tear II** (`dt2`, cabal)  
> PRODUCTION 7 This system is a gravity rift; your ships do not roll for this gravity rift. Place a dimensional tear token beneath this unit as a reminder. Up to 12 fighters in this system do not count against your ships' capacity.  
Tags: `production-unit`, `anomaly-movement`, `capacity-bonus` (12)

**Vortex** (`vtx`, cabal)  
> ACTION: Exhaust this card to choose another player's non-structure unit in a system that is adjacent to 1 or more of your space docks. Capture 1 unit of that type from that player's reinforcements.  
Tags: `timing-action`, `exhaust`, `capture-units`, `requires-adjacent-units`

**Exile II** (`exile2`, crimson)  
> Cost 1, Combat 7, Move 2, ANTI-FIGHTER BARRAGE 6 (x3). At the end of any players' combat in this unit's system or up to 2 systems away, you may place 1 active or inactive breach in that system.  
Tags: `anti-fighter-barrage`, `timing-combat`, `game-structure-change`

**Subatomic Splicer** (`subatomic`, crimson)  
> When one of your ships is destroyed, you may produce a ship of the same type at a space dock in your home system.  
Tags: `unit-revival`, `timing-reaction`

**Hydrothermal Mining** (`hydrothermal`, deepwrought)  
> At the start of the status phase, gain 1 trade good for each ocean card in play.  
Tags: `gain-trade-goods`, `timing-status-phase`

**Radical Advancement** (`radical`, deepwrought)  
> At the start of the status phase, you may replace one of your non-unit upgrade technologies with a technology of the same color that has exactly 1 more prerequisite.  
Tags: `timing-status-phase`, `tech-specialty-use`

**Aetherstream** (`as`, empyrean)  
> After you or one of your neighbors activates a system that is adjacent to an anomaly, you may apply +1 to the move value of all of that player's ships during this tactical action.  
Tags: `move-bonus` (1), `requires-neighbor`, `timing-reaction`

**Voidwatch** (`vw`, empyrean)  
> After a player moves ships into a system that contains 1 or more of your units, they must give you 1 promissory note from their hand, if able.  
Tags: `gain-promissory`, `timing-reaction`, `requires-units-present`

**Neural Parasite (Firmament)** (`parasite-firm`, firmament)  
> At the start of the status phase, you may place 1 infantry from your reinforcements on a planet you control in your home system. Flip this card if the Obsidian faction is in play.  
Tags: `place-units-free`, `timing-status-phase`, `requires-home-system`

**Planesplitter (Firmament)** (`planesplitter-firm`, firmament)  
> When you gain this card, put The Fracture into play. Flip this card if the Obsidian faction is in play.  
Tags: `game-structure-change`, `timing-reaction`

**Dimensional Splicer** (`ds`, ghost)  
> At the start of space combat in a system that contains a wormhole and 1 or more of your ships, you may produce 1 hit and assign it to 1 of your opponent's ships.  
Tags: `pre-combat-hits`, `timing-combat`, `wormhole`, `requires-units-present`

**Wormhole Generator** (`wg`, ghost)  
> ACTION: Exhaust this card to place or move a Creuss wormhole token into either a system that contains a planet you control or a non-home system that does not contain another player's ships.  
Tags: `wormhole`, `timing-action`, `exhaust`

**Production Biomes** (`pm`, hacan)  
> ACTION: Exhaust this card and spend 1 token from your strategy pool to gain 4 trade goods and choose 1 other player; that player gains 2 trade goods.  
Tags: `timing-action`, `strategy-pool-use`, `gain-trade-goods` (4), `give-trade-goods`

**Quantum Datahub Node** (`qdn`, hacan)  
> At the end of the strategy phase, you may spend 1 token from your strategy pool and give another player 3 of your trade goods. If you do, give 1 of your strategy cards to that player and take 1 of their strategy cards.  
Tags: `strategy-card-manipulation`, `give-trade-goods`, `strategy-pool-use`, `timing-end-turn`

**E-Res Siphons** (`ers`, jolnar)  
> After another player activates a system that contains 1 or more of your ships, gain 4 trade goods.  
Tags: `gain-trade-goods` (4), `timing-reaction`, `requires-units-present`

**Spatial Conduit Cylinders** (`scc`, jolnar)  
> You may exhaust this card after you activate a system that contains 1 or more of your units; that system is adjacent to all other systems that contain 1 or more of your units during this activation.  
Tags: `adjacency-grant`, `exhaust`, `timing-reaction`

**Agency Supply Network** (`asn`, keleres)  
> Once per action, when you resolve a unit's PRODUCTION ability, you may resolve another of your unit's PRODUCTION abilities in any system.  
Tags: `once-per-action`, `produce-units`, `timing-reaction`

**Executive Order** (`executiveorder`, keleres)  
> ACTION: Exhaust this card and draw the top or bottom card of the Agenda deck. Players immediately vote on this agenda as if you were the speaker; you can spend trade goods and resources on this agenda as if they were votes.  
Tags: `timing-action`, `agenda-manipulation`, `spend-trade-goods`, `vote-bonus`

**I.I.H.Q. Modernization** (`iihq`, keleres)  
> You are neighbors with all players that have units or control planets in or adjacent to the Mecatol Rex system. Gain the Custodia Vigilia planet card and its legendary planet ability card. You cannot lose these cards, and this card cannot have an X or Y assimilator token placed on it.  
Tags: `requires-neighbor`, `gain-planet`, `timing-passive`

**Inheritance Systems** (`is`, l1z1x)  
> You may exhaust this card and spend 2 resources when you research a technology; ignore all of that technology's prerequisites.  
Tags: `prerequisite-skip`, `exhaust`, `timing-reaction`

**Super Dreadnought II** (`sdn2`, l1z1x)  
> Cost 4, Combat 4, Move 2, Capacity 2 SUSTAIN DAMAGE, BOMBARDMENT 4 This unit cannot be destroyed by "Direct Hit" action cards.  
Tags: `sustain-damage`, `bombardment`, `direct-hit-immunity`

**L4 Disruptors** (`l4`, letnev)  
> During an invasion, units cannot use SPACE CANNON against your units.  
Tags: `space-cannon-immunity`, `timing-passive`

**Non-Euclidean Shielding** (`nes`, letnev)  
> When 1 of your units uses SUSTAIN DAMAGE, cancel 2 hits instead of 1.  
Tags: `sustain-damage`, `cancel-hit`

**Crimson Legionnaire II** (`cl2`, mahact)  
> Cost 1(x2), Combat 7 After this unit is destroyed, gain 1 commodity or convert 1 of your commodities to a trade good. Then, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `gain-commodities` (1), `convert-commodities`, `unit-revival`, `timing-reaction`

**Genetic Recombination** (`gr`, mahact)  
> You may exhaust this card before a player casts votes; that player must cast at least 1 vote for an outcome of your choice or remove 1 token from their fleet pool and return it to their reinforcements.  
Tags: `vote-restrict`, `exhaust`, `timing-agenda-phase`, `remove-command-token`

**Mirror Computing** (`mc`, mentak)  
> When you spend trade goods, each trade good is worth 2 resources or influence instead of 1.  
Tags: `trade-goods-as-resources`

**Salvage Operations** (`so`, mentak)  
> After you win or lose a space combat, gain 1 trade good; if you won the combat, you may also produce 1 ship in that system of any ship type that was destroyed during the combat.  
Tags: `gain-trade-goods` (1), `post-combat-trigger`, `produce-units`

**Magmus Reactor** (`mr`, muaat)  
> Your ships can move into supernovas. Each supernova that contains 1 or more of your units gains the PRODUCTION 5 ability as if it were 1 of your units.  
Tags: `anomaly-movement`, `production-unit`

**Prototype War Sun II** (`pws2`, muaat)  
> Cost 10, Combat 3(x3), Move 3, Capacity 6 SUSTAIN DAMAGE, BOMBARDMENT 3(x3) Other players' units in this system lose PLANETARY SHIELD.  
Tags: `sustain-damage`, `bombardment`, `ignore-planetary-shield`

**Hybrid Crystal Fighter II** (`hcf2`, naalu)  
> Cost 1(x2), Combat 7, Move 2 This unit may move without being transported. Fighters in excess of your ships' capacity count as 1/2 of a ship against your fleet pool.  
Tags: `move-without-transport`

**Neuroglaive** (`ng`, naalu)  
> After another player activates a system that contains 1 or more of your ships, that player removes 1 token from their fleet pool and returns it to their reinforcements.  
Tags: `remove-command-token`, `timing-reaction`, `requires-units-present`

**Pre-Fab Arcologies** (`pfa`, naaz)  
> After you explore a planet, ready that planet.  
Tags: `ready-planets`, `timing-reaction`

**Supercharge** (`sc`, naaz)  
> At the start of a combat round, you may exhaust this card to apply +1 to the result of each of your unit's combat rolls during this combat round.  
Tags: `combat-roll-bonus` (1), `exhaust`, `timing-combat`

**???\_ERROR\_ERROR\_???** (`nekroc4r`, nekro)  
> ACTION: Exhaust this card to place 1 PDS on a planet you control.  ACTION: Exhaust this card to repair all of your damaged units.  ACTION: Exhaust this card and discard 1 action card to draw 1 action card.  
Tags: `timing-action`, `place-structure`, `repair-units`, `draw-action-card` (1), `exhaust`

**???\_NULL\_REFERENCE\_???** (`nekroc4y`, nekro)  
> When one of your ships is destroyed, you may produce a ship of the same type at a space dock in your home system.  
Tags: `unit-revival`, `timing-reaction`

**Valefar Assimilator X** (`vax`, nekro)  
> When you would gain another player's technology using 1 of your faction abilities, you may place the "X" assimilator token on a faction technology owned by that player instead. While that token is on a technology, this card gains that technology's text. You cannot place an assimilator token on technology that already has an assimilator token.  
Tags: `tech-theft`, `share-ability`, `timing-reaction`

**Valefar Assimilator Y** (`vay`, nekro)  
> When you would gain another player's technology using 1 of your faction abilities, you may place the "Y" assimilator token on a faction technology owned by that player instead. While that token is on a technology, this card gains that technology's text. You cannot place an assimilator token on technology that already has an assimilator token.  
Tags: `tech-theft`, `share-ability`, `timing-reaction`

**Memoria II** (`m2`, nomad)  
> Cost 8, Combat 5(x2), Move 2, Capacity 6 SUSTAIN DAMAGE, ANTI-FIGHTER BARRAGE 5(x3) You may treat this unit as if it were adjacent to systems that contain one or more of your mechs.  
Tags: `sustain-damage`, `anti-fighter-barrage`, `adjacency-grant`

**Temporal Command Suite** (`tcs`, nomad)  
> After any player's agent becomes exhausted, you may exhaust this card to ready that agent; if you ready another player's agent, you may perform a transaction with that player.  
Tags: `ready-card`, `transaction`, `timing-reaction`, `exhaust`

**Neural Parasite (Obsidian)** (`parasite-obs`, obsidian)  
> At the start of your turn, destroy 1 of another player's infantry in or adjacent to a system that contains your infantry. This technology cannot be researched.  
Tags: `timing-start-turn`, `destroy-units` (1), `requires-units-present`

**Planesplitter (Obsidian)** (`planesplitter-obs`, obsidian)  
> When you perform a strategic action, you may move an ingress token into a system that contains or is adjacent to your units. This technology cannot be researched.  
Tags: `timing-action`, `relocate-units`, `requires-adjacent-units`

**Linkship II** (`linkship2`, ralnel)  
> Cost 1, Combat 8, Move 4, ANTI-FIGHTER BARRAGE 6 (x3). This unit can use the SPACE CANNON ability of one of your structures in its space area; each linkship can trigger the same structure.  
Tags: `anti-fighter-barrage`, `space-cannon`

**Nanomachines** (`nanomachines`, ralnel)  
> ACTION: Exhaust this card to place 1 PDS on a planet you control. ACTION: Exhaust this card to repair all of your damaged units. ACTION: Exhaust this card and discard 1 action card to draw 1 action card.  
Tags: `timing-action`, `place-structure`, `repair-units`, `draw-action-card` (1), `exhaust`

**Chaos Mapping** (`cm`, saar)  
> Other players cannot activate asteroid fields that contain 1 or more of your ships. At the start of your turn during the action phase, you may produce 1 unit in a system that contains at least 1 of your units that has PRODUCTION.  
Tags: `restrict-activation`, `produce-units`, `timing-start-turn`, `requires-units-present`

**Floating Factory II** (`ffac2`, saar)  
> Move 2, Capacity 5 PRODUCTION 7. This unit is placed in the space area instead of on a planet. This unit can move and retreat as if it were a ship. If this unit is blockaded, it is destroyed.  
Tags: `production-unit`, `destroy-units`

**Exotrireme II** (`exo2`, sardakk)  
> Cost 4, Combat 5, Move 2, Capacity 1 SUSTAIN DAMAGE, BOMBARDMENT 4(x2) This unit cannot be destroyed by "Direct Hit" action cards. After a round of space combat, you may destroy this unit to destroy up to 2 ships in this system.  
Tags: `sustain-damage`, `bombardment`, `direct-hit-immunity`, `destroy-units` (2), `timing-combat`

**Valkyrie Particle Weave** (`vpw`, sardakk)  
> After making combat rolls during a round of ground combat, if your opponent produced 1 or more hits, you produce 1 additional hit.  
Tags: `post-combat-trigger`

**Advanced Carrier II** (`ac2`, sol)  
> Cost 3, Combat 9, Move 2, Capacity 8 SUSTAIN DAMAGE  
Tags: `sustain-damage`

**Spec Ops II** (`so2`, sol)  
> Cost 1(x2), Combat 6 After this unit is destroyed, roll 1 die. If the result is 5 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `unit-revival`, `timing-reaction`

**Hel-Titan II** (`ht2`, titans)  
> Combat 6 PLANETARY SHIELD, SPACE CANNON 5, SUSTAIN DAMAGE, PRODUCTION 1 This unit is treated as both a structure and a ground force. It cannot be transported. You may use this unit's SPACE CANNON against ships that are adjacent to this unit's systems.  
Tags: `planetary-shield`, `space-cannon`, `sustain-damage`, `production-unit`, `ground-force-effect`

**Saturn Engine II** (`se2`, titans)  
> Cost 2, Combat 6, Move 3, Capacity 2 SUSTAIN DAMAGE  
Tags: `sustain-damage`

**Hegemonic Trade Policy** (`htp`, winnu)  
> Exhaust this card when 1 or more of your units use PRODUCTION; swap the resource and influence values of 1 planet you control during that use of Production.  
Tags: `exhaust`, `timing-reaction`, `resources-bonus`

**Lazax Gate Folding** (`lgf`, winnu)  
> During your tactical actions, if you do not control Mecatol Rex, treat its system as if it has both an α and β wormhole. ACTION: If you control Mecatol Rex, exhaust this card to place 1 infantry from your reinforcements on Mecatol Rex.  
Tags: `wormhole`, `requires-planets-controlled`, `exhaust`, `place-units-free`

**Instinct Training** (`it`, xxcha)  
> You may exhaust this card and spend 1 token from your strategy pool when another player plays an action card; cancel that action card.  
Tags: `negate-ability`, `exhaust`, `strategy-pool-use`, `timing-reaction`

**Nullification Field** (`nf`, xxcha)  
> After another player activates a system that contains 1 or more of your ships, you may exhaust this card and spend 1 token from your strategy pool; immediately end that player's turn.  
Tags: `timing-reaction`, `exhaust`, `strategy-pool-use`, `end-turn-skip`

**Impulse Core** (`ic`, yin)  
> At the start of a space combat, you may destroy 1 of your cruisers or destroyers in the active system to produce 1 hit against your opponent's ships; that hit must be assigned by your opponent to 1 of their non-fighters ships if able.  
Tags: `destroy-units` (1), `pre-combat-hits`, `assign-hits-control`, `timing-combat`

**Yin Spinner Omega** (`yso`, yin)  
> After you produce units, place up to 2 infantry from your reinforcements on any planet you control or in any space area that contains 1 or more of your ships.  
Tags: `place-units-free`, `timing-reaction`

**Mageon Implants** (`mi`, yssaril)  
> ACTION: Exhaust this card to look at another player's hand of action cards.  Choose 1 of those cards and add it to your hand.  
Tags: `timing-action`, `look-at-hidden-info`, `discard-opponent-card`

**Transparasteel Plating** (`tp`, yssaril)  
> During your turn of the action phase, players that have passed cannot play action cards.  
Tags: `action-card-restriction`, `timing-passive`

## unit

**Dreadnought II** (`dreadnought2`)  
> This unit cannot be destroyed by "Direct Hit" action cards.  
Tags: `direct-hit-immunity`

**Fighter II** (`fighter2`)  
> This unit may move without being transported. Fighters in excess of your ships' capacity count against your fleet pool.  
Tags: `move-without-transport`

**Infantry II** (`infantry2`)  
> After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home.  
Tags: `unit-revival`, `timing-reaction`

**PDS II** (`pds2`)  
> You may use this unit's SPACE CANNON against ships that are in adjacent systems.  
Tags: `space-cannon`

**Space Dock I** (`spacedock`)  
> This unit's PRODUCTION value is equal to 2 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships' capacity.  
Tags: `production-bonus` (2), `capacity-bonus` (3)

**Space Dock II** (`spacedock2`)  
> This unit's PRODUCTION value is equal to 4 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships' capacity.  
Tags: `production-bonus` (4), `capacity-bonus` (3)

**War Sun** (`warsun`)  
> Other player's units in this system lose PLANETARY SHIELD.  
Tags: `ignore-planetary-shield`

**Duha Menaimon** (`arborec_flagship`, arborec)  
> After you activate this system, you may produce up to 5 units in this system.  
Tags: `produce-units`, `timing-reaction`

**Letani Warrior II** (`arborec_infantry2`, arborec)  
> After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `unit-revival`, `timing-reaction`

**Letani Behemoth** (`arborec_mech`, arborec)  
> DEPLOY: When you would use your Mitosis faction ability you may replace 1 of your infantry with 1 mech from your reinforcements instead.  
Tags: `unit-replacement`, `timing-reaction`, `mech-effect`

**Strike Wing Alpha II** (`argent_destroyer2`, argent)  
> When this unit uses ANTI-FIGHTER BARRAGE, each result of 9 or 10 also destroys 1 of your opponent's infantry in the space area of the active system.  
Tags: `anti-fighter-barrage`, `destroy-units` (1)

**Quetzecoatl** (`argent_flagship`, argent)  
> Other players cannot use space cannon against your ships in this system.  
Tags: `space-cannon-immunity`

**Aerie Sentinel** (`argent_mech`, argent)  
> This unit does not count against capacity if it is being transported or if it is in a space area with 1 or more of your ships that have capacity values.  
Tags: `capacity-bonus`

**The Egeiro** (`bastion_flagship`, bastion)  
> Apply +1 to the result of each of this unit's combat rolls for each non-home system that contains a planet you control.  
Tags: `combat-roll-bonus` (1), `requires-planets-controlled`

**A3 Valiance** (`bastion_mech`, bastion)  
> When this unit is destroyed, if it was galvanized, galvanize up to 3 of your infantry in its system.  
Tags: `timing-reaction`, `gain-attachment`

**4x41C "Helios" V1** (`bastion_spacedock`, bastion)  
> The resource value of this planet is increased by 1. This unit's PRODUCTION value is equal to 2 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships' capacity  
Tags: `resources-bonus` (1), `production-bonus` (2), `capacity-bonus` (3)

**4x41C "Helios" V2** (`bastion_spacedock2`, bastion)  
> The resource value of this planet is increased by 2. This unit's PRODUCTION value is equal to 4 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships' capacity  
Tags: `resources-bonus` (2), `production-bonus` (4), `capacity-bonus` (3)

**The Terror Between** (`cabal_flagship`, cabal)  
> Capture all other non-structure units that are destroyed in this system, including your own.  
Tags: `capture-units`

**Reanimator** (`cabal_mech`, cabal)  
> When your infantry on this planet are destroyed, place them on your faction sheet; those units are captured.  
Tags: `capture-units`, `timing-reaction`

**Dimensional Tear I** (`cabal_spacedock`, cabal)  
> This system is a gravity rift; your ships do not roll for this gravity rift. Place a dimensional tear token beneath this unit as a reminder. Up to 6 fighters in this system do not count against your ships' capacity.  
Tags: `anomaly-movement`, `capacity-bonus` (6)

**Dimensional Tear II** (`cabal_spacedock2`, cabal)  
> This system is a gravity rift; your ships do not roll for this gravity rift. Place a dimensional tear token beneath this unit as a reminder. Up to 12 fighters in this system do not count against your ships' capacity.  
Tags: `anomaly-movement`, `capacity-bonus` (12)

**Exile I** (`crimson_destroyer`, crimson)  
> At the end of any player's combat in this unit's system or an adjacent system, you may place 1 inactive breach in that system.  
Tags: `game-structure-change`, `timing-combat`

**Exile II** (`crimson_destroyer2`, crimson)  
> At the end of any players' combat in this unit's system or up to 2 systems away, you may place 1 active or inactive breach in that system.  
Tags: `game-structure-change`, `timing-combat`

**Quietus** (`crimson_flagship`, crimson)  
> While this unit is in a system that contains an active breach, other players' units in systems with active breaches lose all of their unit abilities  
Tags: `negate-ability`, `timing-passive`

**Revenant** (`crimson_mech`, crimson)  
> DEPLOY: During the "Commit Ground Forces" step of your tactical action in a system that contains an active breach, you may commit 1 mech, even if you have no units in the system.  
Tags: `mech-effect`, `timing-passive`

**D.W.S. Luminous** (`deepwrought_flagship`, deepwrought)  
> This ship can move through systems that contain your units, even if other players' units are present; if it would, apply +1 to its move value for each of those systems.  
Tags: `move-through-ships`, `move-bonus` (1)

**Eanautic** (`deepwrought_mech`, deepwrought)  
> When another player activates this system, if this unit is coexisting, you may move it and any of your infantry on its planet to a planet you control in your home system.  
Tags: `timing-reaction`, `relocate-units`

**Dynamo** (`empyrean_flagship`, empyrean)  
> After any player's unit in this system or an adjacent system uses SUSTAIN DAMAGE, you may spend 2 influence to repair that unit.  
Tags: `repair-units`, `timing-reaction`

**Watcher** (`empyrean_mech`, empyrean)  
> You may remove this unit from a system that contains or is adjacent to another player's units to cancel an action card played by that player.  
Tags: `negate-ability`, `mech-effect`, `timing-reaction`

**Heaven's Eye** (`firmament_flagship`, firmament)  
> If the active system contains units that belong to a player who has a control marker on 1 of your plots, apply +1 to this ship's move value and repair it at the end of every combat round.  
Tags: `move-bonus` (1), `repair-units`, `timing-combat`, `requires-units-present`

**Viper EX-23** (`firmament_mech`, firmament)  
> When ground forces are committed to this planet, you may choose for your units to coexist, if they were not already. Flip this card if your faction becomes the Obsidian.  
Tags: `combat-avoidance`, `timing-reaction`

**Combat Transport II** (`ghemina_carrier2`, ghemina)  
> You may reroll 1 of your unit's combat dice during each round of ground combat on a planet in this system that contains 2 or fewer of your infantry.  
Tags: `reroll`, `timing-combat`, `requires-units-present`

**Hil Colish** (`ghost_flagship`, ghost)  
> This ship's system contains a delta wormhole. During movement, this ship may move before or after your other ships.  
Tags: `wormhole`, `ship-effect`

**Icarus Drive** (`ghost_mech`, ghost)  
> After any player activates a system, you may remove this unit from the game board to place or move a Creuss wormhole token into this system.  
Tags: `timing-reaction`, `mech-effect`, `wormhole`

**Wrath of Kenara** (`hacan_flagship`, hacan)  
> After you roll a die during a space combat in this system, you may spend 1 trade good to apply +1 to the result.  
Tags: `combat-roll-bonus` (1), `spend-trade-goods` (1), `timing-combat`

**Pride of Kenara** (`hacan_mech`, hacan)  
> This planet's card may be traded as part of a transaction; if you do, move all of your units from this planet to another planet you control.  
Tags: `transaction`, `relocate-units`

**J.N.S. Hylarim** (`jolnar_flagship`, jolnar)  
> When making a combat roll for this ship, each result of 9 or 10, before applying modifiers, produces 2 additional hits.  
Tags: `combat-stat-change`

**Shield Paling** (`jolnar_mech`, jolnar)  
> Your infantry on this planet are not affected by your Fragile faction ability.  
Tags: `negate-ability`, `ground-force-effect`

**Artemiris** (`keleres_flagship`, keleresm)  
> Other players must spend 2 influence to activate the system that contains this ship.  
Tags: `restrict-activation`

**Omniopiares** (`keleres_mech`, keleresm)  
> Other players must spend 1 influence to commit ground forces to the planet that contains this unit.  
Tags: `restrict-movement`, `ground-force-effect`

**Super Dreadnought II** (`l1z1x_dreadnought2`, l1z1x)  
> This unit cannot be destroyed by "Direct Hit" action cards.  
Tags: `direct-hit-immunity`

**[0.0.1]** (`l1z1x_flagship`, l1z1x)  
> During a space combat, hits produced by this ship and by your dreadnoughts in this system must be assigned to non-fighter ships if able.  
Tags: `assign-hits-control`, `timing-combat`, `ship-effect`

**Anihilator** (`l1z1x_mech`, l1z1x)  
> While not participating in ground combat, this unit can use it's BOMBARDMENT ability on planets in its system as if it were a ship.  
Tags: `bombardment`, `ship-effect`

**Arc Secundus** (`letnev_flagship`, letnev)  
> Other player's units in this system lose PLANETARY SHIELD. At the start of each space combat round, repair this ship.  
Tags: `ignore-planetary-shield`, `repair-units`, `timing-combat`

**Dunlain Reaper** (`letnev_mech`, letnev)  
> DEPLOY: At the start of a round of ground combat, you may spend 2 resources to replace 1 of your infantry in that combat with 1 mech.  
Tags: `unit-replacement`, `timing-combat`

**Arvicon Rex** (`mahact_flagship`, mahact)  
> During combat against an opponent whose command token is not in your fleet pool, apply +2 to the results of this unit's combat rolls.  
Tags: `combat-roll-bonus` (2), `timing-combat`

**Crimson Legionnaire I** (`mahact_infantry`, mahact)  
> After this unit is destroyed, gain 1 commodity or convert 1 of your commodities to a trade good.  
Tags: `gain-commodities` (1), `convert-commodities`, `timing-reaction`

**Crimson Legionnaire II** (`mahact_infantry2`, mahact)  
> After this unit is destroyed, gain 1 commodity or convert 1 of your commodities to a trade good. Then, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `gain-commodities` (1), `convert-commodities`, `unit-revival`, `timing-reaction`

**Starlancer** (`mahact_mech`, mahact)  
> After a player whose command token is in your fleet pool activates this system, you may spend their token from your fleet pool to end their turn; they gain that token.  
Tags: `timing-reaction`, `end-turn-skip`, `gain-command-token`

**Corsair** (`mentak_cruiser3`, mentak)  
> If the active system contains another player's non-fighter ships, this unit can move through systems that contain other players' ships.  
Tags: `move-through-ships`, `requires-units-present`

**Fourth Moon** (`mentak_flagship`, mentak)  
> Other player's ships in this system cannot use SUSTAIN DAMAGE.  
Tags: `sustain-deny`

**Moll Terminus** (`mentak_mech`, mentak)  
> Other player's ground forces on this planet cannot use SUSTAIN DAMAGE.  
Tags: `sustain-deny`, `ground-force-effect`

**The Inferno** (`muaat_flagship`, muaat)  
> ACTION: Spend 1 token from your strategy pool to place 1 cruiser in this system.  
Tags: `timing-action`, `strategy-pool-use`, `place-units-free`

**Ember Colossus** (`muaat_mech`, muaat)  
> When you use your Star Forge faction ability in this system or an adjacent system, you may place 1 infantry from your reinforcements with this unit.  
Tags: `place-units-free`, `timing-reaction`

**Prototype War Sun I** (`muaat_warsun`, muaat)  
> Other players' units in this system lose the PLANETARY SHIELD ability.  
Tags: `ignore-planetary-shield`

**Prototype War Sun II** (`muaat_warsun2`, muaat)  
> Other players' units in this system lose the PLANETARY SHIELD ability.  
Tags: `ignore-planetary-shield`

**Hybrid Crystal Fighter II** (`naalu_fighter2`, naalu)  
> This unit may move without being transported. Each fighter in excess of your ships' capacity counts as 1/2 of a ship against your fleet pool.  
Tags: `move-without-transport`

**Matriarch** (`naalu_flagship`, naalu)  
> During an invasion in this system, you may commit fighters to planets as if they were ground forces. After combat, return those units to the space area.  
Tags: `fighter-effect`, `relocate-units`, `timing-combat`

**Iconoclast** (`naalu_mech`, naalu)  
> During combat against an opponent who has at least 1 relic fragment, apply +2 to the results of this unit's combat rolls.  
Tags: `combat-roll-bonus` (2), `timing-combat`

**Iconoclast Omega** (`naalu_mech_omega`, naalu)  
> Other players cannot use ANTI-FIGHTER BARRAGE against your units in this system.  
Tags: `space-cannon-immunity`

**Iconoclast TE** (`naalu_mech_te`, naalu)  
> DEPLOY: When another player gains a relic, place 1 mech on any planet you control.  
Tags: `timing-reaction`, `place-units-free`, `mech-effect`

**Z-Grav Eidolon** (`absol_naaz_mech_space`, naaz)  
> If this unit is in the space area of the active system, it is also a ship. At the end of a space battle in the active system, flip this card.  
Tags: `ship-effect`, `timing-combat`

**Visz El Vir** (`naaz_flagship`, naaz)  
> Your mechs in this system roll 1 additional die during combat.  
Tags: `extra-dice` (1), `mech-effect`

**Eidolon** (`naaz_mech`, naaz)  
> If this unit is in the space area of the active system at the start of a space combat, flip this card.  
Tags: `timing-combat`, `requires-units-present`

**Z-Grav Eidolon** (`naaz_mech_space`, naaz)  
> If this unit is in the space area of the active system, it is also a ship. At the end of a space battle in the active system, flip this card.  
Tags: `ship-effect`, `timing-combat`

**Eidolon Maximum** (`naaz_voltron`, naaz)  
> This unit is both a ship and ground force. It cannot be assigned hits from unit abilities. Repair it at the start of every combat round. Game effects cannot place or produce your mechs. When this unit is destroyed or removed, flip this card and return it to your play area.  
Tags: `ship-effect`, `ground-force-effect`, `assign-hits-control`, `repair-units`

**The Alastor** (`nekro_flagship`, nekro)  
> At the start of a space combat, choose any number of your ground forces in this system to participate in that combat as if they were ships.  
Tags: `ground-force-effect`, `ship-effect`, `timing-combat`

**Mordred** (`nekro_mech`, nekro)  
> During combat against an opponent who has an "X" or "Y" token on 1 or more of their technologies, apply +2 to the result of each of this unit's combat rolls.  
Tags: `combat-roll-bonus` (2), `timing-combat`

**Memoria I** (`nomad_flagship`, nomad)  
> You may treat this unit as if it were adjacent to systems that contain 1 or more of your mechs.  
Tags: `adjacency-grant`, `requires-units-present`

**Memoria II** (`nomad_flagship2`, nomad)  
> You may treat this unit as if it were adjacent to systems that contain 1 or more of your mechs.  
Tags: `adjacency-grant`, `requires-units-present`

**Quantum Manipulator** (`nomad_mech`, nomad)  
> While this unit is in a space area during combat, you may use its SUSTAIN DAMAGE ability to cancel a hit that is produced against your ships in this system.  
Tags: `sustain-damage`, `cancel-hit`, `timing-combat`

**Viper Hollow** (`obsidian_mech`, obsidian)  
> If this unit was coexisting when this card flipped to this side, gain control of its planet; the other player's units are now coexisting.  
Tags: `gain-planet`, `combat-avoidance`

**Linkship I** (`ralnel_destroyer`, ralnel)  
> This unit can use the SPACE CANNON ability of one of your structures in its space area; each structure can only be triggered once.  
Tags: `space-cannon`, `once-per-action`

**Linkship II** (`ralnel_destroyer2`, ralnel)  
> This unit can use the SPACE CANNON ability of one of your structures in its space area; each linkship can trigger the same structure.  
Tags: `space-cannon`

**Last Dispatch** (`ralnel_flagship`, ralnel)  
> When this unit retreats, you may destroy 1 ship in the active system that does not have SUSTAIN DAMAGE.  
Tags: `destroy-units` (1), `retreat-control`

**Alarum** (`ralnel_mech`, ralnel)  
> At the end of a round of combat on this planet, you may move up to 2 ground forces to this planet from planets in adjacent systems.  
Tags: `relocate-units`, `timing-combat`

**Scavenger Zeta** (`saar_mech`, saar)  
> DEPLOY: After you gain control of a planet, you may spend 1 trade good to place 1 mech on that planet  
Tags: `timing-reaction`, `spend-trade-goods` (1), `mech-effect`

**Floating Factory I** (`saar_spacedock`, saar)  
> This unit is placed in the space area instead of on a planet. This unit can move and retreat as if it were a ship. If this unit is blockaded, it is destroyed.  
Tags: `ship-effect`, `destroy-units`, `structure-effect`

**Floating Factory II** (`saar_spacedock2`, saar)  
> This unit is placed in the space area instead of on a planet. This unit can move and retreat as if it were a ship. If this unit is blockaded, it is destroyed.  
Tags: `ship-effect`, `destroy-units`, `structure-effect`

**Exotrireme II** (`sardakk_dreadnought2`, sardakk)  
> This unit cannot be destroyed by "Direct Hit" action cards. After a round of space combat, you may destroy this unit to destroy up to 2 ships in the system.  
Tags: `direct-hit-immunity`, `destroy-units` (2), `timing-combat`

**C'morran N'orr** (`sardakk_flagship`, sardakk)  
> Apply +1 to the result of each of your other ship's combat rolls in this system.  
Tags: `combat-roll-bonus` (1)

**Valkyrie Exoskeleton** (`sardakk_mech`, sardakk)  
> After this unit uses it's SUSTAIN DAMAGE ability during ground combat, it produces 1 hit against your opponent's ground forces on this planet.  
Tags: `sustain-damage`, `post-combat-trigger`

**Genesis ** (`sol_flagship`, sol)  
> At the end of the status phase, place 1 infantry from your reinforcements in this system's space area.  
Tags: `timing-end-turn`, `place-units-free`

**Spec Ops II** (`sol_infantry2`, sol)  
> After this unit is destroyed, roll 1 die. If the result is 5 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.  
Tags: `unit-revival`, `timing-reaction`

**ZS Thunderbolt M2** (`sol_mech`, sol)  
> DEPLOY: After you use your Orbital Drop faction ability, you may spend 3 resources to place 1 mech on that planet.  
Tags: `timing-reaction`, `mech-effect`

**Ouranos** (`titans_flagship`, titans)  
> DEPLOY: After you activate a system that contains 1 or more of your PDS, you may replace 1 of those PDS with this unit.  
Tags: `unit-replacement`, `timing-reaction`, `requires-units-present`

**Hecatoncheires** (`titans_mech`, titans)  
> DEPLOY: When you would place a PDS on a planet, you may place 1 mech and 1 infantry on that planet instead.  
Tags: `place-units-free`, `mech-effect`, `timing-reaction`

**Hel-Titan I** (`titans_pds`, titans)  
> This unit is treated as both a structure and a ground force. It cannot be transported.  
Tags: `ground-force-effect`, `structure-effect`

**Hel-Titan II** (`titans_pds2`, titans)  
> This unit is treated as both a structure and a ground force. It cannot be transported. You may use this unit's SPACE CANNON against ships that are adjacent to this unit's systems.  
Tags: `ground-force-effect`, `structure-effect`, `space-cannon`

**Salai Sai Corian** (`winnu_flagship`, winnu)  
> When this unit makes a combat roll, it rolls a number of dice (hit on a 7) equal to the number of your opponent's non-fighter ships in this system.  
Tags: `extra-dice`, `combat-stat-change`

**Reclaimer** (`winnu_mech`, winnu)  
> After you resolve a tactical action where you gained control of this planet, you may place 1 PDS or 1 Space Dock from your reinforcements on this planet.  
Tags: `place-structure`, `timing-reaction`

**Loncara Ssodu** (`xxcha_flagship`, xxcha)  
> You may use this unit's SPACE CANNON against ships that are in adjacent systems.  
Tags: `space-cannon`

**Indomitus** (`xxcha_mech`, xxcha)  
> You may use this unit's SPACE CANNON against ships that are in adjacent systems.  
Tags: `space-cannon`

**Van Hauge** (`yin_flagship`, yin)  
> When this ship is destroyed, destroy all ships in this system.  
Tags: `destroy-units`, `timing-reaction`

**Moyin's Ashes** (`yin_mech`, yin)  
> DEPLOY: When you use your Indoctrination faction ability, you may spend 1 additional influence to replace your opponent's unit with 1 mech instead of 1 infantry.  
Tags: `unit-replacement`, `mech-effect`, `timing-reaction`

**Y'sia Y'ssrila** (`yssaril_flagship`, yssaril)  
> This ship can move through systems that contain other player's ships.  
Tags: `move-through-ships`

**Blackshade Infiltrator** (`yssaril_mech`, yssaril)  
> DEPLOY: After you use your Stall Tactics faction ability, you may place 1 mech on a planet you control.  
Tags: `place-units-free`, `mech-effect`, `timing-reaction`

## leader

**Letani Ospha** (`arborecagent`, arborec)  
> ACTION: | Exhaust this card and choose a player's non-fighter ship: that player may replace that ship with one from their reinforcements that costs up to 2 more than the replaced ship. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `unit-replacement`

**Dirzuga Rophal** (`arboreccommander`, arborec)  
> After another player activates a system that contains 1 or more of your units that have PRODUCTION: | You may produce 1 unit in that system. | UNLOCK: Have 12 ground forces on planets you control.  
Tags: `timing-reaction`, `produce-units`, `leader-unlock-condition`

**Letani Miasmiala** (`arborechero`, arborec)  
> ACTION: | Produce any number of units in any number of systems that contain 1 or more of your ground forces. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `produce-units`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Trilossa Aun Mirik** (`argentagent`, argent)  
> When a player produces ground forces in a system: | You may exhaust this card: that player may place those units on any planets they control in that system and any adjacent systems. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `place-units-free`

**Trrakan Aun Zulok** (`argentcommander`, argent)  
> When 1 or more of your units make a roll for a unit ability: | You may choose 1 of those units to roll 1 additional die. | UNLOCK: Have 6 units that have ANTI-FIGHTER BARRAGE, SPACE CANNON, or BOMBARDMENT on the game board.  
Tags: `timing-reaction`, `extra-dice` (1), `leader-unlock-condition`

**Mirik Aun Sissiri** (`argenthero`, argent)  
> ACTION: | Move any number of your ships from any systems to any number of other systems that contain 1 of your command tokens and no other players' ships. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `relocate-units`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Dame Briar** (`bastionagent`, bastion)  
> When a player's unit is destroyed | You may exhaust this card to galvanize another of that player's units in the destroyed unit's system. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `gain-attachment`

**Nip and Tuck** (`bastioncommander`, bastion)  
> At any time | Your action cards cannot be canceled by "Sabotage" action cards. The Nekro Virus cannot place assimilator tokens on your components. | UNLOCK: There are 3 galvanized units on the game board  
Tags: `timing-passive`, `negate-ability`, `leader-unlock-condition`

**Lyra Keen** (`bastionhero`, bastion)  
> When one of your galvanized units is destroyed | You may purge this card to roll 1 die for each unit in its system that belongs to another player; if the result is equal to or greater than the galvanized unit's combat value, destroy that unit. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `purge`, `hero-one-shot`, `destroy-units`, `leader-unlock-condition`

**F.S.S. Orlando** (`orlandohero`, bastion)  
> When one of your units in the Ordinian system is destroyed | You may purge this card to designate that unit as a catalyst: roll 1 die for each other player's unit in the system. For each result equal to or greater than the catalyst's combat value, destroy that unit. | UNLOCK: The Nekro Virus has 5 victory points.  
Tags: `timing-reaction`, `purge`, `hero-one-shot`, `destroy-units`, `leader-unlock-condition`

**The Stillness of Stars** (`cabalagent`, cabal)  
> After another player replenishes commodities: | You may exhaust this card to convert their commodities to trade goods and capture 1 unit from their reinforcements that has a cost equal to or lower than their commodity value. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `convert-commodities`, `capture-units`

**That Which Molds Flesh** (`cabalcommander`, cabal)  
> When you produce fighter or infantry units: | Up to 2 of those units do not count against your PRODUCTION limit. | UNLOCK: Have units in 3 gravity rifts.  
Tags: `timing-reaction`, `production-bonus` (2), `leader-unlock-condition`

**It Feeds on Carrion** (`cabalhero`, cabal)  
> ACTION: | Each other player rolls a die for each of his non-fighter ships that are in or adjacent to a system that contains a dimensional tear. on a 1-3, capture that unit. If this causes a player's ground forces or fighter to be removed, also capture those units. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `capture-units`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Ahk Ravin** (`crimsonagent`, crimson)  
> ACTION: | Exhaust this card to choose 1 player. That player may swap the position of 2 of their ships in any systems; they may transport units when they swap. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `relocate-units`

**Ahk Siever** (`crimsoncommander`, crimson)  
> At the end of a combat between any players: | Gain 1 commodity or convert 1 of your commodities to a trade good. | UNLOCK: Place a breach token in a system that contains another player's unit.  
Tags: `post-combat-trigger`, `gain-commodities`, `convert-commodities`, `leader-unlock-condition`

**Homesick Phantom** (`crimsonhero`, crimson)  
> When you produce ships | You may place any of those ships onto this card. At the start of a space combat, you may purge this card to place all ships from this card into the active system. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `place-units-free`, `timing-combat`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Doctor Carrina** (`deepwroughtagent`, deepwrought)  
> When another player researches a technology | You may exhaust this card to allow that player to ignore 1 prerequisite; if they do, you may place 1 infantry from your reinforcements into coexistence on a non-home planet they control. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `prerequisite-skip` (1), `place-units-free`

**Aello** (`deepwroughtcommander`, deepwrought)  
> When another player spends resources to research a technology | That player may reduce the cost by 1; if they do, gain 1 commodity or convert 1 of your commodities to a trade good. | UNLOCK: Have an ocean card in play.  
Tags: `timing-reaction`, `cost-reduction` (1), `gain-commodities` (1), `convert-commodities`, `leader-unlock-condition`

**Ta Zern** (`deepwroughthero`, deepwrought)  
> ACTION: | Purge this card and a non-unit upgrade technology you own or from your deck; then, purge all cards with the same name owned by other players and in other players' decks. Then, each player that purged a technology they owned researches another technology. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `purge`, `hero-one-shot`, `research-free`, `leader-unlock-condition`

**Acamar** (`empyreanagent`, empyrean)  
> After a player moves ships into a system that does not contain any planets: | You may exhaust this card: that player gains 1 command token. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `gain-command-token` (1)

**Xuange** (`empyreancommander`, empyrean)  
> After another player moves ships into a system that contains 1 of your command tokens: | You may return that token to your reinforcements. | UNLOCK: Be neighbors with all other players.  
Tags: `timing-reaction`, `remove-command-token`, `leader-unlock-condition`

**Conservator Procyon** (`empyreanhero`, empyrean)  
> ACTION: | Place 1 frontier token in each system that does not contain any planets and does not already have a frontier token. Then, explore each frontier token that is in a system that contains 1 or more of your ships. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `frontier-token`, `explore`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Myru Vos** (`firmamentagent`, firmament)  
> When a player moves ships | You may exhaust this card; if you do, SPACE CANNON cannot be used against those ships. If they are not transporting units, they can also move through other players' ships. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `space-cannon-immunity`, `move-through-ships`

**Captain Aroz** (`firmamentcommander`, firmament)  
> At any time | You can treat planets in systems that contain your ships as if they were controlled by you for the purpose of scoring secret objectives. | UNLOCK: Have a plot card in play  
Tags: `timing-passive`, `score-objective-help`, `leader-unlock-condition`

**Sharsiss** (`firmamenthero`, firmament)  
> ACTION: | Place 1 of your plot cards in play with any other player's control token on it. Then, you may place any player's control token on 1 of your in-play plot cards; one plot cannot have two of the same player's tokens. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `puppet-control`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Emissary Taivra** (`ghostagent`, ghost)  
> After a player activates a system that contains a non-delta wormhole: | You may exhaust this card: if you do, that system is adjacent to all other systems that contain a wormhole during this tactical action. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `adjacency-grant`, `wormhole`

**Sai Seravus** (`ghostcommander`, ghost)  
> After your ships move: | For each ship that has a capacity value and moved through 1 or more wormholes, you may place 1 fighter from your reinforcements with that ship if you have unused capacity in the active system. | UNLOCK: Have units in 3 systems that contain alpha or beta wormholes.  
Tags: `timing-reaction`, `wormhole`, `place-units-free`, `leader-unlock-condition`

**Riftwalker Meian** (`ghosthero`, ghost)  
> ACTION: | Swap the positions of any 2 non-Fracture systems that contain wormholes or your units, other than the Creuss system, Ahk Creuxx system, or the Wormhole Nexus. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `relocate-units`, `wormhole`, `leader-unlock-condition`

**"Who Knows?"** (`redcreussagent`, ghost)  
> ACTION: | Exhaust this card to choose 1 player. That player may swap the position of 2 of their ships in any systems; they may transport units when they swap. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `relocate-units`

**"Total Mystery"** (`redcreusscommander`, ghost)  
> At the end of a combat between any players: | Gain 1 commodity or convert 1 of your commodities to a trade good. | UNLOCK: Resolve a combat with another player  
Tags: `post-combat-trigger`, `gain-commodities`, `convert-commodities`, `leader-unlock-condition`

**"A Tall Stranger"** (`redcreusshero`, ghost)  
> When you produce ships | You may place any of those ships onto this card. At the start of a space combat, you may purge this card to place all ships from this card into the active system. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `place-units-free`, `timing-combat`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Carth of Golden Sands** (`hacanagent`, hacan)  
> During the action phase: | You may exhaust this card to gain 2 commodities or replenish another player's commodities. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `gain-commodities` (2)

**Gila the Silvertongue** (`hacancommander`, hacan)  
> When you cast votes: | You may spend any number of trade goods: cast 2 additional votes for each trade good spent. | UNLOCK: Have 10 trade goods.  
Tags: `timing-reaction`, `spend-trade-goods`, `vote-bonus` (2), `leader-unlock-condition`

**Harrugh Gefhara** (`hacanhero`, hacan)  
> When 1 or more of your units use PRODUCTION: | You may reduce the cost of each of your units to 0 during this use of PRODUCTION. If you do, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `cost-reduction`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Doctor Sucaban** (`jolnaragent`, jolnar)  
> When a player spends resources to research: | You may exhaust this card to allow that player to remove any number of their infantry from the game board. For each unit removed, reduce the resources spent by 1. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `cost-reduction` (1)

**Agnlan Oln** (`jolnarcommander`, jolnar)  
> After you roll dice for a unit ability: | You may reroll any of those dice. | UNLOCK: Own 8 technologies.  
Tags: `timing-reaction`, `reroll`, `leader-unlock-condition`

**Rin, the Master's Legacy** (`jolnarhero`, jolnar)  
> ACTION: | For each non-unit upgrade technology you own, you may replace that technology with any technology of the same color from the deck. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `tech-specialty-use`, `hero-one-shot`, `purge`, `leader-unlock-condition`, `research-free`

**Xander Alexin Victori III** (`keleresagent`, keleres)  
> At any time: | You may exhaust this card to allow any player to spend commodities as if they were trade goods. | UNLOCK: Always Unlocked  
Tags: `timing-passive`, `exhaust`, `trade-goods-as-resources`

**Suffi An** (`kelerescommander`, keleres)  
> After you perform a component action: | You may perform an additional action. | UNLOCK: Spend 1 trade good after you play an action card that has a component action  
Tags: `timing-reaction`, `extra-activation`, `leader-unlock-condition`

**Harka Leeds** (`keleresheroharka`, keleres)  
> ACTION: | Reveal cards from the action card deck until you reveal 3 action cards that have component actions. Draw those cards and shuffle the rest back into the action card deck. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `draw-action-card` (3), `hero-one-shot`, `purge`, `leader-unlock-condition`

**Kuuasi Aun Jalatai** (`keleresherokuuasi`, keleres)  
> At the start of a round of of space combat in a system that contains a planet you control: | Place your flagship and up to a total of 2 cruisers and/or destroyers from your reinforcements in the active system. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-combat`, `requires-planets-controlled`, `place-units-free`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Odlynn Myrr** (`keleresheroodlynn`, keleres)  
> After an agenda is revealed: | You may cast up to 6 additional votes on this agenda. Predict aloud an outcome of this agenda. For each player that abstains or votes for another outcome, gain 1 trade good and 1 command token. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `vote-bonus` (6), `agenda-predict`, `gain-trade-goods` (1), `gain-command-token` (1), `hero-one-shot`, `purge`, `leader-unlock-condition`

**I48S** (`l1z1xagent`, l1z1x)  
> After a player activates a system: | You may exhaust this card to allow that player to replace 1 of their infantry in the active system with 1 mech from their reinforcements. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `unit-replacement`, `mech-effect`

**2RAM** (`l1z1xcommander`, l1z1x)  
> At any time: | Units that have PLANETARY SHIELD do not prevent you from using BOMBARDMENT. | UNLOCK: Have 4 dreadnoughts on the game board.  
Tags: `timing-passive`, `ignore-planetary-shield`, `leader-unlock-condition`

**The Helmsman** (`l1z1xhero`, l1z1x)  
> ACTION: | Choose 1 system that does not contain other players' ships. you may move your flagship and any number of your dreadnoughts from other systems into the chosen system. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `relocate-units`, `hero-one-shot`, `purge`, `leader-unlock-condition`

**Viscount Unlenn** (`letnevagent`, letnev)  
> At the start of a space combat round: | You may exhaust this card to choose 1 ship in the active system: that ship rolls 1 additional die during this combat round. | UNLOCK: Always Unlocked  
Tags: `timing-combat`, `exhaust`, `extra-dice` (1)

**Rear Admiral Farran** (`letnevcommander`, letnev)  
> After 1 of your units uses SUSTAIN DAMAGE: | You may gain 1 trade good. | UNLOCK: Have 5 non-fighter ships in 1 system.  
Tags: `timing-reaction`, `gain-trade-goods` (1), `leader-unlock-condition`

**Darktalon Treilla** (`letnevhero`, letnev)  
> ACTION: | Place this card near the game board. the number of non-fighter ships you can have in systems is not limited by laws or by the number of command tokens in your fleet pool during this game round. At the end of that game round, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `fleet-limit-bonus`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Jae Mir Kan** (`mahactagent`, mahact)  
> When you would spend a command token during the secondary ability of a strategic action: | You may exhaust this card to remove 1 of the active player's command tokens from the board and use it instead. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `remove-command-token`

**Il Na Viroset** (`mahactcommander`, mahact)  
> During your tactical actions: | you can activate systems that contain your command tokens. If you do, return both command tokens to your reinforcements and end your turn. | UNLOCK: Have 2 other factions' command tokens in your fleet pool.  
Tags: `timing-passive`, `extra-activation`, `remove-command-token`, `end-turn-skip`, `leader-unlock-condition`, `movement-exemption`

**Airo Shir Aur** (`mahacthero`, mahact)  
> ACTION: | Move all units in the space area of any system to an adjacent system that contains a different player's ships. Space combat is resolved in that system. neither player can retreat or resolve abilities that would move their ships. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `relocate-units`, `timing-combat`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Suffi An** (`mentakagent`, mentak)  
> After the Pillage faction ability is used against another player: | You may exhaust this card: if you do, you and that player each draw 1 action card. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `draw-action-card` (1)

**S'ula Mentarion** (`mentakcommander`, mentak)  
> After you win a space combat: | You may force your opponent to give you 1 promissory note from their hand. | UNLOCK: Have 4 cruisers on the game board.  
Tags: `post-combat-trigger`, `gain-promissory`, `leader-unlock-condition`

**Ipswitch, Loose Cannon** (`mentakhero`, mentak)  
> At the start of a space combat that you are participating in: | You may purge this card. if you do, for each other player's ship that is destroyed during this combat, place 1 ship of that type from your reinforcements in the active system. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-combat`, `purge`, `hero-one-shot`, `place-units-free`, `leader-unlock-condition`

**Umbat** (`muaatagent`, muaat)  
> ACTION: | Exhaust this card to choose a player: that player may produce up to 2 units that each have a cost of 4 or less in a system that contains one of their war suns or their flagship. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `produce-units`, `requires-units-present`

**Magmus** (`muaatcommander`, muaat)  
> After you spend a token from your strategy pool: | You may gain 1 trade good. | UNLOCK: Produce a war sun.  
Tags: `timing-reaction`, `strategy-pool-use`, `gain-trade-goods` (1), `leader-unlock-condition`

**Adjudicator Ba'al** (`muaathero`, muaat)  
> After you move a war sun into a non-home system other than Mecatol Rex: | You may destroy all other players' units in that system and replace that system tile with the Muaat supernova tile. If you do, purge this card and each planet card that corresponds to the replaced system tile. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `destroy-units`, `game-structure-change`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Z'eu** (`naaluagent`, naalu)  
> ACTION: | Exhaust this card and choose a player. That player may perform a tactical action in a non-home system without placing a command token. That system still counts as being activated. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `extra-activation`, `no-token-cost`

**Z'eu** (`naaluagent-te`, naalu)  
> After any player's command token is placed in a system: | You may exhaust this card to return that token to that player's reinforcements. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `remove-command-token`

**M'aban** (`naalucommander`, naalu)  
> At any time: | You may look at your neighbors' hands of promissory notes and the top and bottom card of the agenda deck. | UNLOCK: Have ground forces in or adjacent to the Mecatol Rex system.  
Tags: `timing-passive`, `look-at-hidden-info`, `leader-unlock-condition`

**The Oracle** (`naaluhero`, naalu)  
> At the end of the status phase: | You may force each other player to give you 1 promissory note from their hand. If you do, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-end-turn`, `gain-promissory`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Garv and Gunn** (`naazagent`, naaz)  
> At the end of a player's turn: | You may exhaust this card to allow that player to explore 1 of their planets. | UNLOCK: Always Unlocked  
Tags: `timing-end-turn`, `exhaust`, `explore`

**Dart and Tai** (`naazcommander`, naaz)  
> After you gain control of a planet that was controlled by another player: | You may explore that planet. | UNLOCK: Have mechs in 3 systems.  
Tags: `timing-reaction`, `explore`, `leader-unlock-condition`

**Hesh and Prit** (`naazhero`, naaz)  
> ACTION: | Gain 1 relic and perform the secondary ability of up to 2 readied or unchosen strategy cards. during this action, spend command tokens from your reinforcements instead of your strategy pool. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `gain-relic`, `strategy-card-manipulation`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Nekro Malleon** (`nekroagent`, nekro)  
> During the action phase: | You may exhaust this card to choose a player: that player may discard 1 action card or spend 1 command token from their command sheet to gain 2 trade goods. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `gain-trade-goods` (2)

**Nekro Acidos** (`nekrocommander`, nekro)  
> After you gain a technology: | You may draw 1 action card. | UNLOCK: Own 3 technologies. A "Valefar Assimilator" technology counts only if its X or Y token is on a technology.  
Tags: `timing-reaction`, `draw-action-card` (1), `leader-unlock-condition`

**UNIT.DSGN.FLAYESH** (`nekrohero`, nekro)  
> ACTION: | Choose a planet that has a technology specialty in a system that contains your units. Destroy any other player's units on that planet. Gain trade goods equal to the planet's combined resource and influence values and gain 1 technology that matches the specialty of that planet. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `destroy-units`, `gain-trade-goods`, `research-free`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Artuno the Betrayer** (`nomadagentartuno`, nomad)  
> When you gain trade goods from the supply: | You may exhaust this card to place an equal number of trade goods on this card. When this card readies, gain the trade goods on this card. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `gain-trade-goods`, `ready-card`

**Field Marshal Mercer** (`nomadagentmercer`, nomad)  
> At the end of a player's turn: | You may exhaust this card to allow that player to remove up to 2 of their ground forces from the game board and place them on planets they control in the active system. | UNLOCK: Always Unlocked  
Tags: `timing-end-turn`, `exhaust`, `relocate-units`

**The Thundarian** (`nomadagentthundarian`, nomad)  
> After the "Roll Dice" step of combat: | You may exhaust this card. If you do, hits are not assigned to either player's units. Return to the start of this combat round's "Roll Dice" step. | UNLOCK: Always Unlocked  
Tags: `timing-combat`, `exhaust`, `reroll`

**Navarch Feng** (`nomadcommander`, nomad)  
> When you produce: | You can produce your flagship without spending resources. | UNLOCK: Have 1 scored secret objective.  
Tags: `timing-reaction`, `produce-units`, `leader-unlock-condition`

**Ahk-Syl Siven** (`nomadhero`, nomad)  
> ACTION: | Place this card near the game board. your flagship and units it transports can move out of systems that contain your command tokens during this game round. At the end of that game round, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `movement-exemption`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Vos Hollow** (`obsidianagent`, obsidian)  
> When a player's ship is destroyed during any combat | You may exhaust this card; if you do, that player's opponent must destroy 1 of their ships of the same type in the active system. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `destroy-units` (1)

**Aroz Hollow** (`obsidiancommander`, obsidian)  
> At any time | Apply +1 to the result of each of your units' combat rolls in The Fracture. | UNLOCK: Have units in The Fracture.  
Tags: `timing-passive`, `combat-roll-bonus` (1), `leader-unlock-condition`

**Sharsiss Hollow** (`obsidianhero`, obsidian)  
> ACTION: | Ready all of your planets. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `ready-planets`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Kan Kip Rel** (`ralnelagent`, ralnel)  
> ACTION: | Exhaust this card and draw 2 action cards; give 1 of those cards to another player. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `draw-action-card` (2)

**Watchful Ojz** (`ralnelcommander`, ralnel)  
> When you declare a retreat | Immediately retreat up to 2 of your ships from the active system to an adjacent system that does not contain another player's ships. Place a command token from your reinforcements into that system. | UNLOCK: Be the last person to pass during the Action Phase  
Tags: `timing-reaction`, `retreat-control`, `place-command-token`, `leader-unlock-condition`

**Director Nel** (`ralnelhero`, ralnel)  
> After the last player passes | You may choose to no longer be passed; if you do, gain 2 command tokens, draw 1 action card, and purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `gain-command-token` (2), `draw-action-card` (1), `purge`, `hero-one-shot`, `leader-unlock-condition`

**Captain Mendosa** (`saaragent`, saar)  
> When a player activates a system: | You may exhaust this card to increase the move value of 1 of that player's ships to match the move value of the ship on the game board that has the highest move value. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `move-bonus`

**Rowl Sarring** (`saarcommander`, saar)  
> When you produce fighters or infantry: | You may place each of those units at any of your space docks that are not blockaded. | UNLOCK: Have 3 space docks on the game board.  
Tags: `timing-reaction`, `place-units-free`, `leader-unlock-condition`

**Gurno Aggero** (`saarhero`, saar)  
> ACTION: | Choose 1 system that is adjacent to 1 of your space docks. Destroy all other players' infantry and fighters in that system. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `destroy-units`, `requires-adjacent-units`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**T'ro** (`sardakkagent`, sardakk)  
> At the end of a player's tactical action: | You may exhaust this card: if you do, that player may place 2 infantry from their reinforcements on a planet they control in the active system. | UNLOCK: Always Unlocked  
Tags: `timing-end-turn`, `exhaust`, `place-units-free`

**G'hom Sek'kus** (`sardakkcommander`, sardakk)  
> During the "Commit Ground Forces" step: | You can commit (move) up to 1 ground force from each planet in the active system and each planet in adjacent systems that do not contain 1 of your command tokens. | UNLOCK: Control 5 planets in non-home systems.  
Tags: `relocate-units`, `leader-unlock-condition`

**Sh'val, Harbinger** (`sardakkhero`, sardakk)  
> After you move ships into the active system: | You may skip directly to the "Commit Ground Forces" step. If you do, after you commit ground forces to land on planets, purge this card and return each of your ships in the active system to your reinforcements. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Evelyn DeLouis** (`solagent`, sol)  
> At the start of a ground combat round: | You may exhaust this card to choose 1 ground force in the active system: that ground force rolls 1 additional die during this combat round. | UNLOCK: Always Unlocked  
Tags: `timing-combat`, `exhaust`, `extra-dice` (1)

**Claire Gibson** (`solcommander`, sol)  
> At the start of a ground combat on a planet you control: | You may place 1 infantry from your reinforcements on that planet. | UNLOCK: Control planets that have a combined total of at least 12 resources.  
Tags: `timing-combat`, `place-units-free`, `leader-unlock-condition`

**Jace X, 4th Air Legion** (`solhero`, sol)  
> ACTION: | Remove each of your command tokens from the game board and return them to your reinforcements. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `remove-command-token`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Tellurian** (`titansagent`, titans)  
> Before a hit would be assigned: | You may exhaust this card to cancel that hit. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `cancel-hit`

**Tungstantus** (`titanscommander`, titans)  
> When 1 or more of your units use PRODUCTION: | You may gain 1 trade good. | UNLOCK: Have 5 structures on the game board.  
Tags: `timing-reaction`, `gain-trade-goods` (1), `leader-unlock-condition`

**Ul the Progenitor** (`titanshero`, titans)  
> ACTION: | Ready Elysium and attach this card to it. Its resource and influence values are each increased by 3, and it gains the SPACE CANNON 5(x3) ability as if it were a unit. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `ready-planets`, `gain-attachment`, `resources-bonus` (3), `space-cannon`, `leader-unlock-condition`

**Berekar Berekon** (`winnuagent`, winnu)  
> When 1 or more of a player's units use PRODUCTION: | You may exhaust this card to reduce the combined cost of the produced units by 2. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `cost-reduction` (2)

**Rickar Rickani** (`winnucommander`, winnu)  
> During combat: | Apply +2 to the result of each of your unit's combat rolls in the Mecatol Rex system, your home system, and each system that contains a legendary planet. | UNLOCK: Control Mecatol Rex or enter into a combat in the Mecatol Rex system.  
Tags: `timing-combat`, `combat-roll-bonus` (2), `requires-home-system`, `leader-unlock-condition`

**Mathis Mathinus** (`winnuhero`, winnu)  
> ACTION: | Perform the primary ability of any strategy card. Then, choose any number of other players. Those players may perform the secondary ability of that strategy card. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `strategy-card-manipulation`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Ggrocuto Rinn** (`xxchaagent`, xxcha)  
> ACTION: | Exhaust this card to ready any planet; if that planet is in a system that is adjacent to a planet you control, you may remove 1 infantry from that planet and return it to its reinforcements. | UNLOCK: Always Unlocked  
Tags: `timing-action`, `exhaust`, `ready-planets`, `requires-planets-controlled`

**Elder Qanoj** (`xxchacommander`, xxcha)  
> When you vote: | Each planet you exhaust to cast votes provides 1 additional vote. Game effects cannot prevent you from voting on an agenda. | UNLOCK: Control planets that have a combined total of at least 12 influence.  
Tags: `timing-reaction`, `vote-bonus` (1), `exhaust-planets`, `leader-unlock-condition`

**Xxekir Grom** (`xxchahero`, xxcha)  
> When you exhaust planets: | combine the values of their resources and influence. Treat the combined value as if it were both resources and influence. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-reaction`, `influence-as-resources`, `leader-unlock-condition`

**Xxekir Grom** (`xxchahero-te`, xxcha)  
> ACTION: | Place any combination of up to 4 PDS or mechs onto planets you control; ready each planet that you place a unit on. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `place-units-free`, `ready-planets`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Brother Milor** (`yinagent`, yin)  
> After a player's unit is destroyed during combat: | You may exhaust this card to allow that player to place 2 fighters in the destroyed unit's system if it was a ship, or 2 infantry on its planet if it was a ground force. | UNLOCK: Always Unlocked  
Tags: `timing-reaction`, `exhaust`, `place-units-free`

**Brother Omar** (`yincommander`, yin)  
> At any time: | This card satisfies a green technology prerequisite. When you research a tech owned by another player, you may return 1 of your infantry to reinforcements to ignore its prerequisites. | UNLOCK: Use one of your faction abilities.  
Tags: `timing-passive`, `prerequisite-skip`, `timing-reaction`, `leader-unlock-condition`

**Dannel of the Tenth** (`yinhero`, yin)  
> ACTION: | Commit up to 3 infantry from your reinforcements to any non-home planets and resolve ground combats on those planets. Players cannot use SPACE CANNON against these units. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `place-units-free`, `timing-combat`, `space-cannon-immunity`, `purge`, `hero-one-shot`, `leader-unlock-condition`

**Ssruu** (`yssarilagent`, yssaril)  
> At any time: | This card has the text ability of each other player's agent, even if that agent is exhausted. | UNLOCK: Always Unlocked  
Tags: `share-ability`, `timing-passive`

**So Ata** (`yssarilcommander`, yssaril)  
> After another player activates a system that contains your units: | You may look at that player's action cards, promissory notes, or secret objectives. | UNLOCK: Have 7 action cards.  
Tags: `timing-reaction`, `look-at-hidden-info`, `leader-unlock-condition`

**Kyver, Blade and Key** (`yssarilhero`, yssaril)  
> ACTION: | Each other player shows you 1 action card from their hand. For each player, you may either take that card or force that player to discard 3 random action cards from their hand. Then, purge this card. | UNLOCK: Have 3 scored objectives.  
Tags: `timing-action`, `look-at-hidden-info`, `discard-opponent-card`, `purge`, `hero-one-shot`, `leader-unlock-condition`

## breakthrough

**Psychospore** (`arborecbt`, arborec)  
> ACTION: Exhaust this card to remove a command token from a system that contains 1 or more of your infantry and return it to your reinforcements. Then, place 1 infantry in that system.  
Tags: `timing-action`, `exhaust`, `remove-command-token`, `place-units-free`

**Wing Transfer** (`argentbt`, argent)  
> When you activate a system that contains only your units, you may place command tokens from your reinforcements into any systems adjacent to that system that contain only your units; at the end of this action, you may move ships among the active system and systems adjacent to it that contain your command tokens.  
Tags: `timing-reaction`, `place-command-token`, `relocate-units`, `timing-end-turn`

**The Icon** (`bastionbt`, bastion)  
> When you produce ships, you may exhaust this card to place those ships in a system that contains 1 of your command tokens, at least 1 of your ground forces, and no other player's ships.  
Tags: `timing-reaction`, `exhaust`, `place-units-free`, `requires-units-present`

**Al'Raith Ix Ianovar** (`cabalbt`, cabal)  
> This breakthrough causes The Fracture to enter play without a roll, if it is not already in play. After this card enters play, move up to 2 ingress tokens into systems that contain gravity rifts. Apply +1 to the MOVE value of each of your ships that start their movement in The Fracture.  
Tags: `game-structure-change`, `relocate-units`, `move-bonus` (1), `timing-reaction`

**Resonance Generator** (`crimsonbt`, crimson)  
> During your tactical actions, apply +1 to the move value of each of your ships that starts its movement in your home system or in a system that contains an active breach. ACTION: Exhaust this card to flip any breach or to place an active breach in a non-home system that contains your units.  
Tags: `timing-action`, `move-bonus` (1), `requires-home-system`, `exhaust`, `game-structure-change`

**Visionaria Select** (`deepwroughtbt`, deepwrought)  
> ACTION: Exhaust this card to allow each other player to spend 3 trade goods and give you 1 promissory note. Each player that does may research a non-faction, non-unit upgrade technology. You also gain each technology researched in this way.  
Tags: `timing-action`, `exhaust`, `spend-trade-goods` (3), `gain-promissory`, `research-free`

**Void Tether** (`empyreanbt`, empyrean)  
> When you activate a system that contains or is adjacent to a unit or planet you control, you may place or move 1 of your Void Tether tokens onto a border that system shares with another system; other players do not treat those systems as adjacent to each other unless you allow it.  
Tags: `timing-reaction`, `adjacency-grant`

**The Sowing** (`firmamentbt`, firmament)  
> When you gain this card and at the start of the status phase, you may place up to 3 of your trade goods on this card. Flip this card if you become The Obsidian faction.  
Tags: `timing-status-phase`, `game-structure-change`

**Particle Synthesis** (`ghostbt`, ghost)  
> Each wormhole in a system that contains your ships gains PRODUCTION 1 as if it were a unit you control. Reduce the combined cost of units you produce in systems that contain wormholes by 1 for each wormhole in that system.  
Tags: `production-unit`, `wormhole`, `cost-reduction` (1)

**Auto-Factories** (`hacanbt`, hacan)  
> When you produce 3 or more non-fighter ships, place 1 command token from your reinforcements into your fleet pool.  
Tags: `timing-reaction`, `gain-command-token` (1)

**Specialist Compounds** (`jolnarbt`, jolnar)  
> When you research technology using the 'Technology' strategy card, you may exhaust a planet that has a technology specialty instead of spending resources; if you do, you must research a technology of that color.  
Tags: `timing-reaction`, `exhaust-planets`, `tech-specialty-use`

**I.I.H.Q. Modernization** (`keleresbt`, keleres)  
> When you gain this card, gain the Custodia Vigilia planet card and its legendary planet ability card. You are neighbors with all players that have units or control planets in or adjacent to the Mecatol Rex system.  
Tags: `timing-reaction`, `gain-planet`, `requires-neighbor`

**Fealty Uplink** (`l1z1xbt`, l1z1x)  
> When you gain control of a planet, place infantry from your reinforcements equal to that planet's influence value on that planet.  
Tags: `timing-reaction`, `place-units-free`

**Gravleash Maneuvers** (`letnevbt`, letnev)  
> Before you roll dice during space combat, apply +X to the results of 1 of your ship's rolls, where X is the number of ship types you have in the combat. During movement, your non-fighter ships' move values are equal to the highest move value amongst moving ships in the system they started in.  
Tags: `combat-roll-bonus`, `timing-combat`, `move-bonus`

**Vaults of the Heir** (`mahactbt`, mahact)  
> ACTION: Exhaust this card and purge 1 of your technologies to gain 1 relic.  
Tags: `timing-action`, `exhaust`, `purge`, `gain-relic`

**The Table's Grace** (`mentakbt`, mentak)  
> If you have the Cruiser II unit upgrade technology, flip this card and place it on top of cruiser II. Corsair [Cost 2, Combat 6, Move 3, Capacity 2] If the active system contains another player's non-fighter ships, this unit can move through systems that contain other players' ships.  
Tags: `move-through-ships`, `requires-units-present`

**Stellar Genesis** (`muaatbt`, muaat)  
> When you gain this card, place the Avernus planet token into a non-home system that is adjacent to a planet you control; gain the Avernus planet card and ready it. After you move one of your war suns out of or through Avernus's system and into a non-home system, you may move the Avernus token with it.  
Tags: `timing-reaction`, `gain-planet`, `ready-card`, `relocate-units`, `requires-planets-controlled`

**Mindsieve** (`naalubt`, naalu)  
> When you would resolve the secondary ability of another player's strategy card, you may give them a promissory note to resolve it without spending a command token.  
Tags: `timing-reaction`, `no-token-cost`, `gain-promissory`

**Absolute Synergy** (`naazbt`, naaz)  
> When you have 4 mechs in the same system, you may return 3 of those mechs to your reinforcements to flip this card and place it on top of your mech card. Eidolon Maximum: [Combat 4 (x4), Move 3, Sustain Damage] This unit is both a ship and ground force. It cannot be assigned hits from unit abilities. Repair it at the start of every combat round. Game effects cannot place or produce your mechs. When this unit is destroyed or removed, flip this card and return it to your play area.  
Tags: `requires-units-present`, `ship-effect`, `assign-hits-control`, `repair-units`, `sustain-damage`

**Valefar Assimilator Z** (`nekrobt`, nekro)  
> When you would gain another player's technology using one of your faction abilities, you may instead place one of your "Z" assimilator tokens on that player's faction sheet. Your flagship gains the text abilities of that faction's flagship in addition to its own.  
Tags: `tech-theft`, `timing-reaction`, `share-ability`

**Thunder's Paradox** (`nomadbt`, nomad)  
> At the start of any player's turn, you may exhaust 1 of your agents to ready any other agent.  
Tags: `timing-start-turn`, `exhaust`, `ready-card`

**The Reaping** (`obsidianbt`, obsidian)  
> Place 1 trade good from the supply onto this card each time you win a combat against a puppeted player. At the start of the status phase, gain all trade goods on this card, then gain an equal number of trade goods from the supply.  
Tags: `timing-status-phase`, `gain-trade-goods`, `post-combat-trigger`, `puppet-control`

**Data Skimmer** (`ralnelbt`, ralnel)  
> During the action phase, if you have not passed, when other players would discard action cards, they are placed on this card instead. When you pass, take 1 action card from this card and discard the rest.  
Tags: `discard-opponent-card`, `timing-reaction`

**Deorbit Barrage** (`saarbt`, saar)  
> ACTION: Exhaust this card and spend any amount of resources to choose a planet up to 2 systems away from an asteroid field that contains your ships; roll a number of dice equal to the amount spent and assign 1 hit to a ground force on that planet for each roll of 4 or greater.  
Tags: `timing-action`, `exhaust`, `destroy-units`, `ground-force-effect`

**N'orr Supremacy** (`sardakkbt`, sardakk)  
> After you win a combat, either gain 1 command token or research a unit upgrade technology.  
Tags: `post-combat-trigger`, `gain-command-token` (1), `research-free`

**Bellum Gloriosum** (`solbt`, sol)  
> When you produce a ship that has capacity, you may also produce any combination of ground forces or fighters up to that ship's capacity; they do not count against your PRODUCTION limit.  
Tags: `timing-reaction`, `produce-units`, `production-bonus`

**Slumberstate Computing** (`titansbt`, titans)  
> When COALESCENCE results in a ground combat, if you commit no other units, you may choose for your units to coexist instead. During the status phase, for each player you are coexisting with, you and that player each draw 1 additional action card.  Other players may allow you to place a sleeper token on a planet they control.  
Tags: `combat-avoidance`, `timing-status-phase`, `draw-action-card` (1), `gain-attachment`

**Imperator** (`winnubt`, winnu)  
> Apply +1 to the results of each of your unit's combat rolls for each "Support for the Throne" in your opponent's play area. After you activate a system that contains a legendary planet, apply +1 to the move value of 1 of your ships during this tactical action.  
Tags: `combat-roll-bonus` (1), `move-bonus` (1), `timing-reaction`

**Archon's Gift** (`xxchabt`, xxcha)  
> You can spend influence as if it were resources. You can spend resources as if it were influence.  
Tags: `influence-as-resources`

**Yin Ascendant** (`yinbt`, yin)  
> When you gain this card or score a public objective, gain the alliance ability of a random, unused faction.  
Tags: `timing-reaction`, `share-ability`

**Deepgloom Executable** (`yssarilbt`, yssaril)  
> You can allow other players to use your STALL TACTICS or SCHEMING faction abilities; when you do, you may resolve a transaction with that player. During the action phase, that transaction does not count against the once-per-player transaction limit for that turn.  
Tags: `share-ability`, `transaction`, `timing-passive`

## promissory

**Alliance** (`<color>_an`)  
> When you receive this card, if you are not the <color> player, you must place it faceup in your play area. While this card is in your play area, you can use the <color> player's commander ability, if it is unlocked. When you activate a system that contains 1 or more of the <color> player's units, return this card to the <color> player.  
Tags: `share-ability`, `timing-passive`, `deal-enforcement`

**Ceasefire** (`<color>_cf`)  
> After the <color> player activates a system that contains 1 or more of your units: The <color> player cannot move units to the active system. Then return this card to the <color> player.  
Tags: `restrict-movement`, `timing-reaction`, `deal-enforcement`

**Political Secret** (`<color>_ps`)  
> When an agenda is revealed: The <color> player cannot vote, play action cards, or use faction abilities until after that agenda has been resolved. Then, return this card to the <color> player.  
Tags: `vote-restrict`, `timing-reaction`, `deal-enforcement`, `action-card-restriction`

**Support for the Throne** (`<color>_sftt`)  
> When you receive this card, if you are not the <color> player, you must place it faceup in your play area and gain 1 victory point. When you activate a system that contains 1 or more of the <color> player's units, or if the <color> player is eliminated, lose 1 victory point and return this card to the <color> player.  
Tags: `gain-victory-point` (1), `timing-reaction`, `deal-enforcement`

**Trade Agreement** (`<color>_ta`)  
> When the <color> player replenishes commodities: The <color> player gives you all of their commodities. Then, return this card to the <color> player.  
Tags: `steal-trade-goods`, `timing-reaction`, `deal-enforcement`

**Stymie** (`stymie`, arborec)  
> After another player moves ships into a system that contains 1 or more of your units: You may place 1 command token from that player's reinforcements in any non-home system. Then, return this card to the Arborec player.  
Tags: `place-command-token`, `timing-reaction`, `deal-enforcement`

**Strike Wing Ambuscade** (`ambuscade`, argent)  
> When 1 or more of your units make a roll for a unit ability: Choose 1 of those units to roll 1 additional die. Then, return this card to the Argent player.  
Tags: `extra-dice` (1), `timing-reaction`, `deal-enforcement`

**Raise the Standard** (`raisethestandard`, bastion)  
> At the end of a combat: Galvanize 1 of your units that participated. Then, return this card to the Last Bastion player.  
Tags: `gain-attachment`, `post-combat-trigger`, `deal-enforcement`

**Crucible** (`crucible`, cabal)  
> After you activate a system: Your ships do not roll for gravity rifts during this movement, apply an additional +1 to the move values of your ships that would move out of or through a gravity rift instead. Then, return this card to the Vuil'raith player.  
Tags: `move-bonus` (1), `anomaly-movement`, `timing-reaction`, `deal-enforcement`

**Sever** (`sever`, crimson)  
> ACTION: place this card face up in your play area, and place the sever token in a system that contains your units, wormholes in that system have no effect during movement. Remove the sever token and return this card to the Rebellion player at end of the status phase.  
Tags: `wormhole`, `timing-action`, `deal-enforcement`

**Share Knowledge** (`shareknowledge`, deepwrought)  
> ACTION: Place this card faceup in your play area and gain 1 non-faction, non-unit upgrade technology that the Deepwrought player owns; place that technology on this card. Return that technology to the deck and this card to the Deepwrought player at the end of the status phase.  
Tags: `tech-theft`, `timing-action`, `deal-enforcement`

**Blood Pact** (`blood_pact`, empyrean)  
> ACTION: Place this card faceup in your play area. When you and the Empyrean player cast votes for the same outcome, cast 4 additional votes for that outcome. If you activate a system that contains 1 or more of the Empyrean player's units, return this card to the Empyrean player.  
Tags: `vote-bonus` (4), `timing-agenda-phase`, `timing-action`, `deal-enforcement`

**Dark Pact** (`dark_pact`, empyrean)  
> ACTION: Place this card faceup in your play area. When you give a number of commodities to the Empyrean player equal to your maximum commodity value, you each gain 1 trade good. If you activate a system that contains 1 or more of the Empyrean player's units, return this card to the Empyrean player.  
Tags: `gain-trade-goods` (1), `give-trade-goods`, `timing-action`, `deal-enforcement`

**Black Ops** (`blackops`, firmament)  
> When you receive this card, if you are not the Firmament: The Firmament player may place 1 facedown plot card in their play area with your control token on it. Then, gain 2 command tokens, gain 2 trade goods, and purge this card.  
Tags: `gain-command-token` (2), `gain-trade-goods` (2), `purge`, `timing-reaction`, `deal-enforcement`, `puppet-control`

**Creuss Iff** (`iff`, ghost)  
> At the start of your turn during the action phase: Place or move a Creuss wormhole token into either a system that contains a planet you control or a non-home system that does not contain another player's ships. Then, return this card to the Creuss player.  
Tags: `wormhole`, `timing-start-turn`, `deal-enforcement`

**Trade Convoys** (`convoys`, hacan)  
> ACTION: Place this card faceup in your play area. While this card is in your play area, you may negotiate transactions with players who are not your neighbor.  If you activate a system that contains 1 or more of the Hacan player's units, return this card to the Hacan player.  
Tags: `transaction`, `requires-neighbor`, `timing-action`, `deal-enforcement`

**Research Agreement** (`ra`, jolnar)  
> After the Jol-Nar player researches a technology that is not a faction technology: Gain that technology. Then, return this card to the Jol-Nar player.  
Tags: `tech-theft`, `timing-reaction`, `deal-enforcement`

**Keleres Rider** (`rider`, keleres)  
> After an agenda is revealed: You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, draw 1 action card and gain 2 trade goods. Then, return this card to the Keleres player.  
Tags: `vote-restrict`, `agenda-predict`, `draw-action-card` (1), `gain-trade-goods` (2), `timing-reaction`, `deal-enforcement`

**Keleres Rider** (`ridera`, keleresa)  
> After an agenda is revealed: You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, draw 1 action card and gain 2 trade goods. Then, return this card to the Keleres player.  
Tags: `vote-restrict`, `agenda-predict`, `draw-action-card` (1), `gain-trade-goods` (2), `timing-reaction`, `deal-enforcement`

**Keleres Rider** (`riderm`, keleresm)  
> After an agenda is revealed: You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, draw 1 action card and gain 2 trade goods. Then, return this card to the Keleres player.  
Tags: `vote-restrict`, `agenda-predict`, `draw-action-card` (1), `gain-trade-goods` (2), `timing-reaction`, `deal-enforcement`

**Keleres Rider** (`riderx`, keleresx)  
> After an agenda is revealed: You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, draw 1 action card and gain 2 trade goods. Then, return this card to the Keleres player.  
Tags: `vote-restrict`, `agenda-predict`, `draw-action-card` (1), `gain-trade-goods` (2), `timing-reaction`, `deal-enforcement`

**Cybernetic Enhancements** (`ce`, l1z1x)  
> When you gain command tokens during the status phase: Gain 1 additional command token. Then, return this card to the L1Z1X player.  
Tags: `gain-command-token` (1), `timing-status-phase`, `deal-enforcement`

**War Funding** (`war_funding`, letnev)  
> After you and your opponent roll dice during space combat: You may reroll all of your opponent's dice. You may reroll any number of your dice. Then, return this card to the Letnev player.  
Tags: `reroll`, `timing-reaction`, `deal-enforcement`

**Scepter of Dominion** (`scepter`, mahact)  
> At the start of the strategy phase: Choose 1 non-home system that contains your units, each other player who has a token on the Mahact player's command sheet places a token from their reinforcements in that system. Then, return this card to the Mahact player.  
Tags: `place-command-token`, `requires-units-present`, `timing-strategy-phase`, `deal-enforcement`

**Promise of Protection** (`pop`, mentak)  
> ACTION: Place this card faceup in your play area. While this card is in your play area, the Mentak player cannot use their Pillage faction ability against you. If you activate a system that contains 1 or more of the Mentak player's units, return this card to the Mentak player.  
Tags: `negate-ability`, `timing-action`, `deal-enforcement`

**Fires of the Gashlai** (`fires`, muaat)  
> ACTION: Remove 1 token from the Muaat player's fleet pool and return it to their reinforcements. Then, gain your war sun unit upgrade technology card.  Then, return this card to the Muaat Player.  
Tags: `research-free`, `remove-command-token`, `timing-action`, `deal-enforcement`

**Gift of Prescience** (`gift`, naalu)  
> At the end of the Strategy Phase: Place this card faceup in your play area and place the Naalu '0' token on your strategy card, you are the first in initiative order. The Naalu player cannot use their Telepathic faction ability during this game round. Return this card to the Naalu player at the end of the status phase.  
Tags: `initiative-change`, `negate-ability`, `timing-end-turn`, `deal-enforcement`

**Black Market Forgery** (`bmf`, naaz)  
> ACTION: Purge 2 of your relic fragments of the same type to gain 1 relic. Then, return this card to the Naaz-Rokha player.  
Tags: `gain-relic`, `timing-action`, `deal-enforcement`

**Antivirus** (`antivirus`, nekro)  
> At the start of a combat: Place this card faceup in your play area. While this card is in your play area, the Nekro player cannot use their Technological Singularity faction ability against you. If you activate a system that contains 1 or more of the Nekro player's units, return this card to the Nekro player.  
Tags: `negate-ability`, `timing-combat`, `deal-enforcement`

**The Cavalry** (`cavalry`, nomad)  
> At the start of a space combat against a player other than the Nomad: During this combat, treat 1 of your non-fighter ships as if it has the SUSTAIN DAMAGE ability, combat value, and ANTI-FIGHTER BARRAGE value of the Nomad's flagship. Return this card to the Nomad player at the end of the combat.  
Tags: `sustain-grant`, `anti-fighter-barrage`, `share-ability`, `timing-combat`, `deal-enforcement`

**Malevolency** (`malevolency`, obsidian)  
> At the end of one of your tactical actions: Spend 1 influence to give this card to one of your neighbors; you can use this ability even if you are the Obsidian player. At the end of the status phase, if you are not the Obsidian player, you must remove 1 command token from your fleet pool and return it to your reinforcements.  
Tags: `gain-promissory`, `requires-neighbor`, `remove-command-token`, `timing-end-turn`, `deal-enforcement`

**Nano-Link Permit** (`nanolink`, ralnel)  
> After you activate a system: You may move your structures from adjacent systems that do not contain your command tokens onto planets you control in the active system. Then, return this card to the Ral Nel player.  
Tags: `relocate-units`, `structure-effect`, `timing-reaction`, `deal-enforcement`

**Raghs Call** (`ragh`, saar)  
> After you commit 1 or more units to land on a planet: Remove all of the Saar player's ground forces from that planet and place them on a planet controlled by the Saar player. Then, return this card to the Saar player.  
Tags: `relocate-units`, `ground-force-effect`, `timing-reaction`, `deal-enforcement`

**Tekklar Legion** (`tekklar`, sardakk)  
> At the start of an invasion combat: Apply +1 to the result of each of your unit's combat rolls during this combat. If your opponent is the N'orr player, apply -1 to the result of each of their unit's combat rolls during this combat. Then, return this card to the N'orr player.  
Tags: `combat-roll-bonus` (1), `combat-roll-penalty` (1), `timing-combat`, `deal-enforcement`

**Military Support** (`ms`, sol)  
> At the start of the Sol player's turn: Remove 1 token from the Sol player's strategy pool, if able, and return it to their reinforcements. Then, you may place 2 infantry from your reinforcements on any planet you control. Then, return this card to the Sol player.  
Tags: `place-units-free`, `strategy-pool-use`, `timing-start-turn`, `deal-enforcement`

**Terraform** (`terraform`, titans)  
> ACTION: Attach this card to a non-home planet you control other than Mecatol Rex. Its resource and influence values are each increased by 1, and it is treated as having all 3 planet traits (cultural, hazardous, and industrial).  
Tags: `resources-bonus` (1), `influence-bonus` (1), `planetary-trait-bonus`, `timing-action`, `deal-enforcement`, `gain-attachment`

**Acquiescence** (`acq`, winnu)  
> When the Winnu player resolves a strategic action: You do not have to spend or place a command token to resolve the secondary ability of that strategy card. Then, return this card to the Winnu player.  
Tags: `no-token-cost`, `timing-reaction`, `deal-enforcement`

**Political Favor** (`favor`, xxcha)  
> When an agenda is revealed: Remove 1 token from the Xxcha player's strategy pool and return it to their reinforcements. Then, discard the revealed agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda instead. Then, return this card to the Xxcha player.  
Tags: `strategy-pool-use`, `agenda-cancel`, `agenda-manipulation`, `timing-reaction`, `deal-enforcement`

**Greyfire Mutagen** (`greyfire`, yin)  
> At the start of a ground combat against 2 or more ground forces that are not controlled by the Yin player: Replace 1 of your opponent's infantry with 1 infantry from your reinforcements. Then, return this card to the Yin player.  
Tags: `unit-replacement`, `timing-combat`, `deal-enforcement`

**Spy Net** (`spynet`, yssaril)  
> At the start of your turn: Look at the Yssaril player's hand of action cards. Choose 1 of those cards and add it to your hand. Then, return this card to the Yssaril player.  
Tags: `look-at-hidden-info`, `discard-opponent-card`, `timing-start-turn`, `deal-enforcement`

## action_card

**Ancient Burial Sites** (`abs`)  
> At the start of the agenda phase | Choose 1 player. Exhaust each cultural planet owned by that player.  
Tags: `exhaust-planets`, `timing-agenda-phase`, `planetary-trait-bonus`

**Archaeological Expedition** (`arch_expedition`)  
> Action | Reveal the top 3 cards of an exploration deck that matches a planet you control; gain any relic fragments that you reveal and discard the rest.  
Tags: `explore`, `gain-relic`, `requires-planets-controlled`, `timing-action`

**Assassinate Representative** (`assassin`)  
> After an agenda is revealed | Choose 1 player. That player cannot vote on this agenda.  
Tags: `vote-restrict`, `timing-reaction`

**Black Market Dealings** (`blackmarketdealing`)  
> When you are negotiating a transaction | You and the other player may include relics, action cards, and unscored secret objectives as part of the transaction. This card cannot be canceled.  
Tags: `transaction`, `timing-reaction`

**Blitz** (`blitz`)  
> At the start of an invasion | Each of your non-fighter ships in the active system that do not have BOMBARDMENT gain BOMBARDMENT 6 until the end of the invasion.  
Tags: `bombardment`, `ship-effect`, `timing-combat`

**Bribery** (`bribery`)  
> After the speaker votes on an agenda | Spend any number of trade goods. For each trade good spent, cast 1 additional vote for the outcome on which you voted.  
Tags: `spend-trade-goods`, `vote-bonus` (1), `timing-reaction`

**Brilliance** (`brilliance`)  
> Action | Ready 1 of your planets that has a technology specialty or choose 1 player to gain their breakthrough.  
Tags: `ready-planets`, `tech-specialty-use`, `timing-action`

**Bunker** (`bunker`)  
> At the start of an invasion | During this invasion, apply -4 to the result of each BOMBARDMENT roll against planets you control.  
Tags: `combat-roll-penalty` (4), `timing-combat`

**Confounding Legal Text** (`confounding`)  
> When another player is elected as the outcome of an agenda | You are the elected player instead.  
Tags: `elect-outcome`, `timing-reaction`

**Confusing Legal Text** (`confusing`)  
> When you are elected as the outcome of an agenda | Choose 1 player. That player is the elected player instead.  
Tags: `elect-outcome`, `timing-reaction`

**Construction Rider** (`const_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, place 1 space dock from your reinforcements on a planet you control.  
Tags: `vote-restrict`, `agenda-predict`, `place-structure`, `timing-reaction`

**Counterstroke** (`counterstroke`)  
> After another player activates a system that contains 1 of your command tokens | Return that command token to your tactic pool.  
Tags: `redistribute-tokens`, `timing-reaction`

**Coup d'Etat** (`coup`)  
> When another player would perform a strategic action | End that player's turn, the strategic action is not resolved and the strategy card is not exhausted.  
Tags: `negate-ability`, `timing-reaction`, `end-turn-skip`

**Courageous to the End** (`courageous`)  
> After 1 of your ships is destroyed during a space combat | Roll 2 dice. For each result equal to or greater than that ship's combat value, your opponent must choose and destroy 1 of their ships.  
Tags: `destroy-units` (1), `timing-reaction`

**Crash Landing** (`crashlanding`)  
> When your last ship in the active system is destroyed | Place 1 of your ground forces from the space area of the active system onto a planet in that system other than Mecatol Rex; if the planet contains other players' units, place your ground force into coexistence.  
Tags: `ground-force-effect`, `timing-reaction`

**Cripple Defenses** (`cripple`)  
> Action | Choose 1 planet. Destroy each PDS on that planet.  
Tags: `destroy-units`, `structure-effect`, `timing-action`

**Crisis** (`crisis`)  
> At the end of any players turn, if there are at least 2 players who have not passed | Skip the next player's turn.  
Tags: `end-turn-skip`, `timing-end-turn`

**Deadly Plot** (`deadly_plot`)  
> During the agenda phase when an outcome would be resolved | If you voted for or predicted another outcome, discard the agenda instead. The agenda is resolved with no effect and it is not replaced. Then, exhaust all of your planets.  
Tags: `agenda-cancel`, `exhaust-planets`, `timing-agenda-phase`

**Decoy Operation** (`decoy`)  
> After another player activates a system that contains 1 or more of your structures | Remove up to 2 of your ground forces from the game board and place them on a planet you control in the active system.  
Tags: `relocate-units`, `ground-force-effect`, `timing-reaction`

**Direct Hit** (`dh1`)  
> After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities | Destroy that ship.  
Tags: `destroy-units`, `timing-reaction`

**Direct Hit** (`dh2`)  
> After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities | Destroy that ship.  
Tags: `destroy-units`, `timing-reaction`

**Direct Hit** (`dh3`)  
> After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities | Destroy that ship.  
Tags: `destroy-units`, `timing-reaction`

**Direct Hit** (`dh4`)  
> After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities | Destroy that ship.  
Tags: `destroy-units`, `timing-reaction`

**Diplomacy Rider** (`diplo_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, choose 1 system that contains a planet you control. Each other player places a command token from their reinforcements in that system.  
Tags: `vote-restrict`, `agenda-predict`, `place-command-token`, `timing-reaction`

**Disable** (`disable`)  
> At the start of an invasion in a system that contains 1 or more of your opponents' PDS units | Your opponents' PDS units lose PLANETARY SHIELD and SPACE CANNON during this invasion.  
Tags: `ignore-planetary-shield`, `negate-ability`, `requires-units-present`, `timing-combat`

**Public Disgrace** (`disgrace`)  
> When another player chooses a strategy card during the strategy phase | That player must choose a different strategy card instead, if able.  
Tags: `strategy-card-manipulation`, `timing-strategy-phase`

**Distinguished Councilor** (`distinguished`)  
> After you cast votes on an outcome of an agenda | Cast 5 additional votes for that outcome.  
Tags: `vote-bonus` (5), `timing-reaction`

**Divert Funding** (`divert_funding`)  
> Action | Return a non-unit upgrade, non-faction technology that you own to your technology deck. Then, research another technology.  
Tags: `research-free`, `timing-action`

**Diplomatic Pressure** (`dp1`)  
> When an agenda is revealed | Choose another player. That player must give you 1 promissory note from their hand.  
Tags: `gain-promissory`, `timing-reaction`

**Diplomatic Pressure** (`dp2`)  
> When an agenda is revealed | Choose another player. That player must give you 1 promissory note from their hand.  
Tags: `gain-promissory`, `timing-reaction`

**Diplomatic Pressure** (`dp3`)  
> When an agenda is revealed | Choose another player. That player must give you 1 promissory note from their hand.  
Tags: `gain-promissory`, `timing-reaction`

**Diplomatic Pressure** (`dp4`)  
> When an agenda is revealed | Choose another player. That player must give you 1 promissory note from their hand.  
Tags: `gain-promissory`, `timing-reaction`

**Economic Initiative** (`economic_initiative`)  
> Action | Ready each cultural planet you control.  
Tags: `ready-planets`, `timing-action`, `planetary-trait-bonus`

**Emergency Repairs** (`emergency`)  
> At the start or end of a combat round | Repair all of your units that have SUSTAIN DAMAGE in the active system.  
Tags: `repair-units`, `timing-combat`

**Exchange Program** (`exchangeprogram`)  
> Action | Choose another player. You and that player may agree to place 1 infantry from each of your reinforcements into coexistence on a planet the other player controls that contains their ground forces; if no agreement is reached, you each discard 1 token from your fleet pool.  
Tags: `place-units-free`, `ground-force-effect`, `deal-enforcement`, `remove-command-token`, `timing-action`

**Experimental Battlestation** (`experimental`)  
> After the active player moves ships into the active system during a tactical action | Choose 1 of your space docks that is either in or adjacent to that system. That space dock uses SPACE CANNON 5(x3) against the active player's ships in the active system.  
Tags: `space-cannon`, `structure-effect`, `timing-reaction`

**Extreme Duress** (`extremeduress`)  
> At the start of another player's turn, if they have a readied strategy card | If that player's next action is not a strategic action, they discard all of their action cards, give you all of their trade goods, and show you all of their secret objectives.  
Tags: `discard-opponent-card`, `steal-trade-goods`, `look-at-hidden-info`, `timing-start-turn`

**Fighter Conscription** (`f_conscription`)  
> Action | Place 1 fighter from your reinforcements in each system that contains 1 or more of your space docks or units that have capacity. They cannot be placed in systems that contain other players' ships.  
Tags: `fighter-effect`, `timing-action`

**Frontline Deployment** (`f_deployment`)  
> Action | Place 3 infantry from your reinforcements on 1 planet you control.  
Tags: `place-units-free`, `ground-force-effect`, `timing-action`

**Fighter Prototype** (`f_prototype`)  
> At the start of the first round of a space combat | Apply +2 to the result of each of your fighters' combat rolls during this combat round.  
Tags: `combat-roll-bonus` (2), `fighter-effect`, `timing-combat`

**Focused Research** (`f_researched`)  
> Action | Spend 4 trade goods to research 1 technology  
Tags: `spend-trade-goods` (4), `research-free`, `timing-action`

**Fire Team** (`fire_team`)  
> After your ground forces make combat rolls during a round of ground combat | Reroll any number of your dice.  
Tags: `reroll`, `timing-reaction`

**Flank Speed** (`fs1`)  
> After you activate a system | Apply +1 to the move value of each of your ships during this tactical action.  
Tags: `move-bonus` (1), `timing-reaction`

**Flank Speed** (`fs2`)  
> After you activate a system | Apply +1 to the move value of each of your ships during this tactical action.  
Tags: `move-bonus` (1), `timing-reaction`

**Flank Speed** (`fs3`)  
> After you activate a system | Apply +1 to the move value of each of your ships during this tactical action.  
Tags: `move-bonus` (1), `timing-reaction`

**Flank Speed** (`fs4`)  
> After you activate a system | Apply +1 to the move value of each of your ships during this tactical action.  
Tags: `move-bonus` (1), `timing-reaction`

**Forward Supply Base** (`fsb`)  
> After another player activates a system that contains your units | Gain 3 trade goods. Then, choose another player to gain 1 trade good.  
Tags: `gain-trade-goods` (3), `give-trade-goods`, `timing-reaction`

**Ghost Ship** (`ghost_ship`)  
> Action | Place 1 destroyer from your reinforcements in a non-home system that contains a wormhole and does not contain other players' ships.  
Tags: `place-units-free`, `ship-effect`, `wormhole`, `timing-action`

**Ghost Squad** (`ghost_squad`)  
> After another player commits units to land on a planet you control | Move any number of your ground forces from any planet you control in the active system to any other planet you control in the active system.  
Tags: `relocate-units`, `ground-force-effect`, `timing-reaction`

**Hack Election** (`hack`)  
> After an agenda is revealed | During this agenda, you vote last.  
Tags: `initiative-change`, `timing-reaction`

**Harness Energy** (`harness`)  
> After you activate an anomaly | Replenish your commodities.  
Tags: `gain-commodities`, `timing-reaction`

**Imperial Rider** (`imp_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, gain 1 victory point.  
Tags: `vote-restrict`, `agenda-predict`, `gain-victory-point` (1), `timing-reaction`

**Impersonation** (`impersonation`)  
> Action | Spend 3 influence to draw 1 secret objective.  
Tags: `gain-secret-objective`, `timing-action`

**Industrial Initiative** (`industrial_initiative`)  
> Action | Gain 1 trade good for each industrial planet you control.  
Tags: `gain-trade-goods`, `planetary-trait-bonus`, `timing-action`

**Infiltrate** (`infiltrate`)  
> When you gain control of a planet | Replace each PDS and space dock that is on that planet with a matching unit from your reinforcements.  
Tags: `unit-replacement`, `timing-reaction`

**Insider Information** (`insider`)  
> After an agenda is revealed | Look at the top 3 cards of the agenda deck.  
Tags: `agenda-manipulation`, `look-at-hidden-info`, `timing-reaction`

**Insubordination** (`insub`)  
> Action | Remove 1 token from another player's tactic pool and return it to their reinforcements.  
Tags: `remove-command-token`, `timing-action`

**Intercept** (`intercept`)  
> After your opponent declares a retreat during a space combat | Your opponent cannot retreat during this round of space combat.  
Tags: `retreat-control`, `timing-reaction`

**Manipulate Investments** (`investments`)  
> At the start of the strategy phase | Place a total of 5 trade goods from the supply on strategy cards of your choice. You must place these tokens on at least 3 different cards.  
Tags: `strategy-card-manipulation`, `timing-strategy-phase`

**Signal Jamming** (`jamming`)  
> Action | Choose 1 non-home system that contains or is adjacent to 1 of your ships. Place a command token from another player's reinforcements in that system.  
Tags: `place-command-token`, `requires-adjacent-units`, `timing-action`

**Leadership Rider** (`lead_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, gain 3 command tokens.  
Tags: `gain-command-token` (3), `vote-restrict`, `agenda-predict`, `timing-reaction`

**Lie in Wait** (`lieinwait`)  
> After 2 of your neighbors resolve a transaction | Look at each of those players' hands of action cards, then choose and take 1 action card from each.  
Tags: `look-at-hidden-info`, `discard-opponent-card`, `requires-neighbor`, `timing-reaction`

**Lost Star Chart** (`lost_star`)  
> After you activate a system | During this tactical action, systems that contain alpha and beta wormholes are adjacent to each other.  
Tags: `adjacency-grant`, `wormhole`, `timing-reaction`

**Lucky Shot** (`lucky`)  
> Action | Destroy 1 dreadnought, cruiser, or destroyer in a system that contains a planet you control.  
Tags: `destroy-units` (1), `ship-effect`, `requires-planets-controlled`, `timing-action`

**Master Plan** (`master_plan`)  
> After you perform an action | Perform an additional action.  
Tags: `extra-activation`, `timing-reaction`

**Morale Boost** (`mb1`)  
> At the start of a combat round | Apply +1 to the result of each of your unit's combat rolls during this combat round.  
Tags: `combat-roll-bonus` (1), `timing-combat`

**Morale Boost** (`mb2`)  
> At the start of a combat round | Apply +1 to the result of each of your unit's combat rolls during this combat round.  
Tags: `combat-roll-bonus` (1), `timing-combat`

**Morale Boost** (`mb3`)  
> At the start of a combat round | Apply +1 to the result of each of your unit's combat rolls during this combat round.  
Tags: `combat-roll-bonus` (1), `timing-combat`

**Morale Boost** (`mb4`)  
> At the start of a combat round | Apply +1 to the result of each of your unit's combat rolls during this combat round.  
Tags: `combat-roll-bonus` (1), `timing-combat`

**Reactor Meltdown** (`meltdown`)  
> Action | Destroy 1 space dock in a non-home system.  
Tags: `destroy-units` (1), `structure-effect`, `timing-action`

**Mercenary Contract** (`mercenarycontract`)  
> Action | Spend 2 trade goods to place 2 neutral infantry on any non-home planet that contains no units; if that planet was owned by another player, they return its planet card to the planet card deck.  
Tags: `spend-trade-goods` (2), `place-units-free`, `ground-force-effect`, `timing-action`

**Rise of a Messiah** (`messiah`)  
> Action | Place 1 infantry from your reinforcements on each planet you control.  
Tags: `place-units-free`, `ground-force-effect`, `timing-action`

**Mining Initiative** (`mining_initiative`)  
> Action | Gain trade goods equal to the resource value of 1 planet you control.  
Tags: `gain-trade-goods`, `timing-action`

**Maneuvering Jets** (`mjets1`)  
> Before you assign hits produced by another player's SPACE CANNON roll | Cancel 1 hit.  
Tags: `cancel-hit`, `space-cannon-immunity`, `timing-reaction`

**Maneuvering Jets** (`mjets2`)  
> Before you assign hits produced by another player's SPACE CANNON roll | Cancel 1 hit.  
Tags: `cancel-hit`, `space-cannon-immunity`, `timing-reaction`

**Maneuvering Jets** (`mjets3`)  
> Before you assign hits produced by another player's SPACE CANNON roll | Cancel 1 hit.  
Tags: `cancel-hit`, `space-cannon-immunity`, `timing-reaction`

**Maneuvering Jets** (`mjets4`)  
> Before you assign hits produced by another player's SPACE CANNON roll | Cancel 1 hit.  
Tags: `cancel-hit`, `space-cannon-immunity`, `timing-reaction`

**Nav Suite** (`nav_suite`)  
> After you activate a system | During the 'Movement' step of this tactical action, ignore the effect of anomalies.  
Tags: `anomaly-movement`, `timing-reaction`

**Overrule** (`overrule`)  
> Action | Perform the primary ability of a readied or unchosen strategy card.  
Tags: `strategy-card-manipulation`, `timing-action`

**Parley** (`parley`)  
> After another player commits units to land on a planet you control | Return the committed units to the space area.  
Tags: `protect-planet`, `timing-reaction`

**Pirate Contract** (`piratecontract1`)  
> Action | Place 1 neutral destroyer in a non-home system that contains no non-neutral ships.  
Tags: `ship-effect`, `place-units-free`, `timing-action`

**Pirate Contract** (`piratecontract2`)  
> Action | Place 1 neutral destroyer in a non-home system that contains no non-neutral ships.  
Tags: `ship-effect`, `place-units-free`, `timing-action`

**Pirate Contract** (`piratecontract3`)  
> Action | Place 1 neutral destroyer in a non-home system that contains no non-neutral ships.  
Tags: `ship-effect`, `place-units-free`, `timing-action`

**Pirate Contract** (`piratecontract4`)  
> Action | Place 1 neutral destroyer in a non-home system that contains no non-neutral ships.  
Tags: `ship-effect`, `place-units-free`, `timing-action`

**Pirate Fleet** (`piratefleet`)  
> Action | Spend 3 resources to place 1 neutral carrier, 1 neutral cruiser, 1 neutral destroyer, and 2 neutral fighters in a non-home system that contains no non-neutral ships.  
Tags: `ship-effect`, `place-units-free`, `timing-action`

**Plagiarize** (`plagiarize`)  
> Action | Spend 5 influence and choose a non-faction technology owned by 1 of your neighbors. Gain that technology.  
Tags: `tech-theft`, `requires-neighbor`, `timing-action`

**Plague** (`plague`)  
> Action | Choose 1 planet that is controlled by another player. Roll 1 die for each infantry on that planet. For each result of 6 or greater, destroy 1 of those units.  
Tags: `ground-force-effect`, `destroy-units`, `timing-action`

**Politics Rider** (`politic_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, draw 3 action cards and gain the speaker token.  
Tags: `vote-restrict`, `agenda-predict`, `draw-action-card` (3), `initiative-change`, `timing-reaction`

**Exploration Probe** (`probe`)  
> Action | Explore a frontier token that is in or adjacent to a system that contains 1 or more of your ships.  
Tags: `explore`, `frontier-token`, `timing-action`

**Puppets on a String** (`puppetsonastring`)  
> At the end of a player's turn, if you have passed | Perform 1 action.  
Tags: `extra-activation`, `timing-end-turn`

**Rally** (`rally`)  
> After you activate a system that contains another player's ships | Place 2 command tokens from your reinforcements in your fleet pool.  
Tags: `gain-command-token` (2), `timing-reaction`

**Refit Troops** (`refit`)  
> Action | Choose 1 or 2 of your infantry on the game board. Replace each of those infantry with mechs.  
Tags: `unit-replacement`, `mech-effect`, `timing-action`

**Reflective Shielding** (`reflective`)  
> When one of your ships uses SUSTAIN DAMAGE during combat | Produce 2 hits against your opponent's ships in the active system.  
Tags: `pre-combat-hits`, `timing-combat`

**Reparations** (`reparations`)  
> After another player gains control of a planet you control | Exhaust 1 planet that player controls and ready 1 planet you control.  
Tags: `exhaust-planets`, `ready-planets`, `timing-reaction`

**Repeal Law** (`repeal`)  
> Action | Discard 1 law from play.  
Tags: `agenda-cancel`, `timing-action`

**Rescue** (`rescue`)  
> After a player moves ships into a system that contains your ships | You may move 1 of your ships into the active system from any system that does not contain one of your command tokens.  
Tags: `relocate-units`, `timing-reaction`

**Reveal Prototype** (`reveal_prototype`)  
> At the start of a combat | Spend 4 resources to research a unit upgrade technology of the same type as 1 of your units that is participating in this combat.  
Tags: `research-free`, `timing-combat`

**Reverse Engineer** (`reverse_engineer`)  
> After another player discards an action card that has a component action | Take that action card from the discard pile.  
Tags: `discard-opponent-card`, `timing-reaction`

**Rout** (`rout`)  
> At the start of the 'Announce Retreats' step of space combat, if you are the defender | Your opponent must announce a retreat, if able.  
Tags: `retreat-control`, `timing-combat`

**Skilled Retreat** (`s_retreat1`)  
> At the start of a combat round | Move all of your ships from the active system into an adjacent system that does not contain another player's ships. The space combat ends in a draw. Then, place a command token from your reinforcements in that system.  
Tags: `retreat-control`, `combat-avoidance`, `place-command-token`, `timing-combat`

**Skilled Retreat** (`s_retreat2`)  
> At the start of a combat round | Move all of your ships from the active system into an adjacent system that does not contain another player's ships. The space combat ends in a draw. Then, place a command token from your reinforcements in that system.  
Tags: `retreat-control`, `combat-avoidance`, `place-command-token`, `timing-combat`

**Skilled Retreat** (`s_retreat3`)  
> At the start of a combat round | Move all of your ships from the active system into an adjacent system that does not contain another player's ships. The space combat ends in a draw. Then, place a command token from your reinforcements in that system.  
Tags: `retreat-control`, `combat-avoidance`, `place-command-token`, `timing-combat`

**Skilled Retreat** (`s_retreat4`)  
> At the start of a combat round | Move all of your ships from the active system into an adjacent system that does not contain another player's ships. The space combat ends in a draw. Then, place a command token from your reinforcements in that system.  
Tags: `retreat-control`, `combat-avoidance`, `place-command-token`, `timing-combat`

**Sabotage** (`sabo1`)  
> When another player plays an action card other than 'Sabotage' | Cancel that action card.  
Tags: `negate-ability`, `timing-reaction`

**Sabotage** (`sabo2`)  
> When another player plays an action card other than 'Sabotage' | Cancel that action card.  
Tags: `negate-ability`, `timing-reaction`

**Sabotage** (`sabo3`)  
> When another player plays an action card other than 'Sabotage' | Cancel that action card.  
Tags: `negate-ability`, `timing-reaction`

**Sabotage** (`sabo4`)  
> When another player plays an action card other than 'Sabotage' | Cancel that action card.  
Tags: `negate-ability`, `timing-reaction`

**Salvage** (`salvage`)  
> After you win a space combat | Your opponent gives you all of their commodities.  
Tags: `steal-trade-goods`, `post-combat-trigger`

**Sanction** (`sanction`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, each player that voted for that outcome returns 1 command token from their fleet supply to their reinforcements.  
Tags: `vote-restrict`, `agenda-predict`, `remove-command-token`, `timing-reaction`

**Scramble Frequency** (`scramble`)  
> After another player makes a BOMBARDMENT, SPACE CANNON, or ANTI-FIGHTER BARRAGE roll | That player rerolls all of their dice.  
Tags: `reroll`, `timing-reaction`

**Scuttle** (`scuttle`)  
> Action | Choose 1 or 2 of your non-fighter ships on the game board and return them to your reinforcements. Gain trade goods equal to the combined cost of those ships.  
Tags: `ship-effect`, `gain-trade-goods`, `timing-action`

**Seize Artifact** (`seize`)  
> Action | Choose 1 of your neighbors that has 1 or more relic fragments. That player must give you 1 relic fragment of your choice.  
Tags: `gain-relic`, `requires-neighbor`, `timing-action`

**Shields Holding** (`sh1`)  
> Before you assign hits to your ships during a space combat | Cancel up to 2 hits.  
Tags: `cancel-hit`, `timing-combat`

**Shields Holding** (`sh2`)  
> Before you assign hits to your ships during a space combat | Cancel up to 2 hits.  
Tags: `cancel-hit`, `timing-combat`

**Shields Holding** (`sh3`)  
> Before you assign hits to your ships during a space combat | Cancel up to 2 hits.  
Tags: `cancel-hit`, `timing-combat`

**Shields Holding** (`sh4`)  
> Before you assign hits to your ships during a space combat | Cancel up to 2 hits.  
Tags: `cancel-hit`, `timing-combat`

**In The Silence Of Space** (`silence_space`)  
> After you activate a system | Choose 1 system. During this tactical action, your ships in the chosen system can move through systems that contain other players' ships.  
Tags: `move-through-ships`, `timing-reaction`

**Solar Flare** (`solar_flare`)  
> After you activate a system | During the "Movement" step of this tactical action, other players cannot use SPACE CANNON against your ships.  
Tags: `space-cannon-immunity`, `timing-reaction`

**Spy** (`spy`)  
> Action | Choose 1 player. That player gives you 1 random action card from their hand.  
Tags: `discard-opponent-card`, `timing-action`

**Political Stability** (`stability`)  
> When you would return your strategy card(s) during the status phase | Do not return your strategy card(s). You do not choose strategy cards during the next strategy phase.  
Tags: `strategy-card-manipulation`, `timing-status-phase`

**Strategize** (`strategize1`)  
> Action | Perform the secondary ability of any readied or unchosen strategy card.  
Tags: `strategy-card-manipulation`, `timing-action`

**Strategize** (`strategize2`)  
> Action | Perform the secondary ability of any readied or unchosen strategy card.  
Tags: `strategy-card-manipulation`, `timing-action`

**Strategize** (`strategize3`)  
> Action | Perform the secondary ability of any readied or unchosen strategy card.  
Tags: `strategy-card-manipulation`, `timing-action`

**Strategize** (`strategize4`)  
> Action | Perform the secondary ability of any readied or unchosen strategy card.  
Tags: `strategy-card-manipulation`, `timing-action`

**Summit** (`summit`)  
> At the start of the strategy phase | Gain 2 command tokens.  
Tags: `gain-command-token` (2), `timing-strategy-phase`

**Tactical Bombardment** (`tactical`)  
> Action | Choose 1 system that contains 1 or more of your units that have BOMBARDMENT. Exhaust each planet controlled by other players in that system.  
Tags: `exhaust-planets`, `bombardment`, `timing-action`

**Technology Rider** (`tech_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, research 1 technology.  
Tags: `research-free`, `vote-restrict`, `agenda-predict`, `timing-reaction`

**Trade Rider** (`trade_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, gain 5 trade goods.  
Tags: `gain-trade-goods` (5), `vote-restrict`, `agenda-predict`, `timing-reaction`

**Unexpected Action** (`unexpected`)  
> Action | Remove 1 of your command tokens from the game board and return it to your reinforcements.  
Tags: `remove-command-token`, `timing-action`

**Unstable Planet** (`unstable`)  
> Action | Choose 1 hazardous planet. Exhaust that planet and destroy up to 3 infantry on it.  
Tags: `exhaust-planets`, `destroy-units` (3), `planetary-trait-bonus`, `timing-action`

**Upgrade** (`upgrade`)  
> After you activate a system that contains 1 or more of your ships | Replace 1 of your cruisers in that system with 1 dreadnought from your reinforcements.  
Tags: `unit-replacement`, `timing-reaction`

**Uprising** (`uprising`)  
> Action | Exhaust 1 non-home planet controlled by another player. Then gain trade goods equal to its resource value.  
Tags: `exhaust-planets`, `gain-trade-goods`, `timing-action`

**Veto** (`veto`)  
> When an agenda is revealed | Discard that agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda instead.  
Tags: `agenda-cancel`, `agenda-manipulation`, `timing-reaction`

**Veto** (`veto3`)  
> When an agenda is revealed | Discard that agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda instead.  
Tags: `agenda-cancel`, `agenda-manipulation`, `timing-reaction`

**Veto** (`veto4`)  
> When an agenda is revealed | Discard that agenda and reveal 1 agenda from the top of the deck. Players vote on this agenda instead.  
Tags: `agenda-cancel`, `agenda-manipulation`, `timing-reaction`

**War Effort** (`war_effort`)  
> Action | Place 1 cruiser from your reinforcements in a system that contains 1 or more of your ships.  
Tags: `ship-effect`, `requires-units-present`, `timing-action`

**War Machine** (`war_machine1`)  
> When 1 or more of your units use PRODUCTION | Apply +4 to the total PRODUCTION value of your units and reduce the combined cost of the produced units by 1.  
Tags: `production-bonus` (4), `cost-reduction` (1), `timing-reaction`

**War Machine** (`war_machine2`)  
> When 1 or more of your units use PRODUCTION | Apply +4 to the total PRODUCTION value of your units and reduce the combined cost of the produced units by 1.  
Tags: `production-bonus` (4), `cost-reduction` (1), `timing-reaction`

**War Machine** (`war_machine3`)  
> When 1 or more of your units use PRODUCTION | Apply +4 to the total PRODUCTION value of your units and reduce the combined cost of the produced units by 1.  
Tags: `production-bonus` (4), `cost-reduction` (1), `timing-reaction`

**War Machine** (`war_machine4`)  
> When 1 or more of your units use PRODUCTION | Apply +4 to the total PRODUCTION value of your units and reduce the combined cost of the produced units by 1.  
Tags: `production-bonus` (4), `cost-reduction` (1), `timing-reaction`

**Warfare Rider** (`war_rider`)  
> After an agenda is revealed | You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is correct, place 1 dreadnought from your reinforcements in a system that contains 1 or more of your ships.  
Tags: `vote-restrict`, `agenda-predict`, `ship-effect`, `timing-reaction`

**Waylay** (`waylay`)  
> Before you roll dice for ANTI-FIGHTER BARRAGE | Hits from this roll are produced against all ships (not just fighters).  
Tags: `anti-fighter-barrage`, `timing-combat`

## agenda

**Judicial Abolishment** (`abolishment`)  
> Elect Law (When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) | For: Discard the elected law from play.  
Tags: `elect-outcome`, `agenda-cancel`

**Imperial Arbiter** (`arbiter`)  
> Elect Player | For: The elected player gains this card. At the end of the strategy phase, the owner of this card may discard this card to swap 1 of their strategy cards with 1 of another player's strategy cards.  
Tags: `elect-outcome`, `strategy-card-manipulation`, `timing-strategy-phase`

**Arms Reduction** (`arms_reduction`)  
> For/Against | For: For: Each player destroys all but 2 of their dreadnoughts and all but 4 of their cruisers. | Against: Against: At the start of the next strategy phase, each player exhausts each of their planets that have a technology specialty.  
Tags: `destroy-units`, `ship-effect`, `exhaust-planets`, `tech-specialty-use`, `timing-strategy-phase`

**Articles of War** (`articles_war`)  
> For/Against | For: For: All mechs lose their printed abilities except for SUSTAIN DAMAGE. | Against: Against: Each player that voted "For" gains 3 trade goods.  
Tags: `law-permanent`, `negate-ability`, `mech-effect`, `gain-trade-goods` (3)

**Ixthian Artifact** (`artifact`)  
> For/Against | For: For: The speaker rolls 1 die. If the result is 6-10, each player may research 2 technologies. If the result is 1-5, destroy all units in Mecatol Rex's system, and each player with units in systems adjacent to Mecatol Rex's system destroys 3 of their own units in each of those systems. | Against: Against: No effect.  
Tags: `research-free`, `destroy-units`

**Political Censure** (`censure`)  
> Elect Player | For: The elected player gains this card and 1 victory point. The elected player cannot play action cards. If the owner of this card loses this card, they lose 1 victory point.  
Tags: `elect-outcome`, `gain-victory-point` (1), `action-card-restriction`

**Checks and Balances** (`checks`)  
> For/Against | For: For: When a player chooses a strategy card during the strategy phase, they give that strategy card to another player who does not have 1 (or a player who does not have 2 in a three- or four-player game), if able. | Against: Against: Each player readies only 3 of their planets at the end of this agenda phase.  
Tags: `law-permanent`, `strategy-card-manipulation`, `timing-strategy-phase`, `ready-planets`, `timing-agenda-phase`

**Clandestine Operations** (`cladenstine`)  
> For/Against | For: For: Each player removes 2 command tokens from their command sheet and returns those tokens to their reinforcements. | Against: Against: Each player removes 1 command token from their fleet pool and returns that token to their reinforcements.  
Tags: `remove-command-token`

**Classified Document Leaks** (`classified`)  
> Elect Scored Secret Objective (When this agenda is revealed, if there are no scored secret objectives, discard this card and reveal another agenda from the top of the deck.) | For: The elected secret objective becomes a public objective - place it near the other public objectives in the common play area.  
Tags: `elect-outcome`, `reveal-public-objective`

**Committee Formation** (`committee`)  
> Elect Player | For: The elected player gains this card. Before players vote on an agenda that requires a player to be elected, the owner of this card may discard this card to choose a player to be elected. Players do not vote on that agenda.  
Tags: `elect-outcome`, `timing-agenda-phase`, `vote-restrict`

**Regulated Conscription** (`conscription`)  
> For/Against | For: For: When a player produces units, they produce only 1 fighter and infantry for its cost instead of 2. | Against: Against: No effect.  
Tags: `law-permanent`, `cost-reduction`

**New Constitution** (`constitution`)  
> For/Against | For: (When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) For: Discard all laws from play. At the start of the next strategy phase, each player exhausts each planet in their home system. | Against: Against: No effect.  
Tags: `agenda-cancel`, `exhaust-planets`, `requires-home-system`, `timing-strategy-phase`

**Conventions of War** (`conventions`)  
> For/Against | For: For: Players cannot use BOMBARDMENT against units that are on cultural planets. | Against: Against: Each player that voted "Against" discards all of their action cards.  
Tags: `law-permanent`, `protect-planet`, `discard-opponent-card`

**Core Mining** (`core_mining`)  
> Elect Planet | For: Attach this card to the elected planet's card. Then, destroy 1 infantry on the planet. The resource value of this planet is increased by 2.  
Tags: `gain-attachment`, `elect-outcome`, `destroy-units` (1), `resources-bonus` (2)

**Covert Legislation** (`covert`)  
> When this agenda is revealed, the speaker draws the next card in the agenda deck but does not reveal it to the other players. Instead, the speaker reads the eligible outcomes aloud (for, against, elect Elect Player, etc.). The other Players vote for these outcomes as if they were outcomes of this agenda, without knowing their effects.  
Tags: *(none)*

**Galactic Crisis Pact** (`crisis`)  
> Elect Strategy Card | For: Each player may perform the secondary ability of the elected strategy card without spending a command token - command tokens placed by the ability are placed from a player's reinforcements instead.  
Tags: `elect-outcome`, `no-token-cost`

**The Crown of Emphidia** (`crown_of_emphidia`)  
> Elect Player | For: The elected player gains this card and 1 victory point. A player gains this card and 1 victory point after they gain control of a planet in the home system of this card's owner. Then, the previous owner of this card loses 1 victory point.  
Tags: `elect-outcome`, `gain-victory-point` (1), `timing-reaction`, `requires-home-system`

**The Crown of Thalnos** (`crown_of_thalnos`)  
> Elect Player | For: The elected player gains this card. During each combat round, the owner of this card may reroll any number of dice; they must destroy each of their units that did not produce a hit with its reroll.  
Tags: `elect-outcome`, `reroll`, `timing-combat`, `destroy-units`

**Homeland Defense Act** (`defense_act`)  
> For/Against | For: For: Each player can have any number of PDS units on planets they control. | Against: Against: Each player destroys 1 of their PDS units.  
Tags: `law-permanent`, `structure-effect`, `destroy-units` (1)

**Demilitarized Zone** (`demilitarized_zone`)  
> Elect Planet | For: Attach this card to the elected planet's card. Then, destroy all units on that planet. Player's units cannot land, be produced, or be placed on this planet.  
Tags: `gain-attachment`, `elect-outcome`, `destroy-units`, `restrict-movement`

**Compensated Disarmament** (`disarmament`)  
> Elect Planet | For: Destroy each ground force on the elected planet. For each unit that was destroyed, the player who control that planet gains 1 trade good.  
Tags: `destroy-units`, `ground-force-effect`, `gain-trade-goods`

**Economic Equality** (`economic_equality`)  
> For/Against | For: For: Each player returns all of their trade goods to the supply. Then, each player gains 5 trade goods. | Against: Against: Each player returns all of their trade goods to the supply.  
Tags: `give-trade-goods`, `gain-trade-goods` (5)

**Public Execution** (`execution`)  
> Elect Player | For: The elected player discards all of their action cards. If they have the speaker token, they give it to the player on their left. The elected player cannot vote on any agendas during this agenda phase.  
Tags: `discard-opponent-card`, `elect-outcome`, `initiative-change`, `vote-restrict`

**Research Grant Reallocation** (`grant_reallocation`)  
> Elect Player | For: The elected player gains any 1 technology of their choice. Then, for each prerequisite on that technology, they remove 1 token from their fleet pool and return it to their reinforcements.  
Tags: `elect-outcome`, `research-free`, `remove-command-token`

**Holy Planet of Ixth** (`holy_planet_of_ixth`)  
> Elect Planet | For: Attach this card to the elected planet's card. The planet's owner gains 1 victory point. Units on this planet cannot use PRODUCTION. When a player gains control of this planet, they gain 1 victory point. When a player loses control of this planet, they lose 1 victory point.  
Tags: `gain-attachment`, `elect-outcome`, `gain-victory-point` (1), `negate-ability`, `timing-reaction`

**Incentive Program** (`incentive`)  
> For/Against | For: For: Draw and reveal 1 stage I public objective from the deck and place it near the public objectives. | Against: Against: Draw and reveal 1 stage II public from the deck and place it near the public objectives.  
Tags: `reveal-public-objective`

**Minister of Antiquities** (`minister_antiquities`)  
> Elect Player | For: The elected player gains 1 relic.  
Tags: `elect-outcome`, `gain-relic`

**Minister of Commerce** (`minister_commerce`)  
> Elect Player | For: The elected player gains this card. After the owner of this card replenishes commodities, they gain 1 trade good for each player that is their neighbor.  
Tags: `elect-outcome`, `timing-reaction`, `gain-trade-goods`, `requires-neighbor`

**Minister of Exploration** (`minister_exploration`)  
> Elect Player | For: The elected player gains this card. When the owner of this card gains control of a planet, they gain 1 trade good.  
Tags: `elect-outcome`, `timing-reaction`, `gain-trade-goods` (1)

**Minister of Industry** (`minister_industry`)  
> Elect Player | For: The elected player gains this card. When the owner of this card places a space dock in a system, their units in that system may use their PRODUCTION abilities.  
Tags: `elect-outcome`, `timing-reaction`, `produce-units`

**Minister of Peace** (`minister_peace`)  
> Elect Player | For: The elected player gains this card. After a player activates a system that contains 1 or more of a different player's units, the owner of this card may discard this card - immediately end the active player's turn.  
Tags: `elect-outcome`, `timing-reaction`, `end-turn-skip`

**Minister of Policy** (`minister_policy`)  
> Elect Player | For: The elected player gains this card. At the end of the status phase, the owner of this card draws 1 action card.  
Tags: `elect-outcome`, `timing-status-phase`, `draw-action-card` (1)

**Minister of Sciences** (`minister_sciences`)  
> Elect Player | For: The elected player gains this card. When the owner of this card resolves the primary or secondary ability of the "Technology" strategy card, they do not need to spend resources to research technology.  
Tags: `elect-outcome`, `research-free`, `timing-reaction`

**Minister of War** (`minister_war`)  
> Elect Player | For: The elected player gains this card. The owner of this card may discard this card after performing an action to remove 1 of their command tokens from the game board and return it to their reinforcements - then they may perform 1 additional action.  
Tags: `elect-outcome`, `remove-command-token`, `extra-activation`, `timing-reaction`

**Miscount Disclosed** (`miscount`)  
> Elect Law | For: (When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) Vote on the elected law as if it were just revealed from the top of the deck.  
Tags: `agenda-cancel`, `timing-reaction`

**Mutiny** (`mutiny`)  
> For/Against | For: For: Each player that voted "For" gains 1 victory point. | Against: Against: Each player that voted "For" loses 1 victory point.  
Tags: `gain-victory-point` (1)

**Nexus Sovereignty** (`nexus`)  
> For/Against | For: For: Alpha and beta wormholes in the wormhole nexus have no effect during movement. | Against: Against: Place a gamma wormhole token in the Mecatol Rex system.  
Tags: `law-permanent`, `wormhole`

**Swords to Plowshares** (`plowshares`)  
> For/Against | For: For: Each player destroys half of their infantry on each planet they control, rounded up. Then, each player gains trade goods equal to the number of their infantry that were destroyed. | Against: Against: Each player places 1 infantry from their reinforcements on each planet they control.  
Tags: `destroy-units`, `ground-force-effect`, `gain-trade-goods`, `place-units-free`

**Prophecy of Ixth** (`prophecy`)  
> Elect Player | For: The elected player gains this card. The owner of this card applies +1 to the result of their fighter's combat rolls. When the owner of this card uses PRODUCTION, they discard this card unless they produce 2 or more fighters.  
Tags: `elect-outcome`, `combat-roll-bonus` (1), `fighter-effect`, `timing-reaction`

**Rearmament Agreement** (`rearmament`)  
> For/Against | For: For: Each player places 1 mech from their reinforcements on a planet they control in their home system. | Against: Against: Each player replaces each of their mechs with 1 infantry from their reinforcements.  
Tags: `mech-effect`, `place-units-free`, `unit-replacement`

**Colonial Redistribution** (`redistribution`)  
> Elect Non-Home Planet Other Than Mecatol Rex | For: Destroy each unit on the elected planet. Then, the player who controls that planet chooses 1 player with the fewest victory points - that player may place 1 infantry from their reinforcements on that planet.  
Tags: `destroy-units`, `elect-outcome`, `place-units-free`

**Fleet Regulations** (`regulations`)  
> For/Against | For: For: Each player cannot have more than 4 tokens in their fleet pool. | Against: Against: Each player places 1 command token from their reinforcements in their fleet pool.  
Tags: `law-permanent`, `place-command-token`

**Representative Government** (`rep_govt`)  
> For/Against | For: For: Players cannot exhaust planets to cast votes during the agenda phase. each player may cast 1 vote on each agenda instead. Players cannot cast additional votes. | Against: Against: At the start of the next strategy phase, each player that voted "Against" exhausts all of their cultural planets.  
Tags: `law-permanent`, `vote-restrict`, `exhaust-planets`, `timing-strategy-phase`

**Representative Government (Base Game)** (`representative_government`)  
> For/Against | For: For: Players cannot exhaust planets to cast votes during the agenda phase. Each player may cast 1 vote on each agenda instead. | Against: Against: At the start of the next strategy phase, each player that voted "Against" exhausts all of their cultural planets.  
Tags: `law-permanent`, `vote-restrict`, `exhaust-planets`, `timing-strategy-phase`

**Anti-Intellectual Revolution** (`revolution`)  
> For/Against | For: For: After a player researches a technology, that player must destroy 1 of their non-fighter ships. | Against: Against: At the start of the next strategy phase, each player chooses and exhausts 1 planet for each technology that they own.  
Tags: `destroy-units` (1), `timing-reaction`, `exhaust-planets`, `timing-strategy-phase`

**Research Team: Biotic** (`rt_biotic`)  
> Elect Planet | For: Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 green prerequisite.  
Tags: `gain-attachment`, `elect-outcome`, `prerequisite-skip` (1), `timing-reaction`, `exhaust`

**Research Team: Cybernetic** (`rt_cybernetic`)  
> Elect Planet | For: Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 yellow prerequisite.  
Tags: `gain-attachment`, `elect-outcome`, `prerequisite-skip` (1), `timing-reaction`, `exhaust`

**Research Team: Propulsion** (`rt_propulsion`)  
> Elect Planet | For: Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 blue prerequisite.  
Tags: `gain-attachment`, `elect-outcome`, `prerequisite-skip` (1), `timing-reaction`, `exhaust`

**Research Team: Warfare** (`rt_warfare`)  
> Elect Planet | For: Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 red prerequisite.  
Tags: `gain-attachment`, `elect-outcome`, `prerequisite-skip` (1), `timing-reaction`, `exhaust`

**Executive Sanctions** (`sanctions`)  
> For/Against | For: For: Each player can have a maximum of 3 action cards in their hand. | Against: Against: Each player discards 1 random action card from their hand.  
Tags: `law-permanent`, `discard-opponent-card`

**Publicize Weapon Schematics** (`schematics`)  
> For/Against | For: For: If any player owns a war sun technology, all players may ignore all prerequisites on war sun technologies. All war suns lose SUSTAIN DAMAGE. | Against: Against: Each player that owns a war sun technology discards all of their action cards.  
Tags: `law-permanent`, `prerequisite-skip`, `war-sun-effect`, `sustain-deny`, `discard-opponent-card`

**Archived Secret** (`secret`)  
> Elect Player | For: The elected player draws 1 secret objective.  
Tags: `gain-secret-objective`, `elect-outcome`

**Seed of an Empire** (`seed_empire`)  
> For/Against | For: For: The player with the most victory points gains 1 victory point. | Against: Against: The player with the fewest victory points gains 1 victory point.  
Tags: `gain-victory-point` (1)

**Senate Sanctuary** (`senate_sanctuary`)  
> Elect Planet | For: Attach this card to the elected planet's card. The influence value of this planet is increased by 2.  
Tags: `gain-attachment`, `elect-outcome`, `influence-bonus` (2)

**Shard of the Throne** (`shard_of_the_throne`)  
> Elect Player | For: The elected player gains this card and 1 victory point. A player gains this card and 1 victory point when they win a combat against the owner of this card. Then, the previous owner of this card loses 1 victory point.  
Tags: `elect-outcome`, `gain-victory-point` (1), `post-combat-trigger`

**Shared Research** (`shared_research`)  
> For/Against | For: For: Each player's units can move through nebulae. | Against: Against: Each player places a command token from their reinforcements in their home system, if able.  
Tags: `law-permanent`, `anomaly-movement`, `place-command-token`, `requires-home-system`

**Armed Forces Standardization** (`standardization`)  
> Elect Player | For: The elected player place command tokens from their reinforcements so that they have 3 tokens in their tactic pool, 3 tokens in their fleet pool, and 2 tokens in their strategy pool. They return any excess tokens to their reinforcements.  
Tags: `elect-outcome`, `place-command-token`, `remove-command-token`

**Terraforming Initiative** (`terraforming_initiative`)  
> Elect Planet | For: Attach this card to the elected planet's card. The resource and influence values of this planet are increased by 1.  
Tags: `gain-attachment`, `elect-outcome`, `resources-bonus` (1), `influence-bonus` (1)

**Enforced Travel Ban** (`travel_ban`)  
> For/Against | For: For: Alpha and beta wormholes have no effect during movement. | Against: Against: Destroy each PDS in or adjacent to a system that contains a wormhole.  
Tags: `law-permanent`, `wormhole`, `structure-effect`, `destroy-units`

**Unconventional Measures** (`unconventional`)  
> For/Against | For: For: Each player that voted "For" draws 2 action cards. | Against: Against: Each player that voted "For" discards all of their action cards.  
Tags: `draw-action-card` (2), `discard-opponent-card`

**Search Warrant** (`warrant`)  
> Elect Player | For: The elected player gains this card and draws 2 secret objectives. The owner of this card plays with their secret objectives revealed.  
Tags: `elect-outcome`, `gain-secret-objective`, `look-at-hidden-info`

**Wormhole Reconstruction** (`wormhole_recon`)  
> For/Against | For: For: All systems that contain either an alpha or beta wormhole are adjacent to each other. | Against: Against: Each player places a command token from their reinforcements in each system that contains a wormhole and 1 or more of their ships.  
Tags: `law-permanent`, `adjacency-grant`, `place-command-token`, `wormhole`

**Wormhole Research** (`wormhole_research`)  
> For/Against | For: For: Each player who has 1 or more ships in a system that contains a wormhole may research 1 technology. Then, destroy all ships in systems that contain an alpha or beta wormhole. | Against: Against: Each player that voted "Against" removes 1 command token from their command sheet and returns it to their reinforcements.  
Tags: `research-free`, `destroy-units`, `remove-command-token`

## public_objective

**Amass Wealth** (`amass_wealth`)  
> Spend 3 influence, 3 resources, and 3 trade goods.  
Tags: `obj-spend`

**Reclaim Ancient Monuments** (`ancient_monuments`)  
> Control 3 planets that have attachments.  
Tags: `obj-control-planets`

**Become a Legend** (`become_legend`)  
> Have units in 4 systems that contain legendary planets, Mecatol Rex, or anomalies.  
Tags: `obj-units-in-systems`, `obj-hold-systems`

**Form Galactic Brain Trust** (`brain_trust`)  
> Control 5 planets that have technology specialties.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Build Defenses** (`build_defenses`)  
> Have 4 or more structures.  
Tags: `obj-structures`

**Centralize Galactic Trade** (`centralize_trade`)  
> Spend 10 trade goods.  
Tags: `obj-spend`

**Command an Armada** (`command_armada`)  
> Have 8 or more non-fighter ships in 1 system.  
Tags: `obj-units-in-systems`

**Conquer the Weak** (`conquer`)  
> Control 1 planet that is in another player's home system.  
Tags: `obj-control-planets`, `requires-home-system`

**Control the Borderlands** (`control_borderlands`)  
> Have units in 5 systems on the edge of the game board other than your home system.  
Tags: `obj-units-in-systems`

**Corner the Market** (`corner`)  
> Control 4 planets that each have the same planet trait.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Explore Deep Space** (`deep_space`)  
> Have units in 3 systems that do not contain planets.  
Tags: `obj-units-in-systems`

**Develop Weaponry** (`develop`)  
> Own 2 unit upgrade technologies.  
Tags: `obj-own-technologies`

**Rule Distant Lands** (`distant_lands`)  
> Control 2 planets that are each in or adjacent to a different, other player's home system.  
Tags: `obj-control-planets`, `requires-home-system`

**Diversify Research** (`diversify`)  
> Own 2 technologies in each of 2 colors.  
Tags: `obj-own-technologies`

**Engineer a Marvel** (`engineer_marvel`)  
> Have your flagship or a war sun on the game board.  
Tags: `obj-units-in-systems`, `war-sun-effect`

**Expand Borders** (`expand_borders`)  
> Control 6 planets in non-home systems.  
Tags: `obj-control-planets`

**Galvanize the People** (`galvanize`)  
> Spend a total of 6 tokens from your tactic and/or strategy pools.  
Tags: `obj-spend`

**Found a Golden Age** (`golden_age`)  
> Spend 16 resources.  
Tags: `obj-spend`

**Improve Infrastructure** (`infrastructure`)  
> Have structures on 3 planets outside of your home system.  
Tags: `obj-structures`

**Intimidate Council** (`intimidate`)  
> Have 1 or more ships in 2 systems that are adjacent to Mecatol Rex's system.  
Tags: `obj-units-in-systems`, `requires-adjacent-units`

**Lead From the Front** (`lead`)  
> Spend a total of 3 tokens from your tactic and/or strategy pools.  
Tags: `obj-spend`

**Discover Lost Outposts** (`lost_outposts`)  
> Control 2 planets that have attachments.  
Tags: `obj-control-planets`

**Make History** (`make_history`)  
> Have units in 2 systems that contain legendary planets, Mecatol Rex, or anomalies.  
Tags: `obj-units-in-systems`, `obj-hold-systems`

**Manipulate Galactic Law** (`manipulate_law`)  
> Spend 16 influence.  
Tags: `obj-spend`

**Construct Massive Cities** (`massive_cities`)  
> Have 7 or more structures.  
Tags: `obj-structures`

**Master the Sciences** (`master_science`)  
> Own 2 technologies in each of 4 colors.  
Tags: `obj-own-technologies`

**Erect a Monument** (`monument`)  
> Spend 8 resources.  
Tags: `obj-spend`

**Populate the Outer Rim** (`outer_rim`)  
> Have units in 3 systems on the edge of the game board other than your home system.  
Tags: `obj-units-in-systems`

**Protect the Border** (`protect_border`)  
> Have structures on 5 planets outside of your home system.  
Tags: `obj-structures`

**Push Boundaries** (`push_boundaries`)  
> Control more planets than each of 2 of your neighbors.  
Tags: `obj-control-planets`, `requires-more-than-opponent`, `requires-neighbor`

**Raise a Fleet** (`raise_fleet`)  
> Have 5 or more non-fighter ships in 1 system.  
Tags: `obj-units-in-systems`

**Found Research Outposts** (`research_outposts`)  
> Control 3 planets that have technology specialties.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Revolutionize Warfare** (`revolutionize`)  
> Own 3 unit upgrade technologies.  
Tags: `obj-own-technologies`

**Subdue the Galaxy** (`subdue`)  
> Control 11 planets in non-home systems.  
Tags: `obj-control-planets`

**Achieve Supremacy** (`supremacy`)  
> Have your flagship or war sun in another player's home system or the Mecatol Rex system.  
Tags: `war-sun-effect`, `obj-hold-systems`

**Sway the Council** (`sway_council`)  
> Spend 8 influence.  
Tags: `obj-spend`

**Negotiate Trade Routes** (`trade_routes`)  
> Spend 5 trade goods.  
Tags: `obj-spend`

**Unify the Colonies** (`unify_colonies`)  
> Control 6 planets that each have the same planet trait.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Hold Vast Reserves** (`vast_reserves`)  
> Spend 6 influence, 6 resources, and 6 trade goods.  
Tags: `obj-spend`

**Patrol Vast Territories** (`vast_territories`)  
> Have units in 5 systems that do not contain planets.  
Tags: `obj-units-in-systems`

## secret_objective

**Adapt New Strategies** (`ans`)  
> Own 2 faction technologies. 'Valefar Assimilator' technologies do not count toward this objective.  
Tags: `obj-own-technologies`

**Betray a Friend** (`baf`)  
> Win a combat against a player whose promissory note you had in your play area at the start of your tactical action.  
Tags: `obj-combat`, `obj-tokens-cards`

**Become a Martyr** (`bam`)  
> Lose control of a planet in a home system.  
Tags: `obj-hold-systems`

**Become the Gatekeeper** (`btgk`)  
> Have 1 or more ships in a system that contains an alpha wormhole and 1 or more ships in a system that contains a beta wormhole.  
Tags: `obj-units-in-systems`, `obj-wormholes-anomalies`

**Brave the Void** (`btv`)  
> Win a combat in an anomaly.  
Tags: `obj-combat`, `obj-wormholes-anomalies`

**Cut Supply Lines** (`csl`)  
> Have 1 or more ships in the same system as another player's space dock.  
Tags: `obj-units-in-systems`

**Control the Region** (`ctr`)  
> Have 1 or more ships in 6 systems.  
Tags: `obj-units-in-systems`

**Defy Space and Time** (`dfat`)  
> Have units in the wormhole nexus.  
Tags: `obj-wormholes-anomalies`, `obj-units-in-systems`

**Destroy Heretical Works** (`dhw`)  
> Purge 2 of your relic fragments of any type.  
Tags: `obj-tokens-cards`

**Dictate Policy** (`dp`)  
> There are 3 or more laws in play.  
Tags: `obj-ownership-count`

**Drive the Debate** (`dtd`)  
> You or a planet you control are elected by an agenda.  
Tags: `elect-outcome`

**Destroy Their Greatest Ship** (`dtgs`)  
> Destroy another player's war sun or flagship.  
Tags: `obj-combat`, `war-sun-effect`

**Darken the Skies** (`dts`)  
> Win a combat in another player's home system.  
Tags: `obj-combat`, `requires-home-system`

**Demonstrate Your Power** (`dyp`)  
> Have 3 or more non-fighter ships in the active system at the end of a space combat.  
Tags: `obj-units-in-systems`

**Establish a Perimeter** (`eap`)  
> Have 4 PDS units on the game board.  
Tags: `obj-structures`

**Establish Hegemony** (`eh`)  
> Control planets that have a combined influence value of at least 12.  
Tags: `obj-control-planets`

**Forge an Alliance** (`faa`)  
> Control 4 cultural planets.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Foster Cohesion** (`fc`)  
> Be neighbors with all other players.  
Tags: `requires-neighbor`

**Form a Spy Network** (`fsn`)  
> Discard 5 action cards.  
Tags: `obj-tokens-cards`

**Fuel the War Machine** (`fwm`)  
> Have 3 space docks on the game board.  
Tags: `obj-structures`

**Fight with Precision** (`fwp`)  
> Destroy the last of a player's fighters in the active system during the anti-fighter barrage step.  
Tags: `obj-combat`, `anti-fighter-barrage`

**Gather a Mighty Fleet** (`gamf`)  
> Have 5 dreadnoughts on the game board.  
Tags: `obj-ownership-count`

**Hoard Raw Materials** (`hrm`)  
> Control planets that have a combined resource value of at least 12.  
Tags: `obj-control-planets`

**Learn the Secrets of the Cosmos** (`lsc`)  
> Have 1 or more ships in 3 systems that are each adjacent to an anomaly.  
Tags: `obj-units-in-systems`, `obj-wormholes-anomalies`

**Make an Example of Their World** (`mew`)  
> Destroy the last of a player's ground forces on a planet during the bombardment step.  
Tags: `obj-combat`, `ground-force-effect`

**Master the Laws of Physics** (`mlp`)  
> Own 4 technologies of the same color.  
Tags: `obj-own-technologies`

**Monopolize Production** (`mp`)  
> Control 4 industrial planets.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Mine Rare Metals** (`mrm`)  
> Control 4 hazardous planets.  
Tags: `obj-control-planets`, `obj-planet-traits`

**Mechanize the Military** (`mtm`)  
> Have 1 mech on each of 4 planets.  
Tags: `obj-units-in-systems`, `mech-effect`

**Occupy the Seat of the Empire** (`ose`)  
> Control Mecatol Rex and have 3 or more ships in its system.  
Tags: `obj-hold-systems`, `obj-units-in-systems`

**Occupy the Fringe** (`otf`)  
> Have 9 or more ground forces on a planet that does not contain 1 of your space docks.  
Tags: `obj-units-in-systems`, `ground-force-effect`

**Prove Endurance** (`pe`)  
> Be the last player to pass during a game round.  
Tags: *(none)*

**Produce en Masse** (`pem`)  
> Have units with a combined PRODUCTION value of at least 8 in a single system.  
Tags: `obj-units-in-systems`

**Seize an Icon** (`sai`)  
> Control a legendary planet.  
Tags: `obj-control-planets`, `planetary-trait-bonus`

**Spark a Rebellion** (`sar`)  
> Win a combat against a player who has the most victory points.  
Tags: `obj-combat`, `requires-more-than-opponent`

**Strengthen Bonds** (`sb`)  
> Have another player's promissory note in your play area.  
Tags: `obj-tokens-cards`

**Stake Your Claim** (`syc`)  
> Control a planet in a system that contains a planet controlled by another player.  
Tags: `obj-control-planets`

**Threaten Enemies** (`te`)  
> Have 1 or more ships in a system that is adjacent to another player's home system.  
Tags: `obj-units-in-systems`, `requires-home-system`

**Turn Their Fleets to Dust** (`ttfd`)  
> Destroy the last of a player's non-fighter ships in the active system during the space cannon offense step.  
Tags: `obj-combat`, `space-cannon`

**Unveil Flagship** (`uf`)  
> Win a space combat in a system that contains your flagship. You cannot score this objective if your flagship is destroyed in the combat.  
Tags: `obj-combat`, `war-sun-effect`, `score-objective-help`

## relic

**Book of Latvinia** (`bookoflatvinia`)  
> When you gain this card, research up to 2 technologies that have no prerequisites. > ACTION: Purge this card; if you control planets that have all 4 types of technology specialties, gain 1 victory point. Otherwise, gain the speaker token.  
Tags: `timing-reaction`, `research-free`, `timing-action`, `requires-planets-controlled`, `gain-victory-point` (1), `initiative-change`, `purge`

**Circlet of the Void** (`circletofthevoid`)  
> Your units do not roll for gravity rifts, and you ignore the effects of all other anomalies on movement.  ACTION: Exhaust this card to explore a frontier token in a system that does not contain any other players' ships.  
Tags: `anomaly-movement`, `exhaust`, `explore`, `frontier-token`, `timing-action`

**The Codex** (`codex`)  
> ACTION: Purge this card to take up to 3 action cards of your choice from the action card discard pile.  
Tags: `timing-action`, `draw-action-card` (3), `purge`

**Dominus Orb** (`dominusorb`)  
> Before you move units during a tactical action, you may purge this card to move and transport units that are in systems that contain 1 of your command tokens.  
Tags: `movement-exemption`, `timing-reaction`, `purge`

**Dynamis Core** (`dynamiscore`)  
> While this card is in your play area, your commodity value is increased by 2. ACTION: Gain trade goods equal to your commodity value, then purge this card.  
Tags: `commodity-value-bonus` (2), `timing-passive`, `timing-action`, `gain-trade-goods`, `purge`

**Scepter of Emelpar** (`emelpar`)  
> When you would spend a token from your strategy pool, you may exhaust this card to spend a token from your reinforcements instead.  
Tags: `strategy-pool-use`, `no-token-cost`, `timing-reaction`, `exhaust`

**The Crown of Emphidia** (`emphidia`)  
> After you perform a tactical action, you may exhaust this card to explore 1 planet you control. At the end of the status phase, if you control the "Tomb of Emphidia," you may purge this card to gain 1 victory point.  
Tags: `timing-reaction`, `explore`, `exhaust`, `timing-status-phase`, `requires-planets-controlled`, `gain-victory-point` (1), `purge`

**Enigmatic Device** (`enigmaticdevice`)  
> Place this card faceup in your play area. ACTION: You may spend 6 resources and purge this card to research 1 technology.  
Tags: `timing-passive`, `timing-action`, `research-free`, `purge`

**Heart of Ixth** (`heartofixth`)  
> After any die is rolled, you may exhaust this card to add or subtract 1 from its result.  
Tags: `combat-roll-bonus` (1), `timing-reaction`, `exhaust`

**Lightrail Ordnance** (`lightrailordnance`)  
> Your space docks gain SPACE CANNON 5 (x2). You may use your space dock's SPACE CANNON against ships that are adjacent to their systems.  
Tags: `space-cannon`, `structure-effect`, `requires-adjacent-units`, `timing-passive`

**Maw of Worlds** (`mawofworlds`)  
> At the start of the agenda phase, you may purge this card and exhaust all of your planets to gain any 1 technology.  
Tags: `exhaust-planets`, `research-free`, `timing-agenda-phase`, `purge`

**Metali Void Armaments** (`metalivoidarmaments`)  
> During the "Anti-Fighter Barrage" step of space combat, you may resolve ANTI-FIGHTER BARRAGE 6 (x3) against your opponent's units.  
Tags: `anti-fighter-barrage`, `timing-combat`

**Metali Void Shielding** (`metalivoidshielding`)  
> Each time hits are produced against 1 or more of your non-fighter ships, 1 of those ships may use SUSTAIN DAMAGE as if it had that ability.  
Tags: `sustain-damage`, `sustain-grant`, `timing-reaction`

**Nano-Forge** (`nanoforge`)  
> ACTION: Attach this card to a non-legendary, non-home planet you control, its resource and influence values are increased by 2 and it is a legendary planet. This action cannot be performed once attached.  
Tags: `gain-attachment`, `resources-bonus` (2), `influence-bonus` (2), `planetary-trait-bonus`, `timing-action`

**Neuraloop** (`neuraloop`)  
> When a public objective is revealed, you may purge one of your relics to discard that objective and replace it with a random objective from any objective deck; that objective is a public objective, even if it is a secret objective.  
Tags: `purge`, `reveal-public-objective`, `timing-reaction`

**The Obsidian** (`obsidian`)  
> When you gain this card, draw 1 secret objective. You can have 1 additional scored or unscored secret objective.  
Tags: `gain-secret-objective`, `timing-reaction`

**The Prophet's Tears** (`prophetstears`)  
> When you research a technology, you may exhaust this card to ignore 1 prerequisite or draw 1 action card.  
Tags: `prerequisite-skip` (1), `draw-action-card` (1), `timing-reaction`, `exhaust`

**The Quantumcore** (`quantumcore`)  
> When you gain this card, gain your breakthrough. You have SYNERGY for all technology types.  
Tags: `timing-reaction`, `tech-specialty-use`

**Shard of the Throne** (`shard`)  
> When you gain this card, gain 1 victory point, when you lose this card, lose 1 victory point. When a player gains control of a legendary planet you control or a planet you control in your home system, that player gains this card.  
Tags: `gain-victory-point` (1), `timing-reaction`, `gain-relic`, `requires-home-system`

**Stellar Converter** (`stellarconverter`)  
> ACTION: Choose 1 non-home, non-legendary planet other than Mecatol Rex in a system that is adjacent to 1 or more of your units that have BOMBARDMENT, destroy all units on that planet and purge its attachments and its planet card. Then, place the destroyed planet token on that planet and purge this card.  
Tags: `timing-action`, `requires-adjacent-units`, `bombardment`, `destroy-units`, `purge`

**The Crown of Thalnos** (`thalnos`)  
> During each combat round, this card's owner may reroll any number of their dice, applying +1 to the results, any units that reroll dice but do not produce at least 1 hit are destroyed.  
Tags: `reroll`, `combat-roll-bonus` (1), `destroy-units`, `timing-combat`

**The Silver Flame** (`thesilverflame`)  
> The Silver Flame may be exchanged as part of a transaction. > ACTION: Roll 1 die and purge this card; if the result is a 10, gain 1 victory point. Otherwise, purge your home system and all units in it; you cannot score public objectives. Put The Fracture into play if it is not already.  
Tags: `transaction`, `timing-action`, `gain-victory-point` (1), `destroy-units`, `score-objective-help`, `game-structure-change`, `purge`

**The Triad** (`thetriad`)  
> This card can be readied and spent as if it were a planet card. Its resource and influence values are equal to 3 plus the number of different types of relic fragments you own.  
Tags: `ready-card`, `resources-bonus`, `influence-bonus`

**JR-XS455-O** (`titanprototype`)  
> ACTION: Exhaust this agent and choose a player, that player may spend 3 resources to place a structure on a planet they control. If they do not, they gain 1 trade good.  
Tags: `exhaust`, `timing-action`, `place-structure`, `gain-trade-goods` (1)

## exploration

**Abandoned Warehouses** (`aw1`)  
> You may gain 2 commodities, or you may convert up to 2 of your commodities to trade goods.  
Tags: `gain-commodities` (2), `convert-commodities`

**Abandoned Warehouses** (`aw2`)  
> You may gain 2 commodities, or you may convert up to 2 of your commodities to trade goods.  
Tags: `gain-commodities` (2), `convert-commodities`

**Abandoned Warehouses** (`aw3`)  
> You may gain 2 commodities, or you may convert up to 2 of your commodities to trade goods.  
Tags: `gain-commodities` (2), `convert-commodities`

**Abandoned Warehouses** (`aw4`)  
> You may gain 2 commodities, or you may convert up to 2 of your commodities to trade goods.  
Tags: `gain-commodities` (2), `convert-commodities`

**Biotic Research Facility** (`biotic`)  
> This planet has a green technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.  
Tags: `tech-specialty-use`, `resources-bonus` (1), `influence-bonus` (1)

**Core Mine** (`cm1`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 trade good.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-trade-goods` (1)

**Core Mine** (`cm2`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 trade good.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-trade-goods` (1)

**Core Mine** (`cm3`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 trade good.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-trade-goods` (1)

**Cultural Relic Fragment** (`crf1`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf2`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf3`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf4`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf5`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf6`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf7`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf8`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cultural Relic Fragment** (`crf9`)  
> ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Cybernetic Research Facility** (`cybernetic`)  
> This planet has a yellow technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.  
Tags: `tech-specialty-use`, `resources-bonus` (1), `influence-bonus` (1)

**Demilitarized Zone** (`dmz`)  
> Return all structures on this planet to your reinforcements. Then, return all ground forces on this planet to the space area. Attach: Units cannot be committed to, produced on, or placed on this planet. During the agenda phase, this planet's planet card can be traded as part of a transaction.  
Tags: `structure-effect`, `ground-force-effect`, `gain-attachment`, `restrict-movement`, `transaction`

**Dyson Sphere** (`ds`)  
> This planet's resource value is increased by 2 and its influence value is increased by 1.  
Tags: `resources-bonus` (2), `influence-bonus` (1)

**Derelict Vessel** (`dv1`)  
> Draw 1 secret objective.  
Tags: `gain-secret-objective`

**Derelict Vessel** (`dv2`)  
> Draw 1 secret objective.  
Tags: `gain-secret-objective`

**Dead World** (`dw`)  
> Draw 1 relic.  
Tags: `gain-relic`

**Enigmatic Device** (`ed1`)  
> Place this card faceup in your play area. ACTION: You may spend 6 resource and purge this card to research 1 technology.  
Tags: `timing-passive`, `timing-action`, `research-free`, `purge`

**Enigmatic Device** (`ed2`)  
> Place this card faceup in your play area. ACTION: You may spend 6 resource and purge this card to research 1 technology.  
Tags: `timing-passive`, `timing-action`, `research-free`, `purge`

**Entropic Field** (`ent`)  
> Gain 1 command token and 2 trade goods.  
Tags: `gain-command-token` (1), `gain-trade-goods` (2)

**Expedition** (`exp1`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, ready this planet.  
Tags: `requires-units-present`, `ground-force-effect`, `ready-planets`

**Expedition** (`exp2`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, ready this planet.  
Tags: `requires-units-present`, `ground-force-effect`, `ready-planets`

**Expedition** (`exp3`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, ready this planet.  
Tags: `requires-units-present`, `ground-force-effect`, `ready-planets`

**Functioning Base** (`fb1`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to draw 1 action card.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `draw-action-card` (1)

**Functioning Base** (`fb2`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to draw 1 action card.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `draw-action-card` (1)

**Functioning Base** (`fb3`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to draw 1 action card.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `draw-action-card` (1)

**Functioning Base** (`fb4`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to draw 1 action card.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `draw-action-card` (1)

**Freelancers** (`frln1`)  
> You may produce 1 unit in this system. You may spend influence as if it were resources to produce this unit.  
Tags: `produce-units`, `influence-as-resources`

**Freelancers** (`frln2`)  
> You may produce 1 unit in this system. You may spend influence as if it were resources to produce this unit.  
Tags: `produce-units`, `influence-as-resources`

**Freelancers** (`frln3`)  
> You may produce 1 unit in this system. You may spend influence as if it were resources to produce this unit.  
Tags: `produce-units`, `influence-as-resources`

**Gamma Relay** (`gamma`)  
> Place a gamma wormhole token in this system. Then, purge this card.  
Tags: `wormhole`, `purge`

**Gamma Wormhole** (`gw`)  
> Place a gamma wormhole token in this system. Then, purge this card.  
Tags: `wormhole`, `purge`

**Hazardous Relic Fragment** (`hrf1`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf2`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf3`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf4`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf5`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf6`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Hazardous Relic Fragment** (`hrf7`)  
> ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Ion Storm** (`ion`)  
> Place the ion storm token in this system with either side faceup. Then, place this card in the common play area. At the end of the "Move Ships" or "Retreat" substep of a tactical action during which 1 or more of your ships use the ion storm wormhole, flip the ion storm token to its opposing side.  
Tags: `wormhole`, `timing-end-turn`

**Industrial Relic Fragment** (`irf1`)  
> ACTION: Purge 3 of your industrial relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Industrial Relic Fragment** (`irf2`)  
> ACTION: Purge 3 of your industrial relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Industrial Relic Fragment** (`irf3`)  
> ACTION: Purge 3 of your industrial relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Industrial Relic Fragment** (`irf4`)  
> ACTION: Purge 3 of your industrial relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Industrial Relic Fragment** (`irf5`)  
> ACTION: Purge 3 of your industrial relic fragments to gain 1 relic.  
Tags: `timing-action`, `gain-relic`

**Keleres Ship** (`kel1`)  
> Gain 2 command tokens.  
Tags: `gain-command-token` (2)

**Keleres Ship** (`kel2`)  
> Gain 2 command tokens.  
Tags: `gain-command-token` (2)

**Lost Crew** (`lc1`)  
> Draw 2 action cards.  
Tags: `draw-action-card` (2)

**Lost Crew** (`lc2`)  
> Draw 2 action cards.  
Tags: `draw-action-card` (2)

**Local Fabricators** (`lf1`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to place 1 mech from your reinforcements on this planet.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `mech-effect`

**Local Fabricators** (`lf2`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to place 1 mech from your reinforcements on this planet.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `mech-effect`

**Local Fabricators** (`lf3`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to place 1 mech from your reinforcements on this planet.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `mech-effect`

**Local Fabricators** (`lf4`)  
> You may gain 1 commodity, or you may spend 1 trade good or 1 commodity to place 1 mech from your reinforcements on this planet.  
Tags: `gain-commodities` (1), `spend-trade-goods` (1), `mech-effect`

**Lazax Survivors** (`ls`)  
> This planet's resource value is increased by 1 and its influence value is increased by 2.  
Tags: `resources-bonus` (1), `influence-bonus` (2)

**Major Entropic Field** (`majent`)  
> Gain 1 command token and 3 trade goods.  
Tags: `gain-command-token` (1), `gain-trade-goods` (3)

**Minor Entropic Field** (`minent`)  
> Gain 1 command token and 1 trade good.  
Tags: `gain-command-token` (1), `gain-trade-goods` (1)

**Mirage** (`mirage`)  
> Place the Mirage planet token in this system. Gain the Mirage planet card and ready it. Then, purge this card.  
Tags: `gain-planet`, `ready-planets`, `purge`

**Mercenary Outfit** (`mo1`)  
> You may place 1 infantry from your reinforcements on this planet.  
Tags: `place-units-free`, `ground-force-effect`

**Mercenary Outfit** (`mo2`)  
> You may place 1 infantry from your reinforcements on this planet.  
Tags: `place-units-free`, `ground-force-effect`

**Mercenary Outfit** (`mo3`)  
> You may place 1 infantry from your reinforcements on this planet.  
Tags: `place-units-free`, `ground-force-effect`

**Merchant Station** (`ms1`)  
> You may replenish your commodities, or you may convert your commodities to trade goods.  
Tags: `gain-commodities`, `convert-commodities`

**Merchant Station** (`ms2`)  
> You may replenish your commodities, or you may convert your commodities to trade goods.  
Tags: `gain-commodities`, `convert-commodities`

**Mining World** (`mw`)  
> This planet's resource value is increased by 2.  
Tags: `resources-bonus` (2)

**Propulsion Research Facility** (`propulsion`)  
> This planet has a blue technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.  
Tags: `tech-specialty-use`, `resources-bonus` (1), `influence-bonus` (1)

**Paradise World** (`pw`)  
> This planet's influence value is increased by 2.  
Tags: `influence-bonus` (2)

**Rich World** (`rw`)  
> This planet's resource value is increased by 1.  
Tags: `resources-bonus` (1)

**Tomb of Emphidia** (`toe`)  
> This planet's influence value is increased by 1. If the player who has the "Crown of Emphidia" relic has control of this planet, they can use that relic to gain 1 VP.  
Tags: `influence-bonus` (1), `gain-victory-point` (1), `requires-planets-controlled`

**Unknown Relic Fragment** (`urf1`)  
> The card counts as a relic fragment of any type.  
Tags: `gain-relic`

**Unknown Relic Fragment** (`urf2`)  
> The card counts as a relic fragment of any type.  
Tags: `gain-relic`

**Unknown Relic Fragment** (`urf3`)  
> The card counts as a relic fragment of any type.  
Tags: `gain-relic`

**Volatile Fuel Source** (`vfs1`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 command token.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-command-token` (1)

**Volatile Fuel Source** (`vfs2`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 command token.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-command-token` (1)

**Volatile Fuel Source** (`vfs3`)  
> If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 command token.  
Tags: `requires-units-present`, `ground-force-effect`, `gain-command-token` (1)

**Warfare Research Facility** (`warfare`)  
> This planet has a red technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.  
Tags: `tech-specialty-use`, `resources-bonus` (1), `influence-bonus` (1)

## galactic_event

**Advent of the War Sun** (`advent_war_sun`)  
> - At the end of setup, all players other than the Embers of Muaat player gain the War Sun unit upgrade technology. - The Embers of Muaat player purges their faction specific promissory note, unlocks their commander, and places 1 additional War Sun in their home system.  
Tags: `research-free`, `war-sun-effect`, `faction-start-setup`, `leader-unlock-condition`, `purge`

**Age of Commerce** (`age_commerce`)  
> - Players do not have to be neighbors to perform transactions with each other. - Players do not have a maximum number of commodities; when they refresh commodities, they gain a number of commodities equal to their commodity value instead. - Players can share non-faction technology with other players as part of a transaction. When sharing tech in this way, the receiving player gains that technology from their own deck; the sharing player does not lose the technology.  
Tags: `transaction`, `commodity-value-bonus`, `gain-commodities`, `tech-theft`, `game-structure-change`

**Age of Exploration** (`age_exploration`)  
> - Relics require only 2 matching fragments be purged instead of 3. - The Naaz-Rokha Alliance's ***Fabrication*** faction ability and ***Black Market Forgery*** promissory note do not require purged fragments to match. - All players can perform the following action: > - ACTION: Exhaust ***Dark Energy Tap*** and choose a non-home edge system that contains your ships to roll 1 die. On a result of 1-4, draw a random unused red tile; on a result of 5-10, draw a random unused blue tile. Place that tile adjacent to the chosen system so that it is touching at least 2 non-home systems. Place a frontier token in the system if it does not contain any planets.  
Tags: `game-structure-change`, `frontier-token`, `timing-action`, `gain-relic`

**Age of Fighters** (`age_fighters`)  
> - During setup, all players gain the Fighter II unit upgrade technology; the Naalu Collective player gains Hybrid Crystal Fighter II technology instead. - All fighters that are counting against fleet pool gain CAPACITY 1 that cannot be used to support fighters. - Non-fighter ships are purged when they are destroyed  
Tags: `research-free`, `capacity-bonus` (1), `fighter-effect`, `fleet-limit-bonus`, `ship-effect`

**Call of the Void** (`call_of_the_void`)  
> After you move 1 or more units into the active system, if that system is in The Fracture, gain 1 command token. - When you activate a system in The Fracture, apply +1 to the move values of each of your ships.  
Tags: `gain-command-token` (1), `timing-reaction`, `move-bonus` (1), `game-structure-change`

**Civilized Society** (`civilized_society`)  
> - During setup, turn all public objectives face up. - There is no limit on the number of public objectives a player may score during the status phase. - The game does not immediately end when a player reaches the required number of victory points; instead, it ends at the end of that round's status phase, and the player with the most victory points wins. In the case of a tie, the tied players total the influence values of their controlled planets and their unspent trade goods; the player or players with the highest total win the game.  
Tags: `game-structure-change`, `score-objective-help`

**Cosmic Phenomenae** (`cosmic_phenomenae`)  
> Anomalies have the following additional rules: > - ***Nebulae:*** Defending ships in nebulae apply an additional +3 to their combat rolls instead of +1. > - ***Asteroid Fields:*** Fighter I's do not participate in space combat in asteroid fields. > - ***Supernovas:*** Units with PRODUCTION in or adjacent to supernovas have their PRODUCTION values increased by 1. > - ***Gravity Rifts:*** You may apply an additional +1 to the MOVE value of any of your ships moving out of gravity rifts; if you do, those ships are destroyed on a roll of 5 or lower. > - ***Entropic Scars:*** Systems that contain entropic scars are adjacent to each other.  
Tags: `combat-roll-bonus` (3), `fighter-effect`, `production-bonus` (1), `move-bonus` (1), `adjacency-grant`, `anomaly-movement`

**Conventions of War Abandoned** (`cowabunga`)  
> - Each hit produced by ***BOMBARDMENT*** rolls destroys 3 units instead of 1. - ***X-89 BACTERIAL WEAPON*** technology gains the following ACTION: Exhaust to choose 1 planet in a system that contains 1 of your units that has bombardment; purge its planet card and all attachments or legendary planet ability cards associated with it.  - You are eliminated if all your home system's planet cards are purged.  
Tags: `bombardment`, `destroy-units` (3), `timing-action`, `requires-units-present`, `purge`, `game-structure-change`

**Cultural Exchange Program** (`cultural_exchange`)  
> - At the end of setup, shuffle the reference cards that correspond to each faction in play. Each player draws 1 of those cards and takes all leaders that correspond to that faction. They belong to that player for the remainder of the game. Then, each player unlocks their commander. - The Obsidian/Firmament player does not participate in the Cultural Exchange Program. Instead, they begin the game with "The Obsidian" relic.  
Tags: `faction-start-setup`, `leader-unlock-condition`, `gain-relic`, `game-structure-change`

**Dangerous Wilds** (`dangerous_wilds`)  
> - During setup, place neutral infantry on each hazardous planet equal to that planet's resource value. - At the end of each round, for each hazardous planet that is not controlled, replenish any neutral units that were destroyed during the game round. - When a player gains control of a hazardous planet from the deck, they may research 1 technology; they may ignore a number of pre-requisites equal to that planet's resource value.  
Tags: `place-units-free`, `ground-force-effect`, `planetary-trait-bonus`, `research-free`, `prerequisite-skip`

**Hidden Agenda** (`hidden_agenda`)  
> - During the agenda phase, only the speaker can talk; all other players must remain silent except when declaring action cards/abilities. Transactions cannot be performed during this phase. - When voting, players secretly and simultaneously write their desired outcome and number of votes and pass them to the speaker. After all players have voted, the speaker secretly tallys the results and reveals only the totals to the other players. - The Argent Flight's votes are public and are known before the other players vote.  
Tags: `vote-restrict`, `transaction`, `timing-agenda-phase`, `game-structure-change`

**Mercenaries for Hire** (`mercenaries`)  
> - At the start of the game, shuffle the alliance cards that correspond to the factions not in play and place them in the common play area; this is the Mercenary deck. - All players can perform the following action: ACTION: Spend 3 trade goods to gain the top card of the mercenary deck and place it in your play area. Players can use the abilities of the mercenaries in their play area.  
Tags: `game-structure-change`, `spend-trade-goods` (3), `share-ability`, `timing-action`

**Minor Factions** (`minor_factions`)  
> - During setup, players are dealt 1 fewer blue tile. Before creating the galaxy, shuffle the reference cards for factions not being played and deal 1 to each player. In speaker order, each player places that faction's home system in the second ring, equidistant from players' home systems (See page xx in the rulebook). Then, that player places 3 neutral infantry on that system's planets, split as evenly as possible. These systems are minor faction systems and do not count as home systems. - When a player controls each planet in a minor faction system, they take that faction's alliance card from the deck or from the player that owned it previously. - Planets in minor faction systems gain all three planet traits (cultural, industrial, hazardous).  
Tags: `game-structure-change`, `place-units-free`, `ground-force-effect`, `share-ability`, `planetary-trait-bonus`

**Rapid Mobilization** (`mobilization`)  
> - After setup, put The Fracture into play. Then, each player simultaneously resolves the following effects in order: > - Place 1 infantry onto each planet adjacent to your home system (or the creuss gate/sorrow); do not explore those planets. Then, ready each of those planets. > - Place 1 space dock on any planet you control and place your flagship and 3 fighters in its system. > - Gain your breakthrough. > - Research 1 technology; if you are the Nomad player, research 1 additional technology.  
Tags: `faction-start-setup`, `place-units-free`, `ground-force-effect`, `place-structure`, `ship-effect`, `research-free`

**Monuments to the Ages** (`monuments_ages`)  
> - When you would place a structure on a non-home planet, you may spend 5 trade goods to place a neutral space dock instead; this is a monument, and is not considered to be a unit of any type. There can only be 3 monuments in play at once. - At the start of the status phase, place 1 commodity beneath each monument. A monument is worth 1 victory point to the player who controls it for every 3 commodities placed beneath it. - When a player gains control of a planet that contains a monument, they may destroy it.  
Tags: `spend-trade-goods` (5), `place-structure`, `gain-victory-point` (1), `timing-reaction`, `destroy-units`, `game-structure-change`

**Stellar Atomics** (`stellar_atomics`)  
> - During setup, each player places one of their control markers on this card. - All players can perform the following action: > - **Action:** Discard your control marker from this card to destroy all ground forces and structures on any non-home planet. - If you do not have a control marker on this card, you cannot vote or play action cards during the agenda phase.  
Tags: `game-structure-change`, `destroy-units`, `vote-restrict`, `timing-action`, `action-card-restriction`

**Total War** (`total_war`)  
> - When a player destroys another player's units, they place commodities from the supply equal to the combined cost of those units on a planet they control in their home system (infantry and fighters are worth 1 each). - When a player gains control of a planet that has commodities on it, they may move those commodities to a planet they control in their home system. - All players can perform the following action: > - ACTION: Discard 10 commodities from planets in your home system to gain 1 victory point.  
Tags: `gain-commodities`, `timing-reaction`, `relocate-units`, `requires-home-system`, `gain-victory-point` (1), `timing-action`

**Weird Wormholes** (`weird_wormholes`)  
> - After a ship moves using at least 1 alpha, beta, or gamma wormhole, roll 1 die and consult the following list: > - [ Fighter -> Destroyer -> Cruiser -> Dreadnought -> Carrier -> Flagship -> War Sun (if researched) ] - For each result of 1-5, replace that ship with the ship BEFORE it from your reinforcements, if able. - For each result of 6-10, replace that ship with the ship AFTER it from your reinforcements, if able. - If there are no ships of that type remaining in your reinforcements, skip that type and replace it with the next available type.  
Tags: `wormhole`, `timing-reaction`, `unit-replacement`

**Wild, Wild Galaxy** (`wild_wild_galaxy`)  
> Action cards are adjusted as follows: > - Direct hit: Can be used against mechs. > - Flank Speed: Applies +2 move instead of +1. > - Maneuvering Jets: Cancels all SPACE CANNON hits > - Morale Boost: Applies +2 to die rolls instead of +1 > - Sabotage: Also take the canceled action card > - Shields Holding: Can be used in ground combat > - Skilled Retreat: Does not place a command token > - War Machine: Reduces cost by 5 instead of 1 > - Diplomatic Pressure: Player must give 3 notes - Additionally, Stellar Converter and Nova Seed can be used against any planets and systems.  
Tags: `game-structure-change`, `combat-roll-bonus` (2), `move-bonus` (2), `space-cannon-immunity`, `cost-reduction` (5)

**Zealous Orthodoxy** (`zealous_orthodoxy`)  
> - The first player to score 2 secret objectives gains 1 victory point. - Then, place that faction's alliance card on this card; all players gain that ability. That faction's alliance promissory note is then purged.  
Tags: `gain-victory-point` (1), `share-ability`, `purge`

## legendary_planet

**Avernus** (`avernus`)  
> The Nucleus | ACTION: Exhaust this card to use the Embers of Muaat's **STAR FORGE** faction ability without spending a command token.  
Tags: `timing-action`, `exhaust`, `no-token-cost`, `share-ability`

**Custodia Vigilia** (`custodiavigilia`)  
> Custodian's Favour | While you control Mecatol Rex, it gains SPACE CANNON 5 and PRODUCTION 3. Gain 2 command tokens when another player scores a victory point with the second clause of the 'Imperial' strategy card.  
Tags: `space-cannon`, `production-bonus` (3), `timing-passive`, `requires-planets-controlled`, `gain-command-token` (2), `timing-reaction`

**Emelpar** (`emelpar`)  
> The Acropolis | You may exhaust this card at the end of your turn to ready another component that isn't a strategy card.  
Tags: `exhaust`, `ready-card`, `timing-end-turn`

**Faunus** (`faunus`)  
> Maxis Central Control | You may exhaust this card when you pass to gain control of a non-home, non-legendary planet that contains no units and has no attachments.  
Tags: `exhaust`, `gain-planet`, `timing-reaction`

**Garbozia** (`garbozia`)  
> Dok 'N Pic's Salvage Yard | You may exhaust this card when you pass to place 1 action card from the discard pile faceup on this card; you can purge cards on this card to play them as if they were in your hand.  
Tags: `exhaust`, `draw-action-card`, `purge`, `timing-reaction`

**Hope's End** (`hopesend`)  
> Imperial Arms Vault | You may exhaust this card at the end of your turn to place 1 mech from your reinforcements on any planet you control or draw 1 action card  
Tags: `exhaust`, `mech-effect`, `place-units-free`, `draw-action-card` (1), `timing-end-turn`

**Illusion** (`illusion`)  
> Illusion Flight Academy | You may exhaust this card at the end of your turn to place up to 2 fighters from your reinforcements in any system that contains 1 or more of your ships  
Tags: `exhaust`, `fighter-effect`, `place-units-free`, `requires-units-present`, `timing-end-turn`

**Industrex** (`industrex`)  
> Aurex Mechanica | You may exhaust this card when you pass to place 1 ship that matches a unit upgrade technology you own from your reinforcements into a system that contains your ships.  
Tags: `exhaust`, `ship-effect`, `requires-units-present`, `timing-reaction`

**Locked Mallice** (`lockedmallice`)  
> Exterrix Headquarters | You may exhaust this card at the end of your turn to gain 2 trade goods or convert all of your commodities to trade goods.  
Tags: `exhaust`, `gain-trade-goods` (2), `convert-commodities`, `timing-end-turn`

**Mallice** (`mallice`)  
> Exterrix Headquarters | You may exhaust this card at the end of your turn to gain 2 trade goods or convert all of your commodities to trade goods.  
Tags: `exhaust`, `gain-trade-goods` (2), `convert-commodities`, `timing-end-turn`

**Mirage** (`mirage`)  
> Mirage Flight Academy | You may exhaust this card at the end of your turn to place up to 2 fighters from your reinforcements in any system that contains 1 or more of your ships  
Tags: `exhaust`, `fighter-effect`, `place-units-free`, `requires-units-present`, `timing-end-turn`

**Mecatol Rex** (`mrte`)  
> The Galactic Council | You may exhaust this card at the end of your turn and discard 1 secret objective to draw 1 secret objective.  
Tags: `exhaust`, `gain-secret-objective`, `timing-end-turn`

**Ordinian** (`ordinian`)  
> 4X41D "Hyperion" V1 | You may exhaust this card when you pass to draw 1 action card and gain 1 command token.  
Tags: `exhaust`, `draw-action-card` (1), `gain-command-token` (1), `timing-reaction`

**Ordinian Rex** (`ordinianc4`)  
> Barren Husk | You may exhaust this card when you pass to draw 1 action card and gain 1 command token  
Tags: `exhaust`, `draw-action-card` (1), `gain-command-token` (1), `timing-reaction`

**Phantasm** (`phantasm`)  
> Phantasm Flight Academy | You may exhaust this card at the end of your turn to place up to 2 fighters from your reinforcements in any system that contains 1 or more of your ships  
Tags: `exhaust`, `fighter-effect`, `place-units-free`, `requires-units-present`, `timing-end-turn`

**Primor** (`primor`)  
> The Atrament | You may exhaust this card at the end of your turn to place up to 2 infantry from your reinforcements on any planet you control  
Tags: `exhaust`, `ground-force-effect`, `place-units-free`, `timing-end-turn`

**Styx** (`styx`)  
> A Song Like Marrow | When you gain this card, gain 1 victory point. When you lose this card, lose 1 victory point.  
Tags: `gain-victory-point` (1), `timing-reaction`

**Tempesta** (`tempesta`)  
> Ionian Fuel Refinery | You may exhaust this card after you activate a system to apply +1 to the move value of 1 of your ships during this tactical action.  
Tags: `exhaust`, `move-bonus` (1), `timing-reaction`

**Thunder's Edge** (`thundersedge`)  
> Jupiter Brain | Gain your breakthrough when you gain this card if you do not already have it. You may exhaust this card at the end of your turn to perform another action.  
Tags: `timing-reaction`, `extra-activation`, `exhaust`, `timing-end-turn`

## strategy_card

**Diplomacy** (`base2`)  
> PRIMARY: Choose 1 system other than the Mecatol Rex system that contains a planet you control; each other player places a command token from their reinforcements in the chosen system. Then, ready each exhausted planet you control in that system. | SECONDARY: Spend 1 token from your strategy pool to ready up to 2 exhausted planets  
Tags: `timing-action`, `place-command-token`, `ready-planets`, `strategy-pool-use`

**Construction** (`base4`)  
> PRIMARY: Place 1 PDS or 1 Space Dock on a planet you control. Place 1 PDS on a planet you control. | SECONDARY: Place 1 token from your strategy pool in any system; you may place either 1 space dock or 1 PDS on a planet you control in that system.  
Tags: `timing-action`, `place-structure`, `place-command-token`, `strategy-pool-use`

**Leadership** (`pok1leadership`)  
> PRIMARY: Gain 3 command tokens Spend any amount of influence to gain 1 command token for every 3 influence spent | SECONDARY: Spend any amount of influence to gain 1 command token for every 3 influence spent  
Tags: `timing-action`, `gain-command-token` (3)

**Diplomacy** (`pok2diplomacy`)  
> PRIMARY: Choose 1 system other than the Mecatol Rex system that contains a planet you control; each other player places a command token from their reinforcements in the chosen system. Then, ready up to 2 exhausted planets you control. | SECONDARY: Spend 1 token from your strategy pool to ready up to 2 exhausted planets you control.  
Tags: `timing-action`, `place-command-token`, `ready-planets`, `strategy-pool-use`

**Politics** (`pok3politics`)  
> PRIMARY: Choose a player other than the speaker.  That player gains the speaker token. Draw 2 action cards Look at the top 2 cards of the agenda deck. Place each card on the top or bottom of the deck in any order. | SECONDARY: Spend 1 token from your strategy pool to draw 2 action cards.  
Tags: `timing-action`, `initiative-change`, `draw-action-card` (2), `agenda-manipulation`, `look-at-hidden-info`, `strategy-pool-use`

**Construction** (`pok4construction`)  
> PRIMARY: Place 1 PDS or 1 Space Dock on a planet you control. Place 1 PDS on a planet you control. | SECONDARY: Spend 1 token from your strategy pool and place it in any system; you may place either 1 space dock or 1 PDS on a planet you control in that system  
Tags: `timing-action`, `place-structure`, `strategy-pool-use`

**Trade** (`pok5trade`)  
> PRIMARY: Gain 3 trade goods. Replenish commodities. Choose any number of other players. Those players use the secondary ability of this strategy card without spending a command token. | SECONDARY: Spend 1 token from your strategy pool to replenish your commodities.  
Tags: `timing-action`, `gain-trade-goods` (3), `gain-commodities`, `no-token-cost`, `strategy-pool-use`

**Warfare** (`pok6warfare`)  
> PRIMARY: Remove 1 of your command tokens from the game board; then, gain 1 command token. Redistribute any number of the command tokens on your command sheet. | SECONDARY: Spend 1 token from your strategy pool to use the PRODUCTION ability of 1 of your space docks in your home system (this token is not placed in your home system).  
Tags: `timing-action`, `remove-command-token`, `gain-command-token` (1), `redistribute-tokens`, `strategy-pool-use`, `produce-units`

**Technology** (`pok7technology`)  
> PRIMARY: Research 1 technology. Spend 6 resources to research 1 technology. | SECONDARY: Spend 1 token from your strategy pool and 4 resources to research 1 technology.  
Tags: `timing-action`, `research-free`, `strategy-pool-use`

**Imperial** (`pok8imperial`)  
> PRIMARY: Immediately score 1 public objective if you fulfill its requirements. Gain 1 victory point if you control Mecatol Rex; otherwise, draw 1 secret objective. | SECONDARY: Spend 1 token from your strategy pool to draw 1 secret objective.  
Tags: `timing-action`, `score-objective-help`, `gain-victory-point` (1), `requires-planets-controlled`, `gain-secret-objective`, `strategy-pool-use`

**Construction** (`te4construction`)  
> PRIMARY: Either place 1 structure on a planet you control, or use the PRODUCTION ability of 1 of your space docks. Place 1 structure on a planet you control. | SECONDARY: Spend 1 token from your strategy pool to place 1 structure on a planet you control.  
Tags: `timing-action`, `place-structure`, `produce-units`, `strategy-pool-use`

**Warfare** (`te6warfare`)  
> PRIMARY: Perform a tactical action in any system without placing a command token, even if the system already has your command token in it: that system still counts as being activated. You may redistribute your command tokens before and after this action. | SECONDARY: Spend 1 token from your strategy pool to use the Production abilities of the units in your home system. (This token is not placed in your home system)  
Tags: `timing-action`, `extra-activation`, `no-token-cost`, `redistribute-tokens`, `strategy-pool-use`, `produce-units`

## plot

**Assail** (`assail`)  
> Apply +1 to the results of each of your combat and unit ability rolls against the puppeted player.  
Tags: `combat-roll-bonus` (1), `puppet-control`

**Enervate** (`enervate`)  
> You can perform the secondary abilities of the puppeted player's strategy cards without spending command tokens; for "Leadership," you can perform the primary ability instead.  
Tags: `puppet-control`, `no-token-cost`, `strategy-card-manipulation`

**Extract** (`extract`)  
> When this card is revealed, gain 1 non-faction technology owned by the puppeted player. When the puppeted player gains a non-faction technology, you may spend 4 resources to gain that technology.  
Tags: `tech-theft`, `puppet-control`, `timing-reaction`

**Mutated Assail** (`mutated_assail`)  
> This plot card has no effect.  
Tags: *(none)*

**Mutated Enervate** (`mutated_enervate`)  
> This plot card has no effect.  
Tags: *(none)*

**Mutated Extract** (`mutated_extract`)  
> This plot card has no effect.  
Tags: *(none)*

**Mutated Seethe** (`mutated_seethe`)  
> This plot card has no effect.  
Tags: *(none)*

**Mutated Siphon** (`mutated_siphon`)  
> This plot card has no effect.  
Tags: *(none)*

**Seethe** (`seethe`)  
> When this card is revealed, destroy all units on a non-home planet controlled by the puppeted player. At the start of the status phase, destroy 1 of the puppeted player's infantry in any system.  
Tags: `puppet-control`, `destroy-units`, `ground-force-effect`, `timing-reaction`, `timing-status-phase`

**Siphon** (`siphon`)  
> When the puppeted player gains commodities, you gain an equal number of trade goods.  
Tags: `puppet-control`, `gain-trade-goods`, `timing-reaction`

## Tag glossary

| tag | cards | meaning |
|---|---|---|
| `action-card-restriction` | 4 | Prevents players from playing action cards, or restricts when/which action cards may be played. |
| `adjacency-grant` | 11 | Treats systems/units as adjacent that normally are not, or creates adjacency. |
| `agenda-cancel` | 10 | Cancel, discard, redo or replace an agenda or its outcome. |
| `agenda-manipulation` | 8 | Look at, reorder, reveal or otherwise manipulate the agenda deck / revealed agendas. |
| `agenda-predict` | 15 | Predict or choose an agenda outcome. |
| `anomaly-movement` | 12 | Ships may enter, move through, or are protected in anomalies (asteroid field, nebula, supernova, gravity rift). |
| `anti-fighter-barrage` | 11 | A unit has or enhances ANTI-FIGHTER BARRAGE. |
| `assign-hits-control` | 6 | Lets a player choose which units take hits, or forces/limits how hits are assigned. |
| `bombardment` | 16 | A unit has, grants, or enhances BOMBARDMENT. |
| `cancel-hit` | 12 | Cancels or ignores hits produced against your units. |
| `capacity-bonus` | 11 | Increases ship capacity or lets units not count against capacity. magnitude = N. |
| `capture-units` | 8 | Captures enemy units into your own capture area / holds units for ransom. |
| `combat-avoidance` | 9 | Prevents or skips a combat, or prevents movement into a system to avoid combat. |
| `combat-roll-bonus` | 24 | Adds a positive modifier to combat rolls (space or ground) of your units or a chosen player's units. magnitude = the +N. |
| `combat-roll-penalty` | 4 | Applies a negative modifier to combat/bombardment/space cannon die rolls, usually the opponent's. magnitude = N (positive number). |
| `combat-stat-change` | 2 | Changes a unit's combat value/hit threshold (e.g. combat value improved or set to a number). |
| `commodity-value-bonus` | 2 | Changes your commodity value / maximum commodities. magnitude = N. |
| `convert-commodities` | 15 | Convert commodities to trade goods or modify commodity conversion. |
| `cost-reduction` | 13 | Reduces the cost of units, technologies or other purchases. magnitude = amount. |
| `deal-enforcement` | 41 | Binding promises or commitments between players (promissory pledges, forced compliance, ceasefire). |
| `destroy-units` | 52 | Destroys units outright (not via combat hits). magnitude = number destroyed if stated. |
| `direct-hit-immunity` | 6 | Unit cannot be destroyed by "Direct Hit" action cards (immunity to one destruction effect; not a hit cancellation). |
| `discard-opponent-card` | 13 | Make another player discard cards, or take/steal cards from their hand. |
| `draw-action-card` | 30 | Draw action cards. magnitude = N. |
| `elect-outcome` | 36 | Effect of electing a player/planet/etc. and what they gain or lose. |
| `end-turn-skip` | 6 | Ends or skips a player's turn. |
| `exhaust` | 90 | Card must be exhausted to use. |
| `exhaust-planets` | 16 | Exhausts planets (yours or others'). |
| `explore` | 11 | Explore planets/frontiers or draw exploration cards. |
| `extra-activation` | 9 | Allows an additional activation, tactical action, or taking another action/turn. |
| `extra-dice` | 8 | Lets units roll additional dice in combat or another roll. magnitude = number of extra dice. |
| `faction-start-setup` | 9 | Modifies setup, starting units, starting technologies, or components at game start. |
| `fighter-effect` | 9 | Directly affects fighters (placing, capacity, barrage). |
| `fleet-limit-bonus` | 4 | Raises the fleet pool / non-fighter ship limit per system, or exempts units from it. |
| `frontier-token` | 5 | Concerns frontier tokens or exploration discovery. |
| `gain-attachment` | 20 | Attaches tokens/attachments to planets or systems. |
| `gain-command-token` | 29 | Gain command tokens. magnitude = N. |
| `gain-commodities` | 26 | Gain or replenish commodities. magnitude = amount if fixed. |
| `gain-planet` | 7 | Take control of a planet or gain a planet card. |
| `gain-promissory` | 11 | Gain, take, steal, or give promissory notes. |
| `gain-relic` | 35 | Gain a relic or relic fragment. |
| `gain-secret-objective` | 8 | Draw, gain, or swap secret objectives. |
| `gain-trade-goods` | 47 | Gain trade goods. magnitude = amount if fixed. |
| `gain-victory-point` | 18 | Directly grants or removes victory points. magnitude = VP. |
| `game-structure-change` | 23 | Alters game structure/rules (e.g. galactic event modifying rules). |
| `give-trade-goods` | 5 | Give trade goods/commodities to others (including as a cost). |
| `ground-force-effect` | 43 | Directly affects infantry/ground forces (placing, killing, boosting, moving). |
| `hero-one-shot` | 33 | Powerful effect followed by purging (hero). |
| `ignore-planetary-shield` | 8 | Units ignore or remove PLANETARY SHIELD. |
| `influence-as-resources` | 5 | Influence may be spent as if it were resources (or resources as influence, or both values combined). |
| `influence-bonus` | 13 | Increases influence of planets or available influence. magnitude = N. |
| `initiative-change` | 7 | Changes initiative order or turn order. |
| `law-permanent` | 14 | A law/persistent rule change that remains in play. |
| `leader-unlock-condition` | 70 | Leader that has an unlock requirement (commanders/heroes). |
| `look-at-hidden-info` | 10 | Look at hidden cards, objectives, or decks (hand, secret objective, top of deck). |
| `mech-effect` | 22 | Directly affects mechs (deploy, boost, revive, mech ability). |
| `move-bonus` | 19 | Adds to the move value of ships (or sets it). magnitude = +N. |
| `move-through-ships` | 8 | Ships may move through or ignore other players' ships / blockades. |
| `move-without-transport` | 5 | A unit may move without being carried, or be carried in unusual ways. |
| `movement-exemption` | 3 | Lifts a normal movement/activation restriction (e.g. move out of or activate systems that contain your command tokens). |
| `negate-ability` | 17 | Disables, cancels or nullifies another card, unit ability, technology, or faction ability. |
| `no-token-cost` | 10 | Allows an action/secondary ability without spending a command token. |
| `obj-combat` | 9 | Objective: win combats, destroy units, or kill units. |
| `obj-control-planets` | 18 | Objective: control a number/type of planets. |
| `obj-hold-systems` | 5 | Objective: occupy Mecatol Rex, home systems, or special systems. |
| `obj-own-technologies` | 6 | Objective: own technologies/upgrades/colors. |
| `obj-ownership-count` | 2 | Objective: own a numeric count of components (units, planets, cards). |
| `obj-planet-traits` | 7 | Objective: planet traits (cultural/hazardous/industrial) or planet attributes. |
| `obj-spend` | 10 | Objective: spend resources, influence, trade goods or tokens. |
| `obj-structures` | 6 | Objective: structures/space docks/PDS on planets. |
| `obj-tokens-cards` | 4 | Objective: possess tokens, cards, relics, or trade goods. |
| `obj-units-in-systems` | 21 | Objective: units/ships in specified systems. |
| `obj-wormholes-anomalies` | 4 | Objective: wormholes, anomalies, or system types. |
| `once-per-action` | 4 | Usable only once per action/combat/round/phase/turn. |
| `place-command-token` | 18 | Place command tokens on the board or on cards. |
| `place-structure` | 13 | Places or builds structures (space dock, PDS) or changes where they may be placed. |
| `place-units-free` | 58 | Places units from reinforcements without paying (not via production). |
| `planetary-shield` | 3 | A unit has/grants PLANETARY SHIELD or prevents bombardment. |
| `planetary-trait-bonus` | 9 | Effect keyed to planet trait or attribute (legendary, cultural, hazardous, industrial). |
| `post-combat-trigger` | 12 | Triggers an effect after a combat round or on winning/losing combat. |
| `pre-combat-hits` | 6 | Produces hits at the start of / before combat rolls (ambush, assault cannon, first strike), or destroys units at combat start. |
| `prerequisite-skip` | 13 | Ignore technology prerequisites. magnitude = N. |
| `produce-units` | 18 | Produces units outside normal production timing, or grants a production action. |
| `production-bonus` | 14 | Increases or adds PRODUCTION capacity / units produced. magnitude = N. |
| `production-unit` | 9 | A unit has the PRODUCTION keyword. |
| `protect-planet` | 2 | Prevents planets from being taken, invaded, exhausted or having units removed. |
| `puppet-control` | 9 | Controls another player's decisions, cards, or components (puppet). |
| `purge` | 57 | Card is purged (removed from the game) after use. |
| `ready-card` | 8 | Ready exhausted cards, leaders or components. |
| `ready-planets` | 18 | Readies exhausted planets. |
| `redistribute-tokens` | 5 | Reallocate command tokens between pools. |
| `relocate-units` | 26 | Moves units outside a normal move (teleport/swap/redeploy, move out of turn, transfer between systems). |
| `remove-command-token` | 20 | Remove command tokens from board or from players' pools. |
| `repair-units` | 9 | Repairs damaged units (removes damage markers). |
| `requires-adjacent-units` | 7 | Effect conditional on units being adjacent / in or near a system. |
| `requires-home-system` | 14 | Effect tied to your home system, home planets, or another player's home. |
| `requires-more-than-opponent` | 2 | Effect conditional on having more/less of something than an opponent. |
| `requires-neighbor` | 13 | Effect targets or requires neighbors. |
| `requires-planets-controlled` | 14 | Effect conditional on controlling planets/planet types/count. |
| `requires-units-present` | 42 | Effect conditional on having particular units in a system or on a planet. |
| `reroll` | 9 | Allows re-rolling dice (combat, exploration, or other rolls). |
| `research-free` | 24 | Gain or research a technology outside of normal research, or free/discounted research. |
| `resources-bonus` | 18 | Increases resources of planets or available resources. magnitude = N. |
| `restrict-activation` | 2 | Prevents players from activating systems or limits where tactical actions can occur. |
| `restrict-movement` | 5 | Prevents or limits movement of ships/units (e.g. cannot move into a system). |
| `retreat-control` | 9 | Grants, forces, or restricts retreats / retreat destinations. |
| `reveal-public-objective` | 3 | Draws, reveals, replaces or creates a public objective. |
| `score-objective-help` | 7 | Helps score, ignore requirements of, or gives conditions to score objectives. |
| `share-ability` | 13 | Lets you (or others) use another player's card/ability/unit as if your own (alliance, commander sharing). |
| `ship-effect` | 25 | Directly affects ships (placing, moving, killing, boosting). |
| `space-cannon` | 16 | A unit has, grants, or enhances SPACE CANNON (including offense/defense variants and ranged use). |
| `space-cannon-immunity` | 12 | Units or players are protected from SPACE CANNON hits (or from ANTI-FIGHTER BARRAGE), or the effect is reduced. |
| `spend-trade-goods` | 20 | Requires spending trade goods as a cost to use an effect. magnitude = cost. |
| `steal-trade-goods` | 4 | Take trade goods or commodities from another player. |
| `strategy-card-manipulation` | 15 | Exchange, take, change or restrict strategy cards or their abilities/initiative. |
| `strategy-pool-use` | 24 | Spends or interacts with tokens from the strategy pool as a cost or resource. |
| `structure-effect` | 13 | Directly affects structures (docks, PDS). |
| `sustain-damage` | 14 | A unit has or gains the SUSTAIN DAMAGE keyword. |
| `sustain-deny` | 3 | Prevents units from using SUSTAIN DAMAGE or from being repaired. |
| `sustain-grant` | 2 | Gives other units/ships the ability to sustain damage, or cancel a hit by sustaining. |
| `tech-specialty-use` | 11 | Interacts with tech specialties, technology colors, or counts technologies owned. |
| `tech-theft` | 10 | Copy, take, or use another player's technology. |
| `timing-action` | 154 | Triggered as an ACTION (taken on your turn as the action). |
| `timing-agenda-phase` | 9 | Triggers in the agenda phase. |
| `timing-combat` | 78 | Triggers during or around a combat. |
| `timing-end-turn` | 28 | Triggers at the end of a turn, round, or phase. |
| `timing-passive` | 31 | Static always-on effect with no trigger. |
| `timing-reaction` | 274 | Reaction triggered by another player's action or event. |
| `timing-start-turn` | 9 | Triggers at the start of a turn, round, or phase beginning. |
| `timing-status-phase` | 15 | Triggers in the status phase. |
| `timing-strategy-phase` | 12 | Triggers in the strategy phase. |
| `trade-goods-as-resources` | 2 | Trade goods count as resources/influence or are worth more when spent; commodities may be spent as trade goods. |
| `transaction` | 11 | Alters transactions: who can trade, what can be traded, or timing. |
| `unit-replacement` | 14 | Replaces a unit with a different (usually better/other) unit from reinforcements. |
| `unit-revival` | 10 | A destroyed unit returns to play, is placed back from reinforcements, or is replaced after destruction. |
| `vote-bonus` | 9 | Gives additional votes or changes voting power. magnitude = N. |
| `vote-restrict` | 23 | Prevents voting or restricts how votes can be cast. |
| `war-sun-effect` | 6 | Directly affects war suns or flagships. |
| `wormhole` | 23 | Interacts with wormholes (counting, treating as wormhole, creating wormhole tokens). |
