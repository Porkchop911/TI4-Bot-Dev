#!/usr/bin/env bash
# Install the CPU-only libtorch distribution required by tch when building on Linux/WSL.
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
target_dir=${LIBTORCH_LINUX_DIR:-"$repo_root/out/libtorch-2.9.1-cpu-linux"}
archive_url=https://download.pytorch.org/libtorch/cpu/libtorch-shared-with-deps-2.9.1%2Bcpu.zip

if [[ "$(uname -s)" != "Linux" ]]; then
    printf 'This script installs Linux libtorch; run it only on Linux or WSL.\n' >&2
    exit 1
fi

if [[ -f "$target_dir/lib/libtorch_cpu.so" ]]; then
    printf 'Linux CPU libtorch is already installed at %s\n' "$target_dir"
    exit 0
fi

if [[ -e "$target_dir" ]]; then
    printf 'Refusing to overwrite incomplete libtorch directory: %s\n' "$target_dir" >&2
    printf 'Remove or repair it manually, then rerun this script.\n' >&2
    exit 1
fi

mkdir -p "$(dirname -- "$target_dir")"
stage_dir=$(mktemp -d "${TMPDIR:-/tmp}/ti4-libtorch.XXXXXX")
trap 'rm -rf -- "$stage_dir"' EXIT

curl --fail --location --show-error --output "$stage_dir/libtorch.zip" "$archive_url"
unzip -q "$stage_dir/libtorch.zip" -d "$stage_dir"

if [[ ! -f "$stage_dir/libtorch/lib/libtorch_cpu.so" ]]; then
    printf 'The downloaded libtorch archive does not contain lib/libtorch_cpu.so.\n' >&2
    exit 1
fi

mv -- "$stage_dir/libtorch" "$target_dir"
printf 'Installed Linux CPU libtorch at %s\n' "$target_dir"
printf 'For this shell: export LIBTORCH="%s"\n' "$target_dir"
printf 'For this shell: export LD_LIBRARY_PATH="$LIBTORCH/lib:${LD_LIBRARY_PATH:-}"\n'
