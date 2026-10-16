#!/usr/bin/env bash
# Packs the MLP models of models/index.json for a server: one .ti4m per entry, its .br, and the
# index, in out/models/ (gitignored). The server loads that directory with TI4_MODEL_DIR=out/models;
# deploy/public/deploy.sh uploads it.
#
#   scripts/pack-models.sh                 # pack every entry, check it against the index
#   scripts/pack-models.sh --update-index  # ... and write the packed shas into models/index.json
#                                          #     (after adding a model or changing the pack format)
#   scripts/pack-models.sh --out DIR       # another output directory
#
# An entry of models/index.json: name, label, temperature, source {bundle, manifest_sha256},
# sha256, offered (and optionally file, default <name>.ti4m). `bundle` is a directory relative to
# the repository (or absolute). `manifest_sha256` is `sha256sum <bundle>/manifest.json`, `sha256`
# is `sha256sum <name>.ti4m` of the packed file: both are checked, and a mismatch stops the script
# with nothing written to the index of the output. The server reads the same index.
#
# ti4-pack is built with `cargo b-pack` (.cargo/config.toml: a release build of ti4-infer only).
# The .br is written with the `brotli` CLI when it is installed and the result is smaller; the
# server serves it to a client that accepts brotli, else the plain file.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INDEX="$REPO/models/index.json"
OUT="$REPO/out/models"
UPDATE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --update-index) UPDATE=1; shift ;;
    --out) OUT="${2:?--out needs a directory}"; shift 2 ;;
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

command -v python3 >/dev/null || { echo "pack-models: python3 is needed to read $INDEX" >&2; exit 1; }
HAS_BROTLI=1
if ! command -v brotli >/dev/null; then
  HAS_BROTLI=0
  echo "pack-models: the brotli CLI is not installed: no .br is written, and the server sends the" >&2
  echo "             plain files (install it, e.g. 'apt install brotli' or 'pacman -S brotli')." >&2
fi

echo "== Building ti4-pack (cargo b-pack)"
(cd "$REPO" && cargo b-pack)
PACK="${CARGO_TARGET_DIR:-$REPO/target}/release/ti4-pack"
[[ -x "$PACK" ]] || { echo "pack-models: $PACK was not built" >&2; exit 1; }

# One line per entry: name, file, label, temperature, bundle, manifest sha, packed sha (tabs).
entries="$(python3 -I - "$INDEX" <<'P'
import json, sys
index = json.load(open(sys.argv[1]))
offered = 0
for e in index["models"]:
    src = e["source"]
    offered += bool(e.get("offered", False))
    fields = [e["name"], e.get("file") or e["name"] + ".ti4m", e["label"], str(e["temperature"]),
              src["bundle"], src["manifest_sha256"], e.get("sha256", "")]
    if any("\t" in f or "\n" in f for f in fields):
        sys.exit(f"pack-models: a field of {e['name']} has a tab or a newline")
    print("\t".join(fields))
if offered > 3:
    sys.exit(f"pack-models: {offered} models are offered, a server offers at most 3")
P
)"

mkdir -p "$OUT"
failed=0
declare -A packed=()
keep=(index.json)
while IFS=$'\t' read -r name file label temperature bundle manifest_sha sha; do
  [[ -n "$name" ]] || continue
  echo "== $name"
  [[ "$bundle" = /* ]] || bundle="$REPO/$bundle"
  actual_manifest="$(sha256sum "$bundle/manifest.json" | cut -d' ' -f1)"
  if [[ "$actual_manifest" != "$manifest_sha" ]]; then
    echo "pack-models: $bundle/manifest.json has sha256 $actual_manifest, the index says $manifest_sha" >&2
    failed=1
    continue
  fi
  "$PACK" "$bundle" "$name" "$OUT/$file" --label "$label" --temperature "$temperature"
  actual="$(sha256sum "$OUT/$file" | cut -d' ' -f1)"
  packed[$name]="$actual"
  keep+=("$file")
  if [[ "$actual" != "$sha" ]]; then
    if (( UPDATE )); then
      echo "   sha256 $actual (the index said ${sha:-nothing}; updated)"
    else
      echo "pack-models: $file has sha256 $actual, the index says ${sha:-nothing}" >&2
      echo "             (a new model, or a new pack format: run with --update-index and commit models/index.json)" >&2
      failed=1
    fi
  fi
  rm -f "$OUT/$file.br"
  if (( HAS_BROTLI )); then
    brotli -q 11 -w 24 -f -o "$OUT/$file.br" "$OUT/$file"
    plain="$(stat -c %s "$OUT/$file")"
    small="$(stat -c %s "$OUT/$file.br")"
    if (( small < plain )); then
      keep+=("$file.br")
      echo "   $file.br: $small bytes of $plain"
    else
      rm -f "$OUT/$file.br"
      echo "   $file.br is not smaller; not kept"
    fi
  fi
done <<<"$entries"

if (( failed )); then
  echo "pack-models: FAILED; $OUT/index.json was not written" >&2
  rm -f "$OUT/index.json"
  exit 1
fi

if (( UPDATE )); then
  args=()
  for name in "${!packed[@]}"; do args+=("$name=${packed[$name]}"); done
  python3 -I - "$INDEX" "${args[@]}" <<'P'
import json, sys
path = sys.argv[1]
shas = dict(arg.split("=", 1) for arg in sys.argv[2:])
index = json.load(open(path))
for e in index["models"]:
    e["sha256"] = shas[e["name"]]
open(path, "w").write(json.dumps(index, indent=2) + "\n")
P
  echo "== updated $INDEX"
fi

# Only the files of the index stay, so that the directory can be uploaded as it is.
for path in "$OUT"/*; do
  base="$(basename "$path")"
  case " ${keep[*]} " in *" $base "*) ;; *) rm -rf "$path"; echo "   removed $base (not in the index)" ;; esac
done
cp "$INDEX" "$OUT/index.json"
echo "== $OUT"
ls -l "$OUT"
