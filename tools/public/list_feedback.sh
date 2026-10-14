#!/usr/bin/env bash
# Print recent playtest feedback reports in the terminal, newest first.
#
#   tools/public/list_feedback.sh [-n 20] [-v] [DATA_DIR]
#
#   tools/public/list_feedback.sh -s ID [-o FILE] [DATA_DIR]
#
# -n N  how many reports (default 20)      -v  also print the context JSON (without the save blob)
# -s ID write the save blob of an offline report (its fb_... id or client report id) to FILE or
#       stdout; replay it with: cargo run -p ti4-wasm --example replay_save -- FILE
# Offline reports show a "source: offline | seed | build | decisions" line.
# DATA_DIR defaults to $TI4_DATA_DIR or /var/lib/ti4-public.
# Uses the server binary when present, otherwise falls back to jq over the JSON files.
set -euo pipefail

limit=20
verbose=""
save_id=""
out_file=""
while getopts "n:vs:o:" opt; do
  case "$opt" in
    n) limit="$OPTARG" ;;
    v) verbose="--verbose" ;;
    s) save_id="$OPTARG" ;;
    o) out_file="$OPTARG" ;;
    *) echo "usage: $0 [-n N] [-v] [-s ID [-o FILE]] [DATA_DIR]" >&2; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
data_dir="${1:-${TI4_DATA_DIR:-/var/lib/ti4-public}}"

# The deployed binary may predate offline reports (and its unknown subcommands start a server),
# so -s only uses an explicit $TI4_SERVER_BIN and otherwise jq.
bins=("${TI4_SERVER_BIN:-}")
[[ -z "$save_id" ]] && bins+=(/opt/ti4-public/current/server)
for bin in "${bins[@]}"; do
  if [[ -n "$bin" && -x "$bin" ]]; then
    if [[ -n "$save_id" ]]; then
      exec "$bin" feedback-save --data-dir "$data_dir" ${out_file:+--out "$out_file"} "$save_id"
    fi
    exec "$bin" feedback-list --data-dir "$data_dir" --limit "$limit" $verbose
  fi
done

command -v jq >/dev/null || { echo "need the server binary or jq" >&2; exit 1; }
if [[ -n "$save_id" ]]; then
  for f in "$data_dir"/feedback/*/*.json; do
    [[ -e "$f" ]] || continue
    if jq -e --arg id "$save_id" '.id == $id or .report_id == $id' "$f" >/dev/null; then
      jq -e '.context.local.save' "$f" > "${out_file:-/dev/stdout}" || { echo "not an offline report" >&2; exit 1; }
      exit 0
    fi
  done
  echo "no report $save_id" >&2; exit 1
fi
# shellcheck disable=SC2012
ls -1t "$data_dir"/feedback/*/*.json 2>/dev/null | head -n "$limit" | while read -r f; do
  jq -r '"=== \(.id) | \(.received_at) | \(.category | ascii_upcase) | client \(.context.build_commit // "-") | game \(.context.game_id // "-") | seat \(.context.viewer_seat // "-") | round \(.context.round // "-") \(.context.phase // "")\n\(if .context.local then "    source: offline | seed \(.context.local.seed) | build \(.context.local.engine.build // "?") | decisions \(.context.local.save.decisions // "?") | answers \(.context.local.answers | length)\n" else "" end)\(if .context.sender or .context.players then "    from \(.context.sender.nickname // .context.sender.typed_name // (if .context.sender.lobby_nickname then "lobby name " + .context.sender.lobby_nickname else "?" end))" + (if .context.players then " | table: " + ([.context.players[] | "\(.nickname // "?")\(if .is_sender then "*" else "" end) (\(.faction // "?"))"] | join(", ")) else "" end) + "\n" else "" end)\(.text)\n    file: '"$f"'\n"' "$f"
  if [[ -n "$verbose" ]]; then jq '.context | if .local then .local |= (.save = "<omitted>" | .answers = "<omitted>" | .setup = "<omitted>") else . end' "$f"; fi
done
