# Nightly UI smoke sweep configuration. Every value can be overridden from the environment,
# which is how the short test run shrinks the window (see README.md).

REPO="${NIGHTLY_REPO:-/root/TI4-Bot-Dev}"
NIGHTLY_DIR="$REPO/tools/nightly-smoke"
REPORT_ROOT="${NIGHTLY_REPORT_ROOT:-$REPO/nightly-reports}"

# Window in Berlin local time. The sweep starts at START (one day) and runs until END (the next
# morning); it stops launching runs so that the last proctor can finish by END. A night is named
# after the Berlin date on which its window started.
NIGHTLY_TZ="${NIGHTLY_TZ:-Europe/Berlin}"
START_HHMM="${NIGHTLY_START:-20:30}"
END_HHMM="${NIGHTLY_END:-06:00}"
# Nights that started before this date (YYYY-MM-DD) are skipped by `tick`, so a new schedule never
# switches a checkout people are working in half way through a day.
NOT_BEFORE="${NIGHTLY_NOT_BEFORE:-2026-10-06}"

# Opus fix rounds. FIXERS lists the rounds that run ("1 2", "2", "" to disable). Round 1 starts
# at FIX1_HHMM (same Berlin day as the window start) while the sweep keeps running; round 2 starts
# once the sweep has ended. The morning summary waits for round 2.
FIXERS="${NIGHTLY_FIXERS-1 2}" # no colon: NIGHTLY_FIXERS="" really disables both rounds
FIXER_MODEL="${NIGHTLY_FIXER_MODEL:-claude-opus-5-5}"
FIX1_HHMM="${NIGHTLY_FIX1:-23:59}"
FIX1_MAX_SECONDS="${NIGHTLY_FIX1_MAX_SECONDS:-14400}"
FIX2_MAX_SECONDS="${NIGHTLY_FIX2_MAX_SECONDS:-10800}"
MAX_FIXES_PER_ROUND="${NIGHTLY_MAX_FIXES:-6}"
# The next night's sweep waits (at most this long past START) for the previous night's round 2
# and summary, which use the same checkout.
START_DEFER_SECONDS="${NIGHTLY_START_DEFER_SECONDS:-7200}"
# Checks and builds for the night branch, run before every game and to verify merged fixes.
BUILD_CMD="${NIGHTLY_BUILD_CMD:-cargo build --quiet -p ti4-server --bin server}"

