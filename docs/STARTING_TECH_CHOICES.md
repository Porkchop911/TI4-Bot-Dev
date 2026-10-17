# Starting technology choices

Branch `starting-tech-choices` (from `consolidated-2026-10-15b`, d284909c). Rules as Felix verified them:

- **Winnu**: start with any 1 technology that has no prerequisites (any basic level-0 technology).
- **Argent Flight** (PoK): choose 2 of Neural Motivator, Sarween Tools, Plasma Scoring; the one left behind stays in the deck.
- **Sardakk N'orr** really start with no technology.
- (Found while checking the sheets, not in the instruction) **Council Keleres**: "any 2 non-faction technologies owned by other players", chosen after every other seat; only one if only one unique starting technology exists among the others (wiki FAQ on the Keleres page). **Thunder's Edge**: Last Bastion, Crimson Rebellion, Ral Nel and Firmament each choose one level-0 technology of two colours; the Deepwrought Scholarate research twice during setup.

## 1. What the engine had

| Question | Before this branch |
|---|---|
| How starting technologies are granted | `ti4_engine::seating::deploy` reads only `startingTech` of the faction record (`Faction::starting_tech`) and inserts the cards into `Player::technologies`; the Nekro get Valefar X/Y on top. `startingTechOptions` / `startingTechAmount` of `factions.json` were never read. Winnu, Argent, the three Keleres, the Thunder's Edge pick factions started with nothing. |
| A pending setup decision | None. Setup builds the whole opening state outside the decision loop (`ti4_host::setup::create` -> `create_game_with_template_seated` -> `deploy`), so there was nowhere to ask. All questions come from `Game::legal_options` / `Game::step` as a `Choice` (player, prompt, options, `DecisionContext` with a stable `subtype`), answered by the `Table` (human, rule bot, random bot) and written to the decision log. |
| Ordering | The first question of every game was the strategy-card draft (`draft_strategy_card`, speaker first). `STRATEGY_PHASE_BEGAN` is emitted just before it. |
| Multi-pick N of M | Not a feature of `Choice`: one answer per question. Multi-pick things are repeated questions (Technology primary researches one, then asks the second) or batches in the client. |
| Prerequisite filtering | `technology::prerequisites(content, tech)` (counts per colour) and `prerequisites_met`/`can_research`/`researchable` exist. "No prerequisites" is `requirements` of `null`/`None`/empty, which gives exactly eight non-faction technologies: Antimass Deflectors, Dark Energy Tap, Neural Motivator, Psychoarchaeology, Plasma Scoring, AI Development Algorithm, Sarween Tools, Scanlink Drone Network (the `startingTechOptions` of the Winnu record are these eight). |
| Skips / colours | Technology specialties come from planets (`technology::specialties`), colours from `types`; neither depends on the starting technology, only `owned_colours` (which reads `Player::technologies`) does. |
| Players, bots, hosts | One path for all: server session worker, `ti4-host` (shared by the page and the server), `ti4-wasm` (offline) and `ti4-selfplay` all drive `Game::step` with a decider; a human seat gets the `Choice` as `pending_choice` in its `state_update`, the rule bot gets it in `ti4_rule_bot::choose`, the random bot picks any option. A new subtype needs no transport work. |

## 2. What was built

### Engine (`crates/ti4-engine/src/starting_tech.rs`)

- `GameState::starting_tech_picks: Vec<StartingTechPick>` (model crate): who still has to pick, how many (`of`, `remaining`) and from where (`Listed(techs)`, `OwnedByOthers`, `Researchable`). **Empty = no choice**, and then not serialized: the state of every game made before this serializes to the same bytes.
- `starting_tech::open(state, content, sources)` is called once on a freshly seated state. It queues one pick per choosing seat from the faction record (`startingTechOptions` resolved against the sources, so a base-only table offers the Winnu four technologies, not eight), the Keleres variants last, then `settle`s.
- `settle` gives out picks that have nothing to choose (as many options as picks left, or one researchable option) without a decision; deterministic from the state, so a replay settles the same way. It runs after `open`, after every answer and at the top of `Game::step`.
- `choice` is the question: subtype `starting_technology`, option kind `starting_technology`, option ids are technology aliases, option payload `colour`, `prerequisites`, `owners` (Keleres), choice details `pick`, `of`, `rule` (`listed`/`owned_by_others`/`researchable`), `faction`, `held`. Multi-pick is one question per card ("1 of 2", "2 of 2"), so the Argent second pick offers only the two that remain.
- `Game::legal_options` returns it before anything else while picks are pending; `apply_choice` routes the answer; `STRATEGY_PHASE_BEGAN` waits until no pick is pending, so the choice is made at game start, after setup and before round 1's strategy phase, and every later offer (research, unit upgrade prerequisites, round-1 moves) sees the technology.
- Order: seat order, Keleres last (their technologies are those the other seats ended up with). Picks are asked one at a time, as every engine question is.
- Deepwrought "research twice during setup": `Researchable` source, prerequisites met strictly, price waived, gained through `technology::gain` (no research triggers). The first pick is a level-0 technology; the second may use it.

