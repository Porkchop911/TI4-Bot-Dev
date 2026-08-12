"""Generate the bounded 100-trace legacy entropy corpus from the pinned oracle.

The generator never writes to the oracle. It holds all traces in memory, verifies that the
oracle's own replay recreates their canonical bytes, checks the fixed artifact budget, and only
then atomically writes the Rust-repository fixtures and checksum manifest.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = REPO_ROOT / "fixtures" / "legacy_entropy" / "bounded-v1"
MAX_CORPUS_BYTES = 20 * 1024 * 1024
MAX_TRACE_BYTES = 512 * 1024
SCENARIOS = ("save54_base", "save54_te", "save52_base", "save52_te")
SEEDS = range(25)


def _atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    try:
        temporary.write_bytes(data)
        temporary.replace(path)
    finally:
        if temporary.exists():
            temporary.unlink()


def _trace_jobs() -> list[tuple[str, int]]:
    return [(scenario, seed) for scenario in SCENARIOS for seed in SEEDS]


def build_corpus() -> tuple[dict[str, Any], list[tuple[str, bytes]]]:
    """Return a verified deterministic manifest and all trace bytes without writing them."""

    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(REPO_ROOT))
    from tools.oracle_exporter.cli import ORACLE_COMMIT
    from tools.oracle_exporter.runner import bounded_game_records, bounded_ndjson_bytes, replay_records

    jobs = _trace_jobs()
    if len(jobs) != 100:
        raise RuntimeError(f"fixture matrix has {len(jobs)} traces, expected 100")
    entries: list[dict[str, Any]] = []
    traces: list[tuple[str, bytes]] = []
    for index, (scenario, seed) in enumerate(jobs, start=1):
        records = bounded_game_records(scenario, seed=seed, rounds=1)
        encoded = bounded_ndjson_bytes(records)
        if bounded_ndjson_bytes(replay_records(records)) != encoded:
            raise RuntimeError(f"oracle replay mismatch for {scenario} seed {seed}")
        if len(encoded) > MAX_TRACE_BYTES:
            raise RuntimeError(f"trace {scenario} seed {seed} exceeds {MAX_TRACE_BYTES} bytes")
        filename = f"trace-{index:03d}.ndjson"
        entries.append(
            {
                "id": f"m03-007b-{index:03d}",
                "scenario": scenario,
                "seed": seed,
                "rounds": 1,
                "path": filename,
                "bytes": len(encoded),
                "sha256": hashlib.sha256(encoded).hexdigest(),
            }
        )
        traces.append((filename, encoded))
    total_bytes = sum(len(trace) for _, trace in traces)
    if total_bytes > MAX_CORPUS_BYTES:
        raise RuntimeError(f"corpus has {total_bytes} bytes, limit is {MAX_CORPUS_BYTES}")
    return (
        {
            "schema_version": "m03-007b-v1",
            "oracle_commit": ORACLE_COMMIT,
            "traces": entries,
        },
        traces,
    )


def main() -> int:
    manifest, traces = build_corpus()
    for filename, encoded in traces:
        _atomic_write(FIXTURE_ROOT / filename, encoded)
    _atomic_write(
        FIXTURE_ROOT / "manifest.json",
        json.dumps(manifest, ensure_ascii=True, indent=2, sort_keys=True).encode("ascii") + b"\n",
    )
    print(f"wrote {len(traces)} traces ({sum(len(trace) for _, trace in traces)} bytes) to {FIXTURE_ROOT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
