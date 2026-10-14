#!/usr/bin/env bash
# Starts the server (and, if installed, a throwaway Caddy with deploy/public/Caddyfile) on loopback
# ports and asserts the headers a browser needs for offline play: wasm content type, immutable cache
# for hash-named assets, br/gzip for wasm, the CSP additions, no COOP/COEP. Uses a fixture web dir
# unless --web-dir is given. Nothing is installed; the live service is not touched.
#
#   deploy/public/check-headers.sh [--bin PATH/TO/server] [--web-dir DIR] [--no-caddy]
#
# Exit 0 when every assertion holds.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$REPO/target}/release/server"
WEB="" USE_CADDY=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="$2"; shift 2 ;;
    --web-dir) WEB="$2"; shift 2 ;;
    --no-caddy) USE_CADDY=0; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
[[ -x "$BIN" ]] || { echo "server binary not found: $BIN (cargo build --release -p ti4-server --bin server)" >&2; exit 2; }

work="${KEEP_WORK:-}"; work="$(mktemp -d "${TMPDIR:-/tmp}/ti4-headers.XXXXXX")"
pids=()
trap 'for p in "${pids[@]:-}"; do [[ -n "$p" ]] && kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; rm -rf "$work"' EXIT

if [[ -z "$WEB" ]]; then # fixture: index, a hash-named wasm with .gz/.br, a hash-named wasm without
  WEB="$work/dist"; mkdir -p "$WEB/assets"
  { echo '<!doctype html><title>t</title><script type=module src=/assets/app-ab12cd34.js></script>'
    for _ in $(seq 1 40); do echo '<p>padding so the page is larger than the proxy minimum length</p>'; done; } >"$WEB/index.html"
  echo 'console.log("hello from the bundle");' >"$WEB/assets/app-ab12cd34.js"
  python3 - "$WEB/assets" <<'P'
import sys, gzip, zlib, os
d = sys.argv[1]
body = b"\0asm\x01\0\0\0" + bytes(range(256)) * 400
for n in ("ti4_wasm-9f8e7d6c.wasm", "plain-11223344.wasm"):
    open(os.path.join(d, n), "wb").write(body)
open(os.path.join(d, "ti4_wasm-9f8e7d6c.wasm.gz"), "wb").write(gzip.compress(body, 9))
P
  node -e 'const z=require("zlib"),fs=require("fs"),d=process.argv[1];
    fs.writeFileSync(d+"/ti4_wasm-9f8e7d6c.wasm.br", z.brotliCompressSync(fs.readFileSync(d+"/ti4_wasm-9f8e7d6c.wasm")))' "$WEB/assets"
fi
# Prefer a wasm that has precompressed siblings (what deploy.sh produces).
WASM="$(cd "$WEB" && { for f in $(find assets -name '*.wasm'); do [[ -e "$f.gz" ]] && echo "$f"; done; find assets -name '*.wasm'; } | sed -n 1p)"
[[ -n "$WASM" ]] || { echo "no .wasm under $WEB/assets" >&2; exit 2; }

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
PORT="$(free_port)"
mkdir "$work/data"
env -i PATH="$PATH" HOME="$work" HOST=127.0.0.1 PORT="$PORT" TI4_PUBLIC=1 TI4_ACCESS_PASSWORD=x \
  TI4_COOKIE_SECRET=x TI4_DATA_DIR="$work/data" TI4_WEB_DIR="$WEB" "$BIN" >"$work/server.log" 2>&1 &
pids+=($!)
for _ in $(seq 1 50); do curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break; sleep 0.2; done

