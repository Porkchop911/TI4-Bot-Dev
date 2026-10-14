# Smoke playthroughs: scripted setups + nightly random sweep — plan

Goal: every planet-pick decision moved onto the map (planet_selection workflow, Elect Planet
votes, `vote_exhaust_planet` map toggling) is reached at least once in a real UI playthrough
driven by `e2e/smokePlaythrough.ts`, and is answered **by clicking a planet on the map**, with the
action/source text visible in the bar at that moment.

Two parts:

- **Part 1 — set up, then play randomly** (sections 1–5): seeded dev scenarios put the game a
  few clicks before each planet-selection trigger, the random clicker is steered to the
  trigger, then keeps playing randomly. *To be implemented.*
- **Part 2 — nightly random sweep** (section 6): scheduled every night 23:00–08:00 Berlin time.
  Haiku proctors launch random-seed games one after another, watch each to round 10 and report
  bugs and interesting events. Opus writes a morning summary with suggested fixes; nothing is
  implemented automatically. *Implemented in `tools/nightly-smoke/`.*

# Part 1 — set up, then play randomly

## Why not just the existing random smoke

`randomUiPlaythrough` starts a fresh game and clicks randomly (or "steer"s toward tactical play).
Most of these decisions need a specific action card, relic, tech, leader, faction or agenda *and*
a board position that makes the card legal (rival planets for Plague/Uprising, a hazardous planet for
Unstable Planet, …). In 200 decisions from a random start, almost none of them will happen.
So the plan is: **start each run from a seeded dev scenario that is a few clicks away from the
trigger, steer toward the trigger, then continue random play** so the surrounding flow (payment,
reaction windows, follow-up decisions) is smoke-tested too.

## 1. Server: parameterised "planet pick" dev scenarios

Location: `crates/ti4-server/src/dev/scenarios.rs` (existing `available_scenarios`,
`launch_scenario`, `setup_base_3p_game`, and the action-card injection pattern used by
`ongoing_combat_four_views` / `ongoing_invasion_parley`).

- Add one scenario id per trigger, prefix `planet_pick_` (e.g. `planet_pick_mining_initiative`),
  all built by a single `build_planet_pick_scenario(seed, trigger)` that:
  1. calls `setup_base_3p_game` (Sol / Hacan / Letnev), puts the game in the right phase
     (action phase round 2 by default; agenda phase for the agenda ones, which requires a
     Mecatol-controlled custodians state),
  2. applies a per-trigger **setup recipe** (table below): inject the card into the human's hand /
     relic / tech / leader / law, give rival control of planets where needed, place units,
  3. makes all seats human (like the four-views scenarios) so the smoke harness drives every seat
     and reactions from other seats are UI-driven too,
  4. sets `SessionConfig` so the human is the active player.
- Recipes are data (a `match trigger` returning a closure over `&mut GameState`), so adding a card
  later is a one-line entry. Keep them out of the engine; only state setup, no new engine API.
- Rust tests: for each scenario, launch it, drive the engine with a scripted answer list
  (`trigger path` below), and assert the expected `context.subtype` is pending, with ≥2
  candidate planets (otherwise the engine auto-resolves and the UI never shows it) and every
  option has `payload.planet` + `payload.system`. This guards the scenarios against engine drift
  independently of Playwright.

## 2. Harness: scenario mode for `randomUiPlaythrough`

File: `web/e2e/smokePlaythrough.ts` (+ `smoke_playthrough.spec.ts`).

New `PlaythroughOptions`:
- `scenarioId?: string` — launch via `POST /api/dev/scenarios/launch` instead of
  `createStartedGame`; open one page per seat using the returned seat tokens.
- `targetSubtypes?: string[]` — subtypes this run must reach.
- `steerTowards?: RegExp[]` — candidate descriptions to boost heavily (weight 200) until every
  target subtype has been seen, e.g. `/Mining Initiative/` on the "play action card" button.
- `afterTargetDecisions?: number` — keep playing randomly for N more decisions after the last
  target was reached (default 30), then stop.

Behaviour changes:
- **Map-first policy for planet decisions.** When the pending subtype is a planet-selection
  workflow (detect via `data-testid="planet-selection-bar"` being mounted), the candidate pool is
  restricted to map planet targets (`[data-target-candidate="true"]` on planet elements) for the
  first click, then the bar buttons (confirm / per-unit buttons / decline). The fallback chip list
  in the bar is excluded in this mode so the map path is really exercised. A second, rarer mode
  (10% via rng) uses the chips, so both paths are covered across seeds.
