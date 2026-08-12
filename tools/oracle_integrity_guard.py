"""Fail closed when the pinned Python oracle is missing, dirty, moved, or changed."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path, PurePosixPath
from typing import Any

DEFAULT_ORACLE_ROOT = Path(r"D:\Projects\ti4-engine")
DEFAULT_MANIFEST = Path(__file__).resolve().parents[1] / "plans" / "oracle_integrity_manifest.json"


class IntegrityError(RuntimeError):
    """An oracle cannot be trusted for a migration operation."""


def _git(root: Path, *arguments: str) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(root), *arguments], check=True, capture_output=True, text=True
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        raise IntegrityError(f"cannot inspect oracle git state at {root}") from exc


def _relative_path(value: str) -> Path:
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or value != path.as_posix():
        raise IntegrityError(f"manifest path must be a normalized relative POSIX path: {value!r}")
    return Path(*path.parts)


def _manifest(path: Path) -> tuple[str, dict[Path, str]]:
    try:
        payload: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise IntegrityError(f"cannot read integrity manifest {path}") from exc
    if not isinstance(payload, dict) or payload.get("schema_version") != "1.0.0":
        raise IntegrityError("unsupported integrity manifest schema")
    commit = payload.get("oracle_commit")
    files = payload.get("files")
    if not isinstance(commit, str) or len(commit) != 40:
        raise IntegrityError("manifest has invalid oracle_commit")
    if not isinstance(files, dict) or not files:
        raise IntegrityError("manifest must contain at least one file hash")
    checked: dict[Path, str] = {}
    for raw_path, digest in files.items():
        if not isinstance(raw_path, str) or not isinstance(digest, str):
            raise IntegrityError("manifest paths and hashes must be strings")
        relative = _relative_path(raw_path)
        if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
            raise IntegrityError(f"manifest hash is not lowercase SHA-256: {raw_path}")
        checked[relative] = digest
    return commit, checked


def verify_oracle(oracle_root: Path, manifest_path: Path) -> int:
    """Verify git cleanliness, pinned commit, and every manifest digest; return file count."""

    root = oracle_root.resolve()
    expected_commit, expected_files = _manifest(manifest_path)
    actual_commit = _git(root, "rev-parse", "HEAD").strip()
    if actual_commit != expected_commit:
        raise IntegrityError(f"oracle commit mismatch: expected {expected_commit}, got {actual_commit}")
    if _git(root, "status", "--porcelain"):
        raise IntegrityError("oracle worktree is dirty")
    for relative, expected_digest in sorted(expected_files.items()):
        candidate = root / relative
        if not candidate.is_file():
            raise IntegrityError(f"manifest file is missing: {relative.as_posix()}")
        actual_digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
        if actual_digest != expected_digest:
            raise IntegrityError(f"oracle hash mismatch: {relative.as_posix()}")
    return len(expected_files)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle-root", type=Path, default=DEFAULT_ORACLE_ROOT)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args(argv)
    try:
        count = verify_oracle(args.oracle_root, args.manifest)
    except IntegrityError as exc:
        parser.error(str(exc))
    print(f"oracle integrity verified: {count} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
