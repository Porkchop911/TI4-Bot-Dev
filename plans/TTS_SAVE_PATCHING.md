# Patching a TTS save so the bridge can act — 2026-09-09

Written because this cost most of an evening. Telemetry was flowing, `!gamedata localhost` was
being typed repeatedly and correctly, and nothing ever executed — and the reason was not on the
wire at all.

## The one thing to understand

**Two different channels, and only one of them is part of the stock mod.**

| channel | what drives it | what it does |
|---|---|---|
| telemetry upload | the mod's own game-data helper, started by `!gamedata localhost` | POSTs the board to the bridge |
| command poll | **this project's Lua, patched into the save** | reads `/poll` and physically moves pieces |

A fresh game has the first and not the second. So uploads arrive, captures are written, the import
works perfectly — and every command queues and sits there for ever. Nothing about that looks like a
missing script, which is why it is worth writing down.

**Diagnosis in one request.** If `GET /` shows uploads climbing while `GET /turn` says *no executor
poll has reported a turn yet*, the save is not patched. `observe_turn` stamps a timestamp on every
poll unconditionally, so a live executor always advances it; even the executor's failure path keeps
polling every 10 seconds and logs `bridge unreachable`. Total silence means the Lua is not there.

## Patching

```powershell
# 1. Save the game in TTS under a name you can find, then:
cd D:\Projects\ti4-engine
$env:PYTHONDONTWRITEBYTECODE = "1"          # never write caches into the pinned repo
python tools\patch_save.py TS_Save_55 --check   # reports, changes nothing
python tools\patch_save.py TS_Save_55           # patches all four, then verifies from disk
```

`--check` prints one line per object. All four must read `patched, and current`:

```
  3c5667: patched, and current (bridge_executor.lua)
  6bcc6f: patched, and current (bridge_system_helper.lua)
  7c37cc: patched, and current (bridge_strategy_helper.lua)
  a3388c: patched, and current (bridge_explore_helper.lua)
```

**Then load the save in TTS.** Patching rewrites the file; a running game will not pick it up.

Saves live in `%USERPROFILE%\Documents\My Games\Tabletop Simulator\Saves`. The tool writes its own
`*.pre-bridge-backup`, and it is re-runnable, but taking your own copy first costs nothing.

## Why all four, and the failure that looks like success

From the executor's own header, and worth quoting because it predicts a genuinely confusing bug:

> FOUR OBJECTS NEED PATCHING, and installing only this one leaves a game that looks like it works.
> Telemetry flows, polling runs, ping replies, `move` and `land` succeed — and then `activate` and
> `control` fail with `missing TI4_SYSTEM_HELPER.bridgeActivate`.

`bridge_executor.lua` goes into `TI4_GAME_DATA_HELPER`; `bridge_system_helper.lua` into
`TI4_SYSTEM_HELPER` (it needs that helper's locals); `bridge_strategy_helper.lua` into
`TI4_STRATEGY_CARD_HELPER`; `bridge_explore_helper.lua` into the explore helper. Use the tool
rather than pasting by hand — it installs each before that object's globals lock and verifies the
result.

## Known-good saves

| save | state |
|---|---|
| `TS_Save_52` | patched and current — the one that worked all along |
| `TS_Save_55` | patched 2026-09-09 |
| autosaves | never patched; TTS writes them from the running game |

## The ordering collision, separately

Even with a patched save, one more thing can look like a dead bridge: **restarting `run_bridge.py`
after the last upload.** `!gamedata localhost` fires once immediately and then runs on a timer, and
the mod suppresses any upload whose CRC matches the last one. So if the bridge restarts while the
table is motionless, every later tick is suppressed and the new process sits at `uploads=0` for
ever. Re-type `!gamedata localhost` (it bypasses the CRC check on its immediate fire) or move a
piece.

`scripts/play_mlp.ps1 -Go` now pings the queue and refuses to play if nothing drains it, so a turn
can no longer be queued into a channel that is not listening.
