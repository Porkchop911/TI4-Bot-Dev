# Community faction strategies (TI4) and what the rule bot could use

What the Twilight Imperium 4 community says about playing each faction, gathered in October 2026
from BoardGameGeek (strategy forum guides and threads), r/twilightimperium (faction guides,
"how do I play X" threads, tier-list threads, async win-rate data posts) and the podcasts and
blogs those threads link to. Each faction section ends with a **Bot feasibility** note for the
rule bot (`crates/ti4-server/src/session/smart_bot.rs`). The doc ends with a ranked list of the
ideas worth building and a list of ideas a rule-based bot cannot realistically follow.

This is input for bot work, not a bot spec. It is meant to sit next to the mid/late-game plan doc
(`crates/ti4-server/docs/FACTION_PLANS.md`, another branch) and the round-1 opening and
objective-scoring work going on in the bot code. Technology and build preferences are described in
prose here on purpose, so they are not copied as tables into the code.

## How to read this

- Everything is paraphrased. Each cluster of claims links the thread or guide it came from. Where
  sources disagree, the section says so ("Disagreement").
- Community advice is written for humans at a table: it assumes negotiation, deals and reading
  people. The feasibility notes say which parts survive translation into deterministic, own-seat
  heuristics.
- Ages matter. Many guides predate Prophecy of Kings (base game, 2017-2020), some predate the
  Codex updates, and the newest posts (late 2025 onwards) are about Thunder's Edge, which changes
  several factions (breakthroughs, new leaders) and generic cards. Each source is dated in its
  link text where it matters.
- "R1" means game round 1; "SC" strategy card; "2c4i" two carriers and four infantry (the
  standard way to take two systems in the first two actions); "TG" trade goods; "CC" command
  tokens; "skip" a planet's technology specialty used to ignore a prerequisite; "slice" the
  systems around a home system; "equidistant" a system halfway between two home systems.

### Source availability

- BoardGameGeek pages answer the web fetcher with HTTP 403, but the forum API
  (`api.geekdo.com/api/forums/threads`, `.../api/articles?threadid=`) answered, so BGG threads
  were read in full. Links point at the normal `boardgamegeek.com/thread/<id>` page.
- Reddit itself is blocked for the fetcher. Threads were read through the Arctic Shift archive
  (`arctic-shift.photon-reddit.com`), which mirrors posts and comment trees. Links point at the
  normal `reddit.com` permalink. Images (tier-list pictures, map screenshots) could not be read;
  where a tier list exists only as a picture, only its text transcription in the comments is used.
- The TI4 Fandom wiki faction pages were reachable through the MediaWiki API but have no strategy
  sections (abilities, FAQ and trivia only), so they are not cited for strategy.
- The Space Cats Peace Turtles (SCPT) faction "Strategy Guide" episodes are audio. Only what the
  Reddit announcement threads and their comments say about them is used.
- The general web search tool returned mostly spam PDFs for "faction guide" queries; those were
  ignored.

### Engine scope that changes the advice

Checked against `crates/ti4-content/content/*.json` and the engine/server code:

- Server games are built from the `POK` source set (`ti4_model::content_types::POK`): base game,
  Prophecy of Kings and Codex I-IV, newest Omega version of each card. Thunder's Edge content is in
  the corpus but not in play: no breakthroughs, no Thunder's Edge factions, and the Thunder's Edge
  card changes are not active. Codex IV versions are: X-89 Bacterial Weapon ΩΩ ("double the hits
  of your bombardment and ground combat rolls") and Magen Defense Grid ΩΩ. Advice that depends on
  Thunder's Edge (breakthrough synergies, the Thunder's Edge Warfare/Xxcha/Mentak changes,
  Crimson/Deepwrought tier talk) is marked as such.
- Seatable factions (`crates/ti4-server/src/preset.rs` `ROSTER`): the 17 base factions and the 7
  Prophecy of Kings factions. The Council Keleres only by explicit request (`+fac:keleresm` and
  so on); the Thunder's Edge factions cannot be seated (their home tiles are not in the source
  set). They get a short appendix only.
- Starting technology choices are not modelled: Winnu, Argent Flight and the Keleres start with
  no technology here (`startingTech` empty), where the printed sheets let them choose. Sardakk
  N'orr start with none, as printed. This makes those factions weaker than the community
  assumes, and removes the "pick a starting tech" decision from the bot.
- Some faction abilities are still blocked in the engine (`ti4_engine::faction_abilities::blocked`,
  minus those a faction module now implements): check before building a bot habit around one.
  Arborec Mitosis, Yssaril Stall Tactics and Naalu Telepathic do have faction modules.
- The Nekro Virus cannot research (Propagation: research gives 3 command tokens instead), as
  printed.
