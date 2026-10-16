# Faction agents in the early rounds (1-3)

How each faction's agent can help in rounds 1-3, how this engine implements and offers it, what
the rule bot did with it before this work, and the rule the bot now follows. Written as the basis
for `crates/ti4-server/src/session/smart_bot_agents.rs`.

Sources: the card texts are `crates/ti4-content/content/leaders.json` (every claim below was
checked against it and against the engine code named); the community view comes from
`COMMUNITY_FACTION_STRATEGIES.md` and `FACTION_PLANS.md` in this folder, which cite the Reddit, BGG
and SCPT threads. Additional web searches for agent rankings and the Z'eu ruling found nothing
usable (search results were Valorant tier lists; the fandom wiki answered HTTP 402 and Reddit is not
fetchable from here), so no new web sources are added; the advice below rests on the cited
community threads plus the card texts.

## How agents work here

- **Timing and cost.** 51.2a: a faction starts with its agent readied. Using it exhausts it
  (`leaders::exhaust`); every exhausted agent readies in the status phase (`leaders::ready_all`,
  81.6). So an agent is a **free, once-per-round** effect: an unused agent is wasted value, and
  spending it early in a round only costs the option of a better use later the same round.
- **Two shapes of offer.**
  - `ACTION:` agents (Arborec, Muaat, Xxcha, Naalu PoK, Crimson, Ral Nel, Creuss alt) are
    **component actions** on the owner's turn: option `component|leader|<id>` in the action-phase
    choice. They take the turn's action but **no command token**. `During the action phase:`
    agents (Hacan, Nekro) are offered the same way, also in the `end_turn` choice, and do not
    end the turn (OP-07, `leaders::uses_the_action`).
  - Every other agent is a **reaction** registered in the timing system: the owner is asked in a
    window `reaction_<when|after>_<EVENT>` with an option `leader:<faction>:<id>:<EVENT>:<rel>`
    (kind `ability`) and `decline`. The event's seat (`trigger.actor`, or `trigger.subject` for
    hits and destroyed units) is the player the agent would act for. Some reactions then ask a
    follow-up (`agent_ship`, `agent_planet`, `stillness_target`, `l1z1x_agent_swap`, ...).
- **Which game.** Server games are built with `POK` sources (`map.rs`): the 24 roster factions
  (`preset::ROSTER`) get their PoK leaders. Thunder's Edge factions and the Council Keleres are not
  seated by the roster; their agents are documented for completeness.
- **Ssruu (Yssaril)** copies every other seat's agent; its offers carry `yssarilagent` and the
  source agent's id (`leader:<f>:<borrower>:yssarilagent:<source>:<agent>:...`,
  `component|leader|yssarilagent|<agent>`). The rules below apply to a copy as to the original.

## What the bot did before (baseline survey)

`SURVEY_TURN_ONE=1-48 SURVEY_TURN_ONE_ROUNDS=2` (192 seats, 24 factions), decision dumps of
rounds 1-2. "Offered / used" counts every time the option was on the table:

| Offer | offered | used |
|---|---:|---:|
| `component|leader|xxchaagent` | 92 | 3 |
| `component|leader|muaatagent` | 65 | 5 (the following `umbat_player` was then declined at random) |
| `component|leader|arborecagent` | 46 | 3 (the cruiser became a cruiser again, at random) |
| `component|leader|nekroagent` (action + end of turn) | 46 | 11 (the payment was then declined) |
| `component|leader|hacanagent` | 10 | 4 (3 of 4 replenished an **opponent**) |
| Nomad Mercer (after any tactical action) | 67 | 26 |
| Saar Mendosa (any activation), + Ssruu copies | 75 | 37 |
| Argent agent (any ground production) | 29 | 19 (mostly for opponents) |
| Sardakk T'ro (any tactical action) | 28 | 14 |
| Cabal agent (another seat replenishes) | 28 | 12 (capture at random, often a fighter) |
| Thundarian, Winnu, Artuno, Naaz, Yin, L1Z1X | 10-22 each | about half |
| Ghost, Letnev, Sol, Titans, Empyrean, Naalu | 0 | - |

So: component agents were almost never used (the bot's action rule answers `tactical` /
strategic / pass, and only the random fallback ever reached a component), and reaction agents were
used at a coin flip, **about half the time for an opponent's benefit**.