# Each run plays until this round (or game over / first failure).
# Games end by themselves in round 9 (the public objectives run out, rule 81.2). Stopping at round 9
# cut every game off before that end-of-game path (final scoring, the VP tie-break), so the limit is
# round 10, which only a runaway game reaches: runs normally stop on game over.
STOP_ROUND="${NIGHTLY_STOP_ROUND:-10}"
MAX_DECISIONS="${NIGHTLY_MAX_DECISIONS:-20000}"
PLAYER_COUNTS="${NIGHTLY_PLAYER_COUNTS:-3 4 6}"
POLICIES="${NIGHTLY_POLICIES:-steer random}"
# Share of runs (percent) that start from a prepared state with fleets beside home systems and
# Mecatol Rex, so combat, casualties and the agenda phase show up early (see ti4-server preset.rs).
PRESET_PROBABILITY="${NIGHTLY_PRESET_PROBABILITY:-70}"
# One name, or a space-separated list to pick from per run (a name twice counts twice). `combat`
# is listed twice: it is the one that reaches the custodians and the agenda phase early. `endgame`
# is left out of the default mix because it ends the game within a round; run it on purpose
# (NIGHTLY_PRESET=endgame). The other presets: cards, agenda, relics, invasion, techs, leaders,
# siege (PDS colonies with garrisons), bombard, capture (Cabal seat), explore (planet and frontier
# exploration, fragments), notes (promissory notes in foreign hands) and world (Prophecy of Kings
# factions beyond the original six, seated by seed, with their faction techs, leaders, flagship and
# notes). `world` is listed twice: every roster faction should sit at a table at least once a night
# or two.
PRESET_NAME="${NIGHTLY_PRESET:-combat combat cards agenda relics invasion techs leaders siege bombard capture explore notes world world}"
# Share of preset runs (percent) that also rotate the factions ("<preset>+rot": Jol-Nar and L1Z1X
# at three and four seats); `leaders` always rotates.
PRESET_ROTATE_PERCENT="${NIGHTLY_PRESET_ROTATE_PERCENT:-20}"
# Share of preset runs (percent, those not rotated, `world` always) that seat factions from the
# engine's roster by seed instead of the original six ("<preset>+fac"; see preset.rs ROSTER).
PRESET_FACTIONS_PERCENT="${NIGHTLY_PRESET_FACTIONS_PERCENT:-25}"
# Presets that start near their end ("<preset>+short": a few victory points each, three public
# objectives left), so a thin preset's game finishes in a few rounds, and how often they do.
SHORT_PRESETS="${NIGHTLY_SHORT_PRESETS:-techs agenda leaders notes}"
PRESET_SHORT_PERCENT="${NIGHTLY_PRESET_SHORT_PERCENT:-70}"
# Share of the eligible preset runs (percent) that also get "<preset>+agenda": the custodians token
# is gone from the start, so every round has an agenda phase. Naturally lifting the custodians
# (Mecatol Rex taken, six influence) is rare in random play, so without this the agenda phase was
# a lottery (2026-10-09: votes in 5 of 22 runs). With 70% preset runs and the list below about half
# of all runs get an agenda phase. Not eligible: presets that already have one (agenda, notes) and
# endgame (the game ends within a round). The pick is raised to AGENDA_BOOST_PERCENT when the last
# night with enough runs had no agenda phase at all, and to 100 when this night's first
# AGENDA_NIGHT_MIN runs had none (coverage.py --agenda-runs).
# Percent of runs (0..100) that play against a production web build (vite build + vite preview, cached
# per commit: web/scripts/e2e-web-build.sh) instead of the vite dev server. 0 = every run on the dev
# server, as before. The choice is recorded as "web" in meta.json and shown in the digest.
WEB_BUILD_SHARE="${NIGHTLY_WEB_BUILD_SHARE:-0}"
PRESET_AGENDA_PERCENT="${NIGHTLY_PRESET_AGENDA_PERCENT:-65}"
AGENDA_EXCLUDE="${NIGHTLY_AGENDA_EXCLUDE:-agenda notes endgame}"
AGENDA_BOOST_PERCENT="${NIGHTLY_AGENDA_BOOST_PERCENT:-90}"
AGENDA_NIGHT_MIN="${NIGHTLY_AGENDA_NIGHT_MIN:-6}"
# `explore` at three seats plays this map in part of the runs (percent): it is the only map of
# the default size with planetless systems that are not hyperlanes, so the only one with frontier
# tokens off the hyperlane tiles (every default 3-, 4- and 5-player map puts them on hyperlanes).
EXPLORE_MAP_TEMPLATE="${NIGHTLY_EXPLORE_MAP_TEMPLATE:-3pInPersonHyperlanes}"
EXPLORE_MAP_PERCENT="${NIGHTLY_EXPLORE_MAP_PERCENT:-30}"
# Share of runs (percent) that play the Prophecy of Kings strategy cards (`strategy_card_set` "pok")
# instead of the default Thunder's Edge set (TE Warfare offers an extra redistribute decision).
POK_PROBABILITY="${NIGHTLY_POK_PROBABILITY:-25}"
# Optional UI exercises, each switched on for a share of runs (percent) through an env switch of the
# harness (web/e2e/smokeExercises.ts): the "ui tour" opens one unit card and the Faction card
# (TI4_SMOKE_UI_TOUR, about 1 run in 6), the recap toggle is turned on for one random non-host seat
# (TI4_SMOKE_RECAP), one guarded "Redo my last turn" round trip (TI4_SMOKE_REDO, rare: the redo
# replays the round). Set a probability to 0 to switch an exercise off.
UI_TOUR_PROBABILITY="${NIGHTLY_UI_TOUR_PROBABILITY:-17}"
RECAP_PROBABILITY="${NIGHTLY_RECAP_PROBABILITY:-20}"
REDO_PROBABILITY="${NIGHTLY_REDO_PROBABILITY:-5}"
# Secondary pre-planning (web/e2e/smokePrep.ts), a fraction 0..1: the chance a waiting follower plans
# its strategy-card secondary per opportunity, and the chance a plan uses the Auto mode (else Review).
# NIGHTLY_PREP_PROBABILITY=0 switches the exercise off (it passes TI4_SMOKE_PREP=0).
PREP_PROBABILITY="${NIGHTLY_PREP_PROBABILITY:-0.5}"
PREP_AUTO_PROBABILITY="${NIGHTLY_PREP_AUTO_PROBABILITY:-0.9}"
# Backend port range of the games (the frontend takes port + 1).
PORT_MIN="${NIGHTLY_PORT_MIN:-20000}"
PORT_MAX="${NIGHTLY_PORT_MAX:-49000}"
# Cap on screenshots a run saves to trace/shots (TI4_SMOKE_SHOT_CAP; a number or "all" = 20).
SHOT_CAP="${NIGHTLY_SHOT_CAP:-20}"
# Free space (GB) needed on the report filesystem to start a run or a fixer round. Below it the loop
# waits (DISK_WAIT_SECONDS between checks) and gives the night up after MAX_DISK_WAITS checks.
# DF_CMD is a test hook (the tests put a stub `df` here).
MIN_FREE_GB="${NIGHTLY_MIN_FREE_GB:-10}"
DISK_WAIT_SECONDS="${NIGHTLY_DISK_WAIT_SECONDS:-120}"
MAX_DISK_WAITS="${NIGHTLY_MAX_DISK_WAITS:-30}"
DF_CMD="${NIGHTLY_DF_CMD:-df}"
# Parallel games ("slots"). NIGHTLY_SLOTS=1 (default) is the classic sweep: one proctored game
# after another in the live checkout. With N >= 2 the sweep supervises N independent workers; each
# has its own git worktree of the night branch (nightly-reports/<night>/slot-K/repo, branch
# nightly-fixes-<night>-sK), its own cargo target dir (nightly-reports/target-slot-K) and its own run
# numbering (run names sK-NN-HHMM). See README.md, "Parallel slots".
SLOTS="${NIGHTLY_SLOTS:-1}"
# Slot K starts (K-1) * this long after the sweep starts, so the first builds do not all collide.
SLOT_STAGGER_SECONDS="${NIGHTLY_SLOT_STAGGER_SECONDS:-60}"
# Memory admission gate (only while another slot has a run active): a new game starts when this
# much memory is available: MEM_BASE_MB + MEM_PER_PLAYER_MB * (the largest table in PLAYER_COUNTS).
# Measured peaks: about 3 GB for three players, 4 GB for four. Starts are serialised and the next
# slot is held back for SLOT_SETTLE_SECONDS after a launch, so the memory a fresh game is still
# ramping up to is already visible when the next slot checks. MEM_AVAIL_CMD prints the available
# memory in MB (test hook).
MEM_BASE_MB="${NIGHTLY_MEM_BASE_MB:-1500}"
MEM_PER_PLAYER_MB="${NIGHTLY_MEM_PER_PLAYER_MB:-1200}"
MEM_WAIT_SECONDS="${NIGHTLY_MEM_WAIT_SECONDS:-120}"
SLOT_SETTLE_SECONDS="${NIGHTLY_SLOT_SETTLE_SECONDS:-300}"
SLOT_POLL_SECONDS="${NIGHTLY_SLOT_POLL_SECONDS:-5}"
MEM_AVAIL_CMD="${NIGHTLY_MEM_AVAIL_CMD:-awk '/^MemAvailable:/ { print int(\$2 / 1024) }' /proc/meminfo}"
# The tree the games, builds and proctors work in. The live checkout unless a slot overrides it.
GAME_REPO="${NIGHTLY_GAME_REPO:-$REPO}"
# A fixer round that fails to start (no disk, no worktree) is retried after this long, up to this
# many attempts in total.
FIX_RETRY_SECONDS="${NIGHTLY_FIX_RETRY_SECONDS:-1800}"
FIX_MAX_ATTEMPTS="${NIGHTLY_FIX_MAX_ATTEMPTS:-2}"
# Do not start a new run with less time than this left before END (seconds).
MIN_RUN_SECONDS="${NIGHTLY_MIN_RUN_SECONDS:-1800}"
# Kill a run when its seat UI makes no progress for this long (seconds); the proctor reports it.
STALL_SECONDS="${NIGHTLY_STALL_SECONDS:-900}"