- The rule bot's fight model (`ti4_policy::fleet_strength`) knows only two faction combat
  modifiers: Jol-Nar -1 and Sardakk +1 (plus the Jol-Nar flagship's extra hits). It does not value
  Mentak Ambush, Letnev Munitions Reserves, Argent anti-fighter barrage, the Yin, Winnu, Mahact or
  Sardakk flagships, or Naaz-Rokha mechs in space.

### What the rule bot can act on today

From the module docs and the `pick` dispatcher: it has opinions on scoring (always), the action
choice, system activation, movement, loading, landing, retreat, production, payment, command-token
pools and redistribution, the strategy-card draft (opening table plus "Imperial when it scores"),
following Technology/Imperial/Warfare secondaries, research (only "Gravity Drive whenever
offered"), trades (accept slightly positive, propose mutual gain) and over-fleet-supply removal.
Everything else (other research, agendas, action cards, faction-ability prompts such as Pillage,
Mitosis placement, Indoctrination, Terragenesis, leader abilities) is answered by the random bot.
It sees only its own seat's observation: public board, public objectives, other seats' public
technologies and its own hand. So "faction knowledge" for the bot means:

- **Draft preference**: per-faction adjustments to `opening_card_score` and the later-round pick.
- **Tech priority**: an ordered wish list per faction for `research_technology` choices (today
  only Gravity Drive is preferred).
- **Build mix**: per-faction weights in `pick_produce` (for example dreadnoughts for Letnev and
  L1Z1X, cruisers for Titans and Mentak, cheap destroyers for Argent, infantry for Sol and
  Arborec, mechs for Naaz-Rokha).
- **Movement/targeting**: per-faction terms in `activation_value` and the fight model.
- **Objective focus**: which public objectives a faction is good at (structures for Titans and
  Argent, "spend" objectives for Hacan, technology objectives for Jol-Nar and Nekro).
- **Faction-ability prompts**: replacing the random answer to an engine prompt with a rule
  ("always use Pillage when offered against a non-ally", "place Mitosis infantry on the most
  threatened planet").
- **Diplomacy**: only through the existing trade accept/propose hooks; no table talk.

Value/effort ratings use: value = expected effect on the bot's results; effort = S (a few lines
in an existing hook), M (a new hook or state reading), L (needs new observation data, a planner,
or engine work).

## Community tier picture

Three tier lists with very different methods; none is authoritative. The two PoK-era columns
matter most for this engine, which plays without Thunder's Edge.

| Faction | SCPT tier list, Jan 2022 (PoK), row 1 = best of 5 | SCPT tier list, Dec 2025 (Thunder's Edge) | Async win-rate tiers, 6000+ 6-player PoK games, Mar 2025 | Notes |
|---|---|---|---|---|
| Mahact | 4 | S | S+ |  |
| Yssaril | 3 | A (many commenters say S) | S+ | About 26% wins in 6-player async games |
| Jol-Nar | 1 | S | S |  |
| Naaz-Rokha | 2 | A | S |  |
| Empyrean | 2 | B | S | Async: third-best win rate, a surprise to the poster |
| Sol | 2 | S | A |  |
| Titans | 3 | A | A |  |
| Nekro | 1 | B | A |  |
| Winnu | 3 | D | A | Wins when the table does not punish them early |
| Naalu | 5 | B | A | SCPT 2022: too slow for 5-round PoK games |
| Saar | 1 | S | B | Commenters: overrated, draws heat |
| Xxcha | 4 | F (Thunder's Edge nerfs) | B |  |
| Yin | 5 | C | B |  |
| Argent | 2 | F (7% async win rate in Thunder's Edge) | B | Commenters split: "a menace to attack" |
| Hacan | 2 | A | B |  |
| Nomad | 1 | B | B |  |
| Cabal | 3 | A | B |  |
| Ghosts | 4 | A | B |  |
| Muaat | 5 | D | B | Commenters: "way too low" |
| L1Z1X | 2 | C | C |  |
| Arborec | 5 | C | C |  |
| Sardakk | 5 | D | C |  |
| Mentak | 4 | F | C | Lowest average VP in the async data |
| Letnev | 4 | B | D | Lowest async win rate |
| Keleres | (not listed) | B | (not listed) |  |

Sources: [SCPT's "Definitive" PoK faction tier list thread, Jan 2022](https://www.reddit.com/r/twilightimperium/comments/rxq1hs/scpts_definitive_pok_faction_tier_list/)
(rows transcribed by a commenter; tournament-based, conquest-minded hosts);
[SCPT Faction Tier List (2025) thread, Dec 2025](https://www.reddit.com/r/twilightimperium/comments/1q0jwu5/scpt_faction_tier_list_2025_one_billion_percent/)
(tiers transcribed by a commenter);
[10 point tier list from 6000+ 6-player async games, Mar 2025](https://www.reddit.com/r/twilightimperium/comments/1jg2dlh/10_point_tier_list_according_to_data_from_6000_6/)
(win-rate based, PoK + codices). An older survey analysis,
[Bellum Gloriosum "What is the Best Faction and Why?", May 2022](https://bellumgloriosum.substack.com/p/what-is-the-best-faction-and-why),
found Winnu's tabletop-simulator win rate far above its community ranking and named an opening that
takes two systems with two planets each ("2c4i") and blue technology as the best predictors of
success; its tables are images and could not be read. An early-PoK data sheet discussed on
[BGG, 2021](https://boardgamegeek.com/thread/2680870) had Winnu and Xxcha on top and Hacan at the
bottom. Disagreement is the rule: tier lists are table-meta dependent, and async data includes many
new-player games. The Yssaril 26% figure (against a 16.7% baseline) is from a comment in
[r/ti "Factions tier list", Jan 2026](https://www.reddit.com/r/twilightimperium/comments/1qctdtz/factions_tier_list/),
which also notes that Mentak rose to about +7% in Thunder's Edge async data.

## Cross-faction knowledge the bot already half-follows

- Round 1 is about taking as many planets as possible: get a second transport (Warfare's
  secondary builds one at home without a token in that system), take two systems in the first two
  actions, and keep the Warfare holder from stalling you out
  ([BGG IncrediSteve on Ghosts R1, 2024](https://boardgamegeek.com/thread/3232040);
  [BGG one-carrier R1 card order, 2019](https://boardgamegeek.com/thread/2259222);
  [BGG Arborec second carrier, 2024](https://boardgamegeek.com/thread/3408517)).
- Blue technology (Gravity Drive first) is the default first research for almost every faction;
  "movement is never bad" ([BGG Sardakk guide, 2019](https://boardgamegeek.com/thread/2323754);
  [BGG Basic Guide to Technology v2, 2026](https://boardgamegeek.com/thread/3690331)).
- Expect 4-6 researches in a 10-point game; plan a path that covers "2 technologies in 2 colours"
  or "2 unit upgrades" early, not a seven-tech dream
  ([BGG Sardakk guide](https://boardgamegeek.com/thread/2323754);
  [BGG Barony guide, 2023](https://boardgamegeek.com/thread/3107097)).
- Bringing one more unit beats +1 to combat rolls until roughly eight units a side
  ([BGG Sardakk guide](https://boardgamegeek.com/thread/2323754)); mass wins fights with
  disproportionately small losses ([BGG Nekro guide, 2019](https://boardgamegeek.com/thread/2283392)).
- Do not lead early; be third. The table gangs up on the leader, then on the next one
  ([BGG "Strategies, tricks, tactics you wish you knew", 2020](https://boardgamegeek.com/thread/2390154);
  [BGG Barony guide](https://boardgamegeek.com/thread/3107097)).
- Taking Mecatol Rex in round 1 usually overextends; set up a top-of-round-2 capture instead
  ([BGG Naalu MR thread, 2023](https://boardgamegeek.com/thread/3090351);
  [BGG Sol opening, 2023](https://boardgamegeek.com/thread/3177352)).
- Politics in the second-to-last round makes you speaker for the last round
  ([BGG tricks thread](https://boardgamegeek.com/thread/2390154)).
- The flagship is a liability for many factions: it gives the "destroy their flagship" secret to
  opponents ([BGG tricks thread](https://boardgamegeek.com/thread/2390154)).

## Base game factions

### The Arborec (`arborec`)

**Engine check.** Home Nestphar 3/2; start 1 carrier, 1 cruiser, 2 fighters, 4 infantry, PDS, dock;
starting tech Magen Defense Grid (the ΩΩ version in this corpus). Letani Warrior infantry have
PRODUCTION 1 (2 after Letani Warrior II, GG). Mitosis: docks cannot produce infantry; at the start of
the status phase place 1 infantry on any controlled planet. Mech has PRODUCTION 2, planetary
shield, sustain, and deploys in place of a Mitosis infantry. Agent: replace any non-fighter ship
with one costing up to 2 more (cruiser to carrier at home in round 1). Commander (12 ground forces
on planets): produce 1 unit when another player activates a system with your production units.
Hero: produce in every system with your ground forces. Flagship: produce 5 after activating its
system.

**Opening.** In PoK the agent turns the cruiser into a carrier at home on turn 1, making Arborec a
two-carrier opener; move the carrier with infantry, take a system, produce there (infantry make
infantry) ([r/ti "First Arborec win, 7 players", 2021](https://www.reddit.com/r/twilightimperium/comments/l8j9mt/first_arborec_win_7_players_the_power_of_pok/);
[r/ti "Best turn one as Arborec speaker", 2025](https://www.reddit.com/r/twilightimperium/comments/1k6v2yu/best_turn_one_as_arborec_speaker_base_game_preset/)).
A second carrier is better built off Warfare's secondary at home than in an activated system,
because a ship built in an activated system cannot move again that round
([BGG, 2024](https://boardgamegeek.com/thread/3408517)). Hazardous explorations that kill
infantry hurt Arborec more than anyone ([r/ti 2021](https://www.reddit.com/r/twilightimperium/comments/l8j9mt/first_arborec_win_7_players_the_power_of_pok/)).

**Strategy cards.** Technology first in round 1 and play it at once for Sarween Tools, so every
later "move then produce" is free infantry ([r/ti 2025](https://www.reddit.com/r/twilightimperium/comments/1k6v2yu/best_turn_one_as_arborec_speaker_base_game_preset/));
Trade, then Leadership, are named the best cards overall
([BGG "Arborec And You!", 2022](https://boardgamegeek.com/thread/2955541);
[r/ti "First time playing Arborec", 2023](https://www.reddit.com/r/twilightimperium/comments/12blb9z/first_time_playing_arborec_any_tips/)).
Disagreement: some say Arborec need not take Warfare in round 1 thanks to the agent; others build a
second carrier with it and take the whole slice.

**Technology.** Sarween Tools first is the broad consensus. Two paths from there: red (Self-Assembly
Routines, Duranium Armor, PDS II) to hold a rich slice, or blue (Antimass, Gravity Drive, Carrier
II, Dreadnought II) when the slice needs movement ([BGG 2022](https://boardgamegeek.com/thread/2955541)).
Disagreement: in PoK some players rate Self-Assembly Routines above Sarween because a free mech per
production doubles production capacity ([r/ti 2021](https://www.reddit.com/r/twilightimperium/comments/l8j9mt/first_arborec_win_7_players_the_power_of_pok/)).
Green is called a trap (Bioplasmosis weak; Letani Warrior II not worth it next to mechs) by the BGG
guide, while a 2023 Reddit reply calls Letani Warrior II "huge" for the engine. Fleet Logistics late.

**Builds.** Infantry everywhere, at least two per owned planet, mechs on key planets for planetary
shield against bombardment; do not spend early money on a wide ship fleet
([r/ti 2023](https://www.reddit.com/r/twilightimperium/comments/12blb9z/first_time_playing_arborec_any_tips/)).
The flagship divides opinion: "it is a space dock, a trap" (BGG) versus "build it for the Mecatol
Rex invasion and leave it there" (Reddit).

**Objectives.** Control and planet-count objectives; dig into Mecatol Rex with infantry late,
surprise with Imperial and the hero. Arborec are much better in 4-player and 14-point games, where
there is time to snowball ([BGG "Are Arborec just waiting for their moment?", 2020](https://boardgamegeek.com/thread/2390209)).

**Diplomacy.** Play up the weak reputation ("we are not a threat"); sell the agent; Arborec are
good at eating a weak neighbour's slice if it comes to that ([r/ti 2023](https://www.reddit.com/r/twilightimperium/comments/12blb9z/first_time_playing_arborec_any_tips/)).

**Mistakes.** Forgetting Mitosis each status phase; forgetting to produce after moving; spreading
thin with one infantry per planet; exploring hazardous planets with the only infantry
([r/ti SCPT Ep 163 thread, 2020](https://www.reddit.com/r/twilightimperium/comments/jousab/scpt_ep_163_arborec_strategy_guide/);
[r/ti 2026 "forgot to produce"](https://www.reddit.com/r/twilightimperium/comments/1r62xpt/)).

**Matchups.** L1Z1X (Harrow bombardment, dreadnoughts) and Muaat (war suns) are hard counters;
X-89 and the Plague action card are feared ([BGG 2020](https://boardgamegeek.com/thread/2390209);
[r/ti "How to destroy Arborec as L1Z1X", 2022](https://www.reddit.com/r/twilightimperium/comments/w6o8y9/how_to_destroy_arborec_as_l1z1x/)).

**Strength.** Bottom tier in base game and in SCPT's 2022 PoK list; C in both 2025 lists. A vocal
minority says PoK made them good.

**Bot feasibility.**
- Production is the faction: every activation of a system holding Arborec infantry or mechs is a
  production. `activation_value` already counts own production in a system (`has_own_production`
  uses `has_production()`), but only gives it value with six or more spendable resources. For
  Arborec, lower that bar to "can pay for two infantry" so the bot activates its own forward
  systems to grow. Value high, effort S.
- Opening: use the agent on the home cruiser in round 1 (component action
  `component|leader|arborecagent`), which the rule bot never picks today (component actions are
  only reached through the random fallback). Value high, effort M (needs a component-action hook,
  shared with Sol, Muaat and others below).
- Draft: Technology first in round 1, then Trade/Leadership. Effort S (per-faction weights in
  `opening_card_score`).
- Research: Sarween Tools first, then Self-Assembly Routines or Gravity Drive. Effort S once a
  per-faction wish list exists.
- Build mix: infantry and mechs over ships; keep two ground forces per planet. Effort S/M in
  `pick_produce` (the "ground glut" guard must not apply to Arborec infantry, which are production).
- Mitosis placement prompt (`mitosis_place`): put the infantry (or mech) on the most exposed
  planet, the one nearest an enemy fleet. Effort S.
- Needs nothing new from the engine. Value/effort overall: medium value, S-M effort.

### The Ghosts of Creuss (`ghost`)

**Engine check.** Home Creuss 4/2 in the play area; the Creuss Gate (delta wormhole) sits on the
board. Start 1 carrier, 2 destroyers, 2 fighters, 4 infantry, dock; starting tech Gravity Drive.
Quantum Entanglement (alpha and beta wormhole systems adjacent), Slipstream (+1 move from home or a
wormhole system), 4 commodities. Agent: after a non-delta wormhole system is activated, it is
adjacent to every wormhole system for that action. Commander: a fighter for each capacity ship that
moved through a wormhole. Hero swaps systems. Faction techs Dimensional Splicer (R) and Wormhole
Generator (BB).

**Opening.** The PoK consensus: take Mallice (the Wormhole Nexus) in round 1 using the agent, with
the carrier, two infantry and a destroyer. It is worth 2 trade goods at once and is the best
two-round tile on the board ("always take Mallice")
([r/ti "Creuss guide/advice (PoK)", 2023](https://www.reddit.com/r/twilightimperium/comments/15h1okg/creuss_guideadvice_pok/);
[r/ti "Creuss best moves round 1", 2025](https://www.reddit.com/r/twilightimperium/comments/1pw5c20/im_creuss_what_are_my_best_moves_round_1_pok/)).
Disagreement: some send the first carrier to a two-planet system and only the second carrier to
Mallice, fearing a Warfare stall with only Creuss and the Nexus
([r/ti "overall advice on Ghosts", 2022](https://www.reddit.com/r/twilightimperium/comments/y3saxv/can_you_guys_give_an_overall_advice_on_how_to/)).
The main problem is starting with one carrier: get a second one in round 1 (Sling Relay, or
Warfare's secondary), and keep a destroyer to make a neighbour for trades and to stall
([BGG IncrediSteve, 2024](https://boardgamegeek.com/thread/3232040)).

**Strategy cards.** Trade first (4 commodities, Mallice to wash your own commodities), then
Technology, Politics, Leadership ([r/ti 2023](https://www.reddit.com/r/twilightimperium/comments/15h1okg/creuss_guideadvice_pok/)).
Disagreement: Technology first, because Sling Relay both builds the second carrier and gives a
stall ([r/ti 2024](https://www.reddit.com/r/twilightimperium/comments/1hlybs9/advices_on_creusss_startegy/)).
In base game, take Warfare and sell its timing ([BGG 2024](https://boardgamegeek.com/thread/3232040)).

**Technology.** Blue: Sling Relay (or Dark Energy Tap with frontier tokens near), then Carrier II,
Fleet Logistics, Light/Wave Deflector; Fighter II/Dreadnought II need a green or yellow skip. The
faction techs are low priority (Wormhole Generator works like the promissory note). An alternative
red line (AI Development, Cruiser II with the commander's free fighters; or Assault Cannon,
Dimensional Splicer, Destroyer II, PDS II) has fans
([r/ti 2024 guide thread](https://www.reddit.com/r/twilightimperium/comments/199sxev/a_new_faction_guide_for_the_ghosts_of_creuss_is/);
[r/ti 2022](https://www.reddit.com/r/twilightimperium/comments/y3saxv/can_you_guys_give_an_overall_advice_on_how_to/)).

**Builds.** Forward dock on a good 3-resource system (Construction, or its secondary over
Technology's) rather than a slow flagship from home; the flagship is a trap if built at home
([BGG 2024](https://boardgamegeek.com/thread/3232040)). Carry a mech with fleets: discarding it puts
a wormhole where you need one ([r/ti 2023](https://www.reddit.com/r/twilightimperium/comments/15h1okg/creuss_guideadvice_pok/)).

**Objectives and play.** An economic faction (trade, Mallice) that attacks only in the last two
rounds; very good at win-slaying, which buys friends. Do not overextend just because you can reach
anywhere ([r/ti 2022](https://www.reddit.com/r/twilightimperium/comments/y3saxv/can_you_guys_give_an_overall_advice_on_how_to/)).
Agendas that hit wormholes (Wormhole Research, Wormhole Reconstruction, Enforced Travel Ban) can
ruin a Ghosts game; keep agenda-phase cards to fight them.

**Strength.** A in SCPT 2025 (Thunder's Edge), B in async PoK data, row 4 of 5 in SCPT 2022.

**Bot feasibility.**
- Opening: agent to Mallice in round 1 is a leader prompt plus a movement the bot already scores;
  needs (a) using the agent when the engine offers it after activating a wormhole system and (b)
  `activation_value` counting Mallice's 2 trade goods. Value medium-high, effort M.
- Draft: Trade/Technology first in round 1 (the opening table currently scores Trade -10 for
  non-Hacan). Effort S.
- Research: Sling Relay, then Carrier II; Gravity Drive is already owned. Effort S.
- Movement: the engine already applies Quantum Entanglement and Slipstream to `movable_into`, so
  the bot reaches wormhole systems without extra code. Not needed.
- Needs engine support the bot cannot use: deliberate wormhole placement (mech, Wormhole
  Generator, hero) requires planning about other players' routes. Value/effort: medium value, M.

### The Emirates of Hacan (`hacan`)

**Engine check.** Home Arretze 2/0, Hercant 1/1, Kamdorn 0/1 (three planets, one dock); start 2
carriers, 1 cruiser, 2 fighters, 4 infantry; starting techs Sarween Tools and Antimass Deflectors; 6
commodities. Master of Trade (Trade secondary costs no token), Guild Ships (trade with
non-neighbours), Arbiters (action cards in transactions). Quantum Datahub Node (YYY): at the end of
the strategy phase spend a strategy token and 3 trade goods to swap a strategy card with another
player. Agent: gain 2 commodities or replenish another player's. Hero: one free production.

**Opening and cards.** Trade in round 1 is the classic advice: Hacan can trade with everyone at
once, refresh the whole table for one trade good each and end round 1 with 13-14 trade goods
([BGG "Space Cats" Hacan guide, 2019](https://boardgamegeek.com/thread/2292488);
[BGG casual Hacan guide replies, 2024](https://boardgamegeek.com/thread/3278806)).
Disagreement: the 2024 guide argues Hacan get their 6 commodities whenever anyone plays Trade, so
Technology (or Leadership, "9 is more than 8") is the better first pick, Trade only from round 4.
Order in the 2019 guide: Trade, Warfare, Technology, Politics, Construction, Leadership, Imperial,
Diplomacy.

**Technology.** Gravity Drive, then Quantum Datahub Node by about round 4 (called "the best tech in
the game" for Hacan: it guarantees Imperial or Leadership in the last round); a yellow skip in the
slice matters; Dreadnought II/Carrier II as unit upgrades; Neural Motivator round 1 in the base
game guide; Production Biomes is weak
([r/ti "How do you play the Emirates of Hacan?", 2026](https://www.reddit.com/r/twilightimperium/comments/1rqzrq3/how_do_you_play_the_emirates_of_hacan/);
[BGG 2022](https://boardgamegeek.com/thread/2811800)). With the hero, war suns become a "free
production" target ([r/ti "Hype me up to play the Hacan", 2022](https://www.reddit.com/r/twilightimperium/comments/xjduzy/hype_me_up_to_play_the_hacan/)).

**Builds.** The problem is turning money into plastic: home production is limited, so get a second
dock (Construction) or a rich forward production planet; buy ships, not ground forces. Disagreement:
three docks/Space Dock II at home versus a forward dock on a 3-resource planet
([r/ti "how do you defend as Hacan?", 2022](https://www.reddit.com/r/twilightimperium/comments/uu9u4a/how_do_you_defend_as_hacan_why_are_pds_never/)).
The flagship is weak; PDS-heavy defence has a high opportunity cost.

**Objectives.** Trade goods pay "spend" objectives; score every round quietly, hold an easy secret,
run second or third, then use Quantum Datahub Node for the final Imperial
([BGG 2019](https://boardgamegeek.com/thread/2292488); [r/ti 2026](https://www.reddit.com/r/twilightimperium/comments/1rqzrq3/how_do_you_play_the_emirates_of_hacan/)).

**Diplomacy.** Be a supermarket with fixed prices, many quick deals; take deals even when the
partner gains more, volume wins; the nightmare is a Trade holder who deals with everyone except
Hacan. Yssaril are the natural partner (action cards); Mentak is the worst neighbour (Pillage), buy
their Promise of Protection ([BGG 2019](https://boardgamegeek.com/thread/2292488);
[r/ti 2022](https://www.reddit.com/r/twilightimperium/comments/xjduzy/hype_me_up_to_play_the_hacan/)).

**Mistakes.** Taking Mecatol Rex first; building the flagship; spending all trade goods on units the
dock cannot produce; leading openly.

**Strength.** A in SCPT 2025; B in async PoK data; bottom in an early-PoK 2021 data sheet; row 2 in
SCPT 2022. The 3-planet home system makes Hacan "slayable".

**Bot feasibility.**
- The bot already drafts Trade first as Hacan (`opening_card_score`), and Master of Trade makes
  the Trade secondary free in the engine. Keep it; the community disagreement (Technology or
  Leadership first) does not justify changing it without game data.
- Trades: Hacan should accept and propose more trades than other seats (volume over margin): lower
  `ACCEPT_NET` for Hacan to about zero and propose commodity washes with every seat each round.
  Value high (it is the faction's engine), effort S-M (the transaction hooks exist).
- Research: Gravity Drive, then the yellow line to Quantum Datahub Node; use it at the end of the
  strategy phase to take Imperial when the bot can score with it (needs a decision on the QDN
  prompt). Value medium, effort M.
- Builds: prefer a second dock early (Construction primary target: a 3-resource planet or home).
  Value medium, effort S.
- Agent (component action): replenish own commodities when a trade partner is available. Effort M
  (component-action hook).
- Needs nothing from the engine.

### The Universities of Jol-Nar (`jolnar`)

**Engine check.** Home Jol 1/2 and Nar 2/3; start 1 dreadnought, 2 carriers, 1 fighter, 2 infantry,
dock, 2 PDS; starting techs Antimass Deflectors, Sarween Tools, Neural Motivator, Plasma Scoring;
4 commodities. Fragile: -1 to combat rolls (not to space cannon, barrage or bombardment rolls,
which are unit-ability rolls). Brilliant: following Technology with a token resolves the primary
(two researches). Analytical: ignore one prerequisite on non-unit-upgrade research. Faction techs
E-Res Siphons (YY: 4 trade goods when another player activates a system with Jol-Nar ships) and
Spatial Conduit Cylinders (BB). Promissory note Research Agreement: the holder gains the
non-faction technology Jol-Nar researches. Agent: pay research partly with infantry. Commander (8
technologies): reroll unit-ability dice.

**Opening.** The weak point is the start: only two infantry, so Jol-Nar depends on Warfare (or
Construction) to take more than one system in round 1. Take Warfare if it comes round; do not
double-research in round 1 unless trade partners are generous, plastic matters more
([r/ti "Universities of Jol-Nar: Tech Strategies", 2025](https://www.reddit.com/r/twilightimperium/comments/1oj5l40/universities_of_jolnar_tech_strategies/);
[BGG "Help to play Jol-Nar", 2025](https://boardgamegeek.com/thread/3605530)). Construction in
round 1 for infantry, fighters and a forward dock is a newer alternative
([r/ti "TE+PoK Jol-Nar tips", 2026](https://www.reddit.com/r/twilightimperium/comments/1vtb8ej/tepok_jolnar_general_and_specific_tips/)).

**Technology.** Hyper Metabolism in round 1 (Analytical makes it cheap and it pays back over the
game), Gravity Drive, E-Res Siphons early (deterrent and income), then Dreadnought II, Carrier II,
PDS II; Graviton Laser with PDS II "so people do not want to fight you". Lean into rolls without
the -1: bombardment, space cannon, anti-fighter barrage; prefer dreadnoughts and war suns (a -1
costs little on a 5) over fighters ([r/ti "Advice on how to play Jol-Nar", 2023](https://www.reddit.com/r/twilightimperium/comments/154yj7u/advice_for_someone_new_on_how_to_play_the_jolnar/);
[r/ti "Tips for Jol Nar", 2023](https://www.reddit.com/r/twilightimperium/comments/13v38bn/tips_for_jol_nar/)).
War sun line (Duranium, Assault Cannon, War Sun) as an alternative. Aim for "2 in 2 colours and 2
unit upgrades" before round 5, "2 in 4 colours and 3 upgrades" by round 5.

**Economy and diplomacy.** "The best economy in the game": 4 commodities, free double research on
the secondary, and the most sellable promissory note. Announce each round what you will research
and sell Research Agreement for about 3-4 trade goods (one estimate puts its value at 7 resources
and suggests splitting it); it can sometimes be sold twice a round. Specify the technology in the
deal. Sell the agent in round 1 ([BGG Research Agreement thread, 2025](https://boardgamegeek.com/thread/3540597);
[r/ti 2020](https://www.reddit.com/r/twilightimperium/comments/jszqv0/new_strategies_for_jolnar_with_al_the_new_content/)).
An aggressive variant drops cruisers and infantry into others' slices: either they pay 4 trade
goods to remove them (E-Res) or Jol-Nar keeps the planets.

**Counters.** Tables are told to punish Jol-Nar early: stall Warfare in round 1, avoid taking
Technology, buy Research Agreement and never use it, rush them with ground-heavy neighbours (Yin,
Sol, Sardakk, Saar, Letnev); Nekro feasts on their technologies
([r/ti "faction good at countering base Jol Nar", 2025](https://www.reddit.com/r/twilightimperium/comments/1py4a6f/is_there_a_faction_thats_especially_good_at/);
[r/ti "5th win in 5 games as Jol-Nar", 2023](https://www.reddit.com/r/twilightimperium/comments/11a61v9/my_5th_win_in_5_games_as_jolnar/)).

**Strength.** S/top row in every list (SCPT 2022 and 2025, async PoK data).

**Bot feasibility.**
- Research: Hyper Metabolism in round 1 if offered, then Gravity Drive, E-Res Siphons,
  Dreadnought II, PDS II. Value high (Jol-Nar researches more than anyone, so the wish list is
  used often), effort S.
- Secondary: always follow Technology (already done); Brilliant turns it into two researches.
- Build mix: dreadnoughts and PDS over fighters (the fight model already applies -1). Effort S.
- Draft: Warfare first in round 1 for Jol-Nar (fixes the two-infantry start), Technology rarely
  needed as primary. Effort S.
- Research Agreement selling needs negotiation; the bot could at least accept any offered trade
  that buys it at 3+ trade goods via the transaction hooks if the engine lists promissory notes in
  transactions. Value medium, effort M.
- Hard: the E-Res "drop ships in their slice" tactic depends on reading opponents.

### The L1Z1X Mindnet (`l1z1x`)

**Engine check.** Home [0.0.0] 5/0 (one planet); start 1 dreadnought, 1 carrier, 3 fighters, 5
infantry, dock, PDS; starting techs Neural Motivator and Plasma Scoring; 2 commodities. Super
Dreadnought I (capacity 2, bombardment 5); Super Dreadnought II (BBY: combat 4, move 2,
bombardment 4, immune to Direct Hit). Assimilate: take over PDS and docks on captured planets.
Harrow: bombard again after each ground-combat round. Inheritance Systems (YY). Commander (4
dreadnoughts on the board): bombard through planetary shields. Hero: move the flagship and all
dreadnoughts into one system.

**Opening and cards.** Expand to two systems next to home, research, build; Technology and
Leadership are the go-to cards, Technology using the 5-resource home; Warfare is "cracked"; with
the rich home even Diplomacy is a fine last pick ([r/ti "How to play L1Z1X?", 2022](https://www.reddit.com/r/twilightimperium/comments/ssbfzw/how_to_play_l1z1x/)).

**Technology.** Super Dreadnought II is the bread and butter, through blue (Antimass or a skip,
Gravity Drive) rather than the yellow faction line: the blue route gets there as fast and its
techs are useful; Inheritance Systems is "near useless in a 10-point game". Sarween early helps
unlock the commander (four dreadnoughts). Skips steer the rest: green skip for Hyper Metabolism or
Infantry II; Destroyer II or Fighter II against fighter swarms; PDS II is board dependent.
Disagreement: a minority likes Inheritance Systems into War Suns against PDS-heavy opponents (same
thread).

**Builds and targets.** Dreadnoughts with infantry inside (capacity 2); add 2-3 infantry at home with
each build to make the one-planet home untakeable. Target forward docks and PDS: Assimilate takes
them and a captured dock can produce the same turn. L1Z1X are natural Mecatol Rex holders and the
faction most able to knock Sol off it. Pressure Arborec early ([r/ti 2022](https://www.reddit.com/r/twilightimperium/comments/w6o8y9/how_to_destroy_arborec_as_l1z1x/)).

**Threats.** Fighter swarms (cheap hit absorption), Yin (Impulse Core, Devotion, Indoctrination of
the few infantry), Sardakk; Direct Hit before Super Dreadnought II. Play the "scary dreadnought
faction, do not mess with me" card diplomatically rather than attacking everyone.

**Strength.** SCPT row 2 (2022), C (2025); async PoK C.

**Bot feasibility.**
- Research: Gravity Drive (via Antimass or a skip), Super Dreadnought II, then Destroyer II or
  Fighter II when neighbours field fighters. Value medium, effort S.
- Build mix: dreadnoughts with two infantry each, plus 2-3 infantry kept at home. Effort S.
- Targeting: in `activation_value`, an enemy PDS or dock on a takeable planet is worth its cost to
  L1Z1X (Assimilate); today structures only count against the attacker. Value medium, effort S-M.
- Fight model: add Harrow to the ground-combat estimate (`ground_needed`), so the bot lands fewer
  infantry when its dreadnoughts are present. Value medium, effort M.

### The Barony of Letnev (`letnev`)

**Engine check.** Home Arc Prime 4/0 and Wren Terra 2/1 (6 resources, the richest home for
production); start 1 dreadnought, 1 carrier, 1 destroyer, 1 fighter, 3 infantry; starting techs
Antimass Deflectors and Plasma Scoring; 2 commodities. Armada: non-fighter ships per system = fleet
pool + 2. Munitions Reserves: 2 trade goods to reroll during a space combat round. Faction techs L4
Disruptors (Y) and Non-Euclidean Shielding (RR). Flagship: planetary shields off, repairs every
round, bombardment. Commander (5 non-fighter ships in a system): a trade good whenever a unit
sustains. Mech deploys mid-combat for 2 resources.

**Opening.** Gravity Drive in round 1 in nearly every game ("90%"), so the dreadnought can strike an
equidistant system; build plastic rather than double-teching round 1; take Warfare to unlock the
commander early (two destroyers at home give five non-fighter ships), and use Armada to pull command
tokens out of the fleet pool in the round 1 status phase for a strong round 2-3
([BGG Barony guide, 2023](https://boardgamegeek.com/thread/3107097);
[r/ti "Space feudalism forever" part 1 and part 2, 2020](https://www.reddit.com/r/twilightimperium/comments/es0rib/space_feudalism_forever_a_barony_of_letnev_guide/);
[BGG "How has the Barony changed in PoK?", 2021](https://boardgamegeek.com/thread/2570291)).

**Technology.** Gravity Drive, then AI Development (or a yellow skip) to Dreadnought II, then
Duranium Armor and Non-Euclidean Shielding (the pair that makes dreadnoughts nearly unkillable and,
with the commander, prints trade goods). Expect about five researches, so Dreadnought II is the
first spike and Non-Euclidean Shielding the second. Let tech skips pick the path: red skip makes
NES viable; avoid green unless forced. L4 Disruptors only against PDS factions (Argent, Titans,
Xxcha). Disagreement: a carrier/fighter/destroyer build (Munitions rerolls help many cheap dice:
Gravity Drive, Carrier II, then AI Development or Self-Assembly) versus the heavy-dreadnought build
(same sources).

**Builds.** The flagship is "a lesser war sun", worth building early at a forward base; a red-skip
fleet example: 2 Dreadnought II, 1 carrier, 2 Destroyer II, 2 fighters, 4 infantry
([r/ti part 2, 2020](https://www.reddit.com/r/twilightimperium/comments/es0wv7/space_feudalism_forever_a_barony_of_letnev_guide/)).
Mechs are bought mid-combat with commander money.

**Objectives.** Slow start: protect one or two rich systems near home from early grabs, watch the
scoring tempo and delay tech for plastic if others score fast; plan an Imperial double score to
catch up; Barony often ends up win-slaying, and should be paid for it
([BGG 2023](https://boardgamegeek.com/thread/3107097)).

**Diplomacy.** Poor trader (2 commodities, a feared war machine). Sell War Funding and the agent
for a trade good whenever possible. Pose as the underdog when attacking ("I need a point") to avoid
table hate. Do not take Mecatol Rex first.

**Matchups.** Armada's independence from the fleet pool soft-counters Naalu's Neuroglaive; the
flagship strips Arborec and Sol planetary shields; Mentak's flagship (no sustain) and Exotrireme II
counter the sustain fleet; Nekro wants Barony's faction techs
([BGG counters thread, 2020-21](https://boardgamegeek.com/thread/2422331);
[BGG 2019](https://boardgamegeek.com/thread/2336546)).

**Strength.** Async PoK data: lowest win rate of all factions (D). SCPT: row 4 of 5 (2022), B
(2025). "Needs six techs to look like a normal faction" versus "beginner friendly, mid power".

**Bot feasibility.**
- Research order Gravity Drive, AI Development, Dreadnought II, Duranium Armor, Non-Euclidean
  Shielding is a pure wish list. Value medium, effort S.
- Build mix: dreadnoughts plus a few fighters/destroyers; flagship once resources allow, at a
  forward dock. The fleet model already values dreadnoughts; add a Letnev preference in
  `pick_produce`. Value medium, effort S.
- Fleet pool: Armada means the bot can keep two fewer fleet tokens than its biggest stack needs;
  `fleet_wanted` should subtract 2 for Letnev, which frees tactic tokens (the faction's real early
  edge). Value high, effort S.
- Commander unlock: in round 1, follow Warfare to build two destroyers at home when that makes five
  non-fighter ships in one system. Value medium, effort M.
- Munitions Reserves prompt: reroll when 2 trade goods are spare and the round's misses are many.
  Effort M (needs a rule for "worth rerolling").
- Fight model: Letnev's rerolls and Non-Euclidean Shielding are not in `fleet_strength`, so the bot
  under-rates its own fleet; add both. Value medium, effort M.

### The Mentak Coalition (`mentak`)

**Engine check.** Home Moll Primus 4/1 (one planet); start 1 carrier, 2 cruisers, 3 fighters, 4
infantry, PDS, dock; starting techs Sarween Tools and Plasma Scoring; 2 commodities. Ambush: before
space combat roll for up to 2 cruisers/destroyers. Pillage: take a trade good or commodity from a
neighbour with 3+ trade goods after they gain trade goods or trade. Faction techs Salvage
Operations (YY) and Mirror Computing (YYY, a trade good is worth 2). Commander (4 cruisers on the
board): take a promissory note after winning a space combat. Agent: after Pillage, both players
draw an action card. Flagship and mech switch off opponents' sustain damage.

**Opening.** Only one carrier: get a second (Warfare's secondary carrier plus infantry) or accept a
one-system round 1 and compensate with an early Cruiser II fleet. Selling Promise of Protection for
trade goods and Pillaging when Trade goes off can fund a round 1 research
([BGG "Mentak Strategy: Skip Neural Motivator?", 2020](https://boardgamegeek.com/thread/2457760);
[BGG "Mentak opening strategy", 2021](https://boardgamegeek.com/thread/2632353)).

**Technology.** Base-game standard: Neural Motivator round 1, Cruiser II round 2, then yellow to
Mirror Computing. PoK default: AI Development round 1 (it also counts as the missing prerequisite),
Cruiser II, then the yellow faction techs (Salvage Operations, Mirror Computing); AI Development
plus Sarween makes cruisers cheap. Mirror Computing comes late without Technology taken once, so
take Technology at least once (round 2 suggested). An alternative red/yellow late-game line
(Graviton, Transit Diodes, Mirror Computing, PDS II, Destroyer II, Assault Cannon) was proposed and
criticised for needing seven techs (same threads).

**Builds and play.** Cruisers are the faction: four of them unlock the commander, they steal
planets and trade goods for about two rounds, then fall off as fleets grow, so Mentak must score
early (control objectives early are good news, technology objectives early are bad news)
([BGG 2020](https://boardgamegeek.com/thread/2457760)).

**Diplomacy.** Pillage makes neighbours poorer and angry: "everyone hates Mentak", so they get
beaten up; the better pilots play "ethical Mentak", pillaging with restraint and selling protection
([r/ti SCPT 2025 tier thread](https://www.reddit.com/r/twilightimperium/comments/1q0jwu5/scpt_faction_tier_list_2025_one_billion_percent/);
[r/ti 2025 tier list comments](https://www.reddit.com/r/twilightimperium/comments/1kylhx4/faction_power_tier_list/)).
Hacan and Saar suffer most from Pillage ([BGG counters](https://boardgamegeek.com/thread/2422331)).

**Strength.** Async PoK: C, the lowest average VP of any faction (possibly many new players).
SCPT: row 4 (2022), F (2025). Disagreement: some call Mentak "vanilla, not bad"; in Thunder's Edge
async data they rose to +7% above average, partly because the async bot prompts every Pillage.

**Bot feasibility.**
- Pillage prompt (`pillage_take`): the async observation that "the bot never forgets to Pillage"
  matters; always take when offered, unless the target holds Promise of Protection (engine handles)
  or is the bot's only trade partner. Value medium, effort S.
- Research: AI Development, Cruiser II, Salvage Operations, Mirror Computing. Effort S.
- Build mix: cruisers first, aim for four early. Effort S.
- Targeting: lone carriers and destroyers are Ambush prey; add Ambush (two pre-combat dice at the
  cruiser's value) to the fight model so the bot attacks fights it actually wins. Value medium,
  effort M.
- Objective focus: score early, cash trade goods via Mirror Computing for spend objectives.
- Hard: "ethical Mentak" restraint is a table-reading skill.

### The Embers of Muaat (`muaat`)

**Engine check.** Home Muaat 4/1; start 1 war sun, 2 fighters, 4 infantry, dock; starting tech
Plasma Scoring; 4 commodities. Star Forge: spend a strategy token to place 2 fighters or 1 destroyer
with a war sun. Gashlai Physiology: move through supernovas. Faction techs Prototype War Sun II
(RRRY: move 3) and Magmus Reactor (RR). Agent (Umbat): any player produces up to 2 units costing 4
or less where they have a war sun or flagship. Commander (produce a war sun): a trade good per
strategy token spent. Promissory note Fires of the Gashlai gives the war sun technology.

**Opening.** Only one ship that moves (speed 1): round 1 wants a card that takes several systems,
Warfare ideally, Technology or Trade next. In PoK the agent builds a carrier and a destroyer at home
in round 1 so two neighbouring systems can be taken
([r/ti "advice on how to play Muaat", 2022](https://www.reddit.com/r/twilightimperium/comments/xf0moq/can_you_guys_give_me_some_advice_on_how_to_play/);
[r/ti "Advice for turn 1 as Muaat", 2024](https://www.reddit.com/r/twilightimperium/comments/1fndgh9/advice_for_turn_1_and_further_for_muaat/)).

**Technology.** Two schools: "Bluaat" (Gravity Drive so the war sun keeps pace with carriers) or red
to Prototype War Sun II, often via AI Development and a red/yellow skip; green only when it comes
free in long games ([BGG 2026](https://boardgamegeek.com/thread/3760430)). Get Hyper Metabolism
through Jol-Nar's Research Agreement if possible ([r/ti 2024](https://www.reddit.com/r/twilightimperium/comments/1fndgh9/advice_for_turn_1_and_further_for_muaat/)).
Disagreement on Duranium Armor and Magmus Reactor (depends on supernova position).

**Builds.** Treat the war sun like a chess queen: never send it unescorted, screen it with fighters
and destroyers (Star Forge), avoid using its sustain unless the direct hits are accounted for,
retreat rather than lose it. Get the second war sun out early (it unlocks the commander). Mechs and
Star Forge fix the ground game ([r/ti "How can we fix Muaat?", 2025](https://www.reddit.com/r/twilightimperium/comments/1iutcf8/how_can_we_fix_muaat/)).

**Objectives and play.** Win by the threat of the war sun more than its use; avoid an early Mecatol
Rex, but Muaat can take it for a round 3-5 Imperial swing. Prefer influence-rich slices and
Leadership: every Star Forge costs a strategy token. Sell Fires of the Gashlai to several players
for 5-7 trade goods each early (volume beats margin).

**Matchups.** Hard counter to Arborec and ground-heavy factions; struggles against Naalu fighter
swarms (disputed: Destroyer II and early pressure fix it).

**Strength.** Bottom row in SCPT 2022, D in SCPT 2025 (commenters: "way too low"), B in async
PoK data.

**Bot feasibility.**
- Star Forge is a component action (`faction|star_forge` style option); the rule bot never takes
  component actions. Using it when a strategy token is spare and the war sun is about to fight is
  the faction's main engine. Value high, effort M (component-action hook).
- Agent in round 1 (carrier plus destroyer at home): effort M, value high.
- Movement: never move the war sun into a fight without a screen; keep it with fighters. Effort M
  (escort rule in `pick_move`).
- Research: Prototype War Sun II when reachable, else Gravity Drive. Effort S.
- Draft: Leadership weighted up (tokens fuel Star Forge). Effort S.

### The Naalu Collective (`naalu`)

**Engine check.** Home Maaluuk 0/2 and Druaa 3/1; start 1 carrier, 1 cruiser, 1 destroyer, 3
fighters, 4 infantry, PDS, dock; starting techs Sarween Tools and Neural Motivator. Telepathic: the
"0" token makes Naalu first in initiative (engine module present). Foresight: when attacked, move
ships to an adjacent empty system for a strategy token. Hybrid Crystal Fighter II (GB): fighters
move 2 on their own and excess fighters count as half a ship for fleet supply. Neuroglaive (GGG).
Flagship: fighters fight on the ground. The corpus records the Thunder's Edge agent (Z'eu: return a
just-placed command token) and mech for Naalu; check which version a POK game resolves.

**Opening.** Taking Mecatol Rex in round 1 is usually not worth the overextension; set up a
top-of-round-2 capture instead (the 0 token acts first). Round 1 priorities: score if possible,
take the three systems around home, research a blue technology. With a poor home slice and rich
systems one step out, take Warfare and use the agent to move both carriers forward
([r/ti "Naalu and Mecatol in R1", 2023](https://www.reddit.com/r/twilightimperium/comments/13si9k6/naalu_and_mecatol_in_r1_with_warfare_valid/);
[r/ti "first few moves with Naalu", 2023](https://www.reddit.com/r/twilightimperium/comments/187jxfq/seeking_newbie_advice_first_few_moves_with_naalu/);
[r/ti "Strategy card R1 as Naalu", 2025](https://www.reddit.com/r/twilightimperium/comments/1opwwlu/need_help_choosing_strategy_card_r1_as_naalu/)).

**Strategy cards.** The 0 token frees Naalu from the speaker race, so they pick whatever card they
need; Technology in round 1 is often better than Warfare (BGG and Reddit threads above).

**Technology.** Blue first (Dark Energy Tap near frontier tokens, Gravity Drive), Hybrid Crystal
Fighter II soon ("turns your faction into a powerhouse"), Fleet Logistics high priority (two actions
from the first initiative slot), Neuroglaive and Hyper Metabolism only with skips; Neuroglaive is a
late-game tool ([r/ti "Tips for playing Naalu", 2022](https://www.reddit.com/r/twilightimperium/comments/y9dora/tips_for_playing_as_the_naalu_collective/)).

**Builds.** A fighter faction: carriers plus many fighters, docks that pump fighters, the flagship
attacking (fighters take planets with it). Spread single fighters into empty systems so Neuroglaive
taxes anyone moving through. Fighter swarms must still respect production limits and fleet supply
([r/ti "How do you deal with Naalu?", 2025](https://www.reddit.com/r/twilightimperium/comments/1pr31g5/how_the_hell_do_you_deal_with_the_naalu/)).

**Diplomacy.** Never give away Gift of Prescience to experienced players: they hold it until their
winning turn; sell the agent instead.

**Counters.** Destroyer II and anti-fighter barrage, Carrier II fleets matching hit points, early
pressure on the home system before the swarm exists; Hacan's Quantum Datahub Node against Naalu's
Imperial plans ([BGG 2019](https://boardgamegeek.com/thread/2336546)).

**Strength.** Bottom row in SCPT 2022 ("PoK games end in round 5, too fast for Naalu's snowball");
A in async PoK data; B in SCPT 2025. Commenters disagree strongly (the 0 token is "great for point
blocking").

**Bot feasibility.**
- Build mix: fighters and carriers (`pick_produce` scales fighters by 0.6 today; for Naalu with
  Hybrid Crystal Fighter II, raise them above 1). Value high, effort S.
- Research: Gravity Drive, Hybrid Crystal Fighter II, Fleet Logistics. Effort S.
- Mecatol timing: the bot already wants Imperial when Mecatol is takeable; for Naalu add "move a
  carrier in range of Mecatol by the end of round 1". Value medium, effort M.
- Foresight prompt: retreat-before-combat when the fight model says the bot loses. Value medium,
  effort S-M.
- The 0 token is automatic. Selling the agent or note is out of reach.

### The Nekro Virus (`nekro`)

**Engine check.** Home Mordai II 4/0; start 1 dreadnought, 1 carrier, 1 cruiser, 2 fighters, 2
infantry, dock; starting tech Daxcive Animators. Propagation: cannot research; a research gives 3
command tokens instead. Technological Singularity: once per combat, after an opponent's unit dies,
gain one of their technologies. Galactic Threat: cannot vote; predict an agenda outcome to copy a
technology from a player who voted that way. Valefar Assimilators X/Y copy faction technologies.
Flagship: ground forces fight in space. Commander (3 technologies): an action card per technology
gained.

**Opening and cards.** The best-known guide says: be the tortoise, not the hare. Grow in rounds 1-2
(units, planets, both resource and influence), do not throw ships at fights just for technology,
start "space risk" from round 3. Card order round 1: Trade (others spend on research while Nekro
builds, and trading calms neighbours), then Leadership (deny the table tokens), then Technology or
Politics, then Construction (PDS at home), Warfare last; never Diplomacy or Imperial in round 1.
Politics in round 2 for the speaker token, which breaks agenda ties so predictions land
([BGG "Nekro - Be the tortoise, not the hare", 2019](https://boardgamegeek.com/thread/2283392)).

**Technology.** Copy movement first (Gravity Drive, unit upgrades): each copied movement tech
opens fights for more technology. Dreadnought II and Cruiser II are the staples; Destroyer II makes
farming technology from anyone easy; no more than two technologies of one colour; prefer unit
upgrades, then two technologies in each of four colours. Pay neighbours to "feed" you a fighter for
a technology rather than fight them (same guide).

**Builds.** Mass: big clumps of units win fights with small losses; bombard every planet; produce
2-4 infantry at home every round; the flagship is best as a home defender with many infantry.

**Diplomacy.** Players fear Nekro; play nice early, buy peace, be fair; Nekro is eliminated more
often than anyone but Winnu when played thematically aggressive. Table counters: deny tech tempo
(do not research deep near Nekro), and the Antivirus note
([BGG 2019](https://boardgamegeek.com/thread/2336546)).

**Matchups.** Counters Jol-Nar (tech-rich, -1 combat); takes Sol's Spec Ops II; PDS II networks
are the hardest problem (bring a bigger fleet, strike the centre).

**Strength.** SCPT top row in 2022 (PoK), A in async PoK data, B in SCPT 2025 (game too fast in
Thunder's Edge for a round-5 faction).

**Bot feasibility.**
- Research prompts become "gain 3 command tokens" (engine) and Technological Singularity picks a
  technology from the opponent: choose by a fixed preference (movement > unit upgrades the bot
  builds > colours it lacks). Value high for Nekro, effort S-M (needs the prompt id from
  `nekro.rs`).
- Targeting: favour fights against seats with valuable technologies the bot lacks; a destroyed
  fighter is enough. `activation_value` could add a bonus per "new technology the defender owns".
  Value high, effort M.
- Draft: Trade/Leadership up, Warfare down in round 1 for Nekro. Effort S.
- Galactic Threat prediction: predict the outcome most votes are already on (the bot cannot see
  intentions before voting; use speaker/current tally if exposed). Value medium, effort M-L
  (agenda handling is random today).
- Hard: buying peace and arranging "feed me a fighter" deals.

### The Clan of Saar (`saar`)

**Engine check.** Home Ragh 2/1 and Lisis II 1/0; start 2 carriers, 1 cruiser, 2 fighters, 4
infantry, Floating Factory (move 1, capacity 4, PRODUCTION 5, in space, destroyed if blockaded);
starting tech Antimass Deflectors. Scavenge: a trade good each time it gains a planet. Nomadic:
can score without controlling its home planets. Faction techs Chaos Mapping (B: asteroid fields
with Saar ships cannot be activated; produce 1 unit at the start of each turn) and Floating Factory
II (YY). Agent: raise one ship's move to the board's fastest. Commander (3 docks): fighters and
infantry may be placed at any dock. Mech deploys for 1 trade good after taking a planet.

**Opening.** Three capacity units at the start (two carriers and the dock) and Scavenge make Saar
the best round-1 expander; prioritise a slice with many two-planet systems and a road to Mecatol
Rex. One school takes Warfare and delays it while spreading 1 carrier + 2 infantry per system,
collecting a trade good per planet; another takes Technology (Gravity Drive and Sarween) then
Mecatol Rex in round 2 with the dock along
([BGG TomBoombs Saar guide, 2020](https://boardgamegeek.com/thread/2349874);
[BGG Saar speaker question, 2020](https://boardgamegeek.com/thread/2437176)).
Do not pop Construction first: wait for the Scavenge trade good to afford research.

**Technology.** Sarween Tools and Gravity Drive first (Gravity Drive moves the dock too), then
Carrier II and Dreadnought II; Floating Factory II is a luxury (mostly for the "both faction
technologies" secret). Disagreement on Chaos Mapping: "game-changing, makes Saar uneliminable" when
an asteroid field is reachable versus "situational"
([BGG "Saar Tech Path", 2021](https://boardgamegeek.com/thread/2620083)).

**Builds and play.** Move docks with the fleet and produce on arrival; never leave a dock without
escorts (blockade destroys it); retreat docks rather than lose them; a dock in an asteroid field
with Chaos Mapping is a permanent base. Keep trade goods low when Mentak can Pillage.

**Objectives.** Plan to reach the first stage II reveal with about 8 points (custodians, support,
secrets, stage I), uneliminable, 16 trade goods and three unit upgrades (TomBoombs). Saar struggle
with technology and structure-on-planet objectives
([r/ti 2025 tier list comments](https://www.reddit.com/r/twilightimperium/comments/1kylhx4/faction_power_tier_list/)).

**Diplomacy.** "The heat won't turn off": Saar draws hate whatever it does; extortion of neighbours
for support/ceasefire swaps is in the playbook. Ghosts placing wormholes in an empty Saar slice and
Mentak's Pillage are the named threats.

**Strength.** SCPT top row 2022 and S 2025; async PoK data only B ("overrated", "draws heat").

**Bot feasibility.**
- The engine already treats the Floating Factory as a non-ship for activation (comment in
  `activation_value`). Moving docks forward with escorts and producing on arrival needs a "dock
  follows fleet" movement rule. Value high, effort M.
- Opening: spread one carrier and two infantry per system (the bot's opening plan does this), and
  value each planet +1 for Scavenge in `planet_worth`. Value medium, effort S.
- Research: Sarween, Gravity Drive, Carrier II, Dreadnought II; Chaos Mapping when an asteroid
  field is adjacent to its systems. Effort S.
- Safety: never end a turn with a dock alone next to enemy ships. Effort M.
- Hard: the extortion game.

### Sardakk N'orr (`sardakk`)

**Engine check.** Home Quinarra 3/1 and Tren'lak 1/0; start 2 carriers, 1 cruiser, 5 infantry, dock,
PDS; no starting technology; 3 commodities. Unrelenting: +1 to all combat rolls (not to space cannon
or barrage). Exotrireme dreadnoughts bombard 4 (x2); Exotrireme II (BBY) moves 2 and can sacrifice
itself to destroy two ships. Valkyrie Particle Weave (RR): an extra ground hit when the opponent
hits. Commander (5 non-home planets): commit ground forces from adjacent systems. Agent: place 2
infantry after any tactical action. Mech: a hit after it sustains. Hero: skip to ground combat.

**Core idea.** +1 combat wins even fights about 2:1, but one extra unit is worth more than +1 until
about eight units a side, so Sardakk should bring numbers and seek big decisive fights late, never
attack outnumbered, and not lose units early. Exotrireme bombardment is the real ground-combat
edge ([BGG "Vaunt's ultimate guide to Sardakk", 2019](https://boardgamegeek.com/thread/2323754)).

**Technology.** No starting tech means paying full price for technology objectives; expect about
five researches. Blue core: Antimass, Gravity Drive, Carrier II; Exotrireme II is the only reason
for yellow; Valkyrie Particle Weave is the best ground-combat technology; Neural Motivator and
Sarween are "not for Sardakk"; tech skips are gold (same guide). Some games skip technology for
plastic entirely ("heresy").

**Builds.** Carriers with fighters (+1 makes fighters hit on 8) or Exotriremes; mass infantry for
the commander's adjacent commits.

**Diplomacy.** The best use of +1 is not rolling at all: be visibly dangerous so neighbours do not
attack. Sell Tekklar Legion. Sardakk is a popular "beginner" pick but plays poorly in new hands.

**Strength.** Async PoK: C; SCPT: bottom row (2022), D (2025); the 2019 guide calls them mid tier
for good players, better in 14-point games. Thunder's Edge breakthroughs help them (not active
here).

**Bot feasibility.**
- The fight model already has Sardakk +1. Engine data makes them start without technology, so the
  bot's Gravity Drive preference needs Antimass first: wish list Antimass, Gravity Drive, Carrier
  II, then Valkyrie Particle Weave or Exotrireme II. Value medium, effort S.
- Targeting: Sardakk can take fights at a slightly lower margin than `WIN_MARGIN` (the +1 is
  already in the matchup, so this is mostly about not under-committing). Value low.
- Agent use after own tactical actions (2 free infantry) through the leader prompt. Value medium,
  effort S-M.

### The Federation of Sol (`sol`)

**Engine check.** Home Jord 4/2; start 2 carriers, 1 destroyer, 3 fighters, 5 infantry, dock;
starting techs Antimass Deflectors and Neural Motivator; 4 commodities. Spec Ops infantry hit on 7;
Advanced Carrier (capacity 6; II: move 2, capacity 8, sustain). Orbital Drop: a strategy token puts
2 infantry on an owned planet (component action `faction|orbital_drop`). Versatile: an extra command
token each status phase. Commander (12 resources of planets): an infantry at the start of each
ground combat on own planets. Hero: return all own command tokens to reinforcements.

**Play.** Sol is the "plays itself" faction: rich home, two carriers, strong infantry, extra
tokens; Gravity Drive is always a must, Dark Energy Tap a good second tech; Advanced Carrier II and
Spec Ops II are the upgrades ([BGG "Playing Federation of Sol - best starting move?", 2023](https://boardgamegeek.com/thread/3177352)).
Do not rush Mecatol Rex next to dreadnought neighbours: keep a forward carrier ready and take it on
the slow route when it opens (same thread). Trade or Politics (for Leadership next round) as
opening cards; follow Warfare for a carrier and infantry. Gravity Drive plus Advanced Carrier II is
"enough to win"; Spec Ops II or Fighter II when objectives call; Hyper Metabolism in 14-point games.
Orbital Drop is expensive as a unit purchase (a strategy token for 2 infantry) and is best used as
a stall that arouses no suspicion, to reinforce a planet that cannot be reached by production (for
example Mecatol Rex right after taking it), or together with the 3-resource mech; stack ground
forces on Jord ([r/ti "Few questions about Sol", 2026](https://www.reddit.com/r/twilightimperium/comments/1s1ks3e/few_questions_about_sol_agree_with_their_s_tier/)).
Disagreement on strength: "S tier, good at everything that makes TI difficult" versus PoK win
rates of about 18-20% (only a little above the 16.7% baseline), possibly dragged down by many new
players picking Sol (same thread).

**Strength.** S in SCPT 2025 ("great on any slice, any speaker position, any tech path"), A in
async PoK data, row 2 in SCPT 2022.

**Bot feasibility.**
- Orbital Drop as a component action: as a stall when the bot would otherwise pass early or
  make a weak move, and right after taking Mecatol Rex or a planet next to an enemy fleet, drop 2
  infantry there (when a strategy token is spare after the Technology/Warfare follow). Value
  high, effort M (shared component-action hook).
- Build mix: Advanced Carriers full of infantry and fighters; carriers are the main warships.
  Effort S.
- Research: Gravity Drive, Advanced Carrier II, Spec Ops II. Effort S.
- Token pools: Versatile gives one more token; the bot's pool logic already adapts.
- Mecatol: the bot's Mecatol bonus is fine; Sol infantry make holding it cheaper, so the "hold"
  value could be higher for Sol. Value low.

### The Winnu (`winnu`)

**Engine check.** Home Winnu 3/4; start 1 carrier, 1 cruiser, 2 fighters, 2 infantry, PDS, dock; no
starting technology in this engine (the printed faction chooses one; not modelled). Blood Ties:
no influence needed to remove the custodians token. Reclamation: a free PDS and dock on Mecatol Rex
after taking it. Faction techs Lazax Gate Folding (BB) and Hegemonic Trade Policy (YY). Commander
(control or fight at Mecatol Rex): +2 to combat at Mecatol Rex, home and legendary planets. Hero:
resolve any strategy card's primary (and let others use the secondary). Flagship rolls one die per
opposing non-fighter ship. Promissory note Acquiescence.

**Play.** The Winnu plan is Mecatol Rex: free custodians, a dock and PDS there, then Imperial. Round
1 card pairs that reach Mecatol: Warfare/Imperial (4-player), Technology+Warfare or
Technology+Trade; with Diplomacy played before Technology and Gravity Drive researched, Warfare
lands on Mecatol in round 1 ([BGG "4 player Winnu strategy?", 2019](https://boardgamegeek.com/thread/2303455)).
In PoK the commander and hero ("a guaranteed Imperial nobody can stop") plus a strong mech made
them a top early-PoK faction ([BGG PoK data thread, 2021](https://boardgamegeek.com/thread/2680870)).
Winnu benefit from being starting speaker more than any faction, and from 3-4 player games (two
strategy cards, fewer rivals for Mecatol). With one carrier, Warfare in round 1 makes sense for
Winnu more than for most. Against aggressive neighbours the advice flips: do not try Mecatol Rex in
rounds 1-3, build plastic or a PDS defence (a mech on a legendary such as Hope's End places
structures), score normally and bank on the hero for a late Imperial swing worth 1-3 points
([r/ti "Playing Winnu vs L1Z1X, Vuil'Raith and Saar", 2025](https://www.reddit.com/r/twilightimperium/comments/1nl2ajq/playing_winnu_vs_l1z1x_vuilraith_and_saar_any/);
[r/ti "First time playing as the Winnu", 2026](https://www.reddit.com/r/twilightimperium/comments/1r2o52l/first_time_playing_ti4_playing_as_the_winnu/)).
Disagreement: in the base game, players say "you are legally required to yolo" Mecatol in round 1
([r/ti "Upcoming Winnu game - to yolo?", 2025](https://www.reddit.com/r/twilightimperium/comments/1pnegvy/upcoming_winnu_game_to_yolo_yes_or_no/)).
"Winnu is good, that is why they are bad": a big early lead gets them mugged
([r/ti "I won with the Winnu", 2026](https://www.reddit.com/r/twilightimperium/comments/1tlrerr/i_won_with_the_winnu_tonight/)).

**Counters.** Anyone sitting on Mecatol Rex first ruins Winnu; experienced tables must punish them
early, and when they do not, Winnu win: async data has them A tier and the Bellum Gloriosum survey
found their win rate far above their reputation; SCPT puts them low (row 3 in 2022, D in 2025)
([r/ti async data thread, 2025](https://www.reddit.com/r/twilightimperium/comments/1jg2dlh/10_point_tier_list_according_to_data_from_6000_6/);
[BGG counters](https://boardgamegeek.com/thread/2422331)).

**Bot feasibility.**
- The bot's Mecatol logic fits Winnu exactly; for Winnu, value Mecatol higher (no 6-influence
  cost: `mecatol_takeable` should ignore the influence check, and `MECATOL_BONUS` can grow with
  Reclamation's free dock and PDS). Value high, effort S.
- Draft: Warfare and Technology in round 1, Imperial from round 2 while Mecatol is held (already
  the bot's rule). Effort S.
- No starting technology in this engine: research Gravity Drive (via Antimass if no skip) to reach
  Mecatol. Effort S.
- Hero (any primary) at the moment it scores: needs a per-card evaluation. Effort L.

### The Xxcha Kingdom (`xxcha`)

**Engine check.** Home Archon Ren 2/3 and Archon Tau 1/1; start 1 carrier, 2 cruisers, 3 fighters, 4
infantry, PDS, dock; starting tech Graviton Laser System; 4 commodities. Peace Accords: after
resolving Diplomacy's primary or secondary, gain an empty planet adjacent to an owned planet.
Quash: a strategy token discards a revealed agenda. Faction techs Instinct Training (G: cancel an
action card) and Nullification Field (YY: end the turn of a player who activates a system with
Xxcha ships; the token stays). Flagship and mech space cannons reach adjacent systems. Agent: ready
a planet. Commander (12 influence): +1 vote per exhausted planet. Hero: planets count resources
plus influence for both.

**Opening.** One carrier: the top goal of round 1 is a second carrier from Warfare's secondary,
with the agent buying a turn. A sequence: turn 1 carrier with 2 infantry and 2 fighters into a
neighbouring two-planet system; turn 2 Diplomacy, readying those planets and using Peace Accords
on the best adjacent empty planet; turn 3 agent to ready a third planet or stall; then Warfare's
secondary ([BGG IncrediSteve, 2020](https://boardgamegeek.com/thread/2557071)). With PoK's
Diplomacy refresh, taking Diplomacy as Xxcha no longer feels bad. AI Development into Cruiser II
if Technology is available ([BGG 2024](https://boardgamegeek.com/thread/3322809)).

**Play.** The trap is turtling with PDS everywhere when no objective rewards it; get the flagship
out early (space cannon into adjacent systems) and park it where it covers the slice. In long games
the Codex hero ("Political Data Nexus") and Quash make Xxcha an agenda power; early-PoK data had
Xxcha near the top ([BGG 2021](https://boardgamegeek.com/thread/2680870)). Rules notes: Peace
Accords works on a planet with no units even when enemy ships are in the system; Nullification
Field leaves the activating token in place ([BGG 2019](https://boardgamegeek.com/thread/2319065);
[BGG 2019](https://boardgamegeek.com/thread/2319040)).

Current advice from players who win with Xxcha: do not turtle early; expand aggressively
(Cruiser II helps), then "bite and hold"; later put many PDS (upgraded to PDS II, with Graviton and
Plasma Scoring) on a central multi-planet system so they cover the whole slice, park the flagship
and mechs in front of home, take Mecatol Rex for a point or two without trying to hold it, and sell
the agent when not needed ([r/ti "Xxcha Kingdom Help", 2026](https://www.reddit.com/r/twilightimperium/comments/1tbnyuq/xxcha_kingdom_help/);
[r/ti "Playing Xxcha with aggressive factions", 2026](https://www.reddit.com/r/twilightimperium/comments/1u30rnz/playing_xxcha_in_a_game_with_aggressive_factions/)).
Disagreement: one 2026 thread calls Xxcha "very broken in PoK", another (Thunder's Edge) "the
second-worst faction" because the game got faster and their kit "ends games at 7-8 points"
([r/ti "Xxcha Peace Turtles seem pretty great now?", 2026](https://www.reddit.com/r/twilightimperium/comments/1vtu0os/xxcha_peace_turtles_seem_pretty_great_now/)).

**Matchups.** Soft counter to Yssaril (Instinct Training, PDS against cruisers); Argent's flagship
blocks Xxcha space cannon.

**Strength.** Async PoK B; SCPT row 4 (2022) and F in 2025, the latter after Thunder's Edge nerfs
that are not active in this engine.

**Bot feasibility.**
- Diplomacy is worth much more to Xxcha: in the draft and when following, Peace Accords is a free
  planet. Score Diplomacy high in rounds 1-3 and always follow its secondary when a Peace Accords
  target exists. Value high, effort S (draft) and M (secondary rule plus the Peace Accords planet
  choice: best `planet_worth`).
- Flagship placement: a system adjacent to the most own planets. Effort M.
- Quash: discard agendas that hurt the bot (needs agenda evaluation; agendas are random today).
  Effort L.

### The Yin Brotherhood (`yin`)

**Engine check.** Home Darien 4/4; start 2 carriers, 1 destroyer, 4 fighters, 4 infantry, dock;
starting tech Sarween Tools; 2 commodities. Indoctrination: 2 influence to replace an opposing
infantry with your own at the start of ground combat. Devotion: after a space combat round,
destroy your cruiser/destroyer to produce a hit. Faction techs Impulse Core (YY) and Yin Spinner
(GG). Flagship: when destroyed, destroys every ship in its system. Commander (use a faction
ability): satisfies a green prerequisite. Agent: 2 fighters or 2 infantry when a unit dies.

**Play.** The common line was Cruiser II with Indoctrination (capacity-1 cruisers carrying an
infantry), via Neural or Plasma; a counter-argument favours blue (Antimass, Gravity Drive,
Dreadnought II, Carrier II) so the flagship moves 2 and threatens more fleets; the reply: "blue is
good for any faction; Devotion is an option, not the reason to play Yin"
([BGG "Yin Tech Path", 2023](https://boardgamegeek.com/thread/3113682)). Yin are a ground-combat
faction, but mechs blunt Indoctrination ([r/ti SCPT 2022 comments](https://www.reddit.com/r/twilightimperium/comments/rxq1hs/scpts_definitive_pok_faction_tier_list/)).
The flagship is a deterrent "nuke" against big fleets. Indoctrination costs influence, so Yin need
influence; against Yin, opponents build mechs and starve them of trade goods
([r/ti Yin guide thread, 2024](https://www.reddit.com/r/twilightimperium/comments/1fqq1yu/a_fresh_yin_brotherhood_faction_guide_is_out/)).
Yin counter Sol's Spec Ops and L1Z1X (Impulse Core and Devotion kill dreadnoughts).
Players who win with Yin describe a defensive faction: Yin Spinner (with Integrated Economy) stacks
infantry on every planet, including Mecatol Rex; Devotion destroyers hidden behind a fighter screen
deter attacks (Devotion lets Yin choose the target, Impulse Core does not); the flagship stays at
home against win-slaying; Indoctrination is used rarely but its threat matters. Some sell "immunity"
from the hero, flagship and promissory note
([r/ti "First win... I like Yin now", 2026](https://www.reddit.com/r/twilightimperium/comments/1v66mqu/first_win_and_i_guess_i_like_yin_now/);
[r/ti "Is Yin good?", 2026](https://www.reddit.com/r/twilightimperium/comments/1vcb39m/is_yin_good/);
[r/ti "Tips for Yin", 2026](https://www.reddit.com/r/twilightimperium/comments/1tdu3aa/tips_for_yin_brotherhood/)).

**Strength.** SCPT bottom row (2022), C (2025); async PoK B.

**Bot feasibility.**
- Indoctrination prompt (`indoctrination_replace` or similar): use it whenever 2 influence is
  spare and the ground fight is close; it swings a fight by two units. Value medium, effort S.
- Devotion/Impulse Core: sacrifice a destroyer to finish a damaged capital ship or flagship when
  the fight model says it flips the result. Effort M.
- Research: Gravity Drive and Dreadnought II or Cruiser II; Yin Spinner with a green skip (two
  infantry with every production). Effort S.
- Keep the flagship at home (it is a deterrent, and its death clears the system). Effort S.

### The Yssaril Tribes (`yssaril`)

**Engine check.** Home Retillion 2/3 and Shalloq 1/2; start 2 carriers, 1 cruiser, 2 fighters, 5
infantry, PDS, dock; starting tech Neural Motivator; 3 commodities. Stall Tactics (component action:
discard an action card), Scheming (draw one extra, discard one), Crafty (no hand limit). Faction
techs Transparasteel Plating (G) and Mageon Implants (GGG). Agent Ssruu copies every other agent.
Flagship moves through enemy ships. Commander (7 action cards): look at hands/notes/secrets of
players activating your systems.

**Play.** Yssaril's power is time: Stall Tactics lets them act after everyone has passed, so they
take control objectives at the end of the round, out-stall Warfare in round 1, and play big
moves no one can answer ([BGG "Stall Tactics", 2022](https://boardgamegeek.com/thread/2997040);
[BGG IncrediSteve on Warfare stalling, 2024](https://boardgamegeek.com/thread/3232040)). The usual
weakness is plastic: low home resources and production. Advice splits: Sarween first and double
dock at home, or blue (Carrier II, Fighter II) plus one AI Development dip; Mageon Implants is "a
trap without a green skip", though Bio-Stims plus Mageon is called terrifying
([BGG "Yssaril: Meta and Strategy", 2020](https://boardgamegeek.com/thread/2518695);
[BGG 2023](https://boardgamegeek.com/thread/3176141)). Sling Relay helps production. Be friendly
(the threat of a full hand deters attacks); Hacan are natural partners. Pick the strategy card that
scores rather than Politics for the extra card (Politics gets taken anyway). With Bio-Stims,
Mageon Implants twice a round is accepted by tables if spread across players; the real edge is
knowing every hand and acting last with full information
([r/ti "Noob question - Yssaril", 2026](https://www.reddit.com/r/twilightimperium/comments/1uwe8cm/noob_question_yssaril_tribes/);
[r/ti "How to use Mageon Implants", 2026](https://www.reddit.com/r/twilightimperium/comments/1wqsyro/how_to_use_mageon_implants_with_yssaril/)).

**Strength.** The best win rate in async PoK data (S+, about 26% in 6-player games against a 16.7%
baseline), A in SCPT 2025 (commenters: should be S), row 3 in SCPT 2022.

**Bot feasibility.**
- Stall Tactics as a component action: when the bot has no activation worth `EAGER_WORTH` and
  others have not passed, discard the worst action card instead of passing or making a weak move;
  then act after the others pass. Value high (it is the faction), effort M (component-action hook
  plus "pass order" awareness; the bot needs to know who has passed, which is public).
- Research: Sarween or Gravity Drive first, Carrier II/Fighter II; avoid Mageon without a green
  skip. Effort S.
- Action cards are random today; Yssaril magnify that gap. A minimal "play cards that are clearly
  good now" layer would help every faction, Yssaril most. Effort L.

## Prophecy of Kings factions

### The Argent Flight (`argent`)

**Engine check.** Home Valk 2/0, Ylir 0/2, Avar 1/1 (three planets); start 1 carrier, 2 destroyers,
2 fighters, 5 infantry, PDS, dock; no starting technology in this engine (the printed sheet picks
two; not modelled). Strike Wing Alpha destroyers: move 2, capacity 1, anti-fighter barrage 9 (x2);
Strike Wing Alpha II (RR): combat 7, barrage 6 (x3), 9-10 also kills infantry in space. Raid
Formation: excess barrage hits damage sustain-damage ships. Zeal: vote first, +1 vote per player.
Aerie Hololattice (Y): no movement through systems with your structures; PRODUCTION 1 on planets with
structures. Flagship: no space cannon against your ships in its system. Mech does not use capacity.
Commander (6 units with barrage/space cannon/bombardment): an extra die on unit-ability rolls.

**Opening.** One of the fastest starts: destroyers that carry an infantry each and move 2 reach
equidistant systems in round 1; with the right adjacent system (Arinam/Meer, a red skip) Strike
Wing Alpha II in round 1 and Mecatol Rex in round 2 are possible ([BGG "Argent Flight: the power of
Meer", 2021](https://boardgamegeek.com/thread/2713427);
[r/ti "Sardakk and Muaat neighbours as Argent", 2023](https://www.reddit.com/r/twilightimperium/comments/14fchyk/how_to_deal_with_sardakk_and_muaat_neighbors_as/)).

**Play.** Stake a claim, hold it with a PDS grid (Aerie Hololattice), then play nice. Strike Wing
Alpha IIs strip fighter screens and damage sustain ships, so cheap destroyers beat expensive fleets
(one guide's estimate: four destroyers worth 4 resources have about a 51% chance against a war sun
with six fighters); Muaat should not fight Argent. Assault Cannon by round 2 is possible. The
three-planet home system is the weak spot, especially against Sardakk's commander (commits from
adjacent systems). Destroyers are excellent for control objectives
([r/ti 2025 tier comments](https://www.reddit.com/r/twilightimperium/comments/1kylhx4/faction_power_tier_list/)).

**Strength.** SCPT row 2 (2022; one commenter "top 3"), async PoK B, but F in SCPT 2025 with 7% async
win rate in Thunder's Edge ("a round-5 faction in round-4 games"). Disagreement is sharp: "a menace
to attack" versus "structure-based without a way to get structures".

**Bot feasibility.**
- Build mix: Strike Wing Alpha destroyers as the main ship (cheap, carry infantry, move 2). In
  `pick_produce` give Argent destroyers carrier-like value for transport. Value high, effort S.
- Opening: activation and movement treat any ship with capacity as a carrier (`capacity() > 0`),
  but the opening plan's transport count (`free_transports_and_ground`) and the opening transport
  bonus in `pick_produce` only count `capacity() >= 2`, so capacity-1 destroyers never count as
  transports there. Count them. Value high, effort S.
- Research: Strike Wing Alpha II first (needs two reds: no starting tech here, so AI Development
  or a red skip), Aerie Hololattice later. Effort S.
- Fight model: anti-fighter barrage and Raid Formation are not in `fleet_strength`; Argent
  under-rates its own destroyers. Value medium, effort M.
- Zeal makes Argent the agenda heavyweight, unused while agendas are random. Effort L.

### The Vuil'raith Cabal (`cabal`)

**Engine check.** Home Acheron 4/0; start 1 dreadnought, 1 carrier, 1 cruiser, 3 fighters, 3
infantry, Dimensional Tear dock (PRODUCTION 5; its system is a gravity rift the Cabal does not roll
for; 6 fighters free of capacity); starting tech Self-Assembly Routines; 2 commodities. Devour:
capture opponents' non-structure units destroyed in combat. Amalgamation: return a captured unit to
produce one of that type free. Riftmeld: return a captured unit to research its upgrade ignoring
prerequisites. Vortex (R): capture a unit from a player near your docks. Agent: after a player
replenishes commodities, convert them to trade goods and capture a unit costing up to their
commodity value. Commander (units in 3 gravity rifts).

**Opening.** Trade in round 1 and play it early, refreshing everyone (goodwill); when a
4-commodity player refreshes, the agent captures a dreadnought or carrier; Warfare's secondary for
two carriers; Construction early to put a Dimensional Tear near Mecatol Rex, lining docks up as
rift highways ([BGG "Any tips on how to play Vuil'Raith Cabal?", 2020-21](https://boardgamegeek.com/thread/2556786)).
Leadership matters: zero starting influence, token poor.

**Play.** Aggression is cheap for the Cabal (captures pay for losses), but do not unify the table
early; from round 3 on, fight a lot ([BGG "The Vuil'Raith Cabal", 2022](https://boardgamegeek.com/thread/2880251)).
Fleet supply caps how many free captured ships can be built. Use captured units with Riftmeld to
get upgrades. They struggle against PDS-heavy defences and tough infantry (Sol, Yin, Sardakk,
Mahact).

**Strength.** SCPT row 3 (2022), A (2025); async PoK B.

**Bot feasibility.**
- Production: Amalgamation prompts (free unit for a captured one) should always be taken. Value
  medium, effort S.
- Agent on commodity replenishment: always use, target the most expensive capturable unit.
  Effort S-M.
- Targeting: fights are cheaper for the Cabal (captures), so accept a slightly lower win margin
  (e.g. `WIN_MARGIN` 0.3). Value medium, effort S.
- Movement through own Dimensional Tears is already a gravity rift without a roll in the engine;
  check that `movable_into` counts the +1. Value low.
- Draft: Trade and Leadership up, Construction up in round 1. Effort S.

### The Empyrean (`empyrean`)

**Engine check.** Home The Dark 3/4 (a nebula; +1 to defenders' combat); start 2 carriers, 1
destroyer, 2 fighters, 4 infantry, dock; starting tech Dark Energy Tap; 4 commodities. Voidborn
(nebulae do not slow you), Aetherpassage (let others move through your ships), Dark Whispers (two
faction notes: Dark Pact and Blood Pact). Aetherstream (BB): +1 move for you or a neighbour activating
next to an anomaly. Voidwatch (G). Agent: a command token for a player moving into a system with no
planets. Commander: be neighbours with everyone. Hero: frontier tokens everywhere, explore those
with your ships.

**Opening.** Be "super friends" in round 1, establish no-go zones, give out Dark Pact early (before
Trade is played) to a partner that offers something (Keleres, Nomad, Jol-Nar, Naaz-Rokha), not to
Hacan or Xxcha; send the destroyer far to explore frontier tokens with the agent (a free token per
move into an empty system). Production 5 at home is a bottleneck: plan a forward dock. Avoid early
Mecatol Rex ([BGG "The Empyrean Guide", 2024](https://boardgamegeek.com/thread/3232221)).

**Cards and tech.** Round 1 order in that guide: Trade, Technology, Construction, Leadership,
Politics, Diplomacy, Warfare, Imperial. Blue-heavy: Gravity Drive into Aetherstream ("S tier,
sell +1 move to neighbours"), Sling Relay, Fleet Logistics, Light/Wave; Dreadnought II or Carrier
II; avoid yellow and green without skips.

**Play.** Stay in the scoring pack mid-game, keep a home fleet, then become a sniper late with
mobility and an Imperial double score.

**Strength.** Async PoK S (third-best win rate); SCPT row 2 (2022), B (2025).

**Bot feasibility.**
- Agent use: whenever the bot (or anyone) moves into an empty system, take the free token; for the
  bot's own moves, park cheap ships in empty systems with frontier tokens (Dark Energy Tap explores
  them). Value medium, effort M (activation value for frontier tokens plus agent prompt).
- Research: Gravity Drive, Aetherstream, Sling Relay, Fleet Logistics. Effort S.
- Draft: Trade/Technology/Construction high in round 1, Warfare low. Effort S.
- Aetherstream and Aetherpassage prompts for other players: answer "yes" for neighbours the bot is
  not fighting (goodwill has no value to a bot, so default "no" is also defensible). Low value.

### The Mahact Gene-Sorcerers (`mahact`)

**Engine check.** Home Ixth 3/5; start 1 dreadnought, 1 carrier, 1 cruiser, 2 fighters, 3
infantry, dock; starting techs Bio-Stims and Predictive Intelligence; 3 commodities. Edict: after
winning a combat, put the opponent's command token in your fleet pool (once per player); those
tokens raise the fleet limit. Imperia: use the commanders of players whose tokens you hold. Hubris:
no Alliance notes. Commander (two other factions' tokens in the fleet pool): activate systems that
already hold your tokens, returning both tokens and ending the turn. Mech: end the turn of a token
owner who activates its system. Agent: pay a strategy-card secondary with the active player's
board token. Crimson Legionnaire II (GG) respawns infantry; Genetic Recombination (G).

**Play.** The prevailing view: not a bully. Take each player's token once (win one skirmish
against each, or sell the agent for "let me win a fight against you, you can retreat": winning, not
destroying, is enough), play friendly and score the minimum for four rounds, bank command tokens
(Leadership whenever possible, Predictive Intelligence to rearrange them), then a final-round swing
with the commander, taking control objectives after everyone has passed. The hero is a win-slaying
tool; use it only for points or for pay. Bio-Stims with a tech-skip planet fixes the weak early
economy ([r/ti "Mahact Misconceptions", 2021](https://www.reddit.com/r/twilightimperium/comments/mqqzbe/mahact_misconceptions/);
[r/ti "How to play Mahact?", 2021](https://www.reddit.com/r/twilightimperium/comments/r7all1/how_to_play_mahact/);
[BGG 2021-22](https://boardgamegeek.com/thread/2581554)).
Disagreement: some go aggressive early (Crimson Legionnaire II, AI Development into Cruiser II,
collect commanders such as Muaat's early) ([BGG 2022](https://boardgamegeek.com/thread/2886198)).
Blue tech is the default (Gravity Drive, Fleet Logistics, Light/Wave), Fighter II with a green
skip, Dreadnought II with a yellow one. Keep a notebook of every commander at the table.

**Strength.** S+/S in the async data and SCPT 2025; row 4 in SCPT 2022 (they were underrated early
in PoK; "banned in competitive environments" now).

**Bot feasibility.**
- Targeting: a fight against a seat whose token is not yet in the bot's fleet pool is worth a
  bonus (Edict), even against a lone destroyer; the flagship gets +2 against those seats (also
  missing in the fight model). Value high, effort M.
- Token economy: the bot's pool rules are generic; Mahact wants a large token bank for the last
  round, so keep more tactic tokens unspent after round 3 when nothing is worth activating. Value
  medium, effort S-M.
- Commander (last-round multiple activations) needs a plan for "which objectives can I reach";
  the per-activation greedy choice works reasonably once the tokens are banked. Effort M.
- Hard: choosing which commanders to borrow and when (needs a model of each commander).

### The Nomad (`nomad`)

**Engine check.** Home Arcturus 4/4; start 1 carrier, 1 destroyer, 3 fighters, the flagship
Memoria (capacity 3, barrage), 4 infantry, dock; starting tech Sling Relay; 4 commodities. Three
agents (Mercer moves ground forces, Thundarian re-rolls a combat round, Artuno banks trade goods
gained from the supply). Future Sight: a trade good when an agenda outcome you voted for or
predicted resolves. Memoria II (GBY). The flagship is adjacent to every system with a Nomad mech.
Commander (one scored secret): build the flagship free.

**Play.** A diplomatic, rich faction: sell The Cavalry to neighbours who fight, lend agents (the
re-roll agent to someone about to fight, Mercer to mech-heavy factions); bank trade goods with
Artuno, most consistently with Trade. Spread mechs so the flagship can jump anywhere; upgrade it,
unlock the commander for a free respawning flagship. Round 1 does not need Warfare: carrier to one
system, flagship plus an infantry to another, Sling Relay a dreadnought and take a third
([BGG IncrediSteve on Xxcha and Nomad, 2020](https://boardgamegeek.com/thread/2557071);
[BGG "Nomad flagship", 2020](https://boardgamegeek.com/thread/2554905)). Artuno triggers only on
trade goods from the supply (Trade's primary, cards), not on trades
([BGG 2021](https://boardgamegeek.com/thread/2570323)). Losing the flagship to the Cabal is a
named disaster ([BGG counters](https://boardgamegeek.com/thread/2422331)).

**Strength.** SCPT top row (2022), B (2025); async PoK B ("rich, but lacks late-game oomph").

**Bot feasibility.**
- Opening: Sling Relay is a component action (build a ship at a dock without a token); the
  opening plan's "third transport" can come from it. Value high, effort M (component-action hook).
- Movement: mechs as flagship anchors; spread mechs to systems the bot wants to reach. Effort M-L.
- Artuno: always bank trade goods gained from Trade. Effort S.
- Research: Memoria II when skips allow, Gravity Drive otherwise. Effort S.

### The Naaz-Rokha Alliance (`naaz`)

**Engine check.** Home Naazir 2/1 and Rokha 1/2; start 2 carriers, 1 destroyer, 2 fighters, 3
infantry, 1 mech, dock; starting techs Psychoarchaeology and AI Development Algorithm. Distant
Suns: explore with a mech present, draw two, keep one. Fabrication: purge two fragments for a relic
or one for a command token. Mechs roll two dice and, in space, become ships. Pre-Fab Arcologies
(GGG: ready a planet after exploring it); Supercharge (R). Agent: a player explores one of their
planets at the end of a turn. Commander (mechs in 3 systems): explore planets taken from others.

**Play.** "A puzzle with a unique solution each game": mechs are the plan (start with one; three
more cost about 6 resources), they fight like dreadnoughts in space and find relics and attachments
on planets. Core techs Scanlink Drone Network (with a forward dock on a hazardous/industrial
planet to churn explorations), Gravity Drive, Pre-Fab Arcologies (best with a green skip; ready a
big attached planet several times a round); Bio-Stims, Hyper Metabolism as extras; Carrier II
supplies the hit points mechs lack. Double-dock home early (production is the bottleneck). An
exploration is worth about 1.5-2 trade goods on average, so do not spend a token only to explore.
Be careful landing every mech: they cannot return to space to defend
([r/ti "Newbie questions: how to play Naaz-Rokha", 2026](https://www.reddit.com/r/twilightimperium/comments/1w0zzcl/newbie_questions_how_to_play_the_naazrokha/);
[r/ti 2025 tier comments](https://www.reddit.com/r/twilightimperium/comments/1kylhx4/faction_power_tier_list/)).

**Strength.** Async PoK S; SCPT row 2 (2022), A (2025); "Yssaril with a better military".

**Bot feasibility.**
- Build mix: mechs early (up to four), then carriers with fighters. `pick_produce` values mechs as
  ground forces; for Naaz-Rokha they are also ships. Value high, effort S.
- Movement: carry mechs to unexplored planets (Distant Suns), especially hazardous ones; give
  unexplored planets with a possible attachment extra worth in `planet_worth` for this faction.
  Value medium, effort M.
- Fabrication: purge fragments for a relic whenever two match. Effort S.
- Research: Gravity Drive, Pre-Fab Arcologies, Carrier II. Effort S.

### The Titans of Ul (`titans`)

**Engine check.** Home Elysium 4/1; start 1 dreadnought, 2 cruisers, 2 fighters, 3 infantry, dock;
starting techs Antimass Deflectors and Scanlink Drone Network; 2 commodities. Saturn Engine
cruisers (move 2, capacity 1; II: combat 6, move 3, capacity 2, sustain; GYR). Hel-Titan PDS
(ground force and structure, sustain, space cannon 6, PRODUCTION 1). Terragenesis: after
exploring a planet, place a sleeper token. Awaken: activating a system with sleepers turns them
into PDS. Commander (5 structures): a trade good each time you produce. Hero: Elysium +3/+3 and a
space cannon battery. Mech replaces a PDS placement with a mech and an infantry.

**Play.** A "foothold" faction: secure a slice with a path to Mecatol Rex; Saturn Engine II is the
first technology priority (3 cruisers with mechs and infantry are a cheap strike force); Hel-Titans
(the best defensive ground unit, mini-factories) hold the slice; the commander makes production a
trade-good engine. Reactivating explored systems awakens sleepers, so Warfare in round 1 suits
Titans (re-activate to awaken and produce). Disagreement: secure the slice first versus grab the
equidistant at once (the sleeper makes it hard to take back) and take Mecatol Rex early in round 2
([BGG Titans of Ul Guide, 2023](https://boardgamegeek.com/thread/3103032);
[BGG 2021](https://boardgamegeek.com/thread/2641111);
[r/ti "How to play Titans in this slice", 2025](https://www.reddit.com/r/twilightimperium/comments/1lmrak1/how_to_play_titans_in_this_slice/)).
Round 1 cards: Trade (x-1 deals), Technology, Warfare, Construction. Traps: cruisers fall off
mid-game without a bigger fleet; awakening costs command tokens. Sell Terraform for 3-4 trade
goods (it can carry an objective). Titans trivially score structure and attachment objectives.

**Strength.** A in both 2025 lists, row 3 in SCPT 2022; early-PoK data called them strong.

**Bot feasibility.**
- Awaken: activating a system with own sleepers is worth a PDS per sleeper; `activation_value`
  should add that value (the engine applies Awaken on activation). Value high, effort S-M.
- Terragenesis prompt: always place a sleeper on the explored planet. Effort S.
- Research: Saturn Engine II (GYR; Scanlink Drone Network is the yellow, so AI Development for the
  red, then a green technology or skip),
  Hel-Titan II only for unit-upgrade objectives. Effort S.
- Build mix: Saturn Engines as transports and warships. Check that capacity-1 cruisers count as
  transports in the opening production rule (same as Argent). Effort S.
- Objective focus: structure objectives; Construction primary always takes a PDS (Titans' PDS are
  ground forces too). Effort S.

## Codex faction

### The Council Keleres (`keleres`: records `keleresm`, `keleresx`, `keleresa`)

**Engine check.** Codex III version (no Thunder's Edge breakthrough here). Three records, one per
borrowed home system and hero (Mentak, Xxcha, Argent); seated only on explicit request, and the
borrowed faction must be unplayed. Start 2 carriers, 1 cruiser, 2 fighters, 2 infantry, dock; no
starting technology in this engine (the printed faction chooses two technologies owned by other
players; not modelled); 2 commodities. Council Patronage: at the start of every strategy phase
replenish commodities and gain a trade good. Law's Order: a trade good or commodity blanks all laws
for a turn. Faction techs I.I.H.Q. Modernization (Y: neighbours with everyone at or next to Mecatol
Rex, gain the Custodia Vigilia planet) and Agency Supply Network (YY: a second production anywhere
once per action). Flagship: others pay 2 influence to activate its system. Mech: others pay 1
influence to commit ground forces to its planet. Agent: anyone may spend commodities as trade goods.
Heroes: Harka Leeds (Mentak: three component-action cards), Odlynn Myrr (Xxcha: votes, then trade
goods and tokens from dissenters), Kuuasi Aun Jalatai (Argent: flagship and two ships appear in a
defended system).

**Play.** Rich and self-sufficient: Council Patronage makes the 2-commodity faction effectively a
5-commodity one and lets Keleres offer trade goods before Trade is even played; "you do not need
Trade". The Mentak home (one planet to defend, most resources) and hero are the consensus pick;
Xxcha's home (more influence) is second. Round 1 cards depend on objectives: Technology is
"never bad" (the agent plus Patronage can pay for a double research), Trade if no better; with only
two infantry, Construction or Warfare's secondary to take more planets. Technology: Sarween ("one
of the few factions where Sarween is good", doubled by Agency Supply Network), Gravity Drive,
Hyper Metabolism, then the faction techs by round 3; Carrier II as the unit upgrade, X-89 as a
late finisher. Mecatol Rex in round 2 if possible, otherwise do not force it. Weak in combat and
board presence; hard to kill and expensive to negotiate against thanks to the flagship and mechs
([r/ti "Council Keleres Strategy Guide (TE edition)", 2025](https://www.reddit.com/r/twilightimperium/comments/1owpvgi/council_keleres_strategy_guide_thunders_edge/);
[r/ti "Keleres tips/tricks", 2026](https://www.reddit.com/r/twilightimperium/comments/1vh24ep/the_council_keleres_tipstricks/);
[r/ti "Best first card as Keleres", 2026](https://www.reddit.com/r/twilightimperium/comments/1rgfpss/whats_the_best_first_card_playing_the_council_of/);
[r/ti "Keleres starting techs", 2026](https://www.reddit.com/r/twilightimperium/comments/1syygrw/keleres_starting_techs/)).
These sources are Thunder's Edge era; the breakthrough advice in them does not apply here. Starting
technology advice (AI Development and Dark Energy Tap, or Neural/Sarween) is moot in this engine.

**Strength.** B in SCPT 2025; not in the PoK-era lists (too few games).

**Bot feasibility.**
- Starting without technology hurts more here than for Winnu: research Gravity Drive (Antimass or
  a blue skip first) and Sarween, then I.I.H.Q. and Agency Supply Network. Effort S.
- Trades: with commodities refreshed every round, the bot should offer commodities early each
  round through the proposal hook. Value medium, effort S-M.
- Production: Agency Supply Network's second production prompt: always take it when resources
  remain. Effort S.
- Build mix: carriers and fighters, mechs on key planets (influence tax on invaders). Effort S.

## Appendix: Thunder's Edge factions (not seatable here)

The Last Bastion, Deepwrought Scholarate, Firmament/Obsidian, Crimson Rebellion and Ral Nel
Consortium have engine modules but cannot be seated in server games (their home tiles are outside
the `POK` source set, see `preset.rs`). Community knowledge is young (the expansion came out in
late 2025) and mostly in video guides. What the threads say, for when they become seatable:

- **Crimson Rebellion**: S in SCPT 2025; "cannot be win-slain" (safe home), strong agent,
  commander late-game money, breach mechanics around the Fracture; an alliance partner gets rich
  ([r/ti SCPT 2025 list](https://www.reddit.com/r/twilightimperium/comments/1q0jwu5/scpt_faction_tier_list_2025_one_billion_percent/);
  [r/ti Crimson guide thread, 2025](https://www.reddit.com/r/twilightimperium/comments/1p6bw9g/crimson_rebellion_faction_guide/);
  [r/ti tier list, 2026](https://www.reddit.com/r/twilightimperium/comments/1wfgam5/faction_tier_list/)).
- **Deepwrought Scholarate**: S in SCPT 2025; "no real downsides" in the 2026 thread.
- **Firmament/Obsidian**: F in SCPT 2025; the unforgiving start ("don't start on Sarween = game
  over"), the debate is when to flip to the Obsidian; "sell the note, flip" versus waiting for 2-3
  puppets ([r/ti 2026 Firmament guide thread](https://www.reddit.com/r/twilightimperium/comments/1vlghbs/the_firmament_the_obsidian_faction_guide_ccc/);
  [r/ti 2026 tier comments](https://www.reddit.com/r/twilightimperium/comments/1qctdtz/factions_tier_list/)).
- **Ral Nel Consortium** and **Last Bastion**: C in SCPT 2025; Last Bastion "slice-first",
  struggles with some round-1 objectives, defended as B+/A by fans.

Bot feasibility: none needed until they can be seated.

## Ranked: the 15 most worthwhile ideas to implement

Ranked by expected value for the rule bot per unit of effort, given the bot's current shape
(greedy per-decision rules, own-seat observation). None needs engine changes.

1. **Per-faction research wish lists** (replacing "Gravity Drive whenever offered"): an ordered
   list per faction from the sections above, falling back to Gravity Drive. Also fixes the factions
   that start without technology in this engine (Sardakk, Winnu, Argent, Keleres: Antimass or AI
   Development first). Value high, effort S.
2. **A component-action hook in `pick_action`**: let the bot choose `component` options (Sol
   Orbital Drop, Muaat Star Forge, Sling Relay for Nomad/Ghosts/anyone who owns it, the Arborec
   agent's cruiser-to-carrier in round 1, the Hacan agent, Yssaril Stall Tactics), each with a
   simple "worth it now" rule; today the rule bot never takes a component action deliberately.
   Value high, effort M.
3. **Per-faction draft weights** in `opening_card_score`: Trade allowed in round 1 for Ghosts,
   Nekro, Empyrean, Cabal, Titans, Arborec (not only Hacan); Diplomacy high for Xxcha (Peace
   Accords); Warfare first for Jol-Nar (two-infantry start); Leadership up for Muaat, Mahact,
   Cabal; Technology first for Arborec and Naalu. Value high, effort S.
4. **Capacity-1 transports**: count Argent Strike Wing Alphas and Titans Saturn Engines (capacity
   1) as transports in the opening plan (`free_transports_and_ground`, the opening transport bonus
   in `pick_produce`, which use `capacity() >= 2`). Without it these factions' openings stall.
   Value high for two factions, effort S.
5. **Faction build-mix weights** in `pick_produce`: fighters for Naalu (Hybrid Crystal Fighter II),
   dreadnoughts for Letnev and L1Z1X, cruisers for Mentak and Titans, mechs for Naaz-Rokha,
   infantry for Arborec and Sol, Strike Wing Alphas for Argent. Value medium-high, effort S.
6. **Always-yes faction prompts**: Pillage (Mentak), Terragenesis (Titans), Amalgamation (Cabal),
   Fabrication with two matching fragments (Naaz-Rokha), the Cabal agent on a replenish, Artuno
   banking (Nomad). The async-data observation that "the bot never forgets to Pillage" is a real
   edge. Value medium, effort S each.
7. **Winnu Mecatol rule**: no influence cost for the custodians (Blood Ties), a free dock and PDS
   on landing (Reclamation): `mecatol_takeable` and `MECATOL_BONUS` per faction. Value high for
   Winnu, effort S.
8. **Own-system activations that produce or awaken**: Arborec infantry/mechs are production units
   (activate own forward systems to build even without six spare resources); Titans sleepers turn
   into PDS when the system is activated. Value medium, effort S-M.
9. **Letnev Armada in the fleet-pool rule**: keep two fewer fleet tokens (`fleet_wanted` minus 2),
   which turns into tactic tokens. Value medium, effort S.
10. **Xxcha Diplomacy**: follow Diplomacy's secondary whenever a Peace Accords planet exists and
    pick the best `planet_worth` target. Value medium, effort S-M.
11. **Faction terms in the fight model** (`ti4_policy::fleet_strength`): Mentak Ambush, Letnev
    Munitions/Non-Euclidean Shielding, Argent barrage, Mahact flagship +2, Winnu and Yin
    flagships, Naaz-Rokha mechs in space, Cabal captures (lower margin). Value medium, effort M.
12. **Technology-steal targeting for Nekro**: value fights by the new technologies the defender
    owns; pick Technological Singularity gains by a fixed preference (movement, then the unit
    upgrades the bot builds). Value medium-high for Nekro, effort M.
13. **Mahact Edict targeting**: a bonus for winning any fight against a seat whose token is not in
    the fleet pool yet; bank tactic tokens for the final rounds. Value medium, effort M.
14. **Hacan trade volume**: lower `ACCEPT_NET` to about zero and propose commodity washes with every
    seat each round (Guild Ships means no neighbour needed). Value medium, effort S-M.
15. **Politics in the second-to-last round** (general, every faction): take Politics when the game
    is likely to end next round so the bot is speaker in the last round; plus "Imperial when it
    scores" (already present). Value medium, effort S (needs a "points left" estimate, which the
    objective-scoring work may already provide).

Next in line: Saar docks that follow the fleet with escorts; Naaz-Rokha mechs to unexplored
planets; Empyrean frontier-token parking; Muaat war-sun escort rule; Yssaril stall before passing.

## Too hard for a rule-based bot (for now)

- **Negotiation as an economy**: selling Research Agreement (Jol-Nar), Fires of the Gashlai (Muaat),
  War Funding (Letnev), agents (Mahact, Titans, Naalu), Empyrean's pacts, Hacan's "supermarket",
  "x-1" trade deals with conditions. The transaction hooks exist, but pricing a promise needs a
  model of the other player's plans.
- **Table reading**: "be third, not first", choosing when to win-slay and getting paid for it,
  "ethical Mentak", posing as the underdog (Letnev), not drawing heat (Saar, Nekro, Winnu).
- **Agenda play**: Xxcha Quash, Argent Zeal, Nekro's Galactic Threat predictions, Mahact Genetic
  Recombination, Keleres riders all need agenda evaluation; agendas are random today.
- **Action-card play**: Yssaril's entire game, and every faction's Sabotage/Direct Hit timing.
- **Multi-round swings**: Mahact's final-round commander turn, Hacan's Quantum Datahub Node grab
  of Imperial, Letnev's hero round, Naalu's hold-the-0-token-to-the-last-turn; they need a plan
  across rounds, not per-decision greed.
- **Positional tricks**: Ghosts wormhole placement and system swaps, Saar's Chaos Mapping
  asteroid base, Titans and Ghosts PDS networks through wormholes, Nomad mech anchors for the
  flagship.
- **Heroes in general**: most heroes are "best moment" plays (Winnu any primary, Mahact fleet
  shove, Muaat supernova, Arborec produce-everywhere); a rule like "use when it scores now" is
  possible per hero but each needs its own evaluator.
