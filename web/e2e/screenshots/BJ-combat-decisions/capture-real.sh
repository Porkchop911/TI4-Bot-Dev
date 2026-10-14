#!/usr/bin/env bash
# Regenerates out/<scenario>/*.png from REAL engine games: the Playwright smoke playthrough (random,
# seeded UI clicks) runs a start preset and, the first time each listed decision subtype is shown,
# saves the deciding seat's page before answering and again right after, at 1440x900 and 390x844
# (TI4_SMOKE_SHOT_SUBTYPES / TI4_SMOKE_SHOT_DIR, see web/e2e/smokePlaythrough.ts).
#
#   cd web && e2e/screenshots/BJ-combat-decisions/capture-real.sh [port-base]
#   ONLY=capture ...    run one scenario
#
# The click seed alone does not make a run repeat exactly (server timing changes which control
# the random policy meets), so each scenario tries a list of click seeds and keeps, per subtype,
# the first attempt that showed it. Needs a built server (cargo build -p ti4-server --bin server;
# set CARGO_TARGET_DIR if you use one). One game and browser at a time; scratch under DATA_ROOT.
set -u
cd "$(dirname "$0")/../../.." || exit 1
HERE="e2e/screenshots/BJ-combat-decisions"
PORT="${1:-42900}"
DATA_ROOT="${DATA_ROOT:-$(pwd)/../nightly-reports/bj-data}"
mkdir -p "$HERE/out" "$DATA_ROOT"

# run <name> <preset> <players> <game_seed> "<click seeds>" <subtypes> <max decisions>
run() {
  local name="$1" preset="$2" players="$3" gseed="$4" cseeds="$5" subtypes="$6" decisions="$7"
  [ -z "${ONLY:-}" ] || [ "$ONLY" = "$name" ] || return 0
  local out="$HERE/out/$name" cs st missing
  mkdir -p "$out"
  for cs in $cseeds; do
    missing=""
    for st in ${subtypes//,/ }; do ls "$out/$st"-*-after-phone.png >/dev/null 2>&1 || missing="$missing,$st"; done
    missing="${missing#,}"
    [ -n "$missing" ] || break
    echo "== $name: $preset, game seed $gseed, click seed $cs, wanting $missing"
    rm -rf "$DATA_ROOT/$name" "$out/.try"
    TI4_SMOKE=1 TI4_SMOKE_PREP=0 TI4_SMOKE_POLICY=steer TI4_SMOKE_CARD_SET=pok \
      TI4_SMOKE_PRESET="$preset" TI4_SMOKE_PLAYERS="$players" \
      TI4_SMOKE_GAME_SEED="$gseed" TI4_SMOKE_CLICK_SEED="$cs" TI4_SMOKE_DECISIONS="$decisions" \
      TI4_SMOKE_SHOT_SUBTYPES="$missing" TI4_SMOKE_SHOT_STOP=1 TI4_SMOKE_SHOT_DIR="$out/.try" \
      TI4_E2E_BACKEND_PORT="$PORT" TI4_E2E_FRONTEND_PORT="$((PORT + 1))" \
      TI4_E2E_DATA_DIR="$DATA_ROOT/$name" \
      nice -n 19 npx playwright test e2e/smoke_playthrough.spec.ts --global-timeout=0 --reporter=line \
      --output="$DATA_ROOT/pw-$name" > "$DATA_ROOT/$name-$cs.log" 2>&1
    for st in ${missing//,/ }; do
      # keep the first sighting of each subtype (its before and after shots together)
      if ls "$out/.try/$st"-*-after-phone.png >/dev/null 2>&1; then
        mv "$out/.try/$st"-*.png "$out/"
        echo "   got $st (click seed $cs)"
      fi
    done
    rm -rf "$out/.try"
  done
}

run siege   siege   3 11 "4 6 2 7 8 9 10" assault_cannon_destroy,reaction_when_ANTI_FIGHTER_BARRAGE_STARTED,courageous_to_the_end_assign_casualty 150
run bombard bombard 3 21 "4 2 3 5 6" bombardment_target 80
run capture capture 3 32 "6 4 5 7 8" vortex_target,stillness_target 200