fail=0
ok()  { printf '  ok    %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; fail=1; }
hdr() { grep -i "^$2:" "$1" | tail -1 | cut -d: -f2- | tr -d '\r' | sed 's/^ *//' || true; }
fetch() { # $1 base, $2 path, $3 accept-encoding ("" = none) ; writes $work/h $work/b
  local args=(-sS -D "$work/h" -o "$work/b" "$1$2")
  [[ -n "$3" ]] && args+=(-H "Accept-Encoding: $3")
  curl "${args[@]}"
}
expect() { # $1 label, $2 actual, $3 regex
  if [[ "$2" =~ $3 ]]; then ok "$1: $2"; else bad "$1: got '$2', want /$3/"; fi
}
expect_absent() { if [[ -z "$2" ]]; then ok "$1 absent"; else bad "$1 present: $2"; fi; }
decoded_equals() { # $1 encoding, compares decoded body with the file on disk
  node -e 'const z=require("zlib"),fs=require("fs");let b=fs.readFileSync(process.argv[1]);
    const e=process.argv[2];if(e==="br")b=z.brotliDecompressSync(b);else if(e==="gzip")b=z.gunzipSync(b);else if(e==="zstd")process.exit(0);
    process.exit(Buffer.compare(b,fs.readFileSync(process.argv[3]))===0?0:1)' "$work/b" "$1" "$WEB/$WASM"
}

check_origin() { # $1 base url, $2 label, $3 = "caddy" to also assert Caddy-only headers
  local base="$1" ce
  echo "== $2 ($base)"
  fetch "$base" / ""; expect "GET / status" "$(head -1 "$work/h" | tr -d '\r')" ' 200'
  expect "GET / content-type" "$(hdr "$work/h" content-type)" '^text/html'
  expect "GET / cache-control" "$(hdr "$work/h" cache-control)" '^no-cache$'
  expect_absent "Cross-Origin-Opener-Policy" "$(hdr "$work/h" cross-origin-opener-policy)"
  expect_absent "Cross-Origin-Embedder-Policy" "$(hdr "$work/h" cross-origin-embedder-policy)"

  fetch "$base" "/$WASM" ""
  expect "wasm status" "$(head -1 "$work/h" | tr -d '\r')" ' 200'
  expect "wasm content-type" "$(hdr "$work/h" content-type)" '^application/wasm$'
  expect "wasm cache-control" "$(hdr "$work/h" cache-control)" 'max-age=31536000.*immutable'
  expect_absent "wasm content-encoding without Accept-Encoding" "$(hdr "$work/h" content-encoding)"

  fetch "$base" "/$WASM" "br, gzip"
  ce="$(hdr "$work/h" content-encoding)"
  expect "wasm Accept-Encoding br: content-encoding" "$ce" '^(br|zstd|gzip)$'
  expect "wasm compressed content-type" "$(hdr "$work/h" content-type)" '^application/wasm$'
  expect "wasm compressed vary" "$(hdr "$work/h" vary)" 'Accept-Encoding'
  decoded_equals "$ce" && ok "wasm br/gzip body decodes to the original" || bad "wasm compressed body differs from the file"

  fetch "$base" "/$WASM" "gzip"
  ce="$(hdr "$work/h" content-encoding)"
  expect "wasm Accept-Encoding gzip: content-encoding" "$ce" '^gzip$'
  decoded_equals "$ce" && ok "wasm gzip body decodes to the original" || bad "wasm gzip body differs from the file"

  fetch "$base" "/assets/app-ab12cd34.js" ""
  expect "js cache-control" "$(hdr "$work/h" cache-control)" 'immutable'
  expect "js content-type" "$(hdr "$work/h" content-type)" 'javascript'

  if [[ "$3" == caddy ]]; then
    local csp
    csp="$(hdr "$work/h" content-security-policy)"
    expect "CSP script-src" "$csp" "script-src 'self' 'wasm-unsafe-eval'"
    expect "CSP worker-src" "$csp" "worker-src 'self'(;|$)"
    expect "CSP default-src" "$csp" "default-src 'self'"
    [[ "$csp" == *"unsafe-eval'"* && "$csp" != *"'wasm-unsafe-eval'"* ]] && bad "CSP allows plain unsafe-eval" || ok "CSP has no plain unsafe-eval"
    expect "nosniff" "$(hdr "$work/h" x-content-type-options)" 'nosniff'
    fetch "$base" / "gzip"
    expect "index compressed by Caddy" "$(hdr "$work/h" content-encoding)" '^(gzip|zstd)$'
    fetch "$base" "/assets/plain-11223344.wasm" "gzip"
    expect "wasm without precompressed file, compressed by Caddy" "$(hdr "$work/h" content-encoding)" '^(gzip|zstd)$'
    expect "  ... content-type" "$(hdr "$work/h" content-type)" '^application/wasm$'
  fi
}

check_origin "http://127.0.0.1:$PORT" "server directly" direct

if (( USE_CADDY )) && command -v caddy >/dev/null 2>&1; then
  CPORT="$(free_port)"
  # Same file, but plain HTTP on a loopback port, our upstream, no log file.
  sed -e "s|^play.example.org {|http://127.0.0.1:$CPORT {|" \
      -e "s|127.0.0.1:18200|127.0.0.1:$PORT|" \
      -e "s|output file [^ ]* {|output file $work/caddy-access.log {|" "$HERE/Caddyfile" >"$work/Caddyfile"
  # Caddy's admin API is off so a running caddy on the box is not touched.
  { printf '{\n\tadmin off\n\tauto_https off\n}\n'; cat "$work/Caddyfile"; } >"$work/Caddyfile.full"
  mv "$work/Caddyfile.full" "$work/Caddyfile"
  caddy validate --config "$work/Caddyfile" --adapter caddyfile >"$work/validate.log" 2>&1 \
    && ok "Caddyfile validates" || { bad "Caddyfile does not validate"; cat "$work/validate.log"; }
  XDG_DATA_HOME="$work/xdg-data" XDG_CONFIG_HOME="$work/xdg-conf" HOME="$work" \
    caddy run --config "$work/Caddyfile" --adapter caddyfile >"$work/caddy.log" 2>&1 &
  pids+=($!)
  for _ in $(seq 1 50); do curl -s -o /dev/null "http://127.0.0.1:$CPORT/health" && break; sleep 0.2; done
  check_origin "http://127.0.0.1:$CPORT" "through Caddy" caddy
else
  echo "== Caddy not run (not installed or --no-caddy): CSP and Caddy compression unchecked"
fi

if (( fail )); then echo "RESULT: FAIL"; exit 1; fi
echo "RESULT: all header assertions hold"