### Version gate (replay safety)

A game asks for starting technologies only if its opening state was made with picks. Where that happens:

| Path | Gate |
|---|---|
| Server lobby start | `ti4_server::session::table::setup_of` sets `TableSetup::starting_tech`; `ti4_host::setup::create` calls `open`. The opening state (picks included) is stored in `init.json`; recovery replays from that state. A game stored earlier has no `starting_tech_picks` key, deserializes to empty, serializes back to the same bytes (the `action_turn` lesson). |
| Page (web2), table | `TableSetup::starting_tech` (not written while false) is set by `Start::settle_bot_version` when the game's bot version is 3 or later; a save records it. |
| Page, single player (`Setup`) | `Setup::starting_tech: Option<bool>`; left out = on for bot version >= 3, so a save made by bot version 1 or 2 resumes without the choice. Not written unless on. |
| Local fixed games, scenarios, start presets (test tables) | Unchanged: no choice. |
| Self-play | `GameSpec::starting_tech` / `--starting-tech`; left out = off, so every summary and the v2 baseline replay as before. |

`bot_version_check` has new cases (Winnu human at bot version 2 and 3, saved, resumed). TableSetup/Setup wire types regenerated (`cargo wire-web`): two optional fields.

### Rule bot (bot version 3)

`BOT_VERSION` is 3. `tuning::for_version(2)` is the current bot with `starting_tech_rule = false`: the question is then left to the seeded random pick, exactly the choice a version-2 bot made when asked anything it has no rule for. Version 3 picks `faction_plans::starting_tech_pick`: the first offered technology in the faction's plan (`Plan::techs`), then in the general order; ties by id. Winnu take Sarween Tools, Argent Plasma Scoring then Sarween Tools, the Keleres Sarween Tools first.

### UI (`web/`)

No new screen: the question is a technology pick like Divert Funding / Plagiarize, so it uses the generic decision dialog with the technology-pick panel (`presentation/technologyPick.ts`, `TechnologyPickParts.tsx`): title "Starting technology" / "Starting technology · 2 of 2", one paragraph that states the rule for the faction (Winnu: one, Argent: the one left stays in the deck, Keleres: held by other players), "You already start with ..." for the second pick, each row with colour, "needs" symbols, printed text and, for the Keleres, the seats that hold the card. Public log line "<seat> starts with <technology>". Screenshots: `web/e2e/screenshots/EP-starting-technology/` (`cd web && npm run screenshots -- EP`). web2 and the offline shell render any unknown subtype through their generic choice list; no web2 change.

## 3. Every faction, with sources

Sources: **E** = `crates/ti4-content/content/factions.json` (mirror of AsyncTI4 `data/factions/{base,pok,keleres,te_factions}.json`, fetched 2026-10-16 from github.com/AsyncTI4/TI4_map_generator_bot master; identical `startingTech` / `startingTechOptions` / `startingTechAmount` for all 34 records), **W** = the Starting Technology page and the faction pages of the Twilight Imperium wiki (twilight-imperium.fandom.com, MediaWiki API, 2026-10-16: `Starting_Technology`, `The_Council_Keleres`, `Winnu`, `Argent_Flight`, `Last_Bastion`, `The_Firmament_/_The_Obsidian`, `Ral_Nel_Consortium`, `The_Crimson_Rebellion`, `The_Deepwrought_Scholarate`). The printed sheets themselves and the LRR PDF were not reachable (no image access to the sheets); the wiki transcribes them. Where E and W agree the row says "matches".