- **Assertions at the target decision** (recorded, failing the test at the end):
  - the bar shows the source name and action text (match against a per-trigger expected string,
    e.g. `Mining Initiative`, `mine`) *before* any click;
  - after the planet click, the confirm row names the clicked planet;
  - the submitted option's `payload.planet` equals the clicked planet (read from the
    server snapshot / event log after the version bump);
  - no hex outside the candidate set was clickable as a target.
- Report gains `targetsReached: Record<subtype, {decision, viaMap: boolean}>`; the test fails if a
  target subtype was not reached within `maxDecisions`, with the click log attached (already
  done for failures).

New spec `e2e/planet_selection_smoke.spec.ts`: a `test.describe` that loops over a table
`{ scenarioId, targetSubtypes, steerTowards, expectText }` and runs one Playwright test per row
(parallelisable, each ~1–3 min). Gate it like the existing smoke (`TI4_SMOKE=1`) and add
`npm run test:e2e:smoke:planets`. Seeds come from env with a fixed default so failures reproduce;
a nightly job can sweep `TI4_SMOKE_CLICK_SEED`.

## 3. Per-decision recipes

"Trigger path" = what the steer regexes must make the random clicker do after launch.
Card aliases must be checked against `ti4-content` when implementing; names below are the
display names.

| Decision (subtype) | Setup recipe | Trigger path |
|---|---|---|
| Mining Initiative (`mining_initiative_pick_planet`) | card in hand; Sol controls ≥2 planets | action phase → play action card → Mining Initiative → **pick planet** |
| Plague (`plague_pick_planet`) | card in hand; Letnev controls ≥2 planets with infantry | play card → **pick planet** |
| Uprising (`uprising_pick_planet`) | card in hand; rival controls ≥2 non-home planets | play card → **pick planet** |
| Unstable Planet (`unstable_planet_pick_planet`) | card in hand; ≥2 hazardous planets with rival structures | play card → **pick planet** |
| Cripple Defenses (`cripple_defenses_pick_planet`) | card in hand; rival PDS on ≥2 planets | play card → **pick planet** |
| Reactor Meltdown (`reactor_meltdown_pick_planet`) | card in hand; rival space docks on ≥2 non-home planets | play card → **pick planet** |
| Archaeological Expedition (`..._pick_planet`) | card in hand; ≥2 controlled planets with different traits | play card → **pick planet** |
| Frontline Deployment (`..._pick_planet`) | card in hand; ≥2 planets with own units | play card → **pick planet** |
| Exchange Program (`exchange_program_pick_planet`) | card in hand; ≥2 offerable planets; other seat human | play card → pick player → **pick planet** → other seat answers (`exchange_program_answer`) |
| Mercenary Contract (`..._pick_planet`) | card in hand (Thunder's Edge source enabled) | play card → **pick planet** |
| Decoy Operation (`decoy_operation_pick_planet`) | card in defender's hand; attacker activates a system adjacent to defender units with ≥2 planets | attacker activates (steer to that hex) → defender reaction → pick units → **pick planet** |
| Reparations (`reparations_exhaust` / `reparations_ready`) | card in Letnev's hand; Sol set up to take a Letnev planet via invasion in the active system | Sol invades & gains control → Letnev reaction → **exhaust**; Letnev **ready** step |
| Crash Landing (`crashlanding_choose_planet`) | card in hand; ships in a system with ≥2 planets, no own ground forces | play card → pick ground force → **pick planet** |
| Scanlink (`scanlink_explore`) | Sol has tech `sdn`; units on ≥2 unexplored planets in a reachable system | activate that system → **pick planet** |
| Xxcha agent (`leader_xxchaagent_ready_planet`) | Xxcha seat with unlocked agent; ≥2 exhausted planets | trigger agent at its window → **pick planet** |
| Stellar Converter (`stellar_converter_choose_target`) | relic in Sol's play area; ≥2 eligible rival planets adjacent | component action → **pick planet** |
| Crown of Emphidia (`crown_of_emphidia_choose_planet`) | relic; ≥2 controlled planets | trigger window (end of tactical action) → **pick planet** |
| Specialist Compounds (`specialist_compounds_choose_planet`) | faction with the ability; ≥2 eligible planets | Technology strategic action → **pick planet** |
| Ready a planet (`ready_planet`) | the source that raises it (check strategy_cards.rs ~750); ≥2 exhausted planets | trigger → **pick planet** |
| Psychoarchaeology (`psychoarchaeology_exhaust_specialty`) | tech `pa`; ≥2 ready tech-specialty planets | research step → **pick planet** |
| Orbital Drop (`orbital_drop_choose_planet`) | Sol, strategy token ≥1; ≥2 controlled planets | component action Orbital Drop → **pick planet** (mech deploy step stays as-is) |
| Peace Accords (`peace_accords_annex`) | Xxcha seat; ≥2 annexable planets adjacent | resolve Diplomacy primary/secondary → **pick planet** or decline |
| Fracture ingress (`fracture_choose_ingress_system`) | fracture enabled in config | trigger fracture → **pick planet** |
| Defense Act (`defense_act_choose_pds`) | agenda resolved with ≥2 own PDS | agenda phase → vote → outcome → **pick planet** |
| Place structure (`place_structure`) | Construction card held; ≥2 controlled planets | Construction primary → **pick planet** → pick structure in bar |
| Legendary place (`legendary_place`) | legendary planet with the place ability controlled & ready | component action → **pick planet** |
| Bio-Stims (`bio_stims_ready`) | tech `bs`; exhausted planet + exhausted tech | end of turn → **pick planet** *or* tech chip (both modes across seeds) |
| The Acropolis (`legendary_acropolis`) | legendary controlled | component action → **pick planet** or non-planet chip |
| Elect Planet vote (`cast_vote`, planet outcomes) | agenda deck top = an "Elect Planet" agenda (e.g. Core Mining); custodians removed | agenda phase → **click planet on map as outcome** → `vote_exhaust_planet` |
| Vote exhaust (`vote_exhaust_planet`) | any agenda; voter with ≥2 ready planets | after outcome → **toggle planets on map** → confirm |
| Payment (regression only) | existing `production_payment_batch` scenario | unchanged; assert map toggling still works |

Rows whose trigger depends on content not modelled in the current source set (Fracture,
Thunder's Edge cards) get the source enabled in the scenario config; if a card turns out
unreachable in the engine, the Rust scenario test (section 1) fails first and the row is marked
`skip` with a reason instead of silently passing.

## 4. Relation to the nightly sweep

Part 1 scenarios are deterministic coverage for decisions random play rarely reaches. Once they
exist, the nightly proctor loop (Part 2) should pick a Part 1 scenario for about 1 run in 4
(`run_game.sh` gets a `SCENARIO` variable that is passed to the harness as `TI4_SMOKE_SCENARIO`),
so the scripted triggers are also exercised under random seeds every night.

## 5. Order of work

1. Harness scenario mode + map-first policy + assertions, validated against one existing scenario
   (`production_payment_batch` for the map-toggle path).
2. Scenario builder + recipes for the generic `pick()` action cards (cheapest, 11 decisions).
3. Bespoke action cards (Reparations, Crash Landing, Decoy Operation, Exchange Program).
4. Techs / relics / leaders / faction abilities / strategy cards.
5. Agenda phase rows (needs the agenda-phase scenario setup).
6. Hook the scenarios into the nightly loop (section 4).

## Open questions

- Should the planet smoke suite run in CI per PR (≈30 scenarios × 1–3 min) or nightly only?
  Suggest: nightly, plus one representative row per group on PR.
- Do we want the harness to fail on `viaMap: false` for a target, or only record it? Suggest fail,
  because exercising the map path is the point of the suite.

# Part 2 — nightly random sweep (implemented)

## 6. What runs when

| Time (Europe/Berlin) | What | Who |
|---|---|---|
| every 5 min (cron) | `tools/nightly-smoke/nightly.sh tick` decides what to do | cron |
| 23:00 | sweep starts: build the server once, then proctored runs back to back | shell loop |
| 23:00 – 07:40 | each run: random game seed, click seed, 3–4 players, `steer` or `random` policy, plays until **round 10**, game over, the first failure, or a stall | Haiku proctor (`claude-haiku-4-5`) |
| ~07:40 | no new run if fewer than 20 minutes are left; the last game is stopped at 07:55 | shell loop |
| 08:00 | morning summary of the night | Opus (`claude-opus-5-5`) |

The window and every other knob live in `tools/nightly-smoke/config.sh` and can be overridden by
`NIGHTLY_*` environment variables. Cron runs in UTC, so `tick` computes the window in Berlin
time itself; summer and winter time are handled without editing the crontab. Missed ticks
(machine asleep, reboot) are caught up: the sweep starts at the first tick inside the window, and the
summary at the first tick within 4 hours after it.

Crontab entry (installed for root):

```
*/5 * * * * /root/TI4-Bot-Dev/tools/nightly-smoke/nightly.sh tick >> /root/TI4-Bot-Dev/nightly-reports/cron.log 2>&1
```

## 7. One run, step by step

1. `nightly.sh loop` creates `nightly-reports/<night>/runs/<NN-HHMM>/` and starts a headless
   Haiku session (`claude -p --permission-mode dontAsk`) with `prompts/proctor.md`.
   - **Sandbox:** the proctor gets Bash, Read, Grep and Glob. Bash may only run the three
     nightly scripts. Edit, Write and Agent are disabled.
2. The proctor runs `run_game.sh start <run-dir>`.
   - The script picks the random seeds itself and records them, plus a repro command, in
     `meta.json`.
   - It launches `e2e/smoke_playthrough.spec.ts` in its own process group, with a private
     backend port and `TI4_SMOKE_TRACE_DIR`.
3. The harness (`web/e2e/smokePlaythrough.ts`) plays every seat through the browser and writes:
   - `trace/trace.jsonl`: one line per decision (round, phase, seat, faction, subtype, source,
     prompt, options with labels);
   - at the end, `report.json`, `final-snapshot.json`, `browser-errors.json`, and
     `failure.txt` if it failed.
   - Playwright's output, including backend stderr where **engine panics** appear, goes to
     `run.log`.
   - The backend's own game record (`decisions.jsonl`, `init.json`, …) is copied to
     `server-data/`.
4. The proctor calls `watch.sh <run-dir> 540` repeatedly (each call blocks for up to 9 minutes).
   - Each call prints progress plus any new suspicious log lines: panics, `ERROR`, rejections,
     `pageerror`, console errors, "did not advance", "no actionable control".
   - The **stall detector** kills a run whose trace has not grown for 15 minutes and marks it
     STALLED.
5. When the run ends, the proctor runs `digest.py`. The digest is ordered for bug hunting:
   - **Potential bugs**: harness failure text, browser errors, server rejections, distinct
     suspicious log lines, and the last pending decision with its options.
   - **Interesting events**:
     - action cards discarded / played / that raised decisions;
     - Mecatol Rex: custodians removed (by whom, which round), controller, how often system 18
       was activated;
     - technologies gained per faction;
     - strategy cards per round, agenda votes and laws, objectives and victory points, relics,
       combat counts;
     - which planet-selection decisions were seen, and a histogram of decision subtypes.
6. The proctor investigates the evidence briefly (at most ~10 tool calls) and separates real
   bugs from harness noise. It answers with one markdown entry:
   - a verdict, seeds and a repro command;
   - **Potential bugs**, each with severity (blocker / high / medium / low), category, evidence
     and the likely origin in the code;
   - Interesting events;
   - Proctor notes.
7. The loop then:
   - appends the entry to `nightly-reports/<night>/report.md`;
   - stops the game if it is still running;
   - writes a mechanical `digest.md`. If the proctor failed, the digest is appended to the
     report instead, so no run goes unreported;
   - starts the next run.

**Primary goal of the proctors: catching bugs.** Ranked:
1. The game cannot continue: no actionable control, a decision that never advances, an idle
   server, a stall.
2. Engine or server errors: panics, HTTP 4xx/5xx, rejected options.
3. JavaScript errors: `pageerror`, console errors, the UI never reaching the server version.
4. Rules or UI inconsistencies visible in the digest.

The harness fails fast on the first browser error or stuck decision. That run ends, it is
reported, and the next random seed starts.

## 8. Morning summary (08:00)

`nightly.sh summary` waits up to 30 minutes for the last proctor to finish. It then starts Opus
read-only (Read, Grep, Glob, `git log/show/diff`; no edits) with `prompts/summary.md`. Opus:
- checks the proctors' findings against the raw evidence;
- de-duplicates and ranks them across runs;
- reads the relevant source and **suggests** fixes, with files, functions, a test, and how
  confident it is, or says why no fix is needed;
- summarises coverage: runs, rounds reached, action cards, Mecatol Rex, technologies, agendas,
  and subtypes never reached.

The output goes to `nightly-reports/<night>/summary.md`, with a symlink at
`nightly-reports/latest-summary.md`. Nothing is implemented or committed.

## 9. Operations

- **Reports:** `nightly-reports/` (git-ignored). Raw run directories can be deleted after review.
- **Manual commands:** `tools/nightly-smoke/nightly.sh loop` (sweep now, within the configured
  window), `FORCE=1 tools/nightly-smoke/nightly.sh summary`.
- **Short test window** (as used for the first test):
  `NIGHTLY_START=<now> NIGHTLY_END=<now+45m> NIGHTLY_STOP_ROUND=2 NIGHTLY_MIN_RUN_SECONDS=900 NIGHTLY_REPORT_ROOT=…/nightly-reports-test nightly.sh tick`,
  then `nightly.sh summary` with the same variables.
- **Cost:** one Haiku session per run, about one tool call per 9 minutes of play plus the
  digest/investigation, and one Opus session per night.
- **Resources:** this machine has 2 cores and 3 GB RAM, so runs are strictly sequential.
- **Disabling:** `crontab -e` and remove the line, or `touch nightly-reports/<night>/sweep.done`
  to skip the rest of one night.
