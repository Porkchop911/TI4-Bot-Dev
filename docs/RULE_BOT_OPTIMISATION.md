# Rule bot optimisation: headless self-play

`selfplay` plays complete games with the rule bot on every seat: no browser, no server, no ports,
no libtorch. It is the entry point for improving the bot (`crates/ti4-rule-bot`): play many games
on fixed seeds, look at what the bot does and where it loses, change something, play the same
seeds again, compare.

```text
tools/rulebot/selfplay.sh smoke        # 10 games, ~10 s on 8 cores
tools/rulebot/selfplay.sh bench        # the golden set, 200 games, ~80 s on 8 cores, checked against baseline.json
tools/rulebot/selfplay.sh ab eager_worth=2,win_margin=0.8 100   # candidate vs shipped bot, 100 tables of 4p and 6p
tools/rulebot/selfplay.sh complete     # time to completion, 3 to 8 players, 19 maps, no cutoff
tools/rulebot/selfplay.sh replay --seed 17 --players 4 --text   # one game, every decision
```

The script builds `cargo build -p ti4-selfplay --release` (about one minute cold) into
`$SELFPLAY_TARGET` (default `<repo>/target`) and writes results to `<target>/selfplay-out/`.
Use `nice` / a private `CARGO_TARGET_DIR` on a shared machine (the script already uses `nice`).
The binary is `selfplay` (`cargo run -p ti4-selfplay --release -- run ...`).

## What a game is

One `GameSpec` is everything a game is made from: seed, number of players, map (template alias or
`random`), the faction of every seat, speaker (`seat`, `random` or a seat number), card set. It is
printed for every problem and stored for every game in the summary (`games[].spec`), and
`selfplay replay` takes it back as flags. Same spec, same build: identical game (checked with a
hash of the decision log, the `decisions_hash` that saves of the browser host carry).

The game is driven exactly as the browser host drives a rule seat (`ti4-wasm`: `Seat::decide` and
`draw`): one decider answers every seat. A question with the position in hand
(`Decider::choose_seeing`) goes to `ti4_rule_bot::choose(choice, seen, seed, seat, n)` with `n`
the number of the decision over all seats; a question without goes to one shared seeded random
stream. Rounds, decisions, victory points and the end of the game are the engine's (`GameState::finished`, ten points; the winner is
`ti4_engine::objectives::leader`, which breaks ties by initiative).

Not the same game as a table on the server: the server's `RandomBotDecider` counts decisions per
seat, the offline host and this tool count them over the table. A game of a human against bots
cannot be replayed here, and a game here cannot be replayed with `replay_save` (that takes the
answers of one human seat); use `selfplay replay` and the hash.

## Setups: maps, factions, seats, seeds

One setup per `--players` value (default `4,6`), or a matrix file (`--config`, JSON) with several.
All flags below are fields of a setup in the file (`tools/rulebot/matrix-example.json`).