CLAUDE_BIN="${NIGHTLY_CLAUDE:-/root/.local/bin/claude}"
PROCTOR_MODEL="${NIGHTLY_PROCTOR_MODEL:-claude-sonnet-5-5}"
SUMMARY_MODEL="${NIGHTLY_SUMMARY_MODEL:-claude-opus-5-5}"

export PATH="/root/.local/bin:/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
export HOME="${HOME:-/root}"

# The clock. Tests fake it: NIGHTLY_NOW_FILE (a file holding an epoch the test advances) wins over
# NIGHTLY_NOW (a fixed epoch); otherwise it is the real time.
now_epoch() {
  if [ -n "${NIGHTLY_NOW_FILE:-}" ] && [ -s "$NIGHTLY_NOW_FILE" ]; then
    cat "$NIGHTLY_NOW_FILE"
  elif [ -n "${NIGHTLY_NOW:-}" ]; then
    echo "$NIGHTLY_NOW"
  else
    date +%s
  fi
}

# Calendar helpers in Berlin time. Day arithmetic goes through the calendar (not "minus 86400"),
# so the two daylight-saving changes a year cannot move a window by an hour.
berlin_date() { TZ="$NIGHTLY_TZ" date -d "@${1:-$(now_epoch)}" +%F; }
shift_day() { TZ="$NIGHTLY_TZ" date -d "$1 12:00 $2 days" +%F; } # shift_day 2026-10-06 +1
day_epoch() { TZ="$NIGHTLY_TZ" date -d "$1 $2" +%s; }            # day_epoch 2026-10-06 09:00
hhmm_num() { printf '%s' "$1" | tr -d ':'; }

