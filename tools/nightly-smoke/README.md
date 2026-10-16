# Nightly UI smoke sweep

Random-click UI playthroughs of the TI4 web client, proctored by Sonnet agents, with two Opus
fix rounds and an Opus morning summary. Cron calls `nightly.sh tick` every five minutes
(`*/5 * * * * /root/TI4-Bot-Dev/tools/nightly-smoke/nightly.sh tick >> nightly-reports/cron.log`);
`tick` decides what is due. The sweep plays one proctored game at a time, or several side by side
with `NIGHTLY_SLOTS=N` (see "Parallel slots").

## Schedule (Berlin time)

| When | What |
|---|---|
| 20:30 | the sweep starts: one proctored game after another (Sonnet) until about 06:00 |
| 23:59 (or earlier on a proctor's request) | Opus fix round 1 starts while the sweep keeps running |
| about 06:00 next day | the last proctor finishes, `sweep.done` appears, Opus fix round 2 starts |
| after round 2 | the Opus morning summary (it lists both fix rounds) |

A night is named after the Berlin date on which its window started (`nightly-reports/2026-10-07/`).
The window is 9.5 hours, 10.5 or 8.5 on the two daylight-saving days. Day arithmetic goes through the
calendar, never "minus 86400".

**First night: 2026-10-06.** `NIGHTLY_NOT_BEFORE` (default `2026-10-06`) makes `tick` ignore every
earlier night, so switching to this schedule never switches a checkout people are working in.

**The 06:00 to 20:30 gap.** Round 2 may use up to 3 hours and the summary up to one more,
so they can run after the next window has started. `tick` therefore also looks at the previous
night, and the next sweep waits while the previous night's round 2 or summary is running or due
(at most `NIGHTLY_START_DEFER_SECONDS`, default 2 hours, past 20:30). Both use the same checkout.

**Early fix rounds.** Proctors are told to ask for changes immediately when they see a real error
(crash, stuck game, rejected-submit loop, console error, broken layout, a UI element that does not
work), with a precise reproducible reason: `request_fix.sh <reason...>`. They do not wait for the
error to repeat. The first request of a night writes `nightly-reports/<night>/fix-requested` (time,
run name, reason); `tick` then treats round 1 as due (it still needs one report entry, round 1
enabled and not yet started, the sweep running and now before END). Once round 1 has started, the
next request writes `fix-requested-2` instead (also when round 1 is disabled but round 2 is on):
round 2 then starts early, as soon as round 1 has finished (`.fixer-1.done`), while the sweep keeps
running; its branch is merged between games like round 1's. Requests beyond that (a second
`fix-requested-2`, or round 2 already started, or no round enabled) are not recorded and the script
tells the proctor to put the problem in the report entry. The reason goes into the fixer prompt
(`{{REQUEST}}` in `prompts/fixer.md`, empty for a plain scheduled round) and a line in `report.md`
notes the request. The script is on the proctors' allowed-tools list. Proctors keep their
minor-repair rights; a proctor who repairs something still reports it. The prompt also has a
"new UI elements to watch for" section: the harness saves screenshots to `trace/shots/` (first
decision of rounds 1 to 3, first sight of the trade desk, production builder, payment bar, token
panel, secondary-prep chrome, combat summaries, toasts, objectives and ballot modals), and the entry
has `UI observations` and `Fix requests` sections.

**Strategy card set.** `run_game.sh` plays the Thunder's Edge cards (server default `te`) in 75% of
the runs and Prophecy of Kings (`pok`) in the rest (`NIGHTLY_POK_PROBABILITY`). `meta.json` has
`card_set`; the repro carries `TI4_SMOKE_CARD_SET=pok` only when it is not the default.

**Optional UI exercises** (`web/e2e/smokeExercises.ts`; switches also work in a manual run, e.g.
`TI4_SMOKE_UI_TOUR=1 TI4_SMOKE_RECAP=1 TI4_SMOKE_REDO=1 TI4_SMOKE_SHOT_CAP=all npm run test:e2e:smoke`).
`meta.json` has `exercises` and the repro carries the switches. Results are in `report.json`
(`exercises`, `findings`, `shots`) and the digest. None of them fails a run; a problem is a finding.
- UI tour: Faction card at a calm turn-menu moment, one unit card when a production builder is open
  (before anything is staged); asserts text, on-screen, unclipped, Escape closes it.