## Engine gaps found

1. **Naalu's PoK agent Z'eu (`naaluagent`) had no implementation.** PoK games deal `naaluagent`
   (`leaders::for_faction` falls back to it when the Thunder's Edge `naaluagent-te` is out of
   scope); nothing offered or resolved it, so Naalu played every game without an agent.
   **Fixed in this branch** (engine: `leaders::ZEU`, `leaders::zeu_systems`, `Game::perform_zeu`;
   tests `zeu_performs_a_tactical_action_without_a_command_token`,
   `zeu_offers_no_home_or_tokened_system_and_may_be_declined`): offered as the component action
   `component|leader|naaluagent`; the Naalu choose a non-home system without their command token
   (`zeu_activate_system`, declinable), the agent exhausts and they perform a tactical action
   there without placing a token; it ends the turn like any tactical action. Limitation: only the
   Naalu seat itself is "that player" (giving another seat a tactical action inside the Naalu's
   turn needs turn-ownership plumbing the engine does not have), and Ssruu does not copy it.
2. **Letnev's Viscount Unlenn and Sol's Evelyn DeLouis are never offered to their owners.**
   `leaders::dispatch_leader` has an arm for them, but no window calls it; only Ssruu copies have a
   combat-round window (`factions::borrowed_round_agents`). The dispatch arm also hard-codes the
   unit (a cruiser / an infantry) instead of letting the owner choose. Not fixed here (low early
   value: few combats in rounds 1-3); the fix is to register an owner's slot in
   `borrowed_round_agents::abilities` that exhausts the owner's agent instead of Ssruu.
3. **Naalu's Thunder's Edge Z'eu** (`naaluagent-te`) is implemented but unreachable in PoK games
   (see 1). Not a bug.