| Faction | Starts with | E vs W | Change |
|---|---|---|---|
| Arborec | Magen Defense Grid | matches | none |
| Barony of Letnev | Antimass Deflectors, Plasma Scoring | matches | none |
| Clan of Saar | Antimass Deflectors | matches | none |
| Embers of Muaat | Plasma Scoring | matches | none |
| Emirates of Hacan | Antimass Deflectors, Sarween Tools | matches | none |
| Federation of Sol | Neural Motivator, Antimass Deflectors | matches | none |
| Ghosts of Creuss | Gravity Drive | matches | none |
| L1Z1X Mindnet | Neural Motivator, Plasma Scoring | matches | none |
| Mentak Coalition | Sarween Tools, Plasma Scoring | matches | none |
| Naalu Collective | Neural Motivator, Sarween Tools | matches | none |
| Nekro Virus | Dacxive Animators; Valefar Assimilator X and Y (engine adds them) | matches | none |
| **Sardakk N'orr** | **nothing** | matches (`startingTechAmount` 0) | none; correct as is |
| Universities of Jol-Nar | Neural Motivator, Antimass Deflectors, Sarween Tools, Plasma Scoring | matches | none |
| **Winnu** | **any 1 of the 8 level-0 non-faction technologies** | W lists the same eight | **choice added** (was nothing) |
| Xxcha Kingdom | Graviton Laser System | matches | none |
| Yin Brotherhood | Sarween Tools | matches | none |
| Yssaril Tribes | Neural Motivator | matches | none |
| **Argent Flight** | **2 of Neural Motivator, Sarween Tools, Plasma Scoring** | matches | **choice added** (was nothing) |
| Vuil'raith Cabal | Self-Assembly Routines | matches | none |
| Empyrean | Dark Energy Tap | matches | none |
| Mahact Gene-Sorcerers | Bio-Stims, Predictive Intelligence | matches | none |
| Nomad | Sling Relay | matches | none |
| Naaz-Rokha Alliance | Psychoarchaeology, AI Development Algorithm | matches | none |
| Titans of Ul | Antimass Deflectors, Scanlink Drone Network | matches | none |
| **Council Keleres** (Mentak / Xxcha / Argent) | **2 non-faction technologies that other players start with, chosen last** | E had `[]` | **choice added** (record unchanged; rule in the engine because the record has no field for it) |
| Last Bastion (TE) | 1 blue or yellow technology without prerequisites: AMD, DET, ST, SDN | matches | choice added (was nothing; not seatable in this build) |
| Crimson Rebellion (TE) | 1 red or blue: DET, AMD, PS, AIDA | matches | choice added |
| Ral Nel Consortium (TE) | 1 red or green: NM, PA, PS, AIDA | matches | choice added |
| Firmament (TE) | 1 green or yellow: NM, PA, ST, SDN | matches | choice added |
| Obsidian (TE) | none (cannot be chosen at setup; flip side of the Firmament) | matches (`N/A`) | none |
| Deepwrought Scholarate (TE) | research 2 technologies during setup, price waived | E: amount 2, no options | **choice added** (`Researchable` source) |
| neutral | n/a | | none |

Fixed starting technologies were therefore already right for all 24 other factions; the data gaps were the five choosing kinds above. No `factions.json` value was changed (it is a verbatim mirror with a checksum), the rules for the Keleres and the Deepwrought are in the engine.

## 4. Replay safety evidence

- `deploy/public/deploy.sh --preflight-only --ref f1e43db0` (read-only copy of `/var/lib/ti4-public`, own target dir): **22 saved games, recovered=21, failed=1, the failed id is `game_a36f40db3b5cc50fabaabcd66a6bad01`** (the known one, "canonical hashes differ"; the script reports it as unexpected only because `--allow-failed` was not given). No other game changed. Nothing was installed.
- Rule bot on this engine: `selfplay.sh bench-v2` (bot version 2, golden seeds, no choice) against `baseline-v2.json` (the baseline recorded before this branch): **200 identical, 0 different**.
- `bot_version_check` (offline saves): new game saved with bot version 3 resumes; Winnu human at bot version 2 is made and resumed without the choice (`starting_tech` not recorded); at version 3 with it; version-1 save without the field resumes as version 1. At stop points 59 and 80 all 15 lines OK. At some stop points (e.g. 61, 62) the example prints a null resume check for one of its cases, also with a non-Winnu faction; this is the example's stop position, not a replay failure, and it is not new.
- Offline Playwright (`npm run test:e2e:offline`, production wasm build): 11 of 11 passed.
- Server test `starting_tech`: the Winnu seat is asked first, the others wait, the answers are log decisions and `replay_session` reproduces the hashes; a state made without the choice has no `starting_tech_picks` key.

## 5. Rule bot A/B

