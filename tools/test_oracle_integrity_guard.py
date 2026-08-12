"""Focused local tests for the fail-closed oracle integrity guard."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

import sys

sys.path.insert(0, str(REPO_ROOT))

from tools.oracle_integrity_guard import IntegrityError, verify_oracle  # noqa: E402


class OracleIntegrityGuardTests(unittest.TestCase):
    def _fixture(self) -> tuple[tempfile.TemporaryDirectory[str], Path, Path]:
        temporary = tempfile.TemporaryDirectory(dir=REPO_ROOT)
        root = Path(temporary.name)
        oracle = root / "oracle"
        oracle.mkdir()
        tracked = oracle / "engine" / "game.py"
        tracked.parent.mkdir()
        tracked.write_text("print('pinned')\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q"], cwd=oracle, check=True)
        subprocess.run(["git", "config", "user.email", "guard@example.invalid"], cwd=oracle, check=True)
        subprocess.run(["git", "config", "user.name", "guard"], cwd=oracle, check=True)
        subprocess.run(["git", "add", "engine/game.py"], cwd=oracle, check=True)
        subprocess.run(["git", "commit", "-qm", "pinned"], cwd=oracle, check=True)
        commit = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=oracle, check=True, capture_output=True, text=True
        ).stdout.strip()
        manifest = root / "manifest.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema_version": "1.0.0",
                    "oracle_commit": commit,
                    "files": {"engine/game.py": hashlib.sha256(tracked.read_bytes()).hexdigest()},
                }
            ),
            encoding="utf-8",
        )
        return temporary, oracle, manifest

    def test_clean_pinned_oracle_verifies(self) -> None:
        temporary, oracle, manifest = self._fixture()
        with temporary:
            self.assertEqual(verify_oracle(oracle, manifest), 1)

    def test_dirty_oracle_fails_before_hash_comparison(self) -> None:
        temporary, oracle, manifest = self._fixture()
        with temporary:
            (oracle / "scratch.txt").write_text("untracked", encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "dirty"):
                verify_oracle(oracle, manifest)

    def test_changed_hash_and_invalid_manifest_are_rejected(self) -> None:
        temporary, oracle, manifest = self._fixture()
        with temporary:
            (oracle / "engine" / "game.py").write_text("print('changed')\n", encoding="utf-8")
            subprocess.run(["git", "add", "engine/game.py"], cwd=oracle, check=True)
            subprocess.run(["git", "commit", "-qm", "changed"], cwd=oracle, check=True)
            with self.assertRaisesRegex(IntegrityError, "commit mismatch"):
                verify_oracle(oracle, manifest)
            manifest.write_text(
                '{"schema_version":"1.0.0","oracle_commit":"0000000000000000000000000000000000000000","files":{"../bad":"0000000000000000000000000000000000000000000000000000000000000000"}}',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(IntegrityError, "relative"):
                verify_oracle(oracle, manifest)


if __name__ == "__main__":
    unittest.main()
