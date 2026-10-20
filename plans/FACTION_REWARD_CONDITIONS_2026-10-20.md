# Per-faction reward conditions for the separate faction models — draft 2026-10-20

Status: **design draft, nothing implemented.** Open questions for the operator are in §5; the
defaults below assume the recommended answers.

## 1. Sources and coverage

| Source | What it covers |
|---|---|
| SCPT faction guides, summarised in `docs/strategy-wiki/summaries/<faction>-strategy-summary.md` | One guide per faction (33 pages) |
| SCPT 437 2025 tier list (`crates/ti4-server/docs/podcast-transcripts/notes/437.md`) | One Thunder's Edge line per faction, including most breakthroughs |
| SCPT 462 (scoring and tempo) and 477 (win-slaying), notes in the same folder | TE games end in round 4 status or round 5 action; tempo matters more than point total; cheap point-blocking beats late win-slays |
| `docs/strategy-wiki/components.md` | Breakthrough card text, used where no guide says anything |

**Full-component guides (Thunder's Edge era):**
- Ultimate guides: Arborec 454, Letnev 463, Mahact 467, Titans 458, Keleres 472, Empyrean 476.
- TE beginner guides: Last Bastion 441, Deepwrought 445, Firmament/Obsidian 449, Crimson 430, Ral Nel 434.

**PoK-era "Super" guides (no breakthrough coverage):** Argent, Cabal, Ghost, Hacan, Jol-Nar,
L1Z1X, Mentak, Muaat, Naalu, Naaz-Rokha, Nekro, Nomad, Saar, Sardakk, Sol, Winnu, Xxcha, Yin and
Yssaril (19 factions). For these, the breakthrough row comes from the 437 tier list where it has
a line. Otherwise it comes from the card text, marked **[card]**, which is my reading of the card
and has no guide behind it.

## 2. Design rules (apply to every faction)

1. **Additive to the current stage-2 reward.** Every faction keeps the r5 terms (VP ×6, build,
   overpay, unused resources, round expansion and so on). The faction terms below are added on
   top of them.
2. **Reward states and milestones, not choices.** Strategy-card picks are **not** rewarded. The
   Sol round-1 pilot showed that forcing the guide's pick cost VP: Trade came in −0.66 to −0.87
   VP against Leadership. Card advice stays in the per-faction notes as context only.
3. **Potential-based.** Each term is `w · min(x, cap)`, and the reward is the change in that
   value, so losing a milestone (Mecatol, a dock, Styx) gives a negative step. Once-only
   unlocks (breakthrough, commander) cannot be lost.
4. **Deadlines.** "@R*n*" means the term pays only if the milestone is reached by the end of
   round *n*. Reaching it later pays nothing. This encodes the guides' timing advice.
5. **Budget (operator, 2026-10-20).** Weight tiers are given in VP: **H = 0.25, M = 0.12,
   L = 0.05**, multiplied by `vp_weight` in config. A faction's full potential is capped at
   **1.0 VP**; if the listed terms sum to more, they are scaled down proportionally.
6. **Five-round horizon (operator, 2026-10-20).** The faction models train at `--rounds 5`;
   r5 used 4. This follows 462/477, which say TE games end in round 4 status or round 5 action.
   Round-5 plans are kept: Winnu's hero catch-up, Naalu's round-5 status scoring, Muaat's Nova
   Seed and Jol-Nar's hero. Only Hacan's round-6 line is dropped. Changing the round count is a
   run change, so nothing launches without the operator's go.
7. **No SftT/VP-source terms**: no reward term counts Support for the Throne. Card text that mentions SftT, such as Winnu's Imperator, is unaffected.
8. **Things the reward never pays for:** hidden information it cannot see, deals that may not
   happen (selling promissory notes, alliance sales, Pillage policy), and "stay out of the lead"
   posture advice. Deals are left to the diplomacy head and VP.

9. **Shared Fracture/Styx discovery shaping (operator, 2026-10-20).** This sits outside the
   per-faction 1.0 VP cap. Bots never take Styx, even though the engine already pays for it:
   - A Song Like Marrow moves 1 VP with Styx control (`legendary.rs`).
   - The first capture of any uncontrolled Fracture planet draws a relic (Fracture rule 15,
     `invasion.rs`).
   - Styx has a neutral garrison of 3 infantry (`fracture.rs`).

   The problem is discovery, not payoff. Proposed shaping, paid as a potential and zeroed at the
   horizon:

   | Term | Base | Notes |
   |---|---|---|
   | first entry into the Fracture (`fracture_entry_bonus`) | 0.10 VP, once | already implemented |
   | each Fracture planet held (`frac`) | 0.15 VP | proxy for the relic plus position |
   | Styx held (`styx`) | 0.30 VP | on top of its real VP |
   | relics held (`relic≥k`) | 0.10 VP each, up to 2 | relics pay no reward today unless they give VP |

   - **No faction multiplier (operator, 2026-10-20):** the same weights apply to every faction.
   - **History:** `styx-bonus 16` (about 14.5 VP) ran in September and was judged unsafe. Review
     then recommended a small entry bonus that is annealed away. These bases are about 10× smaller,
     and they should anneal toward 0 once greedy evals show bots holding Styx.
   - **Check before the run:** count Styx captures and Fracture entries per faction in a
     greedy-eval table.
   - **Implemented 2026-10-20 on DEVniko (uncommitted):** flags `--fracture-entry-bonus`
     (existing), `--fracture-planet-shaping`, `--styx-shaping` and `--relic-shaping` (capped at 2
     relics). Their potentials are taken back at the horizon. At `vp-weight 6` the bases above
     are 0.6, 0.9, 1.8 and 0.6 in raw units.

## 3. Term catalogue

`P` = already a `Progress` field or a `Reward` term. `S` = readable from `GameState`/`Player`
today but needs a new `Progress` field. `N` = needs new bookkeeping.

| Id | Meaning | Source |
|---|---|---|
| `bt` | Breakthrough held (`Player.breakthrough.is_some()`) | S |
| `cmdr` / `hero` | Commander / hero no longer `Locked` (`Player.leaders`) | S |
| `mr` | Controls Mecatol Rex at round end | S |
| `styx`, `frac` | Holds Styx; planets in the Fracture | P (`styx_bonus`, `fracture_planet_bonus`) |
| `tech:X` | Owns technology X | S (`Player.technologies`) |
| `upg≥k` | Owns k unit-upgrade technologies | S |
| `dock≥k` / `fdock` | Space docks on board / at least one outside home | S |
| `struct≥k` | PDS + docks on board | S |
| `inf≥k`, `cap≥k` | Infantry; capacity ships | P |
| `ftr≥k`, `mech≥k`, `ws≥k` | Fighters / mechs on board / war suns | S |
| `home` | Home planets held | P (`home_loss_weight`) |
| `out≥k` | Non-home planets held | P (`planets_gained`, `non_home_planet_value`) |
| `relic≥k`, `frag≥k` | Relics / relic fragments | S |
| `ac≥k` | Action cards in hand | S |
| `cap_units` | Cabal captured units (`Player.captured_units`) | S |
| `assim≥k` | Nekro Valefar/assimilated techs (`Player.assimilated_technologies`) | S |
| `coexist≥k` | Deepwrought coexisting planets (`SystemState.coexisting`) | S |
| `plots≥k`, `flipped` | Firmament plots (`Player.plots`); Obsidian side | S |
| `foreign_tok≥k` | Mahact: other factions' tokens in its fleet pool | N (check faction module) |
| `sleeper≥k`, `heltitan≥k` | Titans tokens / Hel-Titans | N (check faction module) |
| `breach_act` | Crimson active breaches (`breach_tokens`) | S |
| `explore≥k` | Explorations resolved by this seat (`exploration_log`) | S |

## 4. Per-faction conditions

Format: **pay for** (tier, deadline) · **avoid** · **horizon change** · coverage.

### Arborec — 454 Ultimate (full)
- Pay: `bt` H@R1 (Psychospore "makes the plan work"); `mr` M from R2 (Custodians/Imperial route);
  `styx` M@R4; `inf≥8` L; `mech≥1` L@R2; `tech:x89` L@R3 (needed before the Mecatol/Styx ground
  fight).
- Avoid: none beyond the base terms.

### Argent Flight — 289 Super + 437
- Pay: `mr` M@R2 (Custodians through Strike Wing reach); `tech:swa2` M@R3; `tech:pds2` L;
  `cmdr` M@R2 (6 units with AFB/space cannon/bombardment); `fdock` or forward PDS (`struct≥3`) L.
- Breakthrough (Wing Transfer): **no term** (operator, 2026-10-20).

### Last Bastion — 441 TE
- Pay: `dock≥2` M@R2 (second Helios through Construction); `tech:helios_v2` H@R3 (the guide's
  top tech); `inf≥6` M@R1 (Liberate counts infantry); `out≥3` M@R1 ("plastic before tech");
  `mr` M from R3.
- `bt` (The Icon) L: it can stand in as a yellow prerequisite.
- Avoid: a round-1 tech-only line. Covered by `out` @R1, no extra term.

### Vuil'raith Cabal — 302 Super + 437
- Pay: `dock≥2` H@R1 (Construction is "near-automatic"); `dock≥3` M@R3; `tech:vortex` M@R2;
  `tech:cv2`+`tech:ft2` M@R3; `cap_units≥3` L; `cmdr` L (units in 3 gravity rifts).
- Breakthrough (Al'Raith Ix Ianovar): 437 says go to the Fracture in R2 but secure your own
  slice first. `bt` M@R2, `frac≥1` M@R3, `home` weight doubled.

### Crimson Rebellion — 430 TE (starts with its breakthrough)
- Pay: `cmdr` H (combat income engine); `mr` M@R3; `styx` M@R4; `tech:cv2` or `tech:ft2` M@R3;
  `cap≥3` M@R2 ("needs punch and capacity more than reach").
- Avoid: `breach_act` is not rewarded, because the guide says activate breaches only when ready.

### Deepwrought Scholarate — 445 TE
- Pay: `coexist≥3` H@R2 (ocean cards = economy); `bt` M@R1 (Visionaria Select, through Bio-Stims
  or Diplomacy); `tech:cv2`/`tech:dn2` M@R3; `tech:x89` L@R4 (437: "nearly always X-89").
- Horizon: weak finisher, so no extra term; VP already covers it.

### Empyrean — 476 Ultimate
- Pay: `fdock` M@R2 (high-resource forward dock); `tech:gd`, `tech:cv2`, `tech:ft2` M each @R3;
  `bt` L@R2 (Void Tether, "functions without it"); `explore≥1` per round, capped at 3, L;
  `styx` M@R4 (enter late, after others weaken each other).
- Avoid: scattered ships with no production. `cap≥2` M is held as a potential.

### Firmament → Obsidian — 449 TE (one model across the flip)
- Pay: `plots≥2` H@R2 (Extract + Siphon are the top plots); `hero` M@R3; `flipped` M@R3 (the guide
  defaults to R3; R2 is the high-roll line).
- R5: the `flipped` deadline stays R3; the guide says an R4 flip "risks missing economic benefits".
- Avoid: flipping in R1 just for six resources. No term pays it.

### Ghosts of Creuss — 326 Super + 437
- Pay: Malice held M@R1; `cap≥2` M@R1 (second carrier); `tech:sling` M@R1, `tech:cv2` M@R2;
  `cmdr` M@R3 (units in 3 alpha/beta wormhole systems).
- Breakthrough (Particle Synthesis): 437 says it "transforms" Ghosts. Malice becomes a new home,
  and Sling Relay plus a dock on Malice gives many dreadnoughts. `bt` H@R2.
- R5: `hero` L@R4 (3 scored objectives); used for a late control-objective swap.

### Emirates of Hacan — 360 Super + 437
- Pay: `tech:gd` M@R1, `tech:cv2` M@R2, `tech:qdn` H@R3; `dock≥2` at home M@R1 (437: new
  Construction); `ftr≥4` in home system M (screen); `home` weight doubled (three-planet home is
  slayable). The existing hoard/unused terms already cover "money is not a win condition".
- Breakthrough (Auto-Factories): 437 says it "pays itself". `bt` L.
- R5: `hero` L@R3 (the guide hopes for "around round three"). The round-6 and bait-and-rebound
  lines are dropped.

### Universities of Jol-Nar — 340 Super + 437
- Pay: `inf≥4` M@R1 (two starting infantry are the "Achilles heel"); `upg≥2` M@R3 and `upg≥3`
  M@R4 (the guide's R5 target, pulled forward); techs in 3 or more colours, 2 each, M@R4;
  `tech:pds2` L@R2.
- Breakthrough (Specialist Compounds): **no term** (operator, 2026-10-20).
- R5: `upg≥3` moves to @R5 (the guide's own target); `hero` L@R4, used after stage-II information.
- Note (memory): Jol-Nar lags early in stage 2; judge it on R4, not R1.

### Council Keleres — 472 Ultimate (one model, three variants)
- Pay: `bt` H@R2 (IIHQ Modernization is "close to automatic"); `fdock` M@R1; `tech:asn` (Agency
  Supply Network) H@R3; `tech:gd`, `tech:cv2`, `tech:x89` M@R4; `mr` M from R2 (437: root on Rex
  with infantry and X-89); `inf≥8` L.
- The variant changes the opening, not the reward terms.

### L1Z1X Mindnet — 347 Super + 437
- Pay: `tech:gd` M@R2, `tech:sd2` M@R3, `tech:cv2` M@R3; `styx` H@R4 (437: "very good at Styx",
  using the hero); `tech:x89` L@R4; captured structures (Assimilate) L.
- Breakthrough (Fealty Uplink): 437 calls it "worth getting, 5-resource unlock easy". `bt` M@R2.

### Barony of Letnev — 463 Ultimate
- Operator 2026-10-20: the breakthrough is "crazy good and game defining". `bt` gets **0.5 VP
  shaping @R1** (XL tier, exempt from the scale-down).
- Pay: `mr` H@R2 (R2 Custodians/Imperial);
  `styx`/`frac≥1` H@R3; `tech:dd2` M@R2; `tech:gd`, `tech:nes`, `tech:cv2` M@R4; `inf≥5` L (runs
  short on the ground).
- Scaling: listed weights exceed 1.0 VP, so they scale down (§2.5).

### Mahact Gene-Sorcerers — 467 Ultimate
- Pay: `foreign_tok≥2` (= `cmdr`) H@R3; `bt` M@R2; `relic≥1` M@R3 and `relic≥2` L@R4 (Vaults
  "~3 relics"); `tech:gd`, `tech:cv2`, `tech:fl`, `tech:x89` M@R4; `mr` M@R3 (Imperial after
  others); `styx` M@R4; own token kept in fleet pool L (Starlancer safety).

### Mentak Coalition — 344 Super + 437
- Pay: `tech:cv2` H@R2 (also flips the Corsair breakthrough); `cap≥2` M@R1 (missing second
  carrier).
- Breakthrough (The Table's Grace): `bt` M@R2 together with Cruiser II.
- Economy: the existing hoard term covers "convert Pillage income".

### Embers of Muaat — 355 Super + 437
- Pay: `ws≥2` H@R2 (also unlocks the commander); `tech:ws2` M@R3; `mr` M@R3 (Imperial
  recovery); `home` weight doubled (home "nearly undefended").
- Breakthrough (Stellar Genesis): `bt` **H@R2** (operator, 2026-10-20).
- R5: `hero` M@R4 (Nova Seed "usually around round four", to make a staging base before R5).

### Naalu Collective — 270 Super + 437
- Pay: `cap≥2` M@R1; `ftr≥6` M@R3; `tech:hcf2` (Hybrid Crystal Fighter II) H@R3; `cmdr` M@R2
  (ground forces in or adjacent to Rex).
- Breakthrough (Mindsieve): 437 says "breakthrough + hero gives tokens". `bt` L.
- R5: Telepathic round-5 scoring is paid by VP; no extra term (437: it matters less in TE).

### Naaz-Rokha Alliance — 287 Super + 437
- Pay: `mech≥3` on explorable planets M@R2; `explore≥4` M@R3; `relic≥1` M@R4; `fdock` M@R2 (weak
  home production); `tech:gd`→Pre-Fab→`tech:x89` colour route M@R4 (437); `frac≥1` L@R3.
- Breakthrough (Absolute Synergy): 437 calls the Voltron mech "a trap" but values the colour
  synergy. `bt` L; the Eidolon flip is never rewarded.

### Nekro Virus — 323 Super + 437
- Pay: `inf≥4` and `cap≥2` M@R1 ("plastic before tokens"); `assim≥1` M@R2; techs gained ≥3
  H@R4 (Technological Singularity); `ac≥3` L.
- Breakthrough (Valefar Assimilator Z): `bt` **L@R3** (operator, 2026-10-20).

### Nomad — 309 Super + 437
- Pay: `tech:gd` M@R2, `tech:cv2` M@R3; `out≥4` M@R2 ("convert wealth into pieces across the
  map"); `mr` M@R2 (R1–R2 Custodians is "the preferred tempo solution"); `mech≥2` forward L.
- Breakthrough (Thunder's Paradox): 437 says "good only with specific agents". `bt` L.

### Ral Nel Consortium — 434 TE
- Pay: `dock≥2` M@R2; `struct≥5` M@R3 (structure lane); `tech:det`/`tech:gd`/`tech:cv2` M each
  @R3; `mr` M@R3; `inf≥5` L (ground weakness).
- Breakthrough (Data Skimmer, covered by the guide): `bt` L.

### Clan of Saar — 336 Super + 437
- Pay: `dock≥2` H@R1 (Construction "default insurance"); `dock≥3` M@R3 (= `cmdr`);
  `tech:chaos_mapping` H@R2; `tech:cv2` M@R3; `out≥5` M@R2 (437: "best at racing R1–R2");
  `mr` M@R2.
- Breakthrough (Deorbit Barrage): **no term** (operator, 2026-10-20).

### Sardakk N'orr — 379 Super + 437
- Pay: `out≥5` = `cmdr` H@R2 (G'hom Sek'kus is "transformative"); `tech:det` M@R1; `tech:gd`,
  `tech:cv2` M@R3; `inf≥8` M.
- Breakthrough (N'orr Supremacy): 437 says "great but needs fighting". `bt` L.
- Never rewarded: combat wins. The guide says "combat is not a scoring plan".

### Federation of Sol — 328 Super + 437
- Pay: `tech:gd` M@R1; `tech:adv_carrier2` H@R2; `cap≥3` M@R2; `inf≥8` M; `cmdr` M@R3 (12
  resources of planets); `mr` M@R2; `styx` M@R4 (437: the hero reaches Styx).
- Breakthrough (Bellum Gloriosum): 437 calls it the "party bus" and says Sol is "platonically set
  up for TE". `bt` H@R2.
- Pilot: the GD-in-R1 override was null. `tech:gd`@R1 stays M, not H.

### Titans of Ul — 458 Ultimate
- Pay: `tech:saturn_engine2` H@R2; `heltitan≥2` + `struct≥5` (= `cmdr`, 5 structures) M@R3;
  `sleeper≥2` M@R2 (an unredeemed sleeper is "a defining skill"); `mr` M@R2 (Imperial); `styx`
  M@R3.
- `bt` (Slumberstate) L: early playtest text.

### Winnu — 352 Super + 437
- Pay: `home` weight doubled and `cap≥2`+`ftr≥4` M@R2 (survival first); `tech:det` M@R1;
  `tech:gd`/`tech:cv2` M@R3; `mr` M@R4 (take Rex at the end of R4); `cmdr` M@R4.
- Breakthrough (Imperator): `bt` **H@R2** (operator, 2026-10-20: "S tier" and "essential", since Mecatol is legendary).
- R5: `hero` H@R4 (Mathis Mathinus is "the central catch-up tool"); the guide's line is Rex on the
  last action of R4, then hero + Imperial early in R5.

### Xxcha Kingdom — 264 Super + 437
- Pay: `out≥4` M@R2 (Peace Accords); `cmdr` M@R3 (12 printed influence);
  `tech:ai_dev`→`ws≥1` M@R4 (the guide's red War Sun route); `tech:cruiser2` L@R3.
- Breakthrough (Archon's Gift): `bt` **H@R2** (operator, 2026-10-20).
- Hero: unlocks on 2 publics + 1 secret, which VP already rewards. No term.

### Yin Brotherhood — 257 Super + 437
- Pay: `tech:gd`, `tech:cv2` M@R3; `mr` M from R2 (437: "good at squatting on Mecatol"); `styx`
  M@R4 (hero "reliably first"); `cmdr` L.
- Breakthrough (Yin Ascendant): 437 says "commanders from the breakthrough decide the game".
  `bt` H@R2.

### Yssaril Tribes — 358 Super + 437
- Pay: `ac≥7` = `cmdr` M@R3; `tech:bio_stims` + `tech:mageon` M@R2; `tech:gd` M@R3 (437: "must
  get"); `mech≥2` L.
- Avoid: no `mr`/`styx` terms (437: don't go for Custodians or the Fracture).
- Breakthrough (Deepgloom Executable): **no term** (operator, 2026-10-20).

## 5. Gaps and operator decisions

| # | Question | Status |
|---|---|---|
| Q1 | Breakthroughs without a guide (Jol-Nar, Muaat, Nekro, Saar, Winnu, Xxcha, Yssaril, Argent's movement clause) | **Decided 2026-10-20**, one faction at a time (§4). 428 (Codex handover) can still revise these rows. |
| Q2 | Plans past round 4 | **Decided 2026-10-20:** extend to round 5 (§2.6) |
| Q3 | Shaping (zeroed at horizon) or kept objective | **Decided 2026-10-20:** shaping, except the doubled home-held charge, which is kept |
| Q4 | Faction budget | **Decided 2026-10-20:** 1.0 VP |

Assumed without asking: one Keleres model for all three variants; one Firmament model across the
Obsidian flip; deal-based advice is never rewarded (§2.8).

## 6. Revisions from the SCPT topical episodes (operator-approved 2026-10-20)

Source: `plans/evidence/SCPT-TOPICAL-REEXTRACTIONS-2026-10-20.md`, covering episodes 428, 455,
459 and 447. Where these changes differ from §4, they win. The code table
(`crates/ti4-training/src/faction_terms.rs`) is authoritative.

| Faction | Change |
|---|---|
| Cabal | breakthrough M@R2 → H@R2 |
| Sardakk | breakthrough L → H@R2 |
| Mahact | breakthrough M@R2 → H@R2 |
| Ghost | breakthrough deadline R2 → R1 |
| Keleres | breakthrough deadline R2 → R1 |
| Hacan | breakthrough L → M |
| Naalu | breakthrough L → M |
| Argent, Jol-Nar | PDS II term removed (F tier) |
| Naaz-Rokha | relic deadline R4 → R3 |

Xxcha stays H@R2 (operator).