4. **Replay divergences around agents (not fixed, found by the survey's replay check).**
   - A Ssruu copy of Umbat (`component|leader|yssarilagent|muaatagent`) that resolves nothing
     (the Yssaril have no war sun) fails as a component action live, and the recorded game then
     no longer replays (`script wanted "muaat" but p3 was offered [strategic...]`, turn-one
     survey seeds 29 and 33, reached through the random fallback). The bot now never leaves such a
     menu to chance.
   - When the Jol-Nar use Doctor Sucaban for their own research, a later research by the Yssaril
     is offered a Ssruu copy of Sucaban on replay that the live game did not offer (`script wanted
     "end_turn" but p1 was offered ["yes", "decline"]`, seeds 19 and 30; gone when the Jol-Nar
     decline). The bot therefore never uses Sucaban (low early value anyway).
   Both point at Ssruu-copy bookkeeping that differs between a live game and its replay; worth an
   engine investigation, since a human seat can trigger the same paths.

## Per faction

Ratings: value in round 1 / 2 / 3 (H high, M medium, L low, - none) for this bot's games, and
the effort of the bot rule (S small, M medium, L large).

### Arborec - Letani Ospha (`arborecagent`)
- **Text.** ACTION: exhaust, choose a player's non-fighter ship: that player may replace it with
  one from reinforcements costing up to 2 more.
- **Engine.** Implemented (`factions/arborec.rs`, `ospha_ship` then `ospha_replace` asked to the
  ship's owner). Offered as `component|leader|arborecagent`.
- **Early use.** The community opening: turn the home cruiser (2) into a carrier (3) on the first
  action of round 1, so Arborec open with two carriers and take a third system (r/ti "First Arborec
  win", "Best turn one as Arborec speaker"). Later rounds: destroyer -> cruiser, cruiser ->
  dreadnought (4) as a free upgrade on a quiet turn.
- **Interaction with the openings.** Arborec's plan builds the second transport where their troops
  land (`ground_produces`); the agent makes that transport on turn 1 instead, before the
  activations, and costs no token or resources.
- **Bot rule.** Round 1 (transport plan short, `transport_plan`): first action, own cruiser or
  destroyer outside a tokened system, replace with a carrier. Rounds 2-3: as a free action
  instead of passing or when out of tactic tokens, upgrading to the dearest ship allowed. When
  another seat's Ospha offers to upgrade a bot ship, take the dearest (carrier while short of
  transports).
- **Value** H / M / L. **Effort** S.

### Argent Flight - Trilossa Aun Mirik (`argentagent`)
- **Text.** When a player produces ground forces in a system: exhaust: that player may place them
  on any planets they control in that system and adjacent systems.
- **Engine.** Implemented (`factions/argent.rs`, `production.rs`), reaction
  `reaction_when_GROUND_FORCES_BEING_PRODUCED`, then `place_unit` per unit.
- **Early use.** Free movement of newly built infantry to adjacent frontier planets (defence, or
  staging for the next carrier). In rounds 1-2 the bot's plan wants new troops at the dock, where
  the carriers load, so it adds little.
- **Bot rule.** Never for another seat. Own production: decline in rounds 1-2, use from round 3.
- **Value** L / L / M. **Effort** S.

### Vuil'raith Cabal - The Stillness of Stars (`cabalagent`)
- **Text.** After another player replenishes commodities: exhaust to convert their commodities to
  trade goods and capture 1 unit from their reinforcements costing up to their commodity value.
- **Engine.** Implemented (`factions/cabal.rs`), reaction after `COMMODITIES_REPLENISHED`, then
  `stillness_target` (unit names).
- **Early use.** Community: always use, capture the dearest unit (a carrier or dreadnought from a
  3-4 commodity faction) - it denies the unit and feeds Cabal's free production of captured units.
  The cost is the gift of the commodities as trade goods.
- **Bot rule.** Use when the replenishing seat's commodity value is 3 or more; capture the dearest
  unit offered (ties: capacity first).
- **Value** M / M / M. **Effort** S.

### Empyrean - Acamar (`empyreanagent`)
- **Text.** After a player moves ships into a system that does not contain any planets: exhaust:
  that player gains 1 command token.
- **Engine.** Implemented (`factions/empyrean_units.rs`), reaction after `MOVEMENT_FINISHED`.
- **Early use.** +1 command token for the Empyrean whenever they move into an empty system
  (frontier exploration, Voidwatch positions). A token is worth about a system in round 1.
- **Bot rule.** Own moves: always use. Other seats: never. (The bot rarely moves into empty
  systems in the opening, so the use is rare.)
- **Value** M / M / M. **Effort** S.

### Ghosts of Creuss - Emissary Taivra (`ghostagent`)
- **Text.** After a player activates a system that contains a non-delta wormhole: exhaust: that
  system is adjacent to all other systems with a wormhole during this tactical action.
- **Engine.** Implemented (`factions/ghost.rs`), reaction after `SYSTEM_ACTIVATED`; the movement
  options read the link. Never offered in the baseline (the bot rarely activates wormhole systems
  in rounds 1-2).
- **Early use.** Community: take Mallice (the Wormhole Nexus) in round 1 using the agent. The
  bot's activation value does not count the link, so it picks wormhole targets only when they are
  reachable anyway.
- **Bot rule.** Own activation with own ships waiting in another wormhole system: use. Others:
  never. Gap: `activation_value` does not see the agent's reach (effort M).
- **Value** M / L / L. **Effort** S (rule), M (planning).

### Emirates of Hacan - Carth of Golden Sands (`hacanagent`)
- **Text.** During the action phase: exhaust to gain 2 commodities or replenish another player's
  commodities.
- **Engine.** Implemented (`leaders.rs`), `component|leader|hacanagent` in the action and
  end-of-turn choices; `leader_hacanagent_branch` (`self` or a seat).
- **Early use.** Two commodities to wash with a neighbour (2 trade goods for the partner, and the
  partner's commodities back), or a favour to a trade partner. For a bot that trades rarely, the
  own 2 commodities are a small plus; replenishing an opponent is a gift.
- **Bot rule.** At the end of its own turn, when below the commodity cap by 2 or more: use on
  itself. Never replenish another seat.
- **Value** L / L / L. **Effort** S.

### Universities of Jol-Nar - Doctor Sucaban (`jolnaragent`)
- **Text.** When a player spends resources to research: exhaust to let that player remove any
  number of their infantry from the board, reducing the cost by 1 each.
- **Engine.** Implemented (`strategy_cards::doctor_sucaban`), `doctor_sucaban_exhaust` then
  `doctor_sucaban_remove_infantry` per infantry.
- **Early use.** Pays Technology's second research (6) or the secondary (4) with spare infantry -
  Jol-Nar infantry are weak fighters, but rounds 1-2 need them for expansion.
- **Bot rule.** Planned: own research only, removing spare infantry. **Implemented: never use**
  (its own use was measured to break later replays, gap 4).
- **Value** L / M / M. **Effort** S.

### L1Z1X - I48S (`l1z1xagent`)
- **Text.** After a player activates a system: exhaust to let that player replace 1 of their
  infantry in the active system with a mech.
- **Engine.** Implemented (`reactions.rs`), reaction after `SYSTEM_ACTIVATED`, then
  `l1z1x_agent_swap` (`use`/`decline`).
- **Early use.** A free mech (2 resources, sustain, better combat) on a planet in the activated
  system - typically home or a planet just reinforced.
- **Bot rule.** Own activation: use. Other seats: never.
- **Value** M / M / M. **Effort** S.

### Barony of Letnev - Viscount Unlenn (`letnevagent`)
- **Text.** At the start of a space combat round: exhaust to choose 1 ship in the active system:
  it rolls 1 additional die this round.
- **Engine.** **Not offered to its owner** (gap 2). Only a Ssruu copy is offered.
- **Early use.** +1 die in the first space combat of the round (Letnev's round-1/2 fights are rare;
  their plan is a fleet push from round 3).
- **Bot rule.** For a Ssruu copy: use when the bot is a combatant, on its own best ship.
- **Value** - / L / M. **Effort** S (rule), M (engine).

### Mentak Coalition - Suffi An (`mentakagent`)
- **Text.** After the Pillage faction ability is used against another player: exhaust: you and
  that player each draw 1 action card.
- **Engine.** Implemented (`factions/mentak.rs`), `suffi_an` (`exhaust`/`decline`).
- **Early use.** An action card for Mentak (and one for the victim) every round it pillages.
  Symmetric, but Mentak holds more cards and the victim's anger is already paid by the Pillage.
- **Interaction with the openings.** Mentak's round-1 plan is thin on systems; this is not a
  movement agent, so it does not help the third system.
- **Bot rule.** Always use.
- **Value** L / L / L. **Effort** S.

### Embers of Muaat - Umbat (`muaatagent`)
- **Text.** ACTION: exhaust to choose a player: that player may produce up to 2 units each costing
  4 or less in a system with one of their war suns or their flagship.
- **Engine.** Implemented (`factions/muaat.rs`), `umbat_player` then the ordinary production.
- **Early use.** Community: round 1, build a carrier and a destroyer (or infantry) at home with
  the war sun, without a token in home, so the new carrier still moves and Muaat takes two
  neighbouring systems (r/ti "advice on how to play Muaat", "Advice for turn 1 as Muaat").
- **Bot rule.** Rounds 1-2: as the opening's token-free home build, where the plan wants a new
  transport at home (`home_build_wanted`) and a carrier is affordable; choose itself, build through
  the ordinary production rule. Later: instead of passing, with 4+ spare resources. (Used as the
  very first action it cost Muaat a system in the survey: the resources and the turn went before
  the transports had left.)
- **Value** H / M / L. **Effort** S.

### Naalu Collective - Z'eu (`naaluagent`, PoK)
- **Text.** ACTION: exhaust and choose a player: that player may perform a tactical action in a
  non-home system without placing a command token; the system still counts as activated.
  (Thunder's Edge `naaluagent-te`: after any command token is placed, return it.)
- **Engine.** **Was not implemented** (gap 1); now delivered for the Naalu seat itself.
- **Early use.** A tactical action without a token: in round 1 that is a third (or fourth) system
  with the same three tactic tokens; community: "take Warfare and use the agent to move both
  carriers forward".
- **Bot rule.** When the action rule would take a tactical action and a non-home system without
  its token is worth activating, use Z'eu for it instead (the token stays in the pool for the
  next turn). The system is chosen by the ordinary activation value.
- **Value** H / H / M. **Effort** S (rule), M (engine).

### Nekro Virus - Nekro Malleon (`nekroagent`)
- **Text.** During the action phase: exhaust to choose a player: they may discard 1 action card or
  spend 1 command token from their command sheet to gain 2 trade goods.
- **Engine.** Implemented (`factions/nekro_units.rs`), `component|leader|nekroagent`,
  `malleon_player`, `malleon_payment` (`discard`, `token|<pool>`, `decline`).
- **Early use.** Two trade goods for a spare action card: in round 1 that is most of a carrier.
  Spending a token for 2 trade goods is poor before the last turn of a round.
- **Bot rule.** At the end of its turn (or before passing) while holding an action card: choose
  itself, discard. When asked to pay by another Nekro: discard if it has a card, never a token.
- **Value** M / M / L. **Effort** S.

### Nomad - Artuno, Mercer, The Thundarian (three agents)
- **Artuno** (`nomadagentartuno`): when you gain trade goods from the supply, exhaust to place an
  equal number on this card; you gain them when it readies. Implemented (`nomad_agents.rs`, after
  `TRADE_GOODS_GAINED`, only for the Nomad's own gains). It doubles one gain, a round later.
  **Rule:** always use. Value M / M / M.
- **Field Marshal Mercer** (`nomadagentmercer`): at the end of a player's turn, exhaust to let
  that player move up to 2 of their ground forces onto planets they control in the active system.
  Implemented, after `TACTICAL_ACTION_ENDED`, then `mercer_take` per unit. **Rule:** never for
  another seat; own turn: take only spare ground forces (a planet holding two or more, home
  holding more than its transports lift). Value L / L / M.
- **The Thundarian** (`nomadagentthundarian`): after the Roll Dice step of combat, exhaust: no
  hits are assigned; roll again. Implemented (space and ground). The window carries no dice, so
  the bot cannot tell a bad roll. **Rule:** never in a combat it is not part of; own combats left
  to chance. Value L / L / M.
- **Effort** S.

### Naaz-Rokha - Garv and Gunn (`naazagent`)
- **Text.** At the end of a player's turn: exhaust to let that player explore 1 of their planets.
- **Engine.** Implemented (`factions/naaz.rs`), after `TURN_PASSED`, then `agent_planet`.
- **Early use.** A free exploration each round: trade goods, fragments (Naaz's relic plan),
  attachments.
- **Bot rule.** Own turn: use. Others: never.
- **Value** M / M / M. **Effort** S.

### Clan of Saar - Captain Mendosa (`saaragent`)
- **Text.** When a player activates a system: exhaust to raise 1 of that player's ships' move to
  the highest move value on the board.
- **Engine.** Implemented (`factions/saar.rs`), after `SYSTEM_ACTIVATED`, then `agent_ship`
  (labels `<unit> in <system> (move a to b)`).
- **Early use.** A carrier reaching a system two hexes away (or a slow Floating Factory) - a
  third system in round 1 without Gravity Drive. The bot plans activations without it, so the
  agent only matters when a transport could otherwise not reach.
- **Bot rule.** Never for another seat (the baseline did it about half the time). Own activation:
  boost a transport with ground forces to load whose own move cannot reach the active system but
  the raised move can; otherwise keep it.
- **Value** M / L / L. **Effort** S.

### Sardakk N'orr - T'ro (`sardakkagent`)
- **Text.** At the end of a player's tactical action: exhaust: that player may place 2 infantry
  from reinforcements on a planet they control in the active system.
- **Engine.** Implemented (`factions/sardakk.rs`), after `TACTICAL_ACTION_ENDED`, then
  `place_units_from_reinforcements`.
- **Early use.** Two free infantry on a planet just taken: defence and a stock for the next
  carrier.
- **Bot rule.** Own tactical action: use, on the planet it controls there with the fewest of its
  ground forces. Others: never.
- **Value** M / M / M. **Effort** S.

### Federation of Sol - Evelyn DeLouis (`solagent`)
- **Text.** At the start of a ground combat round: exhaust to choose 1 ground force in the active
  system: it rolls 1 additional die.
- **Engine.** **Not offered to its owner** (gap 2); Ssruu copies only.
- **Bot rule.** Ssruu copy: use when the bot fights the ground combat, on its own unit.
- **Value** L / L / M. **Effort** S (rule), M (engine).

### Titans of Ul - Tellurian (`titansagent`)
- **Text.** Before a hit would be assigned: exhaust to cancel that hit.
- **Engine.** Implemented (`factions/titans_leaders.rs`, `combat.rs`, `invasion.rs`), when
  `HITS_TO_ASSIGN` / `GROUND_HITS_TO_ASSIGN`.
- **Early use.** One cancelled hit per round: often a ship or a planet saved in an early fight.
- **Bot rule.** Use when the hit lands on its own units; never on an opponent's.
- **Value** L / M / M. **Effort** S.

### Winnu - Berekar Berekon (`winnuagent`)
- **Text.** When 1 or more of a player's units use PRODUCTION: exhaust to reduce the combined cost
  by 2.
- **Engine.** Implemented (`factions/winnu.rs`), after `PRODUCTION_USED`.
- **Early use.** Two resources off the first production of the round: in round 1 the home build
  of the second carrier costs 1 instead of 3. Winnu have the weakest round 1 in the baseline (2.0
  systems).
- **Bot rule.** Own production: use. Others: never.
- **Value** H / M / M. **Effort** S.

### Xxcha Kingdom - Ggrocuto Rinn (`xxchaagent`)
- **Text.** ACTION: exhaust to ready any planet; if it is in a system adjacent to a planet you
  control, you may remove 1 infantry from it.
- **Engine.** Implemented (`leaders.rs`), `component|leader|xxchaagent`,
  `leader_xxchaagent_ready_planet`, `leader_xxchaagent_remove_infantry`.
- **Early use.** Community: a stalling action ("the agent buying a turn") and resources for the
  build: ready the home's 3-resource planet after a round-1 production or research.
- **Bot rule.** Instead of an actual pass, with an exhausted planet of value: ready its own richest
  planet; never remove its own infantry. (Used in round 1 for the plan's resources it cost the
  turn's tempo and a system, so that use was dropped.)
- **Value** M / M / M. **Effort** S.

### Yin Brotherhood - Brother Milor (`yinagent`)
- **Text.** After a player's unit is destroyed during combat: exhaust: they place 2 fighters (ship)
  or 2 infantry (ground force) there.
- **Engine.** Implemented (`factions/yin.rs`), after `SHIP_DESTROYED` / `GROUND_FORCE_DESTROYED`.
- **Bot rule.** Own unit destroyed: use. Others: never.
- **Value** L / M / M. **Effort** S.

### Yssaril Tribes - Ssruu (`yssarilagent`)
- **Text.** At any time: this card has the text of each other player's agent, even if exhausted.
- **Engine.** Implemented for every copyable agent (Ssruu offers carry `yssarilagent`).
- **Early use.** The best agent at the table, chosen per round: Umbat/Ospha/Z'eu-like actions,
  the Winnu discount, Mendosa's move, I48S's mech.
- **Bot rule.** The rule of the copied agent, as if it were the bot's own (self only).
- **Value** M / M / M. **Effort** S.

### Mahact Gene-Sorcerers - Jae Mir Kan (`mahactagent`)
- **Text.** When you would spend a command token during the secondary of a strategic action:
  exhaust to remove 1 of the active player's command tokens from the board and use it instead.
- **Engine.** Implemented (`factions/mahact.rs`), offered inside the secondary prompt as
  `follow|waived|0|agent|<system>`.
- **Early use.** A free secondary each round (and the active player's system is unlocked, a small
  gift). Worth more than a strategy token in round 1 (Warfare's home build, Diplomacy, Technology).
- **Bot rule.** When it would follow the secondary anyway, follow through the agent; with no
  strategy token, follow cheap secondaries (secret objective, action cards, commodities, readying)
  through the agent. Take the token from a system where the active player has no units.
- **Value** M / M / M. **Effort** S.

### Council Keleres - Xander Alexin Victori III (`keleresagent`)
- **Text.** At any time: exhaust to let any player spend commodities as trade goods.
- **Engine.** Implemented (`factions/keleres.rs`), offered at a payment
  (`keleres_agent_commodities`). Not seated by the roster.
- **Early use.** Turns Keleres' own commodities into a payment (research, production) in round 1.
- **Bot rule.** Own payment that is short: use. Not measured.
- **Value** M / M / L.

### Thunder's Edge factions (not seated in PoK games)
- **Last Bastion - Dame Briar** (`bastionagent`): when a unit is destroyed, galvanize another of
  that player's units there. Implemented. Self only.
- **Deepwrought - Doctor Carrina** (`deepwroughtagent`): when another player researches, let them
  ignore a prerequisite; you may coexist an infantry on their planet. Implemented. A favour.
- **Crimson Rebellion - Ahk Ravin** (`crimsonagent`): ACTION: a player swaps the positions of 2 of
  their ships (with transport). Implemented. Early: move a loaded carrier across the map.
- **Ral Nel - Kan Kip Rel** (`ralnelagent`): ACTION: draw 2 action cards, give 1 away. Implemented.
- **Naalu (TE) - Z'eu** (`naaluagent-te`): return a just-placed command token. Implemented.
- **Firmament - Myru Vos** (`firmamentagent`): moving ships ignore space cannon and pass through
  ships. Implemented.
- **Obsidian - Vos Hollow** (`obsidianagent`): when a ship is destroyed, the opponent destroys one
  of the same type. Implemented.
- **Creuss alternate "Who Knows?"** (`redcreussagent`): an unofficial alternate, not dealt.

## Ranked by value for effort (what was implemented)

1. **Self-only reactions** (decline every agent use that only helps an opponent; use the clearly
   good own ones): Winnu, Sardakk, L1Z1X, Naaz, Yin, Empyrean, Titans, Artuno, Saar, Argent, Ghost,
   Mercer, Thundarian, Ssruu copies (Jol-Nar: never, gap 4). One rule, S.
2. **Round-1 transports**: Arborec cruiser -> carrier, Muaat Umbat build. S.
3. **Naalu Z'eu**: engine implementation plus "a token-free tactical action". M.
4. **Free actions instead of passing**: Xxcha, Arborec, Muaat; Nekro and Hacan at the end of a
   turn. S.
5. **Cabal capture**, **Mentak draw**, **Mahact waiver**. S.

Left out: Letnev/Sol own agents (engine gap 2), the Ghost agent in activation planning (the
Mallice opening), the Saar agent in activation planning, the Thundarian without dice information,
Hacan trade planning around its commodities, Ssruu copies of action agents (only Xxcha, Nekro and
Hacan copies are taken), and Doctor Sucaban (gap 4).

## Measured effect

Turn-one survey (rounds 1-2) and midgame survey (to round 3), seeds 1-288, all-smart 4-seat tables
with roster factions (1152 seats), agent layer on vs off on the same engine (Z'eu delivered in both):

| | off | on |
|---|---:|---:|
| systems gained in round 1 (mean) | 2.86 | 2.93 |
| seats with 3+ systems in round 1 | 79% | 84% |
| systems held after round 2 | 5.27 | 5.36 |
| VP after round 2 / round 3 | 1.65 / 3.32 | 1.74 / 3.44 |
| planets after round 3 | 11.23 | 11.43 |
| tables whose replay diverged | 2 | 0 |

Per faction (n about 40-54 each, so differences below about 0.3 are noise):
Naalu round 1 2.21 -> 3.15 systems, 4.76 -> 6.02 after round 2, VP r3 3.02 -> 3.75 (Z'eu);
Arborec round 1 2.68 -> 2.98 (Ospha's carrier); Winnu round 1 2.14 -> 2.49, VP r3 2.55 -> 2.84
(the discount); Argent VP r3 3.91 -> 4.48; Empyrean VP r3 2.86 -> 3.45; L1Z1X VP r3 3.12 -> 3.60;
Yssaril VP r3 3.02 -> 3.37; Muaat round 1 2.62 -> 2.46 but VP r3 2.87 -> 3.27. Strong factions hold:
Titans 4.04 -> 4.06, Saar 3.80 -> 3.84, Jol-Nar 3.55 -> 3.72. Down without an agent rule of their own
(their neighbours got stronger): Sol VP r3 3.92 -> 3.54, Xxcha systems after round 2 5.49 -> 4.93
(the Xxcha agent is not used in round 1 at all; restricting its stall to real passes changed nothing),
Hacan VP r3 3.57 -> 3.29, Mahact 3.19 -> 2.96.

Note for anyone repeating this: the shared cargo target dir is used by other worktrees with
identically named crates; build the survey with `RUSTFLAGS="--cfg faction_agents_early"` (or any
distinct cfg) so the release artifacts are this worktree's own.
