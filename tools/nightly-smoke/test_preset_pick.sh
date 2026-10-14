#!/usr/bin/env bash
# Checks run_game.sh's preset choice with a stub in place of the real playthrough.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/tools/nightly-smoke"
cp "$here/config.sh" "$here/run_game.sh" "$here/coverage.py" "$tmp/tools/nightly-smoke/"
printf '#!/usr/bin/env bash\nexit 0\n' > "$tmp/tools/nightly-smoke/_run_inner.sh"
chmod +x "$tmp/tools/nightly-smoke/"*.sh
# A repo to record the commit from (the run's meta.json says which code it played).
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
git -C "$tmp" init -q -b main && echo hi > "$tmp/README" && git -C "$tmp" add README && git -C "$tmp" commit -q -m init

# The first checks pin one preset and no rotation; the list checks below set their own.
start() { # start <probability> <run-dir>
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY="$1" NIGHTLY_PRESET=combat NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    "$tmp/tools/nightly-smoke/run_game.sh" start "$2" > /dev/null
}

fail() { echo "FAIL: $*" >&2; exit 1; }

# The suffix knobs below are checked on their own; everywhere else they stay off so the checks see
# plain preset names.
export NIGHTLY_PRESET_FACTIONS_PERCENT=0 NIGHTLY_PRESET_SHORT_PERCENT=0 NIGHTLY_EXPLORE_MAP_PERCENT=0 NIGHTLY_PRESET_AGENDA_PERCENT=0

start 100 "$tmp/always"
grep -q '"preset": "combat"' "$tmp/always/meta.json" || fail "100% should pick the combat preset"
grep -q 'TI4_SMOKE=1 TI4_SMOKE_PRESET=combat ' "$tmp/always/meta.json" || fail "repro should carry the preset"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/always/meta.json" || fail "meta.json must stay valid JSON"

head_commit=$(git -C "$tmp" rev-parse HEAD)
grep -q "\"commit\": \"$head_commit\"" "$tmp/always/meta.json" || fail "meta.json should record the commit under test"
python3 -c "import json,sys; m=json.load(open(sys.argv[1])); assert isinstance(m['dirty_files'], int) and m['dirty_files'] >= 1" "$tmp/always/meta.json" \
  || fail "meta.json should count the files that differ from the commit (the copied tools dir is untracked here)"

start 0 "$tmp/never"
grep -q '"preset": ""' "$tmp/never/meta.json" || fail "0% should pick no preset"
grep -q 'TI4_SMOKE_PRESET' "$tmp/never/meta.json" && fail "repro must not mention a preset when none was picked"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/never/meta.json" || fail "meta.json must stay valid JSON"

hits=0
for i in $(seq 1 60); do
  start 50 "$tmp/half$i"
  grep -q '"preset": "combat"' "$tmp/half$i/meta.json" && hits=$((hits + 1))
done
[ "$hits" -ge 15 ] && [ "$hits" -le 45 ] || fail "50% picked the preset $hits/60 times"
echo "ok: preset pick (50% picked $hits/60)"

# A list picks each member over many runs, and the rotation suffix goes on about as often as asked.
declare -A picked
rot=0
for i in $(seq 1 80); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="cards agenda relics" NIGHTLY_PRESET_ROTATE_PERCENT=50 \
    "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/list$i" > /dev/null
  name=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'])" "$tmp/list$i/meta.json")
  base=${name%+rot}
  picked[$base]=1
  case "$name" in *+rot) rot=$((rot + 1)) ;; esac
  grep -q "TI4_SMOKE_PRESET=$name " "$tmp/list$i/meta.json" || fail "repro should carry $name"
done
[ "${#picked[@]}" -eq 3 ] || fail "the list should reach all three presets, reached ${!picked[*]}"
[ "$rot" -ge 20 ] && [ "$rot" -le 60 ] || fail "50% rotation was added $rot/80 times"
# leaders always rotates inside the preset, so it never gets the suffix.
for i in $(seq 1 10); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET=leaders NIGHTLY_PRESET_ROTATE_PERCENT=100 \
    "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/lead$i" > /dev/null
  grep -q '"preset": "leaders"' "$tmp/lead$i/meta.json" || fail "leaders must not get +rot"
done
echo "ok: preset list and rotation (+rot $rot/80)"

