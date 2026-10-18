# Faction plans: mid- and late-game strategy per faction

What each faction should aim for from round 3 on, and in particular **which technologies to
research and which units to build**. Written as the basis for the rule bot's faction plans
(`crates/ti4-server/src/session/smart_bot_faction_plans.rs`), so every claim was checked
against this engine's content (`crates/ti4-content/content/{factions,technologies,units,
abilities,leaders}.json`). Where the engine differs from the printed game, the engine wins and
the difference is noted.

Scope: every faction in `factions.json`: the 17 base factions, the 7 Prophecy of Kings
factions, the Council Keleres (codex 3, three home-system variants, one profile) and the
Thunder's Edge factions the corpus also carries (Last Bastion, Deepwrought, Firmament/Obsidian,
Crimson Rebellion, Ral Nel). The bot's seeded roster (`preset::ROSTER`) seats only the 24 base
and PoK factions; the others have profiles and data but are not measured by the survey.

## How to read a profile

- **Win path**: how the faction usually gets to 10 points.
- **Kit**: the strengths that come from its sheet (starting fleet, starting tech, abilities,
  faction techs, flagship, mech, commander unlock, hero), as this engine has them.
- **Mid (r3-4) / Late (r5+)**: what to do on the board.
- **Tech**: the ordered research priority, by engine alias, after Gravity Drive (`gd`), which
  every plan takes first when it is offered. The order is achievable: every entry's
  prerequisites are met by the starting techs plus the entries before it (a tech specialty
  planet of the right colour lets a step be skipped). `a|b` means either.
- **Build**: unit priority by phase and the composition to aim at.
- **Cards/tokens**: strategy card and command token tendencies.
- **Pitfalls**: the usual ways the plan goes wrong.

## General rules (all factions)

- **Tech colours and prerequisites** (engine data). Blue: `amd`(-), `gd`(B), `sr`(B), `det`(-),
  `fl`(BB), `lwd`(BBB). Green: `nm`(-), `pa`(-), `dxa`(G), `bs`(G), `hm`(GG), `x89`(GGG).
  Red: `ps`(-), `aida`(-), `sar`(R), `md`(R), `da`(RR), `asc`(RRR). Yellow: `st`(-), `sdn`(-),
  `gls`(Y), `pi`(Y), `td`(YY), `ie`(YYY). Unit upgrades (no colour): `dd2` RR, `cv2` BB,
  `ff2` GB, `inf2` GG, `pds2` RY, `sd2` YY, `cr2` GYR, `dn2` BBY, `ws` RRRY. A faction upgrade
  replaces the generic one (`baseUpgrade`): the generic one is then not researchable.
- **Gravity Drive first** (one more hex for every ship each activation), then the faction's
  key unit upgrade, then whatever completes a colour pair: most factions want two unit upgrades
  by round 4 (the stage I objective for two unit upgrades) and a second colour at two.
- **When the next tech is out of reach**, research the cheapest offered tech of a colour it is
  missing (e.g. `amd` or `sr` for a blue-blue upgrade, `nm`/`dxa` for green, `ps`/`sar` red,
  `st`/`gls` yellow), preferring one that is itself on the faction's list.
- **Tech specialty planets** count as one prerequisite of their colour while ready; planets
  with the plan's key colour (named per faction) are the ones worth a detour. (The bot does not
  value this yet: its planet worth already favours any specialty a little.)
- **Fleets**: dreadnoughts are the efficient sustain-damage backbone (4 resources, one slot);
  carriers with fighters add cheap hit absorbers; destroyers counter fighter screens (anti-
  fighter barrage); cruisers are cheap cubes for objectives and fleet supply; a war sun (needs
  `ws`, Muaat from the start) is the late-game hammer. Ground forces follow the planets you
  still mean to take: two per target planet, more where defended.
- **Rounds 1-2** are expansion (carriers and infantry; see `smart_bot.rs`), so the profiles
  below matter from round 3 on.
- **Engine notes**. Component actions (e.g. Sling Relay, Production Biomes, Wormhole
  Generator) and agents are left to the random pick by the bot, so passive techs and unit
  upgrades are worth more to it than action techs. The Nekro Virus cannot research at all
  (Propagation). Before this branch the engine never let the Muaat produce a war sun (the
  production gate only accepted a technology ending in `ws`; Muaat's war sun is on its faction
  sheet and its upgrade is `pws2`): fixed in `ti4_engine::production::buildable_for`.
  Suspected, not fixed: the Keleres faction techs are tagged `"faction": "keleres"` while the
  seats are `keleresm`/`keleresx`/`keleresa`, and `technology::can_research` compares the two
  literally, so a Keleres seat may never be offered `iihq`/`asn` (the plan then skips them).

---

## Base game

### The Arborec (`arborec`)
- **Win path**: ground sprawl; control and planet-count objectives.
- **Kit**: `md` start; carrier, cruiser, 2 fighters, 4 infantry, PDS, dock. Mitosis (an
  infantry free each status phase; its docks cannot produce infantry). `lw2` Letani Warrior II
  (GG, infantry with production 2), `bio` Bioplasmosis (GG). Flagship produces 5 units after
  activating its system. Mech deploys through Mitosis. Commander at 12 ground forces on planets.
- **Mid**: spread infantry to every planet, build a second dock-equivalent through the flagship,
  fill carriers. **Late**: flagship-led production at the front, dreadnought escort.
- **Tech**: `nm`, `dxa`, `lw2`, `amd|sr`, `cv2`, `bio`, `hm`, `sar`, `da`.
  Key colour green, then blue.
- **Build**: mid carriers (2-3), mechs, flagship once 8 resources free; late dreadnoughts (2+),
  flagship. Fighters only as fodder.
- **Cards/tokens**: Construction and Warfare; a large tactic pool (many small activations).
- **Pitfalls**: infantry with no carriers to move them; expensive war suns (red-red-red-yellow
  is far off its green path).

### The Ghosts of Creuss (`ghost`)
- **Win path**: mobility through wormholes; distant control objectives, Mecatol pressure.
- **Kit**: `gd` start; carrier, 2 destroyers, 2 fighters, 4 infantry. Quantum Entanglement
  (alpha/beta adjacent), Slipstream (+1 move from home/wormholes). `ds` Dimensional Splicer (R),
  `wg` Wormhole Generator (BB). Flagship carries a delta wormhole. Commander: units in 3 alpha/
  beta wormhole systems.
- **Mid**: take wormhole systems, keep a strike group that can reach any wormhole. **Late**:
  dreadnoughts plus the flagship to hold what was taken.
- **Tech**: `sr|amd`, `ps`, `ds`, `cv2`, `st`, `dn2`, `fl`, `wg`, `da`. Key colour blue.
- **Build**: mid carriers and destroyers into dreadnoughts (2); late dreadnoughts (3), flagship.
- **Cards/tokens**: Leadership/Warfare; a strong fleet pool for the strike group.
- **Pitfalls**: opening wormhole routes to neighbours; a weak home (Creuss is off-board).

### The Emirates of Hacan (`hacan`)
- **Win path**: economy; spend objectives, buy points with trade goods.
- **Kit**: `st`, `amd` start; 2 carriers, cruiser, 2 fighters, 4 infantry over 3 home planets.
  Master of Trade (Trade secondary free), Guild Ships, Arbiters. `qdn` (YYY), `pm` Production
  Biomes (GG). Flagship spends trade goods on combat dice. Commander at 10 trade goods.
- **Mid**: hold Trade or trade into it; turn trade goods into dreadnoughts and objectives.
  **Late**: buy the last fleet for control objectives in one production.
- **Tech**: `gls`, `sdn|pi`, `qdn`, `nm`, `dxa`, `pm`, `cv2`, `dn2`, `ie`. Key colour yellow.
- **Build**: mid carriers + fighters, dreadnoughts (2); late dreadnoughts (3), flagship.
- **Cards/tokens**: Trade every round it is open; strategy pool for secondaries.
- **Pitfalls**: hoarding trade goods; looking rich (target); thin fleet for control objectives.

### The Universities of Jol-Nar (`jolnar`)
- **Win path**: technology; tech objectives, then whatever the techs unlock.
- **Kit**: `amd`, `st`, `nm`, `ps` start (one of each colour); dreadnought, 2 carriers,
  fighter, 2 infantry, 2 PDS. Fragile (-1 to combat rolls), Brilliant (Technology primary on
  the secondary), Analytical (ignore one prerequisite). `ers` E-Res Siphons (YY), `scc` (BB).
  Flagship: 9-10 rolls make extra hits. Commander at 8 technologies (rerolls).
- **Mid**: research every round (always follow Technology); defensive fleet near home, PDS.
  **Late**: upgraded dreadnoughts with fighter screens; avoid even fights (Fragile).
- **Tech**: `gls`, `ers`, `sar`, `cv2`, `ff2`, `dxa`, `dn2`, `pi`, `da`, `hm`, `fl`, `scc`,
  `ie`, `asc`. Every colour; aim for 8 techs (commander).
- **Build**: mid carriers + fighters (Fighter II), dreadnoughts (2); late dreadnoughts (3+),
  flagship. Few cruisers/destroyers.
- **Cards/tokens**: Technology first; strategy pool deep enough to follow Technology.
- **Pitfalls**: techs instead of plastic; taking fights at even odds.

### The L1Z1X Mindnet (`l1z1x`)
- **Win path**: combat; dreadnought fleet, conquest and control objectives.
- **Kit**: `ps`, `nm` start; dreadnought, carrier, 3 fighters, 5 infantry, PDS. Assimilate
  (take structures), Harrow (bombard during ground combat). Own dreadnought (capacity 2),
  `sdn2` Super Dreadnought II (BBY: combat 4, move 2, capacity 2), `is` Inheritance Systems
  (YY). Flagship makes dreadnought hits hit non-fighters. Commander at 4 dreadnoughts.
- **Mid**: build dreadnoughts to four (commander), take the strongest neighbouring systems.
  **Late**: super-dreadnought stack with fighter screen through to the objectives.
- **Tech**: `amd|sr`, `st`, `sdn2`, `sar`, `ff2`, `da`, `gls`, `is`, `asc`, `fl`. Key colour
  blue (two) and one yellow.
- **Build**: mid dreadnoughts (to 3-4), one carrier, fighters; late dreadnoughts (5), flagship.
- **Cards/tokens**: Warfare; fleet pool large enough for the dreadnought stack.
- **Pitfalls**: bombarding without the infantry to land; slow fleet without `gd`/`sdn2`.

### The Barony of Letnev (`letnev`)
- **Win path**: combat and economy from a rich home; military objectives.
- **Kit**: `amd`, `ps` start; dreadnought, carrier, destroyer, fighter, 3 infantry. Armada
  (+2 ships per system over the fleet pool), Munitions Reserves (2 trade goods: rerolls). `nes`
  Non-Euclidean Shielding (RR: sustain cancels 2 hits), `l4` L4 Disruptors (Y). Flagship
  repairs. Commander: 5 non-fighter ships in one system.
- **Mid**: dreadnoughts plus carriers out of the 6-resource home. **Late**: deathball of
  dreadnoughts (Armada lets one system hold more) on the objectives.
- **Tech**: `sar`, `nes`, `st`, `dn2`, `da`, `l4`, `asc`, `cv2`. Key colour red, blue for `dn2`.
- **Build**: mid dreadnoughts (3), carrier, fighters; late dreadnoughts (4+), flagship.
- **Cards/tokens**: Warfare/Leadership; fleet pool for the deathball.
- **Pitfalls**: a fleet that does not score; leaving the home exposed.

### The Mentak Coalition (`mentak`)
- **Win path**: tempo and trade goods (Pillage), cheap ships; mixed objectives.
- **Kit**: `ps`, `st` start; carrier, 2 cruisers, 3 fighters, 4 infantry, PDS. Ambush (cruiser/
  destroyer pre-combat shots), Pillage. `mc` Mirror Computing (YYY), `so` Salvage Operations
  (YY). Flagship stops enemy sustain. Commander at 4 cruisers.
- **Mid**: cruisers to four (commander), Salvage Operations for combat income. **Late**:
  `cr2` cruisers plus dreadnoughts and the flagship against sustain-heavy fleets.
- **Tech**: `gls`, `so`, `sdn|pi`, `mc`, `nm`, `cr2`, `sar`, `dd2`, `da`. Key colour yellow.
- **Build**: mid cruisers (to 4), destroyers, carrier; late dreadnoughts (2), flagship,
  cruisers.
- **Cards/tokens**: Trade/Politics depend on the table; tactic tokens for many small actions.
- **Pitfalls**: Pillage instead of an economy; spread too thin.

### The Embers of Muaat (`muaat`)
- **Win path**: war suns; military objectives, Mecatol by force.
- **Kit**: `ps` start; war sun, 2 fighters, 4 infantry. Star Forge (strategy token: 2
  fighters or a destroyer next to a war sun), Gashlai Physiology (through supernovas). Own
  war sun (cost 12, move 1), `pws2` Prototype War Sun II (RRRY: cost 10, move 3), `mr` Magmus
  Reactor (RR). Commander: produce a war sun.
- **Mid**: second war sun as soon as 12 resources come together (commander); carriers for the
  infantry. **Late**: `pws2` war suns move 3: they reach anything.
- **Tech**: `sar`, `st`, `da`, `pws2`, `mr`, `gls`, `amd|sr`, `cv2`, `asc`. Key colour red.
- **Build**: mid war sun (second), carriers, fighters; late war suns, dreadnoughts, fighters.
- **Cards/tokens**: Warfare; strategy pool for Star Forge.
- **Pitfalls**: the war sun stranded at move 1 before `pws2`; no carriers or infantry.
- **Engine**: war sun production was unreachable for Muaat before this branch (see above).

### The Naalu Collective (`naalu`)
- **Win path**: initiative 0 plus fighter swarms; objectives scored first.
- **Kit**: `st`, `nm` start; carrier, cruiser, destroyer, 3 fighters, 4 infantry, PDS.
  Telepathic (the "0" token), Foresight. Own fighters (combat 8), `hcf2` Hybrid Crystal Fighter
  II (GB: combat 7, move 2), `ng` Neuroglaive (GGG). Flagship (capacity 6) commits fighters as
  ground forces. Commander: ground forces in or next to Mecatol Rex.
- **Mid**: `hcf2`, then carriers full of fighters. **Late**: fighter swarm with a dreadnought
  anchor.
- **Tech**: `hcf2`, `dxa`, `amd|sr`, `cv2`, `bs`, `ng`, `ps`, `sar`, `gls`. Key colours green
  and blue.
- **Build**: mid carriers (2-3), fighters (fill capacity); late fighters, dreadnoughts (2),
  flagship.
- **Cards/tokens**: any card (initiative is fixed); tactic tokens.
- **Pitfalls**: fighters without capacity; destroyer barrages.

### The Nekro Virus (`nekro`)
- **Win path**: combat; technology from fights (Technological Singularity), military
  objectives.
- **Kit**: `dxa` start; dreadnought, carrier, cruiser, 2 fighters, 2 infantry. Cannot
  research (Propagation: 3 command tokens instead), cannot vote (Galactic Threat). Valefar
  Assimilators. Flagship lets ground forces fight in space. Commander at 3 technologies.
- **Mid**: pick fights against tech-rich neighbours. **Late**: dreadnought fleet with the
  flagship.
- **Tech**: none researchable (the engine never offers Nekro a research choice).
- **Build**: mid dreadnoughts (2-3), cruisers; late dreadnoughts (4), flagship, mechs.
- **Cards/tokens**: Technology for 3 tokens; a large fleet pool.
- **Pitfalls**: wars for a tech it does not need.

### The Clan of Saar (`saar`)
- **Win path**: mobile production; expansion and distant control.
- **Kit**: `amd` start; 2 carriers, cruiser, 2 fighters, 4 infantry, Floating Factory. Nomadic
  (score without the home), Scavenge. `ffac2` Floating Factory II (YY), `cm` Chaos Mapping (B).
  Mech deploys for a trade good. Commander at 3 docks.
- **Mid**: factories forward, carriers out of them. **Late**: dreadnoughts from forward docks.
- **Tech**: `cv2`, `st`, `gls`, `ffac2`, `cm`, `ps`, `sar`, `da`, `dn2`. Key colours blue and
  yellow.
- **Build**: mid carriers (3), destroyers, mechs; late dreadnoughts (2), carriers.
- **Cards/tokens**: Construction (docks), Leadership.
- **Pitfalls**: lone factories blockaded; one fleet for two objectives.

### Sardakk N'orr (`sardakk`)
- **Win path**: combat; +1 on every roll, conquest and control objectives.
- **Kit**: no starting tech; 2 carriers, cruiser, 5 infantry, PDS. Unrelenting (+1 to combat
  rolls). Own dreadnought, `exo2` Exotrireme II (BBY), `vpw` Valkyrie Particle Weave (RR).
  Flagship +1 to others' rolls. Mech hits after sustaining. Commander: 5 non-home planets.
- **Mid**: carriers and infantry into neighbours, Valkyrie Weave for ground fights. **Late**:
  Exotrireme dreadnoughts plus the flagship.
- **Tech**: `ps`, `sar`, `vpw`, `st`, `amd|sr`, `exo2`, `da`, `dd2`, `asc`. Key colour red, blue
  for `exo2` (starts with nothing: every step counts).
- **Build**: mid carriers (2), dreadnoughts (2), mechs; late dreadnoughts (4), flagship.
- **Cards/tokens**: Warfare; tactic tokens.
- **Pitfalls**: no tech start: chasing every colour; attacking planets that do not matter.

### The Federation of Sol (`sol`)
- **Win path**: infantry and carriers; planet and control objectives.
- **Kit**: `amd`, `nm` start; 2 carriers (capacity 6), destroyer, 3 fighters, 5 infantry
  (combat 7). Orbital Drop, Versatile (+1 command token). `ac2` Advanced Carrier II (BB:
  capacity 8, sustain), `so2` Spec Ops II (GG). Flagship capacity 12, infantry each status
  phase. Commander at 12 resources.
- **Mid**: `ac2`, carriers full of infantry on every reachable planet. **Late**: flagship
  with infantry, dreadnought escort.
- **Tech**: `ac2`, `dxa`, `so2`, `st`, `dn2`, `hm`, `ps`, `sar`, `ff2`. Key colours blue and
  green.
- **Build**: mid carriers (3), infantry; late flagship, dreadnoughts (2), carriers.
- **Cards/tokens**: Leadership (extra tokens suit Versatile), Warfare.
- **Pitfalls**: infantry on planets with nothing in the space above.

### The Winnu (`winnu`)
- **Win path**: Mecatol Rex (Blood Ties, Reclamation) and Imperial.
- **Kit**: one starting tech of choice; carrier, cruiser, 2 fighters, 2 infantry, PDS. `lgf`
  Lazax Gate Folding (BB), `htp` Hegemonic Trade Policy (YY). Flagship rolls one die per
  opponent non-fighter ship. Commander: control Mecatol or fight there (+2 there and at home).
- **Mid**: take and hold Mecatol, dock and PDS there (Reclamation). **Late**: dreadnoughts
  around Mecatol, Imperial.
- **Tech**: `st`, `amd|sr`, `lgf`, `gls`, `htp`, `ps`, `sar`, `da`, `dn2`. Key colours blue and
  yellow.
- **Build**: mid carriers (2), infantry, mechs; late dreadnoughts (3), flagship.
- **Cards/tokens**: Imperial whenever Mecatol is held.
- **Pitfalls**: Mecatol without the fleet to keep it.

### The Xxcha Kingdom (`xxcha`)
- **Win path**: politics and defence; agendas, influence objectives.
- **Kit**: `gls` start; carrier, 2 cruisers, 3 fighters, 4 infantry, PDS. Peace Accords (free
  planets next door), Quash. `it` Instinct Training (G), `nf` Nullification Field (YY).
  Flagship and mech fire space cannon into adjacent systems. Commander at 12 influence.
- **Mid**: take the free planets, PDS cover, cruisers for cheap presence. **Late**: dreadnoughts
  and the flagship as the space-cannon anchor.
- **Tech**: `st`, `nf`, `nm`, `it`, `ps`, `pds2`, `cr2`, `sar`, `amd|sr`, `dn2`. Key colour
  yellow.
- **Build**: mid cruisers (3), carriers, mechs; late dreadnoughts (2), flagship.
- **Cards/tokens**: Politics late, Diplomacy; strategy pool for Quash and Nullification.
- **Pitfalls**: over-defending planets nobody attacks; no fleet for control objectives.

### The Yin Brotherhood (`yin`)
- **Win path**: attrition; cheap ships as weapons (Devotion), infantry conversion.
- **Kit**: `st` start; 2 carriers, destroyer, 4 fighters, 4 infantry. Devotion, Indoctrination.
  `yso` Yin Spinner (GG: 2 infantry after producing), `ic` Impulse Core (YY). Flagship
  destroys all ships when destroyed. Commander: use a faction ability (a green prerequisite).
- **Mid**: Yin Spinner, carriers full of infantry, cruisers/destroyers as Devotion shots.
  **Late**: dreadnoughts with cheap escorts.
- **Tech**: `nm`, `dxa`, `yso`, `gls`, `ic`, `ps`, `sar`, `amd|sr`, `cv2`, `ff2`, `hm`. Key
  colour green.
- **Build**: mid carriers (2), destroyers/cruisers, infantry; late dreadnoughts (2), cruisers.
- **Cards/tokens**: Construction, Warfare.
- **Pitfalls**: sacrificing ships for nothing.

### The Yssaril Tribes (`yssaril`)
- **Win path**: action cards and secrets; flexible scoring.
- **Kit**: `nm` start; 2 carriers, cruiser, 2 fighters, 5 infantry, PDS. Stall Tactics,
  Scheming, Crafty. `tp` Transparasteel Plating (G), `mi` Mageon Implants (GGG). Flagship
  moves 2 through enemy ships. Commander at 7 action cards.
- **Mid**: carriers and infantry for planet objectives. **Late**: dreadnoughts plus the
  flagship for a decisive move.
- **Tech**: `dxa`, `tp`, `mi`, `st`, `ps`, `sar`, `hm`, `amd|sr`, `cv2`, `dn2`. Key colour
  green.
- **Build**: mid carriers (2), cruisers, mechs; late dreadnoughts (2), flagship.
- **Cards/tokens**: Politics (cards), strategy pool.
- **Pitfalls**: stalling without a payoff; no board presence.

## Codex

### The Council Keleres (`keleresm`, `keleresx`, `keleresa`)
- **Win path**: politics and Mecatol (Custodia Vigilia), objectives.
- **Kit**: no starting tech here (the printed game gives two techs owned by other players);
  2 carriers, cruiser, 2 fighters, 2 infantry, dock on the borrowed home. Council Patronage,
  Laws Order. `iihq` I.I.H.Q. Modernization (Y), `asn` Agency Supply Network (YY). Flagship
  capacity 6, costs others 2 influence to activate.
- **Mid**: Mecatol neighbourhood, production twice per action (`asn`). **Late**: dreadnoughts,
  flagship.
- **Tech**: `st`, `iihq`, `asn`, `sdn|pi`, `ps`, `sar`, `amd|sr`, `cv2`, `dn2`, `da`. Key colour
  yellow.
- **Build**: mid carriers (2), cruisers, infantry; late dreadnoughts (3), flagship.
- **Cards/tokens**: Imperial with Mecatol, Politics.
- **Pitfalls**: few starting units: over-extending.

## Prophecy of Kings

### The Argent Flight (`argent`)
- **Win path**: votes (Zeal) and tempo; structure and military objectives.
- **Kit**: two starting techs of choice; carrier, 2 destroyers (own, capacity 1), 2 fighters,
  5 infantry, PDS. Zeal, Raid Formation. `swa2` Strike Wing Alpha II (RR: destroyer combat 7,
  capacity 1, barrage 6x3), `ah` Aerie Hololattice (Y). Commander: 6 units with barrage/space
  cannon/bombardment.
- **Mid**: Strike Wing destroyers (commander count), PDS. **Late**: dreadnoughts behind a
  destroyer screen.
- **Tech**: `ps`, `sar`, `swa2`, `st`, `ah`, `pds2`, `da`, `amd|sr`, `cv2`, `dn2`. Key colour
  red.
- **Build**: mid destroyers (4), carriers; late dreadnoughts (2), destroyers, flagship.
- **Cards/tokens**: Politics/agenda-heavy cards; Warfare.
- **Pitfalls**: destroyers against capital ships.

### The Vuil'raith Cabal (`cabal`)
- **Win path**: combat with capture; military objectives.
- **Kit**: `sar` start; dreadnought, carrier, cruiser, 3 fighters, 3 infantry, dock (a gravity
  rift). Devour, Amalgamation (captured units produce free), Riftmeld. `dt2` Dimensional Tear
  II (YY), `vtx` Vortex (R). Flagship (combat 5) captures everything destroyed in its system.
  Commander: units in 3 gravity rifts.
- **Mid**: dreadnoughts and fighters into fights it can win. **Late**: flagship-led fleet.
- **Tech**: `vtx`, `da`, `st`, `gls`, `dt2`, `asc`, `amd|sr`, `cv2`, `nm`, `ff2`. Key colour
  red.
- **Build**: mid dreadnoughts (3), fighters, carrier; late flagship, dreadnoughts (4).
- **Cards/tokens**: Warfare; a deep fleet pool.
- **Pitfalls**: fighting everyone at once.

### The Empyrean (`empyrean`)
- **Win path**: movement and diplomacy; frontier and distant objectives.
- **Kit**: `det` start; 2 carriers, destroyer, 2 fighters, 4 infantry. Voidborn (nebulae),
  Aetherpassage, Dark Whispers. `as` Aetherstream (BB), `vw` Voidwatch (G). Flagship repairs.
  Commander: neighbours with everyone.
- **Mid**: wide expansion, `as` for reach. **Late**: dreadnoughts for control objectives.
- **Tech**: `as`, `nm`, `vw`, `fl`, `dxa`, `cv2`, `st`, `dn2`. Key colour blue.
- **Build**: mid carriers (3), destroyers; late dreadnoughts (2), flagship.
- **Cards/tokens**: Diplomacy/Leadership.
- **Pitfalls**: a thin fleet.

### The Mahact Gene-Sorcerers (`mahact`)
- **Win path**: combat for command tokens (Edict); military and objective tempo.
- **Kit**: `pi`, `bs` start; dreadnought, carrier, cruiser, 2 fighters, 3 infantry. Edict,
  Imperia (borrowed commanders), Hubris. `cl2` Crimson Legionnaire II (GG), `gr` Genetic
  Recombination (G). Flagship +2 against players whose token it lacks. Commander: 2 other
  factions' tokens.
- **Mid**: win fights against each neighbour once (token each). **Late**: flagship and
  dreadnoughts.
- **Tech**: `gr`, `cl2`, `ps`, `sar`, `da`, `gls`, `amd|sr`, `dn2`, `ff2`, `asc`. Key colour
  green.
- **Build**: mid dreadnoughts (2), carriers, infantry; late dreadnoughts (3), flagship.
- **Cards/tokens**: Warfare.
- **Pitfalls**: fighting every player for every colour.

### The Naaz-Rokha Alliance (`naaz`)
- **Win path**: exploration and mechs; relics, attachments.
- **Kit**: `pa`, `aida` start; 2 carriers, destroyer, 2 fighters, 3 infantry, a mech. Distant
  Suns, Fabrication. Mech rolls 2 dice (a ship in space); `pfa` Pre-Fab Arcologies (GGG), `sc`
  Supercharge (R). Flagship gives mechs a die more. Commander: mechs in 3 systems.
- **Mid**: mechs on explored planets, carriers. **Late**: flagship plus mechs, dreadnoughts.
- **Tech**: `sc`, `dxa`, `sar`, `bs`, `pfa`, `da`, `amd|sr`, `cv2`, `st`, `dn2`. Key colours
  green and red.
- **Build**: mid mechs (4), carriers (2); late flagship, dreadnoughts (2), mechs.
- **Cards/tokens**: Construction, Warfare.
- **Pitfalls**: exploring low-value planets; overextended mechs.

### The Nomad (`nomad`)
- **Win path**: the flagship and three agents; flexible objectives.
- **Kit**: `sr` start; carrier, destroyer, 3 fighters, the flagship, 4 infantry. The Company,
  Future Sight. `m2` Memoria II (GBY: flagship combat 5x2, move 2, capacity 6), `tcs` Temporal
  Command Suite (Y). Commander (one scored secret) builds the flagship free.
- **Mid**: Memoria II, flagship forward with carriers. **Late**: flagship plus dreadnoughts.
- **Tech**: `st`, `nm`, `m2`, `tcs`, `dn2`, `ps`, `sar`, `cv2`, `da`. Key colours yellow and
  green.
- **Build**: mid carriers (2), dreadnoughts (1-2); late dreadnoughts (3), flagship rebuilt.
- **Cards/tokens**: Leadership; tactic tokens.
- **Pitfalls**: the flagship alone.

### The Titans of Ul (`titans`)
- **Win path**: fortified expansion; structures, planet objectives.
- **Kit**: `amd`, `sdn` start; dreadnought, 2 cruisers (own), 2 fighters, 3 infantry.
  Terragenesis, Awaken (sleepers to PDS), Coalescence. `se2` Saturn Engine II (GYR: cruiser
  capacity 2, sustain), `ht2` Hel-Titan II (YR). Commander: 5 structures.
- **Mid**: Saturn Engines (cruisers that carry and sustain), sleepers on held planets.
  **Late**: cruisers plus dreadnoughts.
- **Tech**: `ps`, `nm`, `se2`, `ht2`, `sar`, `st`, `dn2`, `da`. Key colours red and green.
- **Build**: mid cruisers (4), carrier; late cruisers, dreadnoughts (2), flagship.
- **Cards/tokens**: Construction.
- **Pitfalls**: sleepers on planets it will lose.

## Thunder's Edge (in the corpus, not in the seeded roster)

### Last Bastion (`bastion`)
- **Win path**: elite defence (Galvanize: an extra die).
- **Kit**: one starting tech (blue or yellow); dreadnought, carrier, cruiser, 2 fighters,
  3 infantry. `proxima` Proxima Targeting VI (R), `helios2` (YY, its dock upgrade).
- **Tech**: `st`, `gls`, `helios2`, `ps`, `proxima`, `sar`, `da`, `amd|sr`, `dn2`.
- **Build**: mid dreadnoughts (2), carriers; late dreadnoughts (3), flagship.
- **Pitfalls**: a small elite fleet cannot cover distant objectives.

### The Deepwrought Scholarate (`deepwrought`)
- **Win path**: coexistence, ocean cards, technology sharing.
- **Kit**: two starting techs; dreadnought, carrier, 4 fighters, 3 infantry.
  `hydrothermal` (G), `radical` Radical Advancement (G). Flagship capacity 6.
- **Tech**: `nm`, `hydrothermal`, `radical`, `dxa`, `ps`, `sar`, `ff2`, `amd|sr`, `cv2`, `st`,
  `dn2`.
- **Build**: mid carriers, fighters; late dreadnoughts (2), flagship.
- **Pitfalls**: coexisting so widely that control objectives are blocked.

### The Firmament / The Obsidian (`firmament`, `obsidian`)
- **Win path**: plots (copied secrets), then the flip to the Obsidian.
- **Kit**: one starting tech (green or yellow); carrier, 3 fighters, cruiser, destroyer,
  3 infantry. `parasite-firm` (GG), `planesplitter-firm` (YY); the Obsidian sides cannot be
  researched.
- **Tech**: `nm`, `dxa`, `parasite-firm`, `st`, `gls`, `ps`, `sar`, `amd|sr`, `cv2`, `dn2`,
  `da`.
- **Build**: mid carriers, cruisers; late dreadnoughts (2), flagship.
- **Pitfalls**: transforming without a follow-up plan.

### The Crimson Rebellion (`crimson`)
- **Win path**: breaches (epsilon wormholes only); mobility.
- **Kit**: one starting tech (red or blue); carrier, 2 destroyers (own), 3 fighters,
  4 infantry, PDS. `exile2` Exile II (RR), `subatomic` Subatomic Splicer (Y).
- **Tech**: `ps`, `sar`, `exile2`, `st`, `subatomic`, `da`, `amd|sr`, `cv2`, `dn2`.
- **Build**: mid destroyers (3), carriers; late dreadnoughts (2), flagship.
- **Pitfalls**: breaches with no scoring purpose.

### The Ral Nel Consortium (`ralnel`)
- **Win path**: reactive movement (Survival Instinct), transportable structures.
- **Kit**: one starting tech (red or green); dreadnought, carrier, 2 fighters, destroyer
  (own, move 3), 4 infantry, 2 PDS. `nanomachines` (R), `linkship2` Linkship II (RR, move 4).
- **Tech**: `ps`, `nanomachines`, `linkship2`, `sar`, `da`, `st`, `amd|sr`, `dn2`, `cv2`.
- **Build**: mid destroyers (3), carriers; late dreadnoughts (2), destroyers.
- **Pitfalls**: reactive moves that gain nothing.

## In the rule bot

`crates/ti4-server/src/session/smart_bot_faction_plans.rs` holds every profile above as data
(`PLANS`: technology order, unit emphasis per phase, fleet size, preferred strategy cards) and
four hooks the rule bot calls (`smart_bot.rs`):

- **Research** (any round, after Gravity Drive): the first plan entry offered; if the next entry
  is out of reach, the offered technology of a colour it still lacks (on the plan first, then
  the fewest prerequisites); then the same over `GENERAL`. Unit tests check that every plan names
  real, researchable technologies in an order whose prerequisites are met.
- **Production** (round 3 on): each option's worth is multiplied by the unit's phase weight,
  x1.5 while fewer than the plan wants are on the board, x1.15 for an upgraded unit.
- **Fleet size** (round 3 on): a dock is worth a token to build at until the plan's mid/late
  non-fighter ship count (7-11) instead of 6.
- **Strategy cards** (round 3 on): only where the bot's own rules leave the draft open (no
  Imperial to score, no card carrying trade goods), the plan's preferred card.

Rounds 1-2 are unchanged except that research choices other than Gravity Drive follow the plan
instead of the random pick.

### Measured (survey `survey_midgame`, 4 smart seats, roster factions by seed, 96 seeds)

`SURVEY_MID=1-96 SURVEY_MID_ROUNDS=12 cargo test --release -p ti4-server --test smart_bot_survey
survey_midgame -- --ignored --nocapture`; means over all seats at the end of each round, before
(consolidated base) vs after (this branch, engine fix included). Paired per-seed differences
with standard errors in brackets.

| | before | after |
|---|---|---|
| games reaching a winner | 96/96 | 96/96 |
| last completed round (mean) | 5.78 | 5.48 (-0.30 +- 0.14) |
| r3 VP / fleet strength / fleet cost / techs | 2.87 / 3.63 / 18.8 / 2.71 | 2.94 / 3.82 / 19.8 / 2.64 |
| r4 VP / fleet strength / fleet cost / techs | 4.17 / 4.97 / 23.0 / 3.43 | 4.23 / 5.17 / 24.0 / 3.39 |
| r5 VP / fleet strength / fleet cost / techs | 5.28 / 6.03 / 26.3 / 4.20 | 5.30 / 6.24 / 27.4 / 4.17 |

The fleets follow the plans (round 4, favoured unit per seat, before -> after): L1Z1X and
Letnev dreadnoughts 3.2 -> 3.4 and 3.5 -> 4.1, Muaat war suns 0.6 -> 1.2, Mentak cruisers
0.6 -> 2.3, Titans cruisers 1.1 -> 2.9, Argent destroyers 1.3 -> 2.4, Sol carriers 1.5 -> 2.7,
Hacan/Nomad/Sardakk/Winnu dreadnoughts +1.2 to +1.4. Victory points and technology counts do
not move beyond noise (VP +0.07 +- 0.06 at r3; techs -0.07 +- 0.05): what is researched changes,
how often is decided by who plays Technology. Per-faction VP and win counts (12-21 seats each)
are too noisy to read. Round-5 rows cover fewer games (games that already ended drop out).

---

## Sources

Community strategy material was thin on the open web (fan wikis returned 402/403 to
automated fetches, Medium was blocked); the profiles above are the consensus of the sources
below, cross-checked line by line against this repository's content data.

- Per-faction community guides (one page per faction, base + PoK + Keleres + Thunder's Edge):
  https://ti4-galactic-command.shogeveen11.chatgpt.site/faction-guides (e.g.
  `/faction-guides/l1z1x`, `/faction-guides/muaat`, `/faction-guides/naalu`,
  `/faction-guides/sol`, `/faction-guides/jolnar`, `/faction-guides/titans`). Treat with care:
  shallow, partly generic; used for faction identities and tech names, not numbers.
- Faction win-rate and survey discussion (Winnu, Nekro, Titans, Saar strong by win rate;
  starting "tech levels" of Jol-Nar 4, Xxcha 2, Sardakk 0):
  https://bellumgloriosum.substack.com/p/what-is-the-best-faction-and-why
- Beginner faction recommendations and playstyles (Xxcha politics, Jol-Nar tech, Hacan trade):
  https://boardgamegeek.com/thread/2865090/beginner-factions
- Faction-by-faction tech paths (base factions, with tech-skip planets; referenced, not
  fetchable here): https://medium.com/@arborecsoup/faction-by-faction-technology-paths-3cbeccf6c4d9
- Faction technologies and their prerequisites (referenced; checked against
  `technologies.json` instead): https://twilight-imperium.fandom.com/wiki/Faction_Technologies
- Repository data (authoritative for this engine): `crates/ti4-content/content/factions.json`,
  `technologies.json`, `units.json`, `abilities.json`, `leaders.json`;
  `crates/ti4-engine/src/production.rs` (what can be built), `technology.rs` (what can be
  researched).