- Recap: the seat's browser gets `player_turn_recap=true`; recap toasts are counted by a
  MutationObserver. The harness never treats a corner toast as a decision control.
- Turn redo: at most once per run, after some seats completed turns, at the turn menu of another
  seat in the action phase it clicks "Redo my last turn" on the last finished seat's tab, plays the
  new turn with the normal heuristics (the playthrough pauses while the tab fires the auto-play),
  then randomly clicks Restore original timeline or Keep this timeline. Timeouts: 30 s for the
  rewind, 5 min for the new turn, 150 s for the handoff and for the final state change. Any problem
  (409/400, error text, no handoff) records a finding and tries a restore; if that fails too the run
  ends cleanly with outcome `...STUCK`.
- Secondary pre-planning (`web/e2e/smokePrep.ts`, on in every run): at the start of another seat's
  decision, a seat whose "Prepare your secondary" chip is on screen is offered once per card. With
  probability `TI4_SMOKE_PREP_PROBABILITY` it sets Auto or Review on the player sheet, opens the chip,
  answers the stand-in question with the usual candidate heuristics (seeded RNG, follow 75%), clicks
  Save plan if the mode is still open, and checks the chip says "Prepared". When the seat's real
  secondary opens: Auto must answer by itself with no decision dialog (toast "Auto-played your
  prepared secondary", 20 s limit, nothing is clicked); Review shows the "Prepared" bar, which is
  confirmed (75%) or dismissed with "Choose myself". Follow-up steps (technology, planets, a prepared Warfare production and its first payment) are
  checked the same way for 6 s. Any deviation is a finding and the decision is played normally.
  Counters are in `report.json` `prep`. The prepare chrome is clicked only by this exercise (the
  normal heuristics still exclude it). `TI4_SMOKE_PREP=0` disables it for a manual run.
- Screenshots: up to `TI4_SMOKE_SHOT_CAP` (default 20) per run; the tour and redo contribute at most
  a shot or two inside that cap.

## How a night runs

1. The sweep saves the checkout, uncommitted work included, as one snapshot commit on the branch
   `nightly-fixes-<night>`, checks that branch out in the live checkout and plays there. The
   branch the checkout was on is recorded in `nightly-reports/<night>/orig_branch`. Nothing is
   pushed.
2. Before every game, `$NIGHTLY_BUILD_CMD` (default: build the server) must pass. Proctors may
   commit minor repairs on the night branch; a repair or merge that breaks the build is reset to
   the last good commit.
3. **Fix rounds** (`nightly.sh fix <round>`) run in their own git worktree
   `nightly-reports/<night>/fixer-<round>` on branch `nightly-opus-<night>-r<round>`, started from
   the night branch. They never touch the live checkout while games run. Each reads the report,
   clusters the findings, and for the most frequent real bugs writes a failing test, fixes it,
   runs the targeted tests and commits (at most 6 fixes, wall-clock budget 4 h for round 1 and
   3 h for round 2). Rules-timing and engine-semantics changes are left to the user and listed in
   `fixer-<round>.md`.
4. **Hand-off:** a finished round with commits leaves `fixer-<round>.ready`. The sweep merges a
   ready branch into the night branch only between games (before the build check), so a game never
   runs on sources that change underneath it. Round 2 merges directly once the sweep has ended and
   build-checks the result. Outcomes are marker files and lines in `report.md`: `.fixer-N.merged`,
   `.fixer-N.rejected` (the merge broke the build and was dropped), `.fixer-N.conflict` (aborted).
   A branch is never deleted; it stays for the user.
5. The morning summary (`summary.md`) waits for round 2 (done, skipped, disabled, or more than
   `FIX2_MAX_SECONDS` + 30 minutes late).

**Run names and a killed sweep.** Runs are named `NN-HHMM` under `runs/`. A sweep (also one that
cron restarts after a kill) takes the highest existing `NN` in `runs/` and continues from `NN + 1`;
gaps and directories that do not match `NN-HHMM` are ignored, so ids stay unique within a night.
On SIGTERM, SIGINT or SIGHUP (or any exit) while a run is active, the loop kills the proctor's
whole session (`claude` and its children), runs `run_game.sh stop` (server, vite, browsers), builds
`digest.md` with `digest.py` and appends `## Run NN-HHMM — sweep terminated mid-run (SIGTERM),
proctor failed, mechanical digest only` to `report.md` with the last decision number and the
evidence path. The cleanup runs once, uses timeouts, does nothing when no run is active, and does
not write `sweep.done`, so the next cron tick starts the sweep again.

In the morning: `git checkout <orig_branch>` and merge or cherry-pick from `nightly-fixes-<night>`.
Worktrees under `nightly-reports/<night>/fixer-*` and `slot-*/repo` can be removed with
`git worktree remove`.

## Parallel slots (`NIGHTLY_SLOTS=N`)

`NIGHTLY_SLOTS=1` (the default) is everything described above, unchanged. With `N >= 2` the one
sweep process (`nightly.sh loop`, started by `tick` as before, one lock, one `sweep.done`) only
supervises N worker subshells, the slots. Each slot is a complete copy of the one-slot sweep:

| | one slot | slot K of N |
|---|---|---|
| working tree of the proctor and the game | the live checkout | its own git worktree `nightly-reports/<night>/slot-K/repo` |
| branch proctors commit repairs to | `nightly-fixes-<night>` | `nightly-fixes-<night>-sK`, created from the night branch |
| cargo target dir (builds and `cargo run` of the game) | the checkout's `target/` | `nightly-reports/target-slot-K` (about 2 GB, a cold build is about 80 s; kept between nights) |
| run names | `NN-HHMM` | `sK-NN-HHMM`, numbered per slot (a restart continues each slot's own count) |
| `web/node_modules` | the checkout's | a private copy (about 160 MB, vite cache included) of the live checkout's |

The live checkout is switched to the night branch exactly as before (snapshot commit, `orig_branch`),
but with several slots nobody plays in it: it is the base the slot worktrees and the fixers branch from.

- **No shared tree.** Two proctors never share a working tree, index or branch, and a repair or a
  merge in one slot changes no source under the other slot's running game. Builds
  (`NIGHTLY_BUILD_CMD`, run with `CARGO_TARGET_DIR` set) happen in the slot's own worktree and
  target dir before every one of that slot's runs; the proctor's `cargo check`/`cargo test` use the
  same dir (the `CARGO_TARGET_DIR` and `NIGHTLY_GAME_REPO` of the proctor process; `run_game.sh`
  and `_run_inner.sh` work in `NIGHTLY_GAME_REPO`).
- **Fixer branches** (`fixer-N.ready`) are merged into each slot's own branch between that slot's
  games, before its build check. A slot records what it did next to itself:
  `slot-K/.fixer-N.merged`, `.rejected` (it broke that slot's build and was dropped) or `.conflict`
  (it conflicted with the slot's own proctor repairs: aborted, the slot goes on without it). The
  report gets a line per slot. A run in progress is never touched.
- **After the sweep** (all slots finished, before `sweep.done`) the slot branches are merged into the
  night branch in the live checkout, slot 1 first. A conflict between two slots' repairs is aborted
  and reported (`.slot-sK.conflict`); that slot branch is kept for the user and its commits are
  not in the night branch. The same merge runs when a fix round starts (also mid-sweep), so a
  fixer builds on the repairs made so far. The ready branch of a fixer round is merged into the
  night branch after the sweep, as before (`.fixer-N.merged`).
- **The report is shared.** One `report.md`; every append is one write under a lock
  (`.report.lock`), so entries never interleave. An entry's slot is the `sK-` in its run name.
  `digest.py`, `coverage.py`, the fixer and the summary read run directories and do not care about
  the names. `coverage.py` skips worktrees (`slot-*/repo`, `fixer-*`) in the reports directory.
- **Memory gate.** A new game needs `NIGHTLY_MEM_BASE_MB + NIGHTLY_MEM_PER_PLAYER_MB * (largest table
  in NIGHTLY_PLAYER_COUNTS)` of `MemAvailable` (default 1500 + 1200 * 6 = 8700 MB; measured
  peak of about 4 GB for four players and 3 GB for three), but only while another slot has a run
  active: a slot that is alone starts as before, so two slots are never slower than one. Starts are
  one at a time (`.admit.lock`): the slot that launches a game keeps the others waiting for
  `NIGHTLY_SLOT_SETTLE_SECONDS` (300) so the memory the new game ramps up to is already visible.
  A waiting slot rechecks every `NIGHTLY_MEM_WAIT_SECONDS` (120) and notes `## Slot K waiting for
  memory: ...` once in the report. The disk gate (`NIGHTLY_MIN_FREE_GB`) applies per slot as before.
  Slot K also starts `(K-1) * NIGHTLY_SLOT_STAGGER_SECONDS` (60) after the sweep.
- **Failures stay local.** A slot that cannot set up its worktree or whose build fails (nothing of its
  own to reset) reports it and ends; the other slots go on. The sweep ends when all slots have ended.
- **Stopping the sweep.** SIGTERM/SIGINT/SIGHUP to the sweep process tells every slot to clean up its
  own run at once (proctor session killed, `run_game.sh stop`, `digest.md`, one `## Run sK-NN-HHMM —
  sweep terminated mid-run (SIGTERM)` entry each). A run's state is the file `.active-sK` in the night
  directory; whoever moves it first owns the end of that run, so there is exactly one entry per
  run, also when a slot worker died, and a run left behind by a sweep that was killed with SIGKILL
  is closed off (`earlier sweep died`) when the next sweep starts.
- **Proctors** are told which slot they are and that another game runs in parallel on the same
  machine; `request_fix.sh` finds the slot from `NIGHTLY_SLOT` or its working directory
  (`.../slot-K/repo`), uses the slot's active run as the run name and prefixes the reason with
  `[slot K]`. The marker files and the one-request-per-round rules are unchanged. Fixer and summary
  prompts get a sentence about the slots (empty with one slot, so single-slot prompts are
  byte-identical to before).
- **Rolling back.** Unset `NIGHTLY_SLOTS` (or set it to 1) in the cron line: the next sweep is the
  classic one. Slot worktrees, branches and target dirs of an earlier night stay until removed
  (`git worktree remove`, `rm -rf nightly-reports/target-slot-*`). A sweep restarted mid-night with a
  different N continues in the new mode (run names of the other mode are ignored by the numbering).
- **Costs.** Two games at once are about 7 GB of RAM and 2 x 3 cores of browsers and servers on top
  of the fixer's session: this machine (15 GB, 4 GB swap) is at its limit with a four-player game
  plus a fix round plus the phone-play instance, which is what the memory gate is for. Keep an eye
  on `Slot K waiting for memory` lines in the report; raise `NIGHTLY_MEM_PER_PLAYER_MB` to be more
  careful, or set `NIGHTLY_PLAYER_COUNTS=3` to make the gate (and the games) smaller.

## Commands

    nightly.sh tick                 start whatever is due (cron)
    nightly.sh tick-decide          print what tick would start, change nothing
    nightly.sh loop [night]         the sweep
    nightly.sh fix <round> [night]  an Opus fix round
    nightly.sh summary [night]      the morning summary
    request_fix.sh <reason...>      (proctors) ask for a fix round now (round 1, then round 2)

## Settings (environment variables, defaults in `config.sh`)

| Variable | Default | Meaning |
|---|---|---|
| `NIGHTLY_START`, `NIGHTLY_END` | `20:30`, `06:00` | window, Berlin time (`NIGHTLY_TZ`) |
| `NIGHTLY_NOT_BEFORE` | `2026-10-06` | earlier nights are ignored by `tick` |
| `NIGHTLY_FIXERS` | `1 2` | rounds that run; `1`, `2` or an empty value (`NIGHTLY_FIXERS=`) to disable |
| `NIGHTLY_FIXER_MODEL` | `claude-opus-5-5` | model of the fix rounds |
| `NIGHTLY_FIX1` | `23:59` | start of round 1 (a proctor request starts it earlier) |
| `NIGHTLY_FIX1_MAX_SECONDS`, `NIGHTLY_FIX2_MAX_SECONDS` | `14400`, `10800` | wall-clock budgets |
| `NIGHTLY_MAX_FIXES` | `6` | fixes per round |
| `NIGHTLY_START_DEFER_SECONDS` | `7200` | how long the next sweep waits for the old night |
| `NIGHTLY_BUILD_CMD` | `cargo build --quiet -p ti4-server --bin server` | check before every game and after merges |
| `NIGHTLY_PROCTOR_MODEL`, `NIGHTLY_SUMMARY_MODEL` | Sonnet 5.5, Opus 5.5 | |
| `NIGHTLY_PRESET_PROBABILITY` | `70` | percent of runs that start from a prepared state |
| `NIGHTLY_PRESET` | `combat combat cards agenda relics invasion techs leaders siege bombard capture explore notes world world` | one preset name or a list to pick from per run (see `crates/ti4-server/src/preset.rs`; `endgame` is left out of the mix). `siege`, `bombard`, `capture`: see the combat-preset notes above. `explore`: landings on unclaimed trait planets, frontier ships with Dark Energy Tap, fragments, stacked exploration decks. `notes`: promissory notes in foreign hands. `world`: roster factions beyond the original six with their techs, leaders, flagship, notes |
| `NIGHTLY_POK_PROBABILITY` | `25` | percent of runs that play the PoK strategy cards instead of the TE default |
| `NIGHTLY_UI_TOUR_PROBABILITY` | `17` | percent of runs with `TI4_SMOKE_UI_TOUR=1`: open the Faction card and one unit card, check them |
| `NIGHTLY_RECAP_PROBABILITY` | `20` | percent of runs with `TI4_SMOKE_RECAP=1`: one random non-host seat turns the Recap toggle on; recap toasts are counted |
| `NIGHTLY_REDO_PROBABILITY` | `5` | percent of runs with `TI4_SMOKE_REDO=1`: one guarded turn redo round trip (see below) |
| `NIGHTLY_PREP_PROBABILITY` | `0.5` | fraction (0..1) of secondary opportunities a waiting follower plans (`TI4_SMOKE_PREP_PROBABILITY`); `0` switches the exercise off (`TI4_SMOKE_PREP=0`) |
| `NIGHTLY_PREP_AUTO_PROBABILITY` | `0.9` | fraction of plans that use the Auto mode, the rest Review (`TI4_SMOKE_PREP_AUTO_PROBABILITY`) |
| `NIGHTLY_SHOT_CAP` | `20` | cap on screenshots per run (`TI4_SMOKE_SHOT_CAP`; a number or `all` = 20); exercise shots count inside it |
| `NIGHTLY_PRESET_ROTATE_PERCENT` | `20` | percent of preset runs that also rotate the factions (`<preset>+rot`: Jol-Nar and L1Z1X at small tables) |
| `NIGHTLY_PRESET_FACTIONS_PERCENT` | `25` | percent of preset runs (not rotated, not `world`) that seat factions from the engine roster by seed (`<preset>+fac`; `+fac:naalu:mentak` names the first seats) |
| `NIGHTLY_SHORT_PRESETS` / `NIGHTLY_PRESET_SHORT_PERCENT` | `techs agenda leaders notes` / `70` | presets that get `+short` (a few victory points each, three public objectives left: the game ends in a few rounds) and how often |
| `NIGHTLY_PRESET_AGENDA_PERCENT` / `NIGHTLY_AGENDA_EXCLUDE` | `65` / `agenda notes endgame` | percent of eligible preset runs that also get `<preset>+agenda` (custodians gone at the start, trade goods for votes: an agenda phase every round; about half of all runs with the default mix). Raised to `NIGHTLY_AGENDA_BOOST_PERCENT` (90) when the last night with 5+ runs had no vote, and to 100 when the first `NIGHTLY_AGENDA_NIGHT_MIN` (6) runs of tonight had none (`coverage.py --agenda-runs`) |
| `NIGHTLY_WEB_BUILD_SHARE` | `0` | percent of runs that play against a production web build (`TI4_E2E_WEB=build`: `vite build` cached per commit in `web/.e2e-dist`, served by `vite preview` with the same proxy) instead of the vite dev server. `web` in `meta.json`, `web=build` in the digest header and `TI4_E2E_WEB=build` in the repro attribute findings |
| `NIGHTLY_EXPLORE_MAP_TEMPLATE` / `NIGHTLY_EXPLORE_MAP_PERCENT` | `3pInPersonHyperlanes` / `30` | `explore` at three seats plays this map, the only default-size map with frontier tokens off the hyperlane tiles |
| `NIGHTLY_SLOTS` | `1` | games played side by side (see "Parallel slots"); 1 = the classic sweep |
| `NIGHTLY_SLOT_STAGGER_SECONDS` | `60` | slot K starts (K-1) times this after the sweep starts |
| `NIGHTLY_MEM_BASE_MB`, `NIGHTLY_MEM_PER_PLAYER_MB` | `1500`, `1200` | memory a new game needs while another slot has a run active: base + per-player share * the largest table in `NIGHTLY_PLAYER_COUNTS` |
| `NIGHTLY_MEM_WAIT_SECONDS` | `120` | how often a slot rechecks the available memory |
| `NIGHTLY_SLOT_SETTLE_SECONDS` | `300` | after a launch, the other slots wait this long (or until the proctor ends) before their memory check |
| `NIGHTLY_SLOT_POLL_SECONDS` | `5` | how often a slot polls for its turn to start |
| `NIGHTLY_MEM_AVAIL_CMD` | `awk ... /proc/meminfo` | prints the available memory in MB (test hook) |
| `NIGHTLY_PORT_MIN`, `NIGHTLY_PORT_MAX` | `20000`, `49000` | backend port range of the games (frontend = port + 1); ports bound now or picked by a run in progress are skipped |
| `NIGHTLY_NOW`, `NIGHTLY_NOW_FILE` | unset | fake clock for tests |

To switch the Opus rounds off: `NIGHTLY_FIXERS=` in the cron line. Two long Opus sessions per night
are the main cost of this schedule.

## Host resources (`net::ERR_INSUFFICIENT_RESOURCES`)

Playwright launches Chromium with `--disable-dev-shm-usage`, so the browser's shared memory (mojo
data pipes, compositor frames) is kept in unlinked files under `/tmp`. Here `/tmp` is a RAM-backed
tmpfs (7.7 GB), and anything else that fills it (a cargo target dir, a copied `node_modules`, game
data) makes every page after the first fail with `net::ERR_INSUFFICIENT_RESOURCES` and crash the
browser's compositor (night 2026-10-08, 20:51-21:46). Each run therefore:

- removes `/tmp/ti4-playwright-games-*` and `playwright_chromiumdev_profile-*` dirs older than
  `NIGHTLY_STALE_TMP_MINUTES` (90) that no process uses, and deletes its own game dir after copying
  it to `<run>/server-data`;
- sets `TI4_E2E_DEV_SHM=1` when `/dev/shm` has `NIGHTLY_MIN_SHM_MB` (2048) free, so
  `web/playwright.config.ts` drops `--disable-dev-shm-usage` and the browser uses `/dev/shm`, a
  separate tmpfs that a full `/tmp` does not affect;
- writes `<run>/resources.log`: memory, swap, load, `/tmp` and `/dev/shm` use and the biggest `/tmp`
  entries at the start, then a line every `NIGHTLY_RESOURCE_LOG_SECONDS` (30) and one at the end.

## Where a run's time goes

`TI4_SMOKE_TIMING=1` makes the harness add `timing` to `trace/report.json`: per phase the summed
milliseconds and call count (`commit` = wait for the new version after a committing click, `prep`,
`click`, `settle`, `status`/`actor` snapshot GETs, `ui`, `scan`, `tour`, and `batchHttp` = the
POST /batches round trip). `TI4_E2E_SERVER_BIN=<path>` runs an already built server binary instead of
`cargo run` (a debug build, as always). A 150-decision techs game on 3 seats measured: `commit` 58 %,
`prep` 22 %, Playwright `click` 11 %, everything else under 10 %.

## Optimized dependencies and the pre-warm

The dev profile compiles third-party crates with `opt-level = 3` and no debug info
(`[profile.dev.package."*"]` in Cargo.toml); the workspace crates stay plain debug builds (opt-level 0,
debug assertions and overflow checks on). Changing the profile invalidates every existing cargo target
dir, so the first build in each (`target-slot-K`, `target-fixer`, the checkout's `target/`) recompiles
all dependencies: about 2.5 min at `-j2` instead of 1.3 min. The sweep builds before every game
(`BUILD_CMD`, no timeout), so Playwright's backend start (180 s) only ever sees an up-to-date target.
To take that cost out of the night, run `tools/nightly-smoke/prewarm.sh` (add `--fixer` for the fix
rounds' `cargo test` target) before 20:30; `PREWARM_DRY_RUN=1` prints the commands. It refuses to run
while a sweep or fix round is running.

## Tests

    tools/nightly-smoke/test_schedule.sh
    tools/nightly-smoke/test_prewarm.sh     # prewarm.sh with a stub cargo
    tools/nightly-smoke/test_resources.sh   # the host-resource helpers of config.sh

test_schedule.sh runs in a temp directory with stub `claude` binaries and a fake clock: window maths on both sides
of midnight and both daylight-saving changes, the not-before guard, what `tick` starts at each time
(round 1 once at 23:59 or on an early request, a request after round 1 started kept for an early round 2, requests refused after round 2 started or when no round is enabled, the reason reaching the fixer prompt and the report, round 2 only after `sweep.done`, the summary only after round 2, the
previous-night gap), a dry run of a whole night including the between-games merge, a merge that
breaks the build, and a merge conflict. It never starts a real proctor, game or build and never
touches the live checkout. It also SIGTERMs a loop mid-run (fake proctor and game) and checks the
entry, the absence of stray processes, the restart numbering and the `highest-run-number` rule. `test_preset_pick.sh` tests the preset and card set choice.
`coverage.py <reports-dir>...` lists the decision subtypes no run ever offered (and those offered in
fewer than two runs) by diffing the engine's literal subtypes against the runs' `report.json`; `test_coverage.sh` tests it.
`coverage.py --agenda-line <night>/runs` prints `agenda phase: N of M runs voted` (or a loud `NO AGENDA PHASE HAPPENED TONIGHT`); the morning summary starts with that line.

    tools/nightly-smoke/test_slots.sh

The parallel slots, same method (temp dir, universal stub `claude`, fake clock, stub build command, no
real proctor/game/build): two slots overlap in time with unique per-slot run names and one report
entry per run; each proctor works in its own worktree with its own cargo target; repairs of both slots
reach the night branch after the sweep, and a conflict between them is aborted and reported; a fixer
branch that appears mid-run is merged into both slots only between their games (no game sees its
sources change), a fixer merge that conflicts in one slot only is marked for that slot; a failing build
in one slot leaves the other running (and a lone slot is not gated by low memory); a low-memory
reading delays the second start until memory returns while the first game still runs; SIGTERM mid-run
cleans up both runs (games, proctor sessions, digests, exactly two entries, no stray process) and the
restart continues each slot's own numbering; a stale active run of a killed sweep is closed off; the
early fix request carries the slot (env and working-directory routes); a fix round starts from the
slots' repairs; `NIGHTLY_SLOTS=1` keeps the old names, paths, prompt and report. `KEEP_TMP=1` keeps the
temp dir for inspection.