# Roster factions (+fac), short games (+short) and the explore map: each suffix goes on about as
# often as asked, only where it belongs, and never collides with +rot or the `world` preset.
fac=0 short=0 rotfac=0
for i in $(seq 1 80); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="cards" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PRESET_FACTIONS_PERCENT=50 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/fac$i" > /dev/null
  name=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'])" "$tmp/fac$i/meta.json")
  case "$name" in cards+fac) fac=$((fac + 1)) ;; cards) ;; *) fail "unexpected preset $name" ;; esac
  grep -q "TI4_SMOKE_PRESET=$name " "$tmp/fac$i/meta.json" || fail "repro should carry $name"
done
[ "$fac" -ge 20 ] && [ "$fac" -le 60 ] || fail "50% roster factions was added $fac/80 times"
for i in $(seq 1 20); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="cards" NIGHTLY_PRESET_ROTATE_PERCENT=100 \
    NIGHTLY_PRESET_FACTIONS_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/rotfac$i" > /dev/null
  grep -q '"preset": "cards+rot"' "$tmp/rotfac$i/meta.json" || fail "+rot must not also get +fac"
done
for i in $(seq 1 10); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="world" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PRESET_FACTIONS_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/world$i" > /dev/null
  grep -q '"preset": "world"' "$tmp/world$i/meta.json" || fail "world already seats roster factions and gets no suffix"
done
for i in $(seq 1 60); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="techs relics" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PRESET_SHORT_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/short$i" > /dev/null
  name=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'])" "$tmp/short$i/meta.json")
  case "$name" in techs+short) short=$((short + 1)) ;; relics) ;; *) fail "short should only go on techs, got $name" ;; esac
done
[ "$short" -ge 15 ] || fail "100% short went on techs only $short/60 times"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/short1/meta.json" || fail "meta.json must stay valid JSON"
NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="agenda" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
  NIGHTLY_PRESET_FACTIONS_PERCENT=100 NIGHTLY_PRESET_SHORT_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/both" > /dev/null
grep -q '"preset": "agenda+fac+short"' "$tmp/both/meta.json" || fail "both suffixes should stack as agenda+fac+short"
# explore plays the frontier map at three seats only.
maps=0
for i in $(seq 1 60); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="explore" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PLAYER_COUNTS="3 4" NIGHTLY_EXPLORE_MAP_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/map$i" > /dev/null
  players=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['players'])" "$tmp/map$i/meta.json")
  if [ "$players" = 3 ]; then
    grep -q '"map_template": "3pInPersonHyperlanes"' "$tmp/map$i/meta.json" || fail "explore at 3 seats should play the frontier map"
    grep -q 'TI4_SMOKE_MAP_TEMPLATE=3pInPersonHyperlanes ' "$tmp/map$i/meta.json" || fail "repro should carry the map template"
    maps=$((maps + 1))
  else
    grep -q '"map_template": ""' "$tmp/map$i/meta.json" || fail "explore at $players seats keeps the default map"
  fi
done
[ "$maps" -ge 10 ] || fail "explore at three seats got the frontier map only $maps times"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/map1/meta.json" || fail "meta.json must stay valid JSON"
echo "ok: suffix knobs (+fac $fac/80, +short $short/60, frontier map $maps/60)"