| flag | meaning |
|---|---|
| `--players 3,4,5,6,7,8` | table sizes; one setup each (`4p`, `6p`, ...) |
| `--games N --seed-start S` / `--seeds 1..50` / `--seeds 3,5,9` | seeds (`a..b` excludes b, `a..=b` includes it) |
| `--map 4pHyperlanes,4pOuterRim,random` | template aliases (`selfplay maps --players 4`) and/or `random`; default: the recommended template. A map that does not build for the size is skipped and listed under `skipped` |
| `--map-policy cycle` / `random` / `cross` | maps share the seeds: seed `i` plays map `i mod n` (default), each seed draws its map from the seed, or every map for every seed |
| `--factions sol,_,hacan` | fixed seats in seat order (`_` free); the rest are drawn without duplicates |
| `--faction-pool a,b,c,...` | the free seats are drawn from this pool |
| `--rotate cyclic` (alias `latin`) / `all` / `none` | each table is also played with its factions shifted over the seats: `n` games per seed (every faction in every seat once), or all `n!` permutations (up to 6 seats). Separates seat from faction effects |
| `--speaker seat` / `random` / `2` | opening speaker (default `random`: drawn from the seed, as in the product) |
| `--starting-tech` (`"starting_tech": true` in a setup) | the seats that choose a starting technology are asked (Winnu, Argent, Keleres, Thunder's Edge picks; see `docs/STARTING_TECH_CHOICES.md`). Left out: not asked, the game every summary before bot version 3 recorded |
| `--card-set te|pok|base_game_codex1` | strategy card set (default: the host's default) |
| `--max-rounds 50 --max-steps 400000 --max-seconds 300 --loop-steps 5000` | limits (below) |

Faction draws without a pool are the lobby's own (`ti4_engine::seating::lobby_faction_draw`), so an unconstrained
setup is the game the product makes. Maps are those of `ti4-host` (`maps::catalog`); 3 to 8 players work.

A matrix:

```json
{ "limits": {"max_rounds": 50},
  "bot": {"win_margin": "0.7"},
  "setups": [
    {"name": "latin4", "players": 4, "maps": ["4pHyperlanes"], "factions": ["sol","hacan","xxcha","l1z1x"],
     "rotate": "cyclic", "seeds": "1..11", "speaker": "seat"},
    {"name": "maps5", "players": 5, "maps": ["5pHyperlanes","5pOuterRim","random"], "map_policy": "cross",
     "faction_pool": ["sol","hacan","xxcha","l1z1x","jolnar","naalu","yin"], "seeds": "1..4"} ] }
```

Each setup is aggregated on its own and all together (`setups` and `combined` in the summary).

## Termination

`victory` (ten points), `cutoff_rounds` (the round after `--max-rounds` was reached, default 50; a
cutoff, not an error), `cutoff_steps`, `stuck` (the same question answered the same way
`--loop-steps` times in a row, or `--max-seconds` of wall time), `engine_error` (the engine refused
a step), `panic` (caught; message and place in `detail`), `setup_error`. `problems` in the summary lists
everything but victory and `cutoff_rounds`, each with the command that plays it again.
For a time-to-completion measurement the cutoffs are set so high (`tools/rulebot/completion.json`)
that only a real hang ends a game.

## The summary

`selfplay run --out summary.json` writes JSON and prints the table. Per setup and combined:

- `terminations`, `cutoff_rate`, `rounds` / `decisions` / `wall_s` (min, mean, median, p90, max);
- `win_by_seat` and `win_by_faction`: games, wins, win rate with 95% Wilson interval, mean points
  (finished games only); `winner_vp`, `final_vp` histograms, `zero_vp_seat_rate`, `tied_at_top`;
- `completion` / `by_players` / `by_map`: time to completion (finished games), `us_per_decision`;
- `phase_ms_mean` (engine step wall time by the phase the step began in, bot time included) and
  `bot_share` (share of wall time spent inside the bot's decisions; the rest is the engine);
- `by_subtype`: the **profiling hook and decision-type histogram**. Per kind of question
  (`DecisionContext::subtype`, else `~<option kind>`): `n`, `smart` (the bot's own rule chose,
  not the random pick it falls back on), `ns` in the decider. The table shows the top by time;
- `usage_mean`: how often each notable option is taken per game: `action_menu=tactical|strategic|pass|...`,
  `draft_strategy_card=...`, `research_technology=...`, `score_objective=...`, `reaction=<card or ability>`;
- `objective_rate`: share of seat-games that scored each objective; `vp_source_mean`: points per
  game by source (the engine's `vp_ledger`: objective, secret_objective, imperial_primary, support_for_the_throne, custodians, ...);
- `problems`, `skipped`, `meta` (git rev, cores, workers, release/debug, wall time, games per hour);
- `games`: a compact line per game (spec, termination, winner, points, rounds, decisions, wall ms, hash).
  `--games-out FILE.jsonl` writes the full per-game results (the metrics above per game).

## Reproduce and inspect one game

```text
selfplay replay --seed 17 --players 4 --map 4pHyperlanes --factions sol,jolnar,hacan,titans --speaker random \
    --text --from 0 --to 80 --seat a --trace /tmp/g.jsonl --expect-hash bd3aca6e68e88918
selfplay replay --summary summary.json --seed 17 [--setup 4p --nth 0]     # take the spec from a summary
selfplay replay --spec '{"setup":"x","seed":17,...}'
```

`--text` prints one line per decision: number, round, phase, seat, question (`subtype`), `rule` or
`random` (did the bot's own rule choose, or the random pick), the chosen option id, how many
were offered, and the prompt. `--trace` writes the same as JSON lines after a header with the spec and hash.
`--bot key=val` replays with a changed bot. The rule bot gives no written reason beyond this:
to see why a rule fired, look up its question in the map below. `selfplay run --trace-dir DIR` writes traces of games
that did not end in victory (`--trace-which all` of every game).
Exit code 2 when `--expect-hash` differs.

## A/B: baseline against candidate

```text
selfplay compare --players 4,6 --games 200 --seed-start 20000 --candidate eager_worth=2,win_margin=0.8
selfplay compare ... --candidate-config cand.json --baseline win_margin=0.5   # either side may carry overrides
selfplay compare ... --rotate cyclic --mode mixed|separate
```

`mixed` (default): each table (the same spec for both) is played twice. In the first game the even
seats play the candidate and the odd seats the baseline, in the second they swap, so every seat
and faction plays both bots. For each pair: the share of the two games won by a candidate seat,
minus 0.5 (`win_share_advantage`), and the points of the candidate's seats minus the baseline's.
Output: mean and 95% interval (normal approximation over pairs, so use at least 100 tables;
pairs where a game did not end in victory are dropped and counted) and a verdict. Equal bots give exactly 0
(there is a test). Sanity check: `objectives_enabled=false` loses about 17 points of win share on 60 4p tables.
`separate`: the table is played once with the baseline on every seat and once with the candidate on every seat;
use it for length, speed and finish rates, not for strength.

### The knobs

`selfplay knobs` lists them with defaults. They are `ti4_rule_bot::tuning`: the numbers that were
constants (`win_margin`, `retreat_margin`, `activation_threshold`, `eager_worth`, `accept_net`,
`mecatol_bonus`, `expansion_rounds`, `objective_round`, `complete_value`, `progress_value`,
`deny_value`, `short_bonus`, ... ) plus two switches (`objectives_enabled`, `faction_plans_enabled`).
Set with `--bot key=val,key=val` / `--bot-config file.json` (`{"win_margin": 0.7}`), `bot` in a
matrix, or `--candidate`. `bot_version=1` sets every number of that version (`tuning::for_version`), so
`compare --baseline bot_version=1 --candidate win_margin=0.5` plays the current bot against version 1.
`bot_version=5` is the **current bot** (`tuning::v5()` = version 4 plus the decided knobs; `BOT_VERSION` is 5 since 2026-10-17: new games, the offline host and the server bots play it; versions 1 to 4 play exactly as before). Its members so far are `land_all` (the landing step commits every ground force aboard; A/B on 4pBgaBalancedD,
1200 seat-games: neutral, win share +0.04 pp [-1.4, +1.5]) and the three agenda-voting knobs `agenda_value`, `agenda_budget`, `agenda_riders` (docs/BOT_AGENDA_VOTING.md; enabled by Felix's decision). Other candidate knobs are not in it yet, pending decisions: `mecatol_guard` and
`mecatol_imperial_priority` (branch fb-muaat-warsun). A save with `bot_version` 5 resumes
(`bot_version_check` covers it). Overrides are per thread (`tuning::set`, `tuning::with`), so a baseline
and a candidate play in one process, even in one game. The server and the browser never set
them: the defaults are the shipped bot, byte for byte (60 saved nightly games replay to the
same hashes on the build before and after the hook; the golden baseline below checks it again).
To add a knob: add the constant's default and a line in the `tuning!` list in
`crates/ti4-rule-bot/src/tuning.rs`, and read `tuning::name()` where the constant was used.
A sweep is a loop over `selfplay compare`; the JSON (`--out`) has the estimate and interval.

## Golden benchmark and baseline

- `tools/rulebot/golden.json`: 200 games, 4 players seeds 1..100 on `4pHyperlanes` / `4pOuterRim`, 6 players seeds 1..100 on `6pStandard` / `6pOuterRim`.
- `tools/rulebot/baseline.json`: its summary for the shipped bot (version 5 since 2026-10-17; `baseline-v4.json` is version 4 and `bench-v4` checks it; all baselines re-recorded on 2026-10-17: the old files were stale on the engine of that day, v1 to v4 hash-identical before and after the version-5 change; version 4 since round 3, played with `"starting_tech": true` as in `golden.json`; `baseline-v3.json`,
  `baseline-v2.json` and `baseline-v1.json` are versions 3, 2 and 1 on the same seeds without the starting-technology choice (`golden-v2.json`,
  `selfplay.sh bench-v2|bench-v3`, `--bot bot_version=3|2|1`), which still reproduce hash for hash, 200/200), recorded on the build named in `meta.git_rev`
  (AMD EPYC-Genoa, 8 cores, release build, 7 workers, 82 s, other jobs running on the machine).
- Rules versions: a game plays the newest `RULES_VERSION` unless a setup pins `"rules_version": N` (or `--rules-version N`). `baseline-v1..v4.json` were recorded at rules version 11 and are verified with that pin (`golden-v2.json` for v1 to v3, `golden-v4.json` for v4; `selfplay.sh bench-v1|bench-v2|bench-v3|bench-v4`), 200/200 identical on the line that has versions up to 15. `baseline.json` (v5) was re-recorded on 2026-10-18 at rules version 15: from rules version 12 on the bot is asked new questions (which pool a gained command token goes to, then the Self-Assembly mech, the Grant Reallocation technology, Predictive Intelligence votes, Magen Defense hits) and Antimass Deflectors change space cannon; the v5 bot at pin 11 still reproduces the old baseline 200/200.
- `tools/rulebot/selfplay.sh bench` runs the set and compares every game's hash, termination and points with the baseline
  (`--verify`): **identical** is the gate for "I did not change the bot or the engine"; **different** is expected
  after a deliberate bot change (judge it with `compare`) and is a regression if the change was meant to be
  a refactor, or if it came with a rebase of the engine. After an accepted change, rerun `selfplay.sh baseline`
  and commit the new file on its own, saying why it changed.
- `tools/rulebot/completion.json` / `completion-baseline.json`: 6 table sizes, 19 maps, no cutoff (below).
- `tools/rulebot/stress.json`: 340 games over every template of every size plus `random`, for hunting stuck games and panics.

## Time to completion

Measured with `tools/rulebot/completion.json` (3 to 8 players, 19 maps, `--max-rounds 1000`, no step cap, 900 s per game
safety net), release build, AMD EPYC-Genoa 8 cores, **one worker**, while other jobs kept the machine at load average 5 to 11
(so single-game times are somewhat pessimistic; the 8-core run is slower per game than 1 worker by about 10%).
`wall` is seconds per game, `us/dec` the mean wall time per decision, `rounds` / `decis.` per game. The table is also in
`tools/rulebot/completion-baseline.json`.

```text
by player count        games  done wall mean   median      p90    max s   rounds   decis.   us/dec    (rate)
3p                        18    18     0.90     0.89     1.28     1.43     6.61     1215      739      100%
4p                        35    35     1.91     1.85     2.63     3.27     6.94     1930      990      100%
5p                        15    15     2.16     1.98     3.15     3.81     7.13     2011     1072      100%
6p                        20    20     3.35     3.40     4.44     5.07     7.25     2626     1277      100%
7p                         8     8     5.20     4.79     6.45     6.58     6.88     3133     1659      100%
8p                         8     8     8.23     8.27     9.95     11.1     7.75     3658     2249      100%

by map                 games  done wall mean   median      p90    max s   rounds   decis.   us/dec    (rate)
3pBgaBalancedD             6     6     0.85     0.91     1.01     1.08     6.50     1176      721      100%
3pHyperlanes               6     6     0.92     0.80     1.42     1.43     6.67     1224      753      100%
3pLearnToPlayPoK           6     6     0.92     0.89     1.14     1.22     6.67     1244      742      100%
4pBgaBalancedD             7     7     2.21     2.48     2.94     3.27     7.00     2041     1085      100%
4pExpanse                  7     7     1.58     1.53     1.86     1.89     7.14     1869      847      100%
4pHyperlanes               7     7     1.94     1.88     2.37     2.69     6.86     2009      965      100%
4pLearnToPlayPoK           7     7     1.97     2.09     2.54     2.82     7.00     1940     1016      100%
4pOuterRim                 7     7     1.85     2.02     2.36     2.37     6.71     1789     1032      100%
5pBgaBalancedD             5     5     2.20     1.88     3.15     3.35     7.20     2000     1099      100%
5pHyperlanes               5     5     2.31     2.08     3.19     3.81     7.20     2138     1082      100%
5pOuterRim                 5     5     1.96     1.56     2.70     2.76     7.00     1894     1034      100%
6pBgaBalancedD             5     5     3.08     3.42     3.80     4.04     7.40     2493     1236      100%
6pGravity                  5     5     3.63     3.63     4.69     5.07     7.40     2711     1339      100%
6pOuterRim                 5     5     3.29     2.92     4.35     4.38     7.40     2668     1233      100%
6pStandard                 5     5     3.42     3.38     4.58     4.93     6.80     2633     1298      100%
7pAnaphase                 4     4     5.52     5.42     6.39     6.58     7.00     3184     1734      100%
7pHyperlanes               4     4     4.87     4.45     5.81     6.39     6.75     3081     1581      100%
8pBeMyNeighborHyperlanes     4     4     8.74     9.26     10.6     11.1     7.25     4000     2185      100%
8pSpoonMap                 4     4     7.72     7.50     8.56     9.00     8.25     3317     2326      100%
```

**All 104 games finished with a victory; none hit any cap, hung, panicked or hit an engine error.** Games end after
6.6 to 8 rounds (about 1200 decisions at 3 players, 1900 at 4, 2600 at 6, 3700 at 8).
Time per game grows faster than linearly with the players (0.9 s at 3, 1.9 s at 4, 3.4 s at 6, 8.2 s at 8): decisions per game grow
with the players, and so does the cost of each decision (the engine builds the legal options of a larger board).

Throughput (games per hour, same machine, other jobs running):

| | 1 worker | 8 workers | speed-up |
|---|---|---|---|
| 4 players (40 games, 4pHyperlanes, no cutoff) | 2084 | 14366 | 6.9x |
| 6 players (24 games, 6pStandard, no cutoff) | 981 | 6219 | 6.3x |
| the completion mix above (104 games) | 1295 | 8708 | 6.7x |
| golden set (100 x 4p + 100 x 6p, 7 workers) | | 8800 | |

Games are independent and the results do not depend on the number of workers (checked: identical hashes at 1 and 8 workers).

## Where the bot lives (`crates/ti4-rule-bot/src`)

- `lib.rs` - `choose` / `choose_explained` (entry; random pick first, then the rule), `pick` (dispatch on
  `choice.context.subtype`), and the rules: `pick_action` (action menu), `activation_value` / `pick_activation`,
  `pick_move`, `pick_load`, `pick_commit`, `pick_retreat`, `pick_produce`, `pick_pay`, `pick_strategy_card`,
  `pick_token_pool`, `pick_research`, `pick_redistribution`, `pick_answer` / `pick_proposal` (trades), `expanding` (rounds 1-2).
  The module comment at the top lists every opinion. Questions `pick` returns `None` for stay random
  (the table shows them as `smart` < 100%).
- `objectives.rs` - from round 3: which objectives to score and aim at, `gain` of an action, denying a rival near ten points.
- `faction_plans.rs` - per faction technology order, unit weights, preferred strategy card.
- `answers.rs` - (bot version 2) agenda votes, Politics' speaker and agenda placement, sustain damage and casualties,
  readying planets, retreat destinations, Trade's free replenish.
- `planner.rs` - (bot version 2) the objective planner's production weights.
- `agents.rs` - when to use faction agents and other component actions.
- `random.rs` - the seeded random pick under the rule bot (`decision_rng(seed, seat, n)`).
- `tuning.rs` - the knobs, `BOT_VERSION` and `for_version(v)` (version 1 written out in full, version 2 as version 1 plus its
  named changes; `--bot bot_version=1|2` plays them).
- `ti4-selfplay`: `timeline.rs` (per-round record of every game), `store.rs` (stored games and the run cache).

How the seats are driven elsewhere: `ti4-server/src/session/random_bot.rs` (online tables), `ti4-wasm/src/lib.rs`
`Seat::decide` / `draw` (the offline game, the nightly offline mode) - this tool copies the latter.
Other old tooling, not used here: `ti4-sim` (an older simulation and benchmark crate), the MLP / review crates (need libtorch).

## Rules for safe optimisation

1. **Determinism.** `choose` must stay a pure function of (position, seed, seat, decision number). No clock, no thread-local state
   other than `tuning`, no hash-map iteration order, no global counters. The random pick is always drawn first so the stream is the same whether or not a rule
   overrides it; keep that.
2. **No hidden information.** A rule reads only the `SeatObservation` it is given (public board plus the bound seat's own hand and
   secret progress). Do not reach into `GameState` through another path, and do not read what another seat holds
   (hands, secret objectives, the decks' order).
3. **Every answer is one of the offered options** (the index returned is always `< options.len()`); a rule that
   cannot decide returns `None`.
4. **Judge changes on paired seeds**, with `compare --mode mixed`, at least 100 tables each of 4 and 6 players (more for
   effects under 5 points); look at the interval, not the mean. Also run with `--rotate cyclic` once, to see that a change
   does not just help one faction or seat. Check `terminations` (no new cutoffs, stuck or panics) and games per hour.
5. **Gates before a commit:** `cargo test -p ti4-rule-bot -p ti4-selfplay`; `selfplay.sh bench` identical to the baseline
   when you did not mean to change behaviour; `selfplay run --config tools/rulebot/stress.json` with `problems: none`
   when you touched movement, production or anything that loops; for any change in `ti4-rule-bot` that must not change the
   default bot, `replay_save` on saved games gives the same hashes before and after.
6. Keep `crates/ti4-rule-bot` free of dependencies that do not build for `wasm32-unknown-unknown` (the browser links it).
7. A new rule goes behind a knob first (`tuning.rs`, default = old behaviour), is compared, and only then becomes the default
   together with a new baseline.

## Adding a metric

Per game: a field in `GameResult` (`play.rs`), filled in `play()` from the final `GameState` (`game.state`), the decision
log (`game.table.log.records`) or in the decider (`SelfPlay::decide`, which sees every question and answer; the
`Shared` struct holds the counters). Per run: a field in `Agg` (`stats.rs`), filled in `aggregate`, printed in `agg_text`.
Counting a chosen option by name: add its question to `usage_key` (`play.rs`). Add a line to the schema test in
`crates/ti4-selfplay/tests/selfplay.rs` if the field must stay.

## First data and where to start

Data from the golden set, a 340-game stress run (every template of every size, random maps, 3 to 8 players), 300 more games
with the `pok` and `base_game_codex1` card sets, and a 240-game Latin square (6 players, `--factions jolnar,saar,sol,hacan,yssaril,muaat --rotate cyclic`,
`6pStandard`, 40 seeds).

**Health.** About 1000 games in all: no stuck game, no engine error, no panic, no cutoff. Nothing to fix in the engine was found.
The only oddities: a game can end with a winner on 7 to 9 points (7 of 200 golden games: 1 on 7, 6 on 9; the public objective deck ran
out, or the last objective was scored), and the top two seats were tied on points in 11 of 200 golden games (the tie goes to initiative).

**Where the time goes.** The bot is only about 26% of the wall time; the rest is the engine (building the legal options of each
question). Two questions are 82% of the bot's own time: `action_menu` (49%, about 1.9 ms each) and `activate_system` (33%, 2.9 ms each; it values every
system on the board). Everything else is under 2.5% each. Speeding the bot up means `activation_value` / `pick_action`; speeding the
games up beyond that means the engine's `legal_options`.

**Strength and balance (what a first round should look at).**

- Seats are fair: win rates by seat are flat (the Latin square: 15 to 20% for 6 seats, expected 16.7%).
- Factions are not. Latin square, same map and the same table with the factions shifted over the seats: sol 29%, saar 19%, jolnar 17%,
  muaat 12.5%, hacan 11%, yssaril 11% (95% intervals about +-5 points). Over the golden set (24 factions, 29 to 58 games each,
  intervals +-12): jolnar 41%, saar 38%, mentak 33%, titans 31%, sol 28% against yssaril 8.5%, l1z1x 9%, nekro 9%, hacan 10%, arborec 10%.
  A faction that wins rarely is a faction whose plan (`faction_plans.rs`) or abilities the bot plays badly: start with yssaril,
  hacan, muaat, nekro, l1z1x, naaz, mahact, empyrean.
- The objective layer matters: `objectives_enabled=false` loses 17 points of win share (CI -24 to -9). Scoring comes from public
  objectives (19 points per game over all seats), secret objectives (7), Imperial (1.8), Support for the Throne (3.3). The rarely
  scored public objectives (`ose`, `dfat`, `command_armada`, `become_legend`, `master_science`, `fwp`: under 0.3% of seat-games)
  are ones the bot never aims at.
- Questions where the bot has no opinion and answers at random (share `smart` under 5%, number of decisions in 340 games):
  `end_turn` 55k, `vote_exhaust_planet` 30k and `cast_vote` 11k (the agenda phase: the bot votes and spends influence at random), `ready_planet` 7k,
  `fight_ground_combat_round` 6.7k, `trade_choose_replenish` 4.7k, `reaction_after_AGENDA_REVEALED` 4.3k, `sustain_damage` 4k,
  `assign_casualty` 3.1k, `politics_place_agenda` 3.1k. Voting, casualty assignment and sustain damage are the cheapest places to add
  a rule; in the stress run `agenda` points are only 0.4 per game, so the agenda phase is nearly irrelevant to the winner now.
- Action menu per game over all seats (340 games): tactical 82, strategic 35, pass 34, action cards 6. The strategy card draft takes
  technology, construction, imperial, leadership, warfare most; politics and diplomacy least.
- Games end by round 7 on 10 points almost always; the bot is fast and aggressive early. Whether that is good play against humans is
  not measurable here (no cutoff ever happens, so cutoff rate is not a useful signal yet).

**Suggested first optimisation round.** (1) Sweep the knobs with `compare` (`win_margin`, `activation_threshold`, `eager_worth`,
`complete_value`, `progress_value`, `short_bonus`, `mecatol_bonus`, `expansion_rounds`) in one-at-a-time steps, 200 tables each.
(2) Add rules for the random questions above, starting with agenda voting and casualty/sustain choices.
(3) Rework faction plans for the weak factions, judged with `--rotate cyclic` setups for the faction in question.
(4) Cut `activate_system` / `action_menu` time if the sweeps need more games per hour.

## Round 1: bot version 2 (2026-10-15)

Branch `rulebot-opt1`. Every change was built behind a knob (default = version 1), judged with
`selfplay compare` (mixed mode, seat-swapped pairs, 200 tables of 4 players on `4pHyperlanes`/`4pOuterRim`
and 200 of 6 players on `6pStandard`, seeds 20000..), and stacked: each candidate was compared against the
changes accepted before it. Accepted: win share and points per seat not negative (95% interval not clearly
negative). The accepted knobs are the new defaults; `tuning::for_version(1)` (`--bot bot_version=1`) is the
old bot, written out in full, and plays the golden set hash-identical to `baseline-v1.json` (200/200).

### Change log (A/B against the accepted set before it)

`win share` = share of the pair's two games won by a candidate seat minus 50%, in points; `VP` = points per seat,
candidate minus baseline; 400 tables unless noted.

| change (knob) | why | win share pp [95% CI] | VP/seat [95% CI] | verdict |
|---|---|---|---|---|
| `casualty_rule` | sustain damage was declined at random and casualties were random: sustain whenever possible (cheapest sustaining unit, flagship/war sun last), least valuable casualty first, a damaged one before a fresh one | +6.88 [+4.08, +9.67] | +0.426 [+0.303, +0.549] | accepted |
| `vote_rule` | agenda votes and planet exhaustion were random: vote for its own election (cards, points), its own planet for an attachment, the rival leader's planet / the rival leader for a harmful one, the points agendas by the standings (never hand a rival the lead), all planets for a wanted outcome (they are readied after the agenda phase), else abstain; no trade goods for votes. Politics: itself as speaker (fb_1791901529, else the seat with fewest points), a wanted agenda on top | +3.88 [+0.99, +6.76] | +0.167 [+0.040, +0.293] | accepted |
| `misc_rule` | random small windows: ready the most valuable planet, retreat to the system with most of its own, Trade's free replenish for nobody | +1.88 [-1.17, +4.92] | +0.222 [+0.084, +0.361] | accepted |
| `ground_rule` + no empty fights (first version) | ground forces pile up at home (measured: 8.1 per seat at the end of a 4p game; fb_1791902555, fb_1792038973); fights that take nothing (fb_1791896411) | -1.25 [-4.23, +1.73] | -0.101 [-0.235, +0.034] | rejected, split below |
| `mecatol_rival_bonus=4` | Mecatol Rex held by a rival scores them Imperial's point (fb_1792039064) | +0.75 [-1.58, +3.08] | +0.002 [-0.083, +0.087] | accepted (neutral) |
| `planner_weight=1` | objective planner: production weights for objectives within 3 steps that more of a unit meets (warships for Raise a Fleet / Command an Armada and presence objectives, flagship/war sun for Engineer a Marvel / Supremacy, the unit secrets) | +0.50 [-1.93, +2.93] | +0.054 [-0.035, +0.143] | accepted |
| `progress_gap=3` | progress toward objectives three steps short counts for activation | -1.12 [-2.53, +0.28] | -0.031 [-0.077, +0.015] | rejected |
| `objective_round=2` | objective layer from round 2 | -3.38 [-6.51, -0.24] | -0.137 [-0.273, -0.002] | rejected |
| `nebula_margin=0.3` | attack a fleet in a nebula only with a bigger edge (defender +1) | -0.12 [-0.37, +0.12] | -0.006 [-0.016, +0.005] | rejected (rarely fires) |
| `home_guard` | no warship leaves home if the rest could not hold it against what a rival can move there now (`movable_into`, map-exact) | -4.38 [-6.83, -1.92] | -0.185 [-0.274, -0.097] | rejected |
| `plans_v2` | weak factions (golden set under 10%): Yssaril without Politics, L1Z1X/Nekro Leadership first, more carriers for Hacan/Arborec/Yssaril (table pool: the five plus sol, jolnar, saar, mentak, titans) | +2.50 [-0.22, +5.22] | +0.109 [+0.007, +0.211] | accepted |
| `rift_caution` on `6pGravity` (200 6p tables) | outside a fight, no move whose route exits a gravity rift (the engine marks its preview unresolved) | -3.25 [-7.31, +0.81] | -0.237 [-0.406, -0.067] | rejected: the rift's extra reach is worth its risk |
| `rift_caution` on the standard mix | | +0.00 [-0.49, +0.49] | +0.020 [-0.006, +0.046] | (no rifts there) |
| `complete_value=9` | | -0.25 [-2.21, +1.71] | -0.007 [-0.087, +0.074] | rejected |
| `sftt_rule` | never give away a Support for the Throne | -2.12 [-5.13, +0.88] | -0.210 [-0.342, -0.077] | rejected: two bots swapping notes both gain a point |
| `empty_fight_rule` | rounds 1-2: no fight that takes no planet away from its own systems (fb_1791896411) | -0.62 [-3.70, +2.45] | -0.018 [-0.143, +0.107] | rejected |
| `ground_rule` (free lift only) | troops only for ships' free capacity | +0.50 [-1.90, +2.90] | -0.027 [-0.112, +0.059] | rejected (neutral, and the pile-up hardly moved) |
| `end_turn_rule` | end the turn when nothing else is wanted, always use legendary end-of-turn abilities | -3.88 [-6.94, -0.81] | -0.316 [-0.461, -0.171] | rejected: the random free options (agents, component actions) are worth taking |
| `deny_within=4` | deny rivals from 6 points | +0.88 [-1.16, +2.91] | +0.019 [-0.045, +0.083] | accepted |
| `win_margin=0.4` | | -1.00 [-3.48, +1.48] | -0.032 [-0.120, +0.056] | rejected |
| `secondary_rule` | from round 3 Diplomacy/Trade/Construction/Politics secondaries only with a spare token and where they pay | -1.12 [-3.97, +1.72] | +0.009 [-0.108, +0.126] | rejected |
| `progress_value=4.5` | | -0.12 [-2.01, +1.76] | -0.042 [-0.114, +0.031] | rejected |
| `deny_value=5` | | -1.12 [-3.07, +0.82] | -0.029 [-0.086, +0.029] | rejected |

The rejected rules stay in the code behind their knobs (default off) for later rounds.

### Version 2 against version 1

| setup (fresh seeds) | tables | win share pp [95% CI] | VP/seat [95% CI] | rounds |
|---|---|---|---|---|
| 200 x 4p + 200 x 6p, recommended maps, seeds 50000.. | 400 | +8.13 [+5.12, +11.13] | +0.526 [+0.387, +0.665] | -0.02 [-0.14, +0.11] |
| 100 x 4p + 100 x 6p, pool yssaril, l1z1x, nekro, hacan, arborec, sol, jolnar, saar, `--rotate cyclic`, seeds 60000.. | 1000 | +12.55 [+10.58, +14.52] | +0.750 [+0.666, +0.834] | +0.03 [-0.05, +0.10] |

Version-2 seats win 58% (62.5% with the weak factions in the pool) of the games against version-1 seats at the same
tables; game length is unchanged.

### Scoring and points (golden set, all seats one bot: v1 vs v2)

VP by source, per game over all seats, on the golden set (200 games, every seat the same bot, so the points
per game are bounded by the ending: the shares say what the bots score, the A/B above says who wins).

| VP source | v1 | v2 |
|---|---|---|
| objective (public) | 18.98 | 18.54 |
| secret_objective | 7.05 | 7.42 |
| support_for_the_throne | 3.32 | 3.40 |
| imperial_primary | 1.84 | 1.65 |
| custodians | 0.99 | 0.99 |
| agenda | 0.41 | 0.77 |
| imperial_rider | 0.14 | 0.12 |
| shard | 0.07 | 0.07 |

Share of seat-games scoring each objective (selection; 1000 seat-games each, so 0.1% is one seat):

| objective | v1 | v2 | | objective | v1 | v2 |
|---|---|---|---|---|---|---|
| `corner` | 25.0% | 24.9% | | `supremacy` | 0.9% | 1.6% |
| `sway_council` | 24.6% | 24.4% | | `conquer` | 1.5% | 1.2% |
| `monument` | 24.5% | 23.7% | | `raise_fleet` | 1.8% | 2.5% |
| `lead` | 23.7% | 23.6% | | `command_armada` | 0.1% | 0.4% |
| `engineer_marvel` | 21.6% | 23.1% | | `master_science` | 0.2% | 0.5% |
| `build_defenses` | 19.3% | 18.9% | | `ose` | 0.1% | 0.8% |
| `outer_rim` | 17.7% | 17.4% | | `dfat` | 0.1% | 0.0% |
| `expand_borders` | 15.1% | 14.6% | | `become_legend` | 0.1% | 0.2% |
| `develop` | 14.5% | 12.0% | | `fwp` | 0.2% | 0.6% |
| `galvanize` | 11.2% | 9.6% | | `pem` | 0.5% | 1.1% |
| `golden_age` | 5.1% | 6.0% | | `subdue` | 3.5% | 2.6% |

The planner's unit objectives (Command an Armada, Raise a Fleet, Occupy the Seat, Engineer a Marvel, Supremacy, Produce en
Masse) are scored more often; the totals hardly move because the game still ends at ten points in round 7 (6.89 -> 6.81
rounds). Agenda points nearly double (the bot now votes for its own elections).

### Factions

Golden set (all seats one bot; 19 to 58 games per faction, intervals about +-12 points): the five weak factions
yssaril 9% -> 19%, l1z1x 9% -> 22%, nekro 9% -> 12%, hacan 10% -> 29%, arborec 11% -> 21% (the strongest fall back:
jolnar 41% -> 22%, saar 38% -> 24%, mentak 33% -> 20%).

Latin squares (every faction in every seat; all seats one bot, so these are shares among the factions at the table):

| 6p `6pStandard`, 40 seeds x 6 rotations | v1 win [95%] | v2 win [95%] | v1 VP | v2 VP |
|---|---|---|---|---|
| yssaril | 10.4% [7, 15] | 10.0% [7, 14] | 6.38 | 6.09 |
| nekro | 11.7% [8, 16] | 9.2% [6, 13] | 6.49 | 6.22 |
| arborec | 13.8% [10, 19] | 16.2% [12, 21] | 6.49 | 6.49 |
| hacan | 13.8% [10, 19] | 12.5% [9, 17] | 6.62 | 6.49 |
| l1z1x | 20.8% [16, 26] | 24.2% [19, 30] | 7.00 | 7.38 |
| sol | 29.6% [24, 36] | 27.9% [23, 34] | 7.63 | 7.48 |

| 4p `4pHyperlanes`, 50 seeds x 4 rotations | v1 win [95%] | v2 win [95%] | v1 VP | v2 VP |
|---|---|---|---|---|
| nekro | 19.0% [14, 25] | 14.5% [10, 20] | 6.30 | 6.45 |
| yssaril | 19.0% [14, 25] | 20.0% [15, 26] | 6.53 | 6.76 |
| l1z1x | 30.0% [24, 37] | 31.5% [25, 38] | 7.33 | 7.55 |
| hacan | 32.0% [26, 39] | 34.0% [28, 41] | 7.66 | 7.94 |

Among themselves the weak factions barely move (l1z1x and arborec up, nekro down, all inside the intervals): the
version-2 gains are general (casualties, votes) and help every faction. The plan rework is measured where it belongs,
in the mixed A/B (+2.5 pp, +0.11 VP); Nekro's Leadership-first plan is the first thing to revisit.

### Maps

Version 2 against version 1 (`compare --players N --map M --games 60 --seed-start 30000 --baseline bot_version=1`),
mixed mode: 60 tables per map, 120 games. Win share of the version-2 seats over 50%, points per seat, and the change in
game length:

| map | tables | v2 win share pp [95% CI] | VP/seat [95% CI] | rounds v2-v1 | wall ms/game v2-v1 |
|---|---|---|---|---|---|
| 4-4pHyperlanes | 60 | +11.7 [+4.5, +18.8] | +0.70 [+0.28, +1.13] | +0.10 | +141 |
| 4-4pOuterRim | 60 | +0.0 [-7.0, +7.0] | +0.29 [-0.07, +0.65] | -0.15 | -61 |
| 4-4pExpanse | 60 | +4.2 [-3.3, +11.6] | +0.45 [+0.01, +0.89] | +0.22 | +286 |
| 4-4pPyrrhicVictory | 60 | +9.2 [-0.0, +18.3] | +0.55 [+0.14, +0.95] | +0.18 | +103 |
| 4-4pBgaBalancedD | 60 | +6.7 [-0.9, +14.2] | +0.51 [+0.15, +0.87] | -0.28 | -186 |
| 4-4pHyperlanesNucleus | 60 | +11.7 [+4.5, +18.8] | +0.70 [+0.28, +1.13] | +0.10 | +108 |
| 5-5pSmallAndRich | 60 | +21.7 [+15.3, +28.0] | +0.83 [+0.51, +1.15] | -0.13 | -72 |
| 5-5pScptKnockout2020 | 60 | +8.3 [+0.5, +16.1] | +0.55 [+0.19, +0.90] | -0.25 | -116 |
| 5-5pRaceForMecatol | 60 | +11.7 [+3.2, +20.2] | +0.87 [+0.55, +1.18] | +0.00 | +123 |
| 5-5pHyperlanes | 60 | +13.3 [+4.4, +22.3] | +0.84 [+0.50, +1.19] | +0.05 | -72 |
| 6-6pStandard | 60 | +18.3 [+11.0, +25.7] | +1.07 [+0.79, +1.35] | -0.07 | +145 |
| 6-6pGravity | 60 | +21.7 [+14.2, +29.2] | +1.04 [+0.74, +1.35] | +0.33 | +308 |
| 6-6pSmallAndRich | 60 | +8.3 [+0.2, +16.5] | +0.62 [+0.30, +0.94] | -0.03 | -19 |
| 6-6pOuterRim | 60 | +11.7 [+3.8, +19.5] | +0.90 [+0.60, +1.20] | -0.08 | -160 |
| 6-6pClearSkies | 60 | +9.2 [+1.3, +17.1] | +0.63 [+0.35, +0.91] | -0.07 | -143 |
| 6-6pBalancedSpiral | 60 | +20.0 [+13.3, +26.7] | +0.99 [+0.67, +1.31] | +0.03 | +173 |
| 3-3pHurtsDonut | 60 | +9.2 [+3.7, +14.6] | +0.96 [+0.54, +1.38] | -0.08 | -29 |
| 3-3pHyperlanes | 60 | +9.2 [+2.0, +16.3] | +0.56 [+0.16, +0.97] | +0.10 | +96 |
| 4-random | 60 | +17.5 [+8.8, +26.2] | +1.07 [+0.73, +1.42] | +0.00 | +37 |
| 6-random | 60 | +18.3 [+11.0, +25.7] | +1.07 [+0.79, +1.35] | -0.07 | +89 |

No map regresses: the win share is positive on every map except `4pOuterRim` (+0.0, VP +0.29), and points per seat
are higher on every map. The largest gains are on the six-player and the rich maps (`5pSmallAndRich` +21.7,
`6pGravity` +21.7, `6pBalancedSpiral` +20.0). Two rows are not what they say: `4pHyperlanesNucleus` builds the same
board as `4pHyperlanes` (identical decision hashes, its nucleus slices are not used) and `--map random` at 6 players
builds the same board as `6pStandard` (identical hashes) - see the open questions.

Map-specific rules (C): the engine's movement generator (`Observed::movable_into`, the movement step's options)
already follows asteroid fields, supernovae, nebulae (move 1, entering only as the active system), gravity rifts
(the bonus, and a preview marked unresolved for a route that exits one), wormholes and hyperlane tiles (passable,
never a stop; their connections are not modelled, which the bot inherits), so the bot never plans an illegal route.
Three map rules were tried on top: a bigger edge to attack into a nebula (no effect: it rarely applies), no
gravity-rift gamble outside a fight (worse on `6pGravity`: -3.25 pp, -0.24 VP; the extra reach pays for the
risk) and a home guard against the fleets a rival can actually bring along the map's paths (worse everywhere:
-4.4 pp; it makes the bot passive). None is on.

### Speed and completion

| | v1 | v2 |
|---|---|---|
| golden set (200 games, 7 workers, same load): wall per game | 2.70 s | 2.62 s |
| golden set: rounds / decisions per game | 6.89 / 2266 | 6.81 / 2256 |
| golden set: bot time per decision (all questions) | 309 us | 311 us |
| golden set: `action_menu` / `activate_system` per decision | 1837 / 2813 us | 1848 / 2853 us |
| golden set: wall per decision (engine included) | 1192 us | 1163 us |
| completion set (104 games, 3-8 players, 1 worker each, run side by side) | 3.15 s, 7.03 rounds, 2177 decisions | 3.40 s, 7.12 rounds, 2263 decisions |
| completion set: bot time per decision | 384 us | 383 us |
| completion set: finished | 104/104 victories | 104/104 victories |

`completion-baseline.json` is the version-2 run of that table (`completion-baseline-v1.json` the old one). The new rules
cost nothing measurable per decision (they answer questions that were random, with a table lookup or one pass over the
options). Six-player games of the completion set run longer (8.05 vs 7.25 rounds over 20 games), the golden set's do not.
Health: the 340-game stress matrix (`tools/rulebot/stress.json`, every template of every size plus random) and 100
offline-style games (random maps, 3 to 6 players) with version 2: all victories, no cutoff, stuck game, engine error or
panic. Three offline games through the native wasm host (`bot_version_check 3 4 60`) finished, and their saves resumed
identically.

### Offline saves: the bot version

An offline save stores only the person's answers; the rule seats' answers are recomputed when it is resumed,
so any change to the bot would make an in-progress offline game replay differently (the engine-mismatch dialog,
then "no longer replays the same way"). Server games are not affected: recovery replays every recorded decision
with `ti4_host::replay::ReplayDecider` and never asks a bot.

Cheapest safe approach, implemented:

- `ti4_rule_bot::tuning::BOT_VERSION` (2) and `tuning::for_version(v)`; version 1 is the old bot, field by field.
- `ti4_wasm::host_setup::Setup::bot_version` (optional). `Start::settle_bot_version`: a new game records the current
  version; a resume uses the setup's, else the resume's own (`Resume::bot_version`; saves and restarts of this host carry it),
  else 1 (a save made before the field existed). `host_api::prepare` installs that version's tuning before the game is played.
- `crates/ti4-wasm/examples/bot_version_check.rs` plays offline games natively through the host, saves mid-game and
  resumes: a new save records version 2 and replays identically; a version-1 save with the field removed resumes as
  version 1 and replays identically; the same save resumed with the current bot does not reach its position.
- Caveats: the engine-identity prompt (wasm sha differs) still appears after every new engine build; "try to resume"
  then replays correctly. A save written by this build has `setup.bot_version`, which an older build (whose `Setup` denies
  unknown fields) rejects; that is a downgrade only.
- A later bot change bumps `BOT_VERSION` and keeps the previous numbers in `for_version` (a rule that is not a knob
  needs its own version check).

### Alpha feedback

- fb_1791901529 (speaker): fixed (`vote_rule`).
- fb_1792039064 (Mecatol Rex of a human not contested): `mecatol_rival_bonus` (neutral in A/B, kept).
- fb_1791902555, fb_1792038973 (ground forces at home): reproduced in self-play (`end_count_mean.home_ground`, 6.5 of
  14 ground forces per seat at the end of a golden game are in the home system); the two rules tried (free-lift
  production, no fight without troops) did not help and are off. Starting forces and Mitosis/production fillers are
  most of it; shipping them needs a target, which the bot only has when it can win a planet.
- fb_1791896411 / fb_1791897348 (attack without transports): from round 3 the objective layer already drops fights that
  take nothing; the round 1-2 rule was neutral to slightly negative and is off.

### Next ideas

- The end-of-turn free options and Support for the Throne swaps are worth more than they look (both "fixes" lost):
  look at which free options win (agents, component actions) and make them deliberate instead of random.
- Ground-force logistics: a planned "ship idle troops to the next takeable planet" (multi-turn), rather than the
  production cap.
- Map rules lost or did nothing (home guard, rift caution, nebula margin): the engine's movement generator already
  follows the anomalies; map gains must come from target choice (distance to Mecatol, chokepoints), not from filters.
- Stage II objectives are still rare because games end in round 7: the planner could reserve resources for a
  2-point spend objective one round ahead.
- `activate_system` / `action_menu` are still 80% of the bot's time.


## Stored games (round 2)

Since round 2 no played game is thrown away. `run` and `compare` (so `selfplay.sh ab`) write every game, with a
per-round **timeline** recorded while it is played (`crates/ti4-selfplay/src/timeline.rs`, the logic of the former
`analyze` tool), to `tools/rulebot/results/<key>.jsonl` (git-ignored; `SELFPLAY_RESULTS` or `--results DIR` moves it,
`SELFPLAY_RESULTS=off` / `--no-cache` disables it; `bench` and `baseline` never read it).

- The timeline per game: map facts per seat, a snapshot per seat at the end of every action phase (victory points, planets,
  resources/influence, technologies, ships/fighters/ground and where they stand, tokens, strategy cards, scored and held
  secrets), victory point events with round and source, activations (what was attacked, planets gained), attacks on home
  systems, objectives that were eligible at the status phase and what was scored, losses per round. Plus `vp_by_round`
  (points of every seat after each round).
- The key hashes every job (spec, limits, every seat's bot numbers, the seat mask), the shipped defaults and the harness
  version (`store::HARNESS_VERSION`). The same run again reads the file (0.02 s instead of minutes). A bot change that no
  knob shows needs `--no-cache` or `SELFPLAY_SALT=x`.
- `selfplay metrics results/<key>.jsonl` prints the A/B metrics of a stored `compare` run with no rerun; any other analysis
  reads the same lines (`jq`, python). Results are identical on 1 and 8 workers (checked), and stored files are rebuilt by
  rerunning, so deleting them is always safe (about 50 KB a game).
- `compare` now reports **VP per round** (final points over rounds played) and **VP after rounds 3, 4 and 5**, each paired by
  table (mixed: candidate seats minus baseline seats; `--mode separate`: the all-candidate game against the all-baseline game,
  an absolute strength measure), next to win share, VP per seat, rounds, and the effect per faction and per map.

**Method note.** Round 1 judged changes on win share first. Win share is zero-sum (a change that raises every faction ties), so
from round 2 the primary metric is VP per round (paired), with VP after rounds 3/4/5 and win share secondary. Round 1's numbers
stay valid as what they are (win share and VP per seat; the accepted changes there also raised VP per seat), but VP per round
was not primary then.

## Round 2: bot version 3 (2026-10-15/16)

Branch `rulebot-opt2` (from `rulebot-opt1`). Every candidate is a knob (`v3:` in `tuning.rs`); `for_version(2)` is version 2 written
out and plays the golden set hash-identical (200/200), as does `for_version(1)`. `BOT_VERSION` is 3. Sources of the ideas:
`crates/ti4-server/docs/COMMUNITY_FACTION_STRATEGIES.md` (CFS), `PODCAST_STRATEGIES.md` (PS), `FACTION_PLANS.md` (FP), and the
analyses `BOT_V2_ANALYSIS_4P.md`, `BOT_V2_GAME_ANALYSIS.md`, `OBJECTIVE_SCORING_RATES.md`, `THEORETICAL_VP_PACE.md` (branches
`bot-v2-analysis-4p`, `bot-v2-game-analysis`, `objective-rates`, `theoretical-vp-pace`).

### Where v2 stands (6p, 1200 games, all seats v2, about 300 seats per faction, +-4.5 pp)

Strongest sol 26.9%, l1z1x 23.8%, nomad 23.7%, mentak 22.0%; weakest **empyrean 6.8%, winnu 8.6%, ghost 11.1%, hacan 11.3%, naaz 11.8%,
nekro 13.8%**, mahact 14.2%. Nekro (round 1's worry: 19% -> 14.5% in 4p) is inside the noise of the middle now; the plan order
that changed for it (Leadership first) is only consulted when the draft is otherwise open from round 3, so it could not have
caused the move. **Finding: the faction plans (research order, unit weights, preferred card) have no measurable effect.**
`faction_plans_enabled=false` against the shipped bot: win share +0.00 [-3.08, +3.08], VP/seat -0.03 [-0.16, +0.10] (400 tables).

### Change log (v2 baseline; screening 200 or 400 tables of 4p+6p, seeds 70000.., VP/rd = VP per round, paired)

First batch, judged on win share and VP per seat (400 tables, before the VP/round metric existed):

| change (knob) | source | win share pp [CI] | VP/seat [CI] | verdict |
|---|---|---|---|---|
| `r1_trade` Trade in R1 for nekro, ghost, cabal, empyrean, titans | CFS Nekro/Ghosts/Cabal/Empyrean, PS C | -1.88 [-4.70, +0.95] | -0.06 [-0.19, +0.06] | rejected |
| `r1_leadership` Leadership R1 for muaat, mahact, cabal | CFS ranked #3 | -1.62 [-4.01, +0.76] | -0.05 [-0.15, +0.05] | rejected |
| `r1_diplomacy` Diplomacy R1 for xxcha | CFS Xxcha | -0.88 [-3.13, +1.38] | +0.04 [-0.05, +0.13] | rejected |
| `winnu_mecatol` Blood Ties: no 6 influence, Reclamation worth | CFS Winnu #7 | -0.12 [-0.55, +0.30] | -0.01 [-0.02, +0.01] | rejected (never matters) |
| `letnev_armada` two fewer fleet tokens | CFS #9 | -0.62 [-2.69, +1.44] | -0.00 [-0.08, +0.07] | rejected |
| `tech_hunt=1` Nekro fights for technologies | CFS #12 | -0.62 [-3.16, +1.91] | -0.03 [-0.13, +0.08] | rejected |
| `empyrean_rule` Aetherpassage only for itself | engine text | -0.38 [-3.37, +2.62] | -0.05 [-0.17, +0.08] | rejected |
| `adjacent_bonus=0.5` territory contiguity (map) | round-1 idea list | -1.12 [-4.20, +1.95] | -0.05 [-0.19, +0.08] | rejected |
| `home_last_ship` last warship stays home from R3 (narrow home guard) | BOT_V2_GAME_ANALYSIS | -2.25 [-4.81, +0.31] | -0.09 [-0.18, -0.00] | rejected |
| `mecatol_early=8` (worth in R2-3) | BOT_V2_ANALYSIS_4P #1 | -0.25 [-0.94, +0.44] | -0.01 [-0.04, +0.01] | rejected (never binds) |
| `mecatol_bonus=9` | | -0.12 [-0.77, +0.52] | -0.01 [-0.04, +0.02] | rejected |
| `eager_worth=2` | | -0.38 [-3.25, +2.50] | -0.01 [-0.12, +0.10] | rejected |
| `expansion_rounds=3` | | -3.88 [-6.76, -0.99] | -0.08 [-0.20, +0.04] | rejected |
| `retreat_margin=-0.2` | | -0.88 [-2.82, +1.07] | -0.03 [-0.09, +0.03] | rejected |
| `accept_net=0` | | -1.00 [-4.04, +2.04] | -0.13 [-0.24, -0.01] | rejected |
| `fleet_tokens_wanted=2` | | -4.38 [-7.26, -1.49] | -0.12 [-0.24, -0.00] | rejected |
| `plenty_resources=4` | | -0.75 [-3.52, +2.02] | +0.03 [-0.09, +0.14] | rejected |
| `win_margin=0.65` | | -0.50 [-2.95, +1.95] | -0.02 [-0.10, +0.07] | rejected |
| `faction_plans_enabled=false` | | +0.00 [-3.08, +3.08] | -0.03 [-0.16, +0.10] | no effect (finding above) |

Second batch under the primary metric (200 tables, seeds 70000..; the baseline is v2):

| change (knob) | VP/rd [CI] | VP after R3 [CI] | VP after R5 [CI] | win pp [CI] | verdict |
|---|---|---|---|---|---|
| `tech_pick_rule` (Nekro fixed) | +0.024 [+0.002, +0.046] | +0.011 [-0.038, +0.061] | +0.102 [-0.008, +0.213] | +1.75 [-2.1, +5.6] | **accepted**, confirmed |
| `r1_tech` (pool) | +0.016 [-0.013, +0.046] | +0.015 [-0.095, +0.125] | +0.053 [-0.103, +0.209] | +2.75 [-1.6, +7.1] | **accepted**, confirmed |
| `nekro_tech` (Nekro fixed) | +0.009 [-0.019, +0.037] | +0.015 [-0.071, +0.101] | +0.078 [-0.067, +0.223] | +3.5 [-0.6, +7.6] | **accepted**, confirmed |
| `early_margin=0.3` | +0.012 [-0.013, +0.038] | +0.100 [+0.024, +0.175] | +0.071 [-0.056, +0.199] | +4.75 [+0.7, +8.8] | rejected: not confirmed |
| `early_margin=0.6` | +0.001 [-0.027, +0.029] | +0.008 [-0.080, +0.095] | +0.026 [-0.105, +0.157] | +1.5 [-2.7, +5.7] | rejected |
| `r1_tech2` winnu, sardakk, argent (pool) | +0.017 [-0.014, +0.048] | -0.045 [-0.153, +0.062] | +0.017 [-0.137, +0.171] | +1.25 [-2.9, +5.4] | rejected |
| `short_bonus=2` | +0.010 [-0.009, +0.029] | +0.025 [-0.003, +0.052] | -0.044 [-0.125, +0.038] | +3.25 [-0.4, +6.9] | rejected |
| `short_bonus=3` | +0.001 [-0.018, +0.021] | +0.010 [-0.021, +0.040] | -0.084 [-0.172, +0.005] | +3.5 [-0.1, +7.1] | rejected |
| `ground_margin=0.5` (troops piling up) | +0.003 [-0.018, +0.025] | +0.038 [+0.000, +0.075] | +0.002 [-0.095, +0.100] | -1.0 [-4.8, +2.8] | rejected |
| `lift_rule=1` (transports for idle troops) | -0.003 [-0.010, +0.004] | +0.005 [-0.001, +0.010] | -0.011 [-0.043, +0.021] | +0.75 [-0.7, +2.2] | rejected (neutral) |
| `activation_threshold=0.3` (activation floor) | +0.005 [-0.005, +0.015] | +0.002 [-0.001, +0.006] | -0.000 [-0.035, +0.035] | +0.25 [-1.4, +1.9] | rejected (neutral) |
| `plan_off=1` plans off for ghost, hacan, empyrean, naaz, winnu (pool) | -0.004 [-0.030, +0.021] | -0.047 [-0.109, +0.015] | +0.042 [-0.077, +0.160] | -2.0 [-5.8, +1.8] | rejected |
| `planner_weight=2` | -0.005 [-0.016, +0.006] | +0.002 [-0.006, +0.010] | -0.015 [-0.052, +0.022] | 0.0 [-2.0, +2.0] | rejected |
| `planner_gap=5` | +0.006 [-0.001, +0.013] | +0.000 | +0.030 [+0.005, +0.056] | +1.0 [-0.5, +2.5] | rejected (small) |
| `planner_weight=2,planner_gap=5` | +0.003 [-0.011, +0.016] | +0.002 | +0.015 [-0.030, +0.060] | +1.0 [-1.5, +3.5] | rejected |
| `progress_gap=3` | +0.002 [-0.007, +0.011] | -0.001 | +0.018 [-0.006, +0.042] | +1.75 [-0.4, +3.9] | rejected |
| `complete_value=9` | -0.025 [-0.042, -0.008] | -0.013 | -0.050 | -1.75 [-4.7, +1.2] | rejected (worse) |
| `influence_early=0.5` (influence of planets worth more in R1-3, for the custodians) | +0.021 [-0.013, +0.054] (screen) | +0.053 | +0.095 | -1.0 | confirmed on fresh seeds 80000..: **-0.015 [-0.036, +0.006]**, R5 -0.108 [-0.208, -0.008]: rejected |
| `influence_early=1.0` | +0.011 [-0.026, +0.047] | -0.009 | +0.059 | +1.25 | rejected |

Confirmation on fresh seeds 80000.. (400 tables each): `tech_pick_rule` VP/rd **+0.023 [+0.005, +0.041]**, VP/seat +0.130 [+0.019, +0.240]
(Nekro fixed); `r1_tech` +0.019 [-0.002, +0.040], VP after R5 +0.121 [+0.016, +0.226]; `nekro_tech` +0.020 [-0.002, +0.042], VP/seat +0.116
[-0.019, +0.252]; `early_margin=0.3` +0.003 [-0.017, +0.023] (not confirmed). Both seed ranges point the same way for the three accepted
changes; none of them is large, and only `tech_pick_rule` has a confidence interval above zero on its own.

### What v3 contains

- `tech_pick_rule`: a technology gained outside a research (the Nekro's Technological Singularity and the other
  `*_pick_technology` / `*_choose_technology` prompts) is the best one by the seat's own order (plan, then `GENERAL`), and the Nekro always
  use Technological Singularity when offered (both were random: `assimilate` 85 times in 200 golden games).
- `r1_tech`: Technology first in round 1 for the Arborec (Sarween Tools: free infantry later), Naalu and L1Z1X (CFS Arborec, Naalu, L1Z1X).
- `nekro_tech`: the Nekro's research is three command tokens (Propagation), so Technology is drafted first in every round (CFS Nekro: Trade then
  Leadership, but the engine's Propagation makes Technology a free Leadership).

### Version 3 against version 2 (seeds 90000.., fresh)

Mixed (400 tables, 200 x 4p `4pHyperlanes` + 200 x 6p `6pStandard`): VP per round **+0.011 [-0.005, +0.028]**, VP/seat +0.072 [-0.031, +0.176], VP after
rounds 3/4/5 +0.019 / +0.040 / +0.019 (all intervals include 0), win share +0.88 pp [-1.71, +3.46], rounds +0.10 [+0.00, +0.19].
Uniform tables (every seat the same bot, 200 tables, seeds 95000..): VP per round -0.004 [-0.020, +0.012], rounds -0.035 [-0.16, +0.09]: **v3 is not
measurably stronger or faster in absolute terms; the gain is confined to the three factions' paths above and small.** Honest summary: round 2
found three small, twice-confirmed improvements and rejected everything else; the bot sits on a plateau for these levers.

Per faction (v3 as candidate, 400 mixed tables; the unchanged factions show the noise, about +-9 pp): nekro +9 pp / +0.52 VP, mentak +14 / +0.54
(unchanged by v3: noise), l1z1x +4 / -0.04, arborec +4 / +0.07, naalu -6 / -0.21; everything else inside the noise.

### Per map (v3 against v2, mixed, 50 tables each, seeds 31000..; 4pHyperlanes and 6pStandard from the 400-table run)

| map | tables | VP/rd [CI] | win share pp [CI] | VP/seat [CI] |
|---|---|---|---|---|
| 4-4pHyperlanes | 200 | n/a | +2.0 [-1.2, +5.2] | +0.11 [-0.05, +0.27] |
| 6-6pStandard | 200 | n/a | -0.2 [-4.3, +3.8] | +0.03 [-0.10, +0.17] |
| 3pHurtsDonut | 50 | +0.069 [-0.001, +0.140] | +7.0 [+0.7, +13.3] | +0.435 [+0.010, +0.860] |
| 4pExpanse | 50 | -0.007 [-0.067, +0.054] | -2.0 [-9.9, +5.9] | -0.075 [-0.459, +0.309] |
| 4pOuterRim | 50 | -0.001 [-0.051, +0.050] | +3.0 [-4.1, +10.1] | +0.080 [-0.240, +0.400] |
| 4pPyrrhicVictory | 50 | -0.028 [-0.075, +0.020] | -6.0 [-12.0, +0.0] | -0.160 [-0.440, +0.120] |
| 5pRaceForMecatol | 50 | +0.043 [-0.007, +0.093] | +7.0 [+1.4, +12.6] | +0.325 [-0.045, +0.695] |
| 5pSmallAndRich | 50 | +0.004 [-0.039, +0.046] | -1.0 [-9.2, +7.2] | +0.038 [-0.258, +0.335] |
| 6pBalancedSpiral | 50 | -0.018 [-0.071, +0.036] | +1.0 [-6.1, +8.1] | -0.113 [-0.462, +0.235] |
| 6pGravity | 50 | +0.003 [-0.038, +0.044] | +3.0 [-5.1, +11.1] | +0.077 [-0.224, +0.378] |
| 6pOuterRim | 50 | -0.012 [-0.049, +0.024] | -3.0 [-9.5, +3.5] | -0.077 [-0.338, +0.184] |

### Hypotheses from the analysis reports: covered or not

Tested (rejected or neutral): Mecatol push (`mecatol_early`, `influence_early`; earlier `mecatol_bonus=9`), activation floor (`activation_threshold=0.3`),
early-fight gate (`early_margin`), narrow home guard (`home_last_ship`), idle-army use (`ground_margin`, `lift_rule`, round 1's `ground_rule`),
Gravity Drive/Technology first for slow factions (`r1_tech`, `r1_tech2`), plan changes of the weakest factions (`plan_off`), objective planner strength
(`planner_weight`, `planner_gap`, `progress_gap`, `complete_value`). Not built (open): a fleet stack at the end of a round for raise_fleet /
command_armada / ose / supremacy, planetless-system parking for deep_space / vast_territories, a one-ship-per-system sweep for ctr / control_borderlands
(OBJECTIVE_SCORING_RATES.md); a deliberate Mecatol-in-round-2 plan with Imperial every round and a second public slot chosen by reachable objective
(THEORETICAL_VP_PACE.md: Imperial's Mecatol point is 0.34 of a possible 3 after round 3, custodians held by 43% of winners). Why: the objective layer
already values each of these through the engine's own requirement functions, so a blanket plan would duplicate it; the productive form is an
objective-*dependent* production/movement bias in `planner.rs` (the stacking and parking moves need multi-step movement planning, the same
obstacle as round 1's "ship idle troops to the next planet"). The weakest factions (empyrean 6.8%, winnu 8.6%, ghost 11.1%, hacan 11.3%, naaz 11.8%)
are not helped by any tested knob; their gap is in the opening (territory by round 2), which the existing scripted expansion handles identically for all.

### Gates run on this branch

`cargo test -p ti4-rule-bot -p ti4-selfplay -p ti4-wasm` pass; clippy clean; fmt (the three crates); golden `bench` identical to the new
`baseline.json` (200/200), `--bot bot_version=2` identical to `baseline-v2.json` (200/200) and `bot_version=1` to `baseline-v1.json` (200/200);
stress matrix (340 games) all victories, no problems; completion set (104 games, 3 to 8 players) all victories, 7.10 rounds, 3.49 s a game
(`completion-baseline.json` is the version-3 run, `completion-baseline-v2.json` the old one); `bot_version_check 3 4 60`: new games record version 3 and
resume identically, a version-2 save and a version-1 save without the field replay as theirs.

### Open decisions

1. Ship v3 at all: its measured gain is +0.011 VP per round [-0.005, +0.028] mixed and zero in uniform tables; all three changes are risk-free
   in the sense that they only touch Nekro technology picks, the Arborec/Naalu/L1Z1X first card and the Nekro's cards.
2. The objective-dependent Mecatol/Imperial/Custodians plan and the end-of-round fleet stack deserve their own round with movement planning.
3. The dominant lever left is territory by round 2 (BOT_V2_ANALYSIS_4P): needs opening work, not knobs.


## Round 3: bot version 4 (2026-10-16)

Branch `bot-v4` (from `backward-a` + `backward-b`). Felix's decision: ship ONE improvement containing (a) the round-2
recommendation (v3 content minus `nekro_tech`, plus `planner_gap=5`; `tech_pick_rule` and `r1_tech` stay) and (b) backward planning
(`docs/BACKWARD_PLANNING.md`, `BACKWARD_PLANNING_B.md`) with everything on except the fleet shape.

### What v4 contains (`tuning::for_version(4)` = `Tuning::default()`)

| knob | v3 | v4 |
|---|---|---|
| `nekro_tech` | on | **off** |
| `planner_gap` | 3 | **5** |
| `bw_enabled`, `bw_prep_build`, `bw_prep_warfare`, `bw_prep_tech_range`, `bw_prep_tech_strength` | off | **on** |
| `bw_combat_abilities`, `bw_agenda`, `bw_holdings`, `bw_ttfd`, `bw_production` | off | **on** |
| `bw_territory`, `bw_fleet`, `bw_payment`, `bw_technology`, `bw_combat` | on (inert without `bw_enabled`) | on |
| `bw_fleet_shape` | off | **off** (direct A/B: build + fleet shape against build only: -0.013 ctrl VP/round [-0.023, -0.003]) |

Replay safety: `for_version(3)` is written out (`v3()` in `tuning.rs`: version 2 plus the three `v3:` knobs, the old `nekro_tech` and
`planner_gap`, all `bw_*` switches off), so versions 1, 2 and 3 play exactly as before. `BOT_VERSION` was 4 until version 5 (above); new games, the offline host
(`Start::settle_bot_version`), the server's bot seats (they run `Tuning::default()`) and `selfplay` default to it. A save records its
`bot_version` (HostSave format 2) and resumes with it; a save without the field is version 1. `bot_version_check` now also resumes a
version-3 save as version 3. The server records no version: a saved server game replayed on a build with v4 plays its bot seats as v4
(the same as at every earlier bump). The alpha release (609a5bad) has no v4.

### Version 4 against 2 and 3 (fresh seeds, mixed seat-swapped pairs, 400 tables; candidate minus baseline, 95% CI paired by table)

Seeds: 110000.. for (i) and (ii) (200 tables 4p + 200 6p, maps 4pHyperlanes, 4pOuterRim, 6pStandard, 6pOuterRim cycled), 130000.. for (iii)
(100 seeds x 4 cyclic rotations of sol/letnev/jolnar/mentak on 4pBgaBalancedD). None used before. Games, logs and scripts in
`/root/rulebot-results/bot-v4/`.

| | (i) v4 vs v2, mixed | (ii) v4 vs v3, mixed | (iii) v4 vs v2, fixed table |
|---|---|---|---|
| win share | +2.6 pp [-0.3, +5.5] | +3.6 pp [+0.4, +6.8] | +5.4 pp [+2.4, +8.3] |
| VP per seat | +0.203 [+0.071, +0.335] | +0.169 [+0.041, +0.296] | +0.394 [+0.264, +0.524] |
| total VP per round | +0.031 [+0.011, +0.052] | +0.024 [+0.005, +0.044] | +0.064 [+0.043, +0.085] |
| **ctrl VP per round** | **+0.028 [+0.009, +0.046]** | **+0.015 [-0.003, +0.033]** | **+0.046 [+0.026, +0.065]** |
| ctrl after R3 | +0.021 [-0.020, +0.061] | +0.005 [-0.028, +0.038] | +0.048 [+0.010, +0.085] |
| ctrl after R4 | +0.100 [+0.038, +0.163] | +0.044 [-0.011, +0.100] | +0.102 [+0.041, +0.164] |
| ctrl after R5 | +0.128 [+0.039, +0.217] | +0.050 [-0.030, +0.130] | +0.163 [+0.074, +0.251] |
| total VP after R3 / R4 / R5 | +0.042 / +0.100 / +0.123 | +0.031 / +0.066 / +0.075 | +0.068 / +0.155 / +0.257 |
| imperial_primary | +0.106 [+0.064, +0.148] | +0.064 [+0.026, +0.102] | +0.060 [+0.026, +0.094] |
| objective (public) | +0.011 [-0.078, +0.100] | -0.028 [-0.118, +0.062] | +0.106 [+0.012, +0.201] |
| secret_objective | +0.038 [-0.014, +0.089] | +0.052 [+0.006, +0.098] | +0.104 [+0.055, +0.154] |
| custodians | +0.025 [+0.008, +0.041] | +0.020 [+0.006, +0.033] | +0.005 [-0.001, +0.011] |
| rounds | -0.037 [-0.153, +0.078] | n/a | +0.003 [-0.104, +0.109] |

ctrl VP gained per round (candidate minus baseline per seat; full series with CIs in `ser_i.txt`, `ser_ii.txt`, `ser_iii.txt`):
(i) R1 -0.011, R2 -0.001, R3 +0.032, R4 +0.080, R5 +0.028, R6 +0.026, R7 +0.025 (cumulative +0.180 [+0.058, +0.301]);
(iii) R1 0.000, R2 +0.002, R3 +0.046, R4 +0.055, R5 +0.060, R6 +0.046, R7 +0.067 (cumulative +0.276 [+0.155, +0.397]).
The gain is made in rounds 3 to 5, when backward planning starts.

Per table size and map (win pp, ctrl VP per round):

| group | n | (i) win pp | (i) ctrl/rd | (ii) win pp | (ii) ctrl/rd |
|---|---|---|---|---|---|
| 4p | 200 | +6.8 [+2.9, +10.6] | +0.049 [+0.021, +0.077] | +6.8 [+2.6, +10.9] | +0.031 [+0.003, +0.059] |
| 4pHyperlanes | 100 | +4.5 [-1.1, +10.1] | +0.011 [-0.030, +0.052] | +3.0 [-2.9, +8.9] | +0.001 [-0.038, +0.040] |
| 4pOuterRim | 100 | +9.0 [+3.7, +14.3] | +0.087 [+0.050, +0.124] | +10.5 [+4.7, +16.3] | +0.061 [+0.023, +0.099] |
| 6p | 200 | -1.5 [-5.8, +2.8] | +0.007 [-0.017, +0.031] | +0.5 [-4.3, +5.3] | -0.001 [-0.023, +0.022] |
| 6pStandard | 100 | -4.0 [-10.0, +2.0] | -0.005 [-0.036, +0.026] | +2.0 [-5.0, +9.0] | +0.002 [-0.029, +0.033] |
| 6pOuterRim | 100 | +1.0 [-5.2, +7.2] | +0.019 [-0.018, +0.056] | -1.0 [-7.7, +5.7] | -0.003 [-0.036, +0.029] |

Reading: the gain is a 4-player gain (the outer-rim map most of all); at 6 players v4 is level with v2 and v3 (no loss, no gain). Over the
mix v4 is at or above v3 on every ctrl measure (point estimates positive, none significantly negative) and clearly above v2.
Against v3 the mixed-table ctrl VP per round (+0.015) is not significant by itself; win share and total VP are. The fixed table (iii),
the one the planners were developed on, shows the largest effect and is an upper bound for other maps.

### Cost

Golden set (200 games, all seats one bot, 6 workers, back to back): v3 3.08 s a game, bot 24% of it (0.74 s, 0.32 ms a decision);
v4 5.08 s a game, bot 50% (2.52 s, 1.08 ms a decision): **bot time 3.4x, game time 1.65x**; games per hour 6915 -> 4182. The cost is in
`action_menu` (1.9 -> 5.4 ms), `activate_system` (2.9 -> 6.3 ms), `produce_unit` (0.14 -> 3.7 ms), `draft_strategy_card` (0.06 -> 5.6 ms),
`research_technology` (0.6 -> 4.9 ms) per question: the plans are rebuilt for every one of those questions (no cache across the questions
of one turn). Completion set (104 games, 3 to 8 players, 1 worker, v3 and v4 run in parallel): v4 5.96 s a game against 3.57 s (bot share
51% against 24%), 8 players 16.8 s a game. Not blown up, but a per-turn plan cache is the obvious next speed-up.

### Gates run

- `cargo test -p ti4-rule-bot -p ti4-selfplay -p ti4-wasm`, clippy `-D warnings`, fmt: clean (see the final report of the branch).
- Golden `bench`: `--bot bot_version=1|2|3` identical to `baseline-v1.json`, `baseline-v2.json`, `baseline-v3.json` (200/200 each); the
  default bot is the new `baseline.json` (version 4; the games differ from v3 by design).
- Stress (340 games, every template 3 to 8 players plus random maps), all bw knobs on: 340 victories, no stuck, panic, engine error or cutoff.
- Completion set (104 games, 3 to 8 players): 104 victories, 7.07 rounds (`completion-baseline.json` is the version-4 run,
  `completion-baseline-v3.json` the version-3 run).
- Uniform golden set, VP per game by source (v3 -> v4): objective 18.74 -> 19.41, secret_objective 7.39 -> 8.02, imperial_primary 1.69 -> 1.50,
  agenda 0.84 -> 0.63, support_for_the_throne 3.29 -> 3.44.

### Open

1. Speed: cache the plans within a turn (bot time 3.4x).
2. 6p shows no gain; the planners were developed on one 4p map. A 6p-specific round (more steps, a different `bw_steer`) is the next lever.
3. Not in v4: fleet shape (`bw_fleet_shape`), which lost directly against build alone; the other rejected knobs keep their old defaults.

## Controllable VP (analysis on what is already stored; no new games)

**Definition.** Controllable VP = what a seat earns by playing: public objectives (stage I and II, including the extra
one Imperial scores), secret objectives, the custodians, Imperial's Mecatol point. **Noisy / not controllable** (ledger sources seen in
4000+ stored games): `agenda:*` (mutiny, censure, ...), `support_for_the_throne` (a swap two bots both accept, +1 each),
`imperial_rider`, relic points (`shard` of the Throne, `emphidia` Crown, `bookoflatvinia`) and any other source (`relic_other`;
exploration/frontier rewards give no VP on their own in the ledger). Share of final VP: 13% (4p) and 15% (6p) of a seat's points.

**Harness.** `compare` and `selfplay metrics` print a *controllable* block next to the total-VP metrics, computed from the stored
timelines (`vp_events` by source): `ctrl_vp_per_round`, `ctrl_r3/r4/r5`, `noisy_final` and `cat:<category>:r3|r5|final` for the eight
categories, all paired by table with 95% CIs (mixed: candidate seats minus baseline seats; `--mode separate`: all-candidate minus
all-baseline table). `--exclude-sources a,b` makes everything except the named categories controllable (names:
`objective secret_objective custodians imperial_primary support_for_the_throne agenda imperial_rider relic_other`). Games without a
stored timeline are skipped and counted (`no_split`). Code: `crates/ti4-selfplay/src/controllable.rs`, `run.rs` (`controllable_pair`),
unit tests for both.

**What could be re-judged.** Only data with the per-source split can: (a) the v2 self-play timelines of the analyses
(`/root/scratch-bpa/a4.jsonl` 2000 games 4p, `/root/scratch-ba6/a6.jsonl` 1800 games 6p; all seats v2), (b) the golden baselines
(per-source means per game for v1, v2, v3). **Every A/B result of rounds 1 and 2 has no split and is listed under "needs rerun"
below.** The stored games of this round's own A/B runs were deleted with the private result directory at the end of the round
(my cleanup; the harness now keeps them, and a rerun is a cache fill).

### Good against bad on controllable VP (v2 self-play, paired within game: seat class means)

Mean cumulative controllable VP per seat after rounds (objective + secret + custodians + Imperial point), and the noisy VP:

| class (4p, 2000 games) | VP/round total | VP/round controllable | ctrl after R3 | R5 | final | noisy at R3 / final |
|---|---|---|---|---|---|---|
| winners | 1.562 +-0.012 | 1.367 +-0.011 | 2.90 | 6.35 | 8.47 | 0.72 / 1.19 |
| bad (<=3 total VP, winner 10) | 0.438 +-0.010 | 0.360 +-0.011 | 1.06 | 1.93 | 2.03 | 0.58 / 0.43 |
| bad on controllable (<=3 ctrl VP) | 0.551 +-0.011 | 0.409 +-0.008 | 1.21 | 2.22 | 2.34 | 0.70 / 0.82 |
| all seats | 1.067 +-0.009 | 0.922 +-0.008 | 2.23 | 4.69 | 5.90 | 0.69 / 0.92 |

| class (6p, 1800 games) | VP/round total | VP/round controllable | ctrl after R3 | R5 | final | noisy at R3 / final |
|---|---|---|---|---|---|---|
| winners | 1.366 +-0.010 | 1.180 +-0.010 | 2.13 | 4.93 | 7.72 | 0.75 / 1.22 |
| bad | 0.361 +-0.006 | 0.278 +-0.007 | 0.78 | 1.57 | 1.84 | 0.59 / 0.59 |
| bad on controllable | 0.462 +-0.007 | 0.327 +-0.005 | 0.90 | 1.82 | 2.20 | 0.68 / 0.93 |
| all seats | 0.860 +-0.007 | 0.728 +-0.006 | 1.57 | 3.50 | 5.00 | 0.70 / 0.95 |

Cumulative controllable VP by category (4p winners / bad / all seats), after round 3 / 5 / final (7): objectives 1.43 / 3.52 / 4.94 against
0.76 / 1.37 / 1.43 against 1.26 / 2.84 / 3.62; secrets 0.71 / 1.55 / 1.92 against 0.24 / 0.49 / 0.53 against 0.57 / 1.25 / 1.54; custodians
0.41 / 0.43 / 0.44 against 0.04 / 0.04 / 0.04 against 0.24 / 0.25 / 0.25; Imperial Mecatol point 0.34 / 0.84 / 1.17 against 0.01 / 0.02 / 0.02
against 0.16 / 0.35 / 0.49. 6p winners: objectives 1.24 / 3.03 / 4.89, secrets 0.46 / 1.17 / 1.75, custodians 0.28 / 0.30 / 0.31, Imperial
0.15 / 0.43 / 0.76 (bad: 0.57 / 1.12 / 1.28, 0.17 / 0.38 / 0.49, 0.04 / 0.04 / 0.04, 0.01 / 0.02 / 0.02).

Findings (verdicts of the earlier analyses stand):
- The gap between good and bad seats is controllable VP: winners 8.47 against 2.03 (4p) at the end, and the *noisy* VP is nearly the same for
  both (1.19 against 0.43 final, 0.72 against 0.58 at round 3). Excluding noise changes the winners' pace from 1.56 to 1.37 VP per round and
  does not narrow the gap (3.6x total, 3.8x controllable).
- The winner is not the top seat on controllable VP in 4.8% (4p) / 6.3% (6p) of games, and the winners' final points drop 9.99 -> 8.77
  (4p), 9.94 -> 8.61 (6p) without the noisy sources, so about 1.2 to 1.3 VP of a win come from noise (mostly Support for the Throne, 0.66 per seat in
  4p, 0.72 in 6p, then agenda 0.23/0.21).
- Pace targets of THEORETICAL_VP_PACE.md on controllable VP: custodians and Imperial's Mecatol point are where the winners lag (Mecatol point
  0.34 / 0.84 / 1.17 after rounds 3 / 5 / 7 in 4p; custodians 0.41 after round 3, flat afterwards); stage-I objectives 1.43 / 3.52 / 4.94.
- Per faction (4p, 6p tables in `/root/rb2-ctrl/faction.txt`): the ranking by controllable VP per round hardly differs from the win-rate
  ranking. The one real shift is the **Nekro: 0.62 (4p) / 0.67 (6p) noisy VP per seat against 0.8 to 1.1 for everyone else** (they cannot vote,
  so no agenda points); on controllable VP per round they sit at 0.847 (4p, rank 17 of 24) and 0.733 (6p, rank 13), above their win-rate rank (20 of 24 and
  17 of 24). Their weakness is partly the missing agenda income, not play; Xxcha and Argent are the reverse (1.09 / 1.18 noisy VP, rank falls on
  controllable).

### Uniform golden set (200 games, seeds 1..100, all seats one bot)

| version | rounds | total VP / seat-round | controllable VP / seat-round | noisy share |
|---|---|---|---|---|
| v1 | 6.890 | 0.953 | 0.838 | 12.0% |
| v2 | 6.805 | 0.969 | 0.841 | 13.2% |
| v3 | 6.925 | 0.956 | 0.832 | 13.0% |

(per-game sums by source over all seats, five seats on average; means, no per-seat pairing, so no confidence intervals.) **Verdict change:**
v2's apparent gain in total pace (+1.7%) is almost entirely noisy VP (agenda 0.41 -> 0.77 per game from `vote_rule`); controllable pace is
unchanged (+0.4%). That is the all-seats-equal view; v2's head-to-head gain (casualties, votes, the planner) is a different quantity and cannot be re-judged
without the split. v3 against v2 in uniform tables: controllable -1.0%, i.e. nothing, as the total-VP result already said.

### Needs rerun (no per-source split stored)

All of these were judged on total VP and win share; their controllable-VP verdicts are unknown until they are rerun with the stored-games
harness (a rerun fills the cache, ~6 min for 200 tables at 6 workers):
- Round 1 (`ab-results/*.json` of the round-1 session): casualty_rule, vote_rule, misc_rule, ground_rule (+ no empty fights), mecatol_rival_bonus, planner_weight,
  progress_gap=3, objective_round=2, nebula_margin, home_guard, plans_v2, rift_caution (6pGravity and mix), complete_value=9, sftt_rule, empty_fight_rule,
  end_turn_rule, deny_within=4, win_margin=0.4, secondary_rule, progress_value=4.5, deny_value=5; v2 against v1 (overall, cyclic, maps, Latin squares).
- Round 2 (`/root/rb2-out/*.json`, aggregates only): tech_pick_rule, r1_tech, nekro_tech, early_margin 0.3/0.6, r1_tech2, short_bonus 2/3, ground_margin, lift_rule,
  activation_threshold, plan_off, planner_weight/planner_gap, progress_gap, complete_value, influence_early, home_last_ship, r1_trade, r1_leadership, r1_diplomacy,
  winnu_mecatol, letnev_armada, tech_hunt, empyrean_rule, adjacent_bonus, mecatol_early, the numeric sweeps, v3 against v2 (mixed, uniform, per map).
Expected effects by mechanism (not measured): the agenda and Support for the Throne rules (`vote_rule`, `sftt_rule`, Politics) are the ones whose
verdict is most likely to change under controllable VP; tech_pick_rule, r1_tech, nekro_tech touch only research and cards, so their controllable
effect should track the total effect. BOT_VERSION 3 content is unchanged pending the reruns.