# Prints "<start-epoch> <end-epoch>" of the window that starts on Berlin date $1. The window
# crosses midnight when END is not after START.
window_for_night() {
  local start end_day="$1"
  start=$(day_epoch "$1" "$START_HHMM")
  [ "$(hhmm_num "$END_HHMM")" -le "$(hhmm_num "$START_HHMM")" ] && end_day=$(shift_day "$1" +1)
  echo "$start $(day_epoch "$end_day" "$END_HHMM")"
}

# Prints "<night-id> <start-epoch> <end-epoch>" for the window containing now, or the most recent
# window that already ended (used by the summary and the fix rounds).
current_window() {
  local now today night start end
  now=$(now_epoch)
  today=$(berlin_date "$now")
  read -r start end < <(window_for_night "$today")
  night="$today"
  if [ "$now" -lt "$start" ]; then
    night=$(shift_day "$today" -1)
    read -r start end < <(window_for_night "$night")
  fi
  echo "$night $start $end"
}

# Free space in whole GB on the filesystem holding the reports (the builds and games live there too).
free_gb() {
  mkdir -p "$REPORT_ROOT" 2>/dev/null || true
  $DF_CMD -Pk "$REPORT_ROOT" 2>/dev/null | awk 'NR == 2 { print int($4 / 1048576) }'
}
disk_ok() {
  local free
  free=$(free_gb)
  [ -n "$free" ] && [ "$free" -ge "$MIN_FREE_GB" ]
}

log() { echo "[$(TZ="$NIGHTLY_TZ" date -d "@$(now_epoch)" '+%F %T %Z')] $*"; }

# Kill every process that belongs to the run on <port>. All of a run's processes (Playwright,
# its web servers, browsers, the backend) inherit TI4_E2E_BACKEND_PORT from _run_inner.sh, which
# survives the new process groups/sessions Playwright creates and re-parenting to init.
kill_run_processes() {
  local port="$1" sig pids
  for sig in TERM KILL; do
    pids=$(grep -lsz "^TI4_E2E_BACKEND_PORT=$port\$" /proc/[0-9]*/environ 2>/dev/null \
      | cut -d/ -f3 | grep -vx -e "$$" -e "${BASHPID:-$$}" || true)
    [ -n "$pids" ] || return 0
    kill -"$sig" $pids 2>/dev/null || true
    [ "$sig" = TERM ] && sleep 5
  done
}