# The default preset list reaches the new presets.
declare -A seen_default
for i in $(seq 1 300); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET_ROTATE_PERCENT=0 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/def$i" > /dev/null
  seen_default[$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'].split('+')[0])" "$tmp/def$i/meta.json")]=1
done
for want in combat cards agenda relics invasion techs leaders explore notes world; do
  [ -n "${seen_default[$want]:-}" ] || fail "the default list never picked $want"
done
echo "ok: default preset list reaches every preset"

# The strategy card set: Thunder's Edge by default (nothing in the repro), pok on part of the runs.
NIGHTLY_REPO="$tmp" NIGHTLY_POK_PROBABILITY=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/pok" > /dev/null
grep -q '"card_set": "pok"' "$tmp/pok/meta.json" || fail "100% should pick pok"
grep -q 'TI4_SMOKE_CARD_SET=pok ' "$tmp/pok/meta.json" || fail "repro should carry the pok card set"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/pok/meta.json" || fail "meta.json must stay valid JSON"
NIGHTLY_REPO="$tmp" NIGHTLY_POK_PROBABILITY=0 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/te" > /dev/null
grep -q '"card_set": "te"' "$tmp/te/meta.json" || fail "0% should keep te"
grep -q 'TI4_SMOKE_CARD_SET' "$tmp/te/meta.json" && fail "repro must not mention the card set for the default te"
pok=0
for i in $(seq 1 80); do
  NIGHTLY_REPO="$tmp" "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/cs$i" > /dev/null
  grep -q '"card_set": "pok"' "$tmp/cs$i/meta.json" && pok=$((pok + 1))
done
[ "$pok" -ge 6 ] && [ "$pok" -le 36 ] || fail "the default 25% picked pok $pok/80 times"
echo "ok: card set pick (pok $pok/80)"

# Optional UI exercises: tour, recap and redo each have a probability knob; the repro carries the switch.
for knob in UI_TOUR:ui-tour:TI4_SMOKE_UI_TOUR RECAP:recap:TI4_SMOKE_RECAP REDO:redo:TI4_SMOKE_REDO; do
  IFS=: read -r name label envname <<< "$knob"
  var="NIGHTLY_${name}_PROBABILITY"
  env "$var=100" NIGHTLY_REPO="$tmp" "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/ex-$label-on" > /dev/null
  grep -q "$envname=1 " "$tmp/ex-$label-on/meta.json" || fail "$var=100 should put $envname=1 in the repro"
  grep -q "\"exercises\": \".*$label" "$tmp/ex-$label-on/meta.json" || fail "$var=100 should list $label in meta.json"
  env "$var=0" NIGHTLY_REPO="$tmp" "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/ex-$label-off" > /dev/null
  grep -q "$envname" "$tmp/ex-$label-off/meta.json" && fail "$var=0 must not enable $envname"
done
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/ex-ui-tour-on/meta.json" || fail "meta.json must stay valid JSON with exercises"
tour=0 recap=0 redo=0
for i in $(seq 1 120); do
  NIGHTLY_REPO="$tmp" "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/exd$i" > /dev/null
  grep -q 'TI4_SMOKE_UI_TOUR=1' "$tmp/exd$i/meta.json" && tour=$((tour + 1))
  grep -q 'TI4_SMOKE_RECAP=1' "$tmp/exd$i/meta.json" && recap=$((recap + 1))
  grep -q 'TI4_SMOKE_REDO=1' "$tmp/exd$i/meta.json" && redo=$((redo + 1))
done
[ "$tour" -ge 5 ] && [ "$tour" -le 40 ] || fail "the default 17% picked the tour $tour/120 times"
[ "$recap" -ge 8 ] && [ "$recap" -le 45 ] || fail "the default 20% picked recap $recap/120 times"
[ "$redo" -le 16 ] || fail "the default 5% picked redo $redo/120 times"
echo "ok: exercise knobs (tour $tour, recap $recap, redo $redo of 120)"

# Secondary pre-planning knobs: defaults and overrides reach meta.json and the repro; 0 switches it off.
NIGHTLY_REPO="$tmp" "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/prep-default" > /dev/null
grep -q '"prep_probability": "0.5"' "$tmp/prep-default/meta.json" || fail "default prep probability should be 0.5"
grep -q 'TI4_SMOKE_PREP_AUTO_PROBABILITY=0.9 ' "$tmp/prep-default/meta.json" || fail "default auto probability should be 0.9 in the repro"
NIGHTLY_REPO="$tmp" NIGHTLY_PREP_PROBABILITY=1 NIGHTLY_PREP_AUTO_PROBABILITY=0.25 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/prep-set" > /dev/null
grep -q 'TI4_SMOKE_PREP_PROBABILITY=1 TI4_SMOKE_PREP_AUTO_PROBABILITY=0.25 ' "$tmp/prep-set/meta.json" || fail "overridden prep knobs should appear in the repro"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/prep-set/meta.json" || fail "meta.json must stay valid JSON with prep knobs"
echo "ok: prep knobs"

# The +agenda suffix: goes on eligible presets only (not agenda, notes, endgame) and stacks with the
# other suffixes in the repro.
agenda=0
for i in $(seq 1 60); do
  NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET="cards agenda notes endgame" NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PRESET_AGENDA_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/ag$i" > /dev/null
  name=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'])" "$tmp/ag$i/meta.json")
  case "$name" in cards+agenda) agenda=$((agenda + 1)) ;; agenda|notes|endgame) ;; *) fail "unexpected agenda pick $name" ;; esac
  grep -q "TI4_SMOKE_PRESET=$name " "$tmp/ag$i/meta.json" || fail "repro should carry $name"
