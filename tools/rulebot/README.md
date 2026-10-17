# tools/rulebot

Rule-bot self-play (`selfplay.sh`, see `docs/RULE_BOT_OPTIMISATION.md`).

- `golden.json` / `baseline*.json`: the golden benchmark (200 games) and its recorded summaries
  (`baseline-v1.json`, `-v2`, `-v3` are bot versions 1 to 3; `baseline.json` the shipped bot, version 4).
- `completion*.json`, `stress.json`: time-to-completion and stuck/panic hunting sets.
- `results/` (git-ignored): **stored games**. Every `selfplay run` and `compare` (so `selfplay.sh ab`)
  writes its games, one JSON line each, with the per-round timeline (snapshots of every seat at the end
  of each action phase, victory point events, activations, home attacks, eligible objectives, losses; the
  shape of the former `analyze` output) to `results/<key>.jsonl`. The key hashes the games, every seat's
  bot numbers, the shipped defaults and the harness version. The same run again reads the file instead of
  playing (`--no-cache` plays anyway; `bench` and `baseline` always do; `SELFPLAY_SALT=x` forces new keys
  after a bot change that no knob shows). `selfplay metrics results/<key>.jsonl` prints the A/B metrics
  of a stored compare run without any rerun. Delete files freely; they are rebuilt by rerunning.
- A/B metrics (`compare`): VP per round and VP after rounds 3, 4 and 5 (paired by table; the primary
  strength measure from round 2 on), win share, VP per seat, rounds, per faction and per map.
  `--mode separate` plays the table once with the baseline on every seat and once with the
  candidate on every seat: the same VP metrics are then the absolute strength of the bot.