# ---------------------------------------------------------------------------------------------
# Host resources. Playwright starts Chromium with --disable-dev-shm-usage, which makes Chromium back
# its shared memory (mojo data pipes, compositor frames) with unlinked files in TMPDIR (/tmp). /tmp
# is a RAM-backed tmpfs here, so a /tmp that is nearly full (a cargo target dir, a copied
# node_modules, leftover game data) makes the browser refuse page modules and fetches with
# net::ERR_INSUFFICIENT_RESOURCES and crash its compositor (2026-10-08, 20:51-21:46). So a run
#  * leaves the browser its default /dev/shm backing (a separate tmpfs) when that has room
#    (TI4_E2E_DEV_SHM=1, read by web/playwright.config.ts),
#  * removes /tmp leftovers of finished runs before it starts, and
#  * records memory, swap, /tmp and /dev/shm in <run-dir>/resources.log every RESOURCE_LOG_SECONDS.
SHM_MIN_MB="${NIGHTLY_MIN_SHM_MB:-2048}"
RESOURCE_LOG_SECONDS="${NIGHTLY_RESOURCE_LOG_SECONDS:-30}"
TMP_ROOT="${NIGHTLY_TMP_ROOT:-/tmp}"
SHM_ROOT="${NIGHTLY_SHM_ROOT:-/dev/shm}"
STALE_TMP_MINUTES="${NIGHTLY_STALE_TMP_MINUTES:-90}"

# Free MB of the filesystem holding <dir> (empty when it cannot be read).
free_mb() { df -Pk "$1" 2>/dev/null | awk 'NR == 2 { print int($4 / 1024) }'; }

# True when the browser can keep its default /dev/shm backing.
dev_shm_ok() {
  local free
  free=$(free_mb "$SHM_ROOT")
  [ -n "$free" ] && [ "$free" -ge "$SHM_MIN_MB" ]
}

# One line: when, memory, swap, load, /tmp and /dev/shm use, and how many browsers and servers exist.
resource_snapshot() {
  local mem swap load tmp shm browsers servers
  mem=$(awk '/^MemAvailable:/ { a = int($2 / 1024) } /^MemTotal:/ { t = int($2 / 1024) } /^Shmem:/ { s = int($2 / 1024) } END { printf "mem_avail=%sMB/%sMB shmem=%sMB", a, t, s }' /proc/meminfo 2>/dev/null)
  swap=$(awk '/^SwapTotal:/ { t = int($2 / 1024) } /^SwapFree:/ { f = int($2 / 1024) } END { printf "swap_used=%sMB/%sMB", t - f, t }' /proc/meminfo 2>/dev/null)
  load=$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null)
  tmp=$(df -Pk "$TMP_ROOT" 2>/dev/null | awk 'NR == 2 { printf "tmp_used=%dMB/%dMB tmp_free=%dMB", $3 / 1024, ($3 + $4) / 1024, $4 / 1024 }')
  shm=$(df -Pk "$SHM_ROOT" 2>/dev/null | awk 'NR == 2 { printf "shm_used=%dMB/%dMB", $3 / 1024, ($3 + $4) / 1024 }')
  browsers=$(pgrep -fc 'chrome-headless-shell|chromium' 2>/dev/null || true)
  servers=$(pgrep -fc 'ti4-server|bin/server' 2>/dev/null || true)
  echo "$(date '+%F %T') $mem $swap load=$load $tmp $shm browsers=${browsers:-0} servers=${servers:-0}"
}

# Largest entries of the temp dir (what is filling it), for the start of a run's resources.log.
tmp_top() { du -sm "$TMP_ROOT"/* "$TMP_ROOT"/.[!.]* 2>/dev/null | sort -rn | head -"${1:-8}"; }

# Remove temp leftovers of finished runs: game data dirs and Playwright browser profiles older than
# STALE_TMP_MINUTES that no running process mentions (a live run's processes carry the path in their
# command line or environment; another agent's browser carries its own profile path).
clean_stale_tmp() {
  local dir busy pat
  for dir in "$TMP_ROOT"/ti4-playwright-games-* "$TMP_ROOT"/playwright_chromiumdev_profile-* "$TMP_ROOT"/playwright-artifacts-*; do
    [ -d "$dir" ] || continue
    [ -n "$(find "$dir" -maxdepth 0 -mmin "-$STALE_TMP_MINUTES" 2>/dev/null)" ] && continue
    # Bracketing the first character keeps this grep's own command line from matching its pattern.
    pat="[${dir:0:1}]$(printf '%s' "${dir:1}" | sed 's/[][\.*^$/]/\\&/g')"
    busy=$(grep -lsa -e "$pat" /proc/[0-9]*/cmdline /proc/[0-9]*/environ 2>/dev/null | head -1 || true)
    [ -z "$busy" ] || continue
    rm -rf -- "$dir" && echo "removed stale $dir"
  done
}