done
[ "$agenda" -ge 4 ] || fail "100% +agenda went on cards only $agenda/60 times"
NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET=techs NIGHTLY_PRESET_ROTATE_PERCENT=0 NIGHTLY_PRESET_FACTIONS_PERCENT=100 \
  NIGHTLY_PRESET_SHORT_PERCENT=100 NIGHTLY_PRESET_AGENDA_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/agall" > /dev/null
grep -q '"preset": "techs+fac+short+agenda"' "$tmp/agall/meta.json" || fail "suffixes should stack as techs+fac+short+agenda"
NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET=techs NIGHTLY_PRESET_ROTATE_PERCENT=0 NIGHTLY_PRESET_AGENDA_PERCENT=0 \
  "$tmp/tools/nightly-smoke/run_game.sh" start "$tmp/agnone" > /dev/null
grep -q '"preset": "techs"' "$tmp/agnone/meta.json" || fail "0% +agenda must leave the name plain"

# Coverage-aware: tonight's first runs without a vote, or a previous night without one, force +agenda.
mkfake() { # mkfake <night-dir> <runs> <voting runs>
  for r in $(seq 1 "$2"); do
    mkdir -p "$1/runs/r$r/trace"
    if [ "$r" -le "$3" ]; then echo '{"subtypes": {"cast_vote": 2}}' > "$1/runs/r$r/trace/report.json"
    else echo '{"subtypes": {"play_card": 2}}' > "$1/runs/r$r/trace/report.json"; fi
  done
  mkdir -p "$1/runs/new"
}
rr="$tmp/reports"
agenda_pick() { # agenda_pick <night>: a techs run at 0% base probability
  rm -f "$rr/$1/runs/new/run.pid"
  NIGHTLY_REPORT_ROOT="$rr" NIGHTLY_REPO="$tmp" NIGHTLY_PRESET_PROBABILITY=100 NIGHTLY_PRESET=techs NIGHTLY_PRESET_ROTATE_PERCENT=0 \
    NIGHTLY_PRESET_AGENDA_PERCENT=0 NIGHTLY_AGENDA_BOOST_PERCENT=100 "$tmp/tools/nightly-smoke/run_game.sh" start "$rr/$1/runs/new" > /dev/null
  python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['preset'])" "$rr/$1/runs/new/meta.json"
}
mkfake "$rr/2026-10-01" 6 6
mkfake "$rr/2026-10-02" 6 0
mkfake "$rr/2026-10-03" 6 3
mkfake "$rr/2026-10-04" 6 3
mkfake "$rr/2026-10-05" 3 0
[ "$(agenda_pick 2026-10-02)" = techs+agenda ] || fail "6 runs tonight without votes should force +agenda"
[ "$(agenda_pick 2026-10-03)" = techs+agenda ] || fail "a previous night without votes should boost +agenda"
[ "$(agenda_pick 2026-10-04)" = techs ] || fail "voting nights before and now keep the base share"
[ "$(agenda_pick 2026-10-05)" = techs ] || fail "3 runs without votes are too few to force it"
[ "$(agenda_pick 2026-10-01)" = techs ] || fail "the first night has nothing to learn from"
echo "ok: +agenda suffix and coverage-aware pick (+agenda on cards $agenda/60)"

# Web build share: default 0 keeps every run on the dev server; 100 puts every run on the build.
web_pick() { # web_pick <share> <run-dir>
  NIGHTLY_WEB_BUILD_SHARE="$1" start 0 "$2"
  python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['web'])" "$2/meta.json"
}
[ "$(web_pick 0 "$tmp/web0")" = dev ] || fail "share 0 must keep the dev server"
grep -q 'TI4_E2E_WEB' "$tmp/web0/meta.json" && fail "dev runs must not mention TI4_E2E_WEB in the repro"
[ "$(web_pick 100 "$tmp/web100")" = build ] || fail "share 100 must pick the production build"
grep -q 'cd web && TI4_E2E_WEB=build TI4_SMOKE=1' "$tmp/web100/meta.json" || fail "a build run's repro must carry TI4_E2E_WEB=build"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tmp/web100/meta.json" || fail "meta.json must stay valid JSON"
builds=0
for i in $(seq 1 60); do [ "$(web_pick 30 "$tmp/web30-$i")" = build ] && builds=$((builds + 1)); done
[ "$builds" -ge 5 ] && [ "$builds" -le 35 ] || fail "share 30 picked $builds/60 build runs"
echo "ok: web build share (30% picked $builds/60)"
