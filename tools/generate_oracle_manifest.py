"""Generate the pinned oracle's integrity manifest, read-only, from a clean worktree.

Companion to `oracle_integrity_guard.py`: this writes what that verifies. It refuses to run
against an oracle that is dirty or off the pinned commit, so a manifest can never record
hashes of files that were not the pinned ones.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from oracle_integrity_guard import (
    DEFAULT_MANIFEST,
    DEFAULT_ORACLE_ROOT,
    IntegrityError,
    _git,
)

PINNED_COMMIT = "37061c511a4780d4c0719e0342533a498cd4b457"

# What the migration actually ports, and therefore what must not change underneath it:
# the engine and bridge sources, the test suite that specifies their behaviour, the data
# and configuration they read, and the project definition that pins their dependencies.
#
# Deliberately excluded: `docs/`, `out/`, `tts/`, `tools/`, and the repository dotfiles.
# None of them define engine behaviour, and a manifest that fails on an unrelated
# documentation edit would train its readers to bypass it.
INCLUDED_PREFIXES = ("engine/", "bridge/", "tests/", "data/", "configs/")
INCLUDED_FILES = ("pyproject.toml",)

SCHEMA_VERSION = "1.0.0"


def selected_paths(root: Path) -> list[str]:
    """Tracked paths at HEAD that define oracle behaviour, in sorted POSIX form."""

    listing = _git(root, "ls-tree", "-r", "HEAD", "--name-only")
    paths = [
        line
        for line in listing.splitlines()
        if line.startswith(INCLUDED_PREFIXES) or line in INCLUDED_FILES
    ]
    if not paths:
        raise IntegrityError("no oracle files matched the manifest selection")
    return sorted(paths)


def build_manifest(root: Path, expected_commit: str) -> dict[str, object]:
    """Hash every selected file, refusing anything but a clean worktree at the pin."""

    commit = _git(root, "rev-parse", "HEAD").strip()
    if commit != expected_commit:
        raise IntegrityError(f"oracle commit mismatch: expected {expected_commit}, got {commit}")
    if _git(root, "status", "--porcelain"):
        raise IntegrityError("refusing to generate a manifest from a dirty oracle worktree")

    files: dict[str, str] = {}
    for relative in selected_paths(root):
        candidate = root / relative
        if not candidate.is_file():
            raise IntegrityError(f"tracked file is missing from the worktree: {relative}")
        files[relative] = hashlib.sha256(candidate.read_bytes()).hexdigest()

    # Re-check afterwards: a long hashing pass must not straddle a change to the tree.
    if _git(root, "rev-parse", "HEAD").strip() != expected_commit or _git(
        root, "status", "--porcelain"
    ):
        raise IntegrityError("oracle changed while the manifest was being generated")

    return {"schema_version": SCHEMA_VERSION, "oracle_commit": commit, "files": files}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle-root", type=Path, default=DEFAULT_ORACLE_ROOT)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--expect-commit", default=PINNED_COMMIT)
    args = parser.parse_args(argv)

    try:
        manifest = build_manifest(args.oracle_root.resolve(), args.expect_commit)
    except (IntegrityError, subprocess.SubprocessError) as exc:
        parser.error(str(exc))

    # Sorted keys and a trailing newline so regeneration is byte-stable and diffs are legible.
    args.manifest.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {args.manifest} — {len(manifest['files'])} files at {manifest['oracle_commit']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
