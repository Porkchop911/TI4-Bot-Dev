# SCPT topical re-extractions for the faction rewards (2026-10-20)

**Status:** model-extracted drafts, not verified against the audio. They are evidence for
`plans/FACTION_REWARD_CONDITIONS_2026-10-20.md`, not rules or training labels. The topical
notes from `.worktrees/rule-bot/plans/HANDOVER_2026-10-20_SCPT_TOPICS.md` had dropped the
per-faction and per-card verdicts the reward table needs, so the full transcripts were re-read:

- **#428:** Sonnet.
- **#455, #459 and #447:** Haiku.

Source transcripts are under `D:\Projects\ti4-engine-rs\.worktrees\rule-bot\out\strategy-wiki\episodes\`.

- **Speakers:** the ASR has no speaker labels, so "host 1 / host 2" pairs are not reliably tied
  to a named host.
- **Names:** ASR names were corrected against the content JSON. Timestamps are publisher-audio
  offsets.

## Reward-table changes made from this evidence (operator-approved 2026-10-20)

| Faction | Change | Source |
|---|---|---|
| Cabal | bt M@2 → H@2 | 428: 4 / 1, "star of the expansion" |
| Sardakk | bt L → H@2 | 428: 5 / 2, "incredible kit optimizer" |
| Mahact | bt M@2 → H@2 | 428: 10 / 1, easy unlock (ready a tech-spec planet) |
| Ghost | bt H@2 → H@1 | 428: "get it on round 1" |
| Keleres | bt H@2 → H@1 | 428: "race it turn 1", "return on investment excellent" |
| Hacan | bt L → M | 428: 17 / 9 |
| Naalu | bt L → M | 428: 15 / 11, "nasty in play" |
| Argent, Jol-Nar | PDS II term removed | 455/459: PDS II is F tier |
| Naaz-Rokha | relic≥1 deadline R4 → R3 | 447: relic value is tempo, early draws worth more |

Xxcha's breakthrough stays H@R2 by operator decision. 428 ranks it 21 / 16 and calls it "not
transformational".

## #428 Breakthroughs and Synergies RANKED: per-faction verdicts

The hosts ranked the 25 non-Thunder's-Edge factions 1–25 (1 = best). They skipped the new
TE factions (3:29:12): Bastion, Crimson (starts with it, 23:40), Deepwrought, Firmament,
Obsidian and Ral Nel.

| Faction | Breakthrough | Rank h1/h2 | Verdict | Timing | Pays off when | Time |
|---|---|---|---|---|---|---|
| Argent | Wing Transfer | 25/25 | "Too limited" | – | – | 45:34 |
| Yssaril | Deepgloom Executable | 24/23 | Adds little | – | Needs trust to sell Stall/Scheming | 49:33 |
| Jol-Nar | Specialist Compounds | 24/23 | "Nothing thing" | – | Double-tech turn on two tech-spec planets | 54:00 |
| Saar | Deorbit Barrage | 22/18 | Ability bad, adds heat; synergy liked | – | Destroyer II without a detour | 59:50 |
| Nekro | Valefar Assimilator Z | 22/19 | "Gimme" feel | – | Needs the right flagships on the table | 1:03:59 |
| Nomad | Thunder's Paradox | 21/20 | Hard to see the value | – | Integrated Economy; double-use agents | 1:09:00 |
| Xxcha | Archon's Gift | 21/16 | Nerfed old hero; "not transformational" | – | Lopsided planets; Instinct Training/Nullification | 1:14:00 |
| Titans | Slumberstate Computing | 20/16 | Interesting but hard | – | Rich, high-influence slice | 1:20:00 |
| Empyrean | Void Tether | 19/8 | Split: useless vs "fracture clogger" | – | Clogging Styx; map-dependent | 1:27:18 |
| Keleres | I.I.H.Q. Modernization | 18/8 | "Return on investment is excellent" | Race on turn 1 | Custodia Vigilia 2/2 | 1:36:04 |
| Naaz-Rokha | Absolute Synergy | 17/15 | Cool but homework; maybe worse | – | All 4 mechs in one system | 1:43:09 |
| Hacan | Auto-Factories | 17/9 | Hosts split | – | Three-ship builds, fleet-pool room | 1:49:00 |
| Naalu | Mindsieve | 15/11 | "Nasty in play", ~1.5 tokens/round | Pop hero early | Free secondaries via promissory notes | 1:59:21 |
| L1Z1X | Fealty Uplink | 14/14 | Underrated, "more than just good" | – | R2 Mecatol then Styx | 2:04:42 |
| Yin | Yin Ascendant | 13/12 | "Game changer" | – | Random alliance; Integrated Economy | 2:11:44 |
| Muaat | Stellar Genesis | 13/10 | Guaranteed value, solid | Worth getting every time | Avernus travels with the war sun | 2:15:15 |
| Mentak | The Table's Grace | 12/5 | One host's top five | Race if on a green/yellow skip | Cruiser II round 1 | 2:20:33 |
| Sol | Bellum Gloriosum | 11/7 | Best "least sexy" | Need not race round 1 | Free fill of capacity ships | 2:29:29 |
| Mahact | Vaults of the Heir | 10/1 | Widest split; very high ceiling | Easy: ready a tech-spec planet | ~3 relics; used R2–3 | 2:35:10 |
| Winnu | Imperator | 9/3 | Good floor, high ceiling | – | Mecatol/Styx access | 2:48:17 |
| Ghost | Particle Synthesis | 7/3 | "Transformative" | Get it round 1 | Mallice, three wormholes | 2:55:42 |
| Letnev | Gravleash Maneuvers | 6/6 | "Fantastic" | – | Fixes the tech tempo problem | 2:59:33 |
| Sardakk | N'orr Supremacy | 5/2 | "Incredible kit optimizer" | – | Win combat for a token or upgrade | 3:04:06 |
| Arborec | Psychospore | 4/2 | "Transforming" | – | Reliable expansion to Mecatol/Fracture | 3:10:20 |
| Cabal | Al'Raith Ix Ianovar | 4/1 | "Star of the expansion" | Guarantees the Fracture | Ingress into rifts, +1 move from the Fracture | 3:15:19 |

General points:

- **Expedition costs (14:41):** 5 resources, 5 influence, 2 action cards, 1 unscored secret,
  exhausting a tech-spec planet, or 3 trade goods.
- **Discarding a secret (16:33–19:50):** acceptable only with a plan to recover it, such as
  taking Imperial in round 2.
- **Game length (26:53):** the game effectively ends in round 4, so don't plan around stage-II
  objectives.

## #455 / #459 Every Generic Technology Ranked: combined tier list

| Tier | Technologies |
|---|---|
| S | Fleet Logistics (top, 459), Gravity Drive ("best tech"), Carrier II, Light/Wave Deflector (bottom), X-89 Bacterial Weapon (current version), Bio-Stims |
| A | Destroyer II, AI Development Algorithm, Sling Relay, Fighter II, War Sun, Predictive Intelligence (455; lowered in 459) |
| B | Integrated Economy, Magen Defense Grid (current), Transit Diodes, Anti-Mass Deflectors, Dark Energy Tap (low), Psychoarchaeology (455 low B; 459 top C), Cruiser II, Dreadnought II (one host C), Space Dock II (455 low B; 459 C) |
| C | Sarween Tools (split), Neural Motivator, Scanlink Drone Network, Duranium Armor, Assault Cannon (C or F) |
| F | PDS II, Infantry II, Hyper Metabolism, Graviton Laser System, Plasma Scoring, Self-Assembly Routines, X-89 base and first Ω version, older Magen versions, Dacxive Animators (worst) |

Timing and conditions:

- **Gravity Drive:** take it in round 1 to set up Custodians in round 2 (455, 1:08:59).
- **Dark Energy Tap:** only with 2–3 uncontested explores (455, 0:15:06).
- **Destroyer II:** once the fleet pool reaches about 4 (459, 0:31:24).
- **Dreadnought II:** only if movement or capacity is needed (459, 0:19:17).
- **Hyper Metabolism:** round 1 only (459, 0:48:49).
- **Fleet Logistics:** the finisher (459, 0:46:55).
- **X-89 (current version):** stretch the green path for it in rounds 1–3. Factions named: Yin,
  Naaz-Rokha, L1Z1X, Sol, Titans and Arborec (459, 1:23:01).
- **War Sun:** Hacan "about 80% of the time" (459, 1:00:15).
- **Space Dock II:** for production-capped factions such as Jol-Nar and Keleres (455, 1:04:08).

## #447 Every Relic RANKED: final list (1:59:10)

| Rank | Relic | Victory point? | Notes |
|---|---|---|---|
| 1 | Book of Latvinia | Conditional (all 4 tech types) | Early |
| 2 | The Obsidian | No | Early, scoring tempo |
| 3 | The Codex | No | Late, full discard pile |
| 4 | Dominus Orb | No | Mid, Styx setup |
| 5 | Shard of the Throne | +1 while held | Hard to hold; Yin likes it |
| 6 | The Crown of Emphidia | Conditional (Tomb) | Awkward timing |
| 7 | Maw of Worlds | No | Take Fleet Logistics or X-89; Titans, Muaat |
| 8 | Circlet of the Void | No | Best economy relic early |
| 9 | Neuraloop | No | – |
| 10 | The Silver Flame | Gamble (roll a 10) | Late |
| 11 | Dynamis Core | No | Economy; Sardakk |
| 12 | JR-XS455-O | No | Early structure tempo |
| 13 | Metali Void Shielding | No | Capital-ship fleets; Letnev, Cabal |
| 14 | Nano-Forge | No | Early, production planet |
| 15 | The Triad | No | Late, many fragments |
| 16 | The Quantumcore | No | Round 1 only |
| 17 | Metali Void Armaments | No | Fighter-heavy combat |
| 18 | Lightrail Ordnance | No | – |
| 19 | The Prophet's Tears | No | Early tech skips; Arborec |
| 20 | The Heart of Ixth | No | Jol-Nar |
| 21 | Scepter of Emelpar | No | Rarely |
| 22 | Stellar Converter | No | Stall or point-block |
| 23 | The Crown of Thalnos | No | Mahact |

General points:

- **Relic value:** tempo-based and decaying ("use it or lose it", 0:20:24). Early draws are
  worth more and late draws are often dead (1:25:02).
- **Not settled:** the value of fighting into the Fracture for a relic, and fragment-to-relic
  value.
- **Naaz-Rokha:** not mentioned.