Self-play, `--factions winnu,argent` (seats a and b, the rest drawn), 4 and 6 players, seeds 1..450 each, **900 paired games** (same seeds, all victories), 4 workers. O = old (bot version 2, no choice: they start with nothing), R = choice offered, bot version 2 picks at random, N = choice offered, version 3 picks by plan. Paired difference per seed, mean with 95% interval:

| | Winnu VP | Winnu VP/round | Winnu win rate | Argent VP | Argent VP/round | Argent win rate |
|---|---|---|---|---|---|---|
| O (nothing) | 5.79 | 0.868 | 10.2% | 7.67 | 1.179 | 31.8% |
| R (random pick) | 5.96 | 0.907 | 13.7% | 7.70 | 1.205 | 33.9% |
| N (plan pick) | 5.98 | 0.901 | 11.3% | 7.87 | 1.222 | 35.4% |
| N - O | +0.19 +-0.19 | +0.033 +-0.027 | +1.1 +-2.7 pts | +0.19 +-0.18 | +0.042 +-0.032 | +3.7 +-4.0 pts |
| N - R | +0.02 +-0.18 | -0.006 +-0.026 | -2.3 +-2.8 pts | +0.17 +-0.15 | +0.016 +-0.027 | +1.6 +-3.6 pts |

Both factions improve in the point estimate when they get a technology (Winnu +0.19 VP, Argent +0.19 VP, about +4% VP per round), but no difference is larger than its 95% interval at 900 games: the effect of one or two level-0 technologies is small against the noise of a seven-round game. The plan pick beats a random pick only for Argent (Plasma Scoring plus Sarween Tools; +0.17 VP, interval just includes 0). The bot picks Sarween Tools for the Winnu every time and Plasma Scoring plus Sarween Tools for the Argent; other factions' mean VP did not move (6.71 O, 6.62 N). Rounds 6.68 O, 6.63 N.

Golden benchmark (200 games, the 4p/6p set): old `baseline-v2.json` against the re-recorded `baseline.json` (bot version 3, `"starting_tech": true` in `golden.json`): 136 games hash-identical; the 64 games with a Winnu or Argent seat all differ and no other game does. Mean rounds 6.885 to 6.910, mean decisions 2262.9 to 2271.6, tied-at-top 6 to 6, 200 of 200 victories in both. Winnu (26 seat-games) mean VP 5.46 to 6.00, wins 2 to 4; Argent (44) 6.86 to 7.11, wins 8 to 11. `baseline-v2.json` is kept (`selfplay.sh bench-v2`, `golden-v2.json`); `baseline.json` is the new one, to be committed with this explanation.

Not simulated: the Keleres (the lobby does not draw them; the Tribuni rule needs the base faction unseated) and the Thunder's Edge factions (cannot be seated).

## 6. Open decisions

- Start presets (the test tables of the server and the smoke sweep: `combat`, `world`, ...) keep the fixed start and do not ask. Say if the nightly sweep should exercise the question; then `ti4_host::setup::create` should call `starting_tech::open` for presets too (their cases are applied after it).
- Picks are asked one seat at a time in seat order, Keleres last. Alternative: ask all choosing seats at once (needs a multi-seat pending state, which the engine does not have).
- Keleres: the rule comes from the Codex III wiki transcription ("any 2 non-faction technologies owned by other players", after the others, fewer if fewer exist). The printed sheet and the Thunder's Edge rules reference were not reachable; worth a look at the physical card, especially whether Thunder's Edge changed it.
- Deepwrought "research twice during setup" is modelled as two free picks with prerequisites (no Carrina window, no research triggers, no Propagation). Not exercised by a seatable game here.
- The bot always takes Sarween Tools with the Winnu. The effect is within noise; a faction-specific table (for example Antimass Deflectors for a fleet plan) could be tuned later with more games.
- `web2` has no dedicated screen for the question (its generic choice list shows it); the page of `web/` has the technology-pick panel (screenshots EP).
- The shared screenshot helper `openMockedGame` now routes `/api/catalog` to `fixtures/catalog/catalog.json`; without it every capture on this branch stops at "The cards could not be loaded" (the catalog fetch came with the PP merge).
- Existing e2e specs that assume the draft is the first question (`empty_movement`, `multiplayer_invariants`, `gameplay_workflows`, `tactical_movement_combat`) use seeds whose drawn factions (nomad/muaat/letnev, nekro/l1z1x/naaz, yssaril/sardakk/nomad) do not choose, so they are unaffected; a new seed that draws a Winnu, Argent or Keleres seat would need to answer the question first.
