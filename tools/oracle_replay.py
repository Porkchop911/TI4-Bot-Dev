"""Replay a bounded oracle export from captured legal option IDs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from oracle_exporter.cli import _output_path
from oracle_exporter.runner import bounded_ndjson_bytes, replay_records


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Replay a bounded oracle NDJSON export")
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    try:
        source = Path(args.input).resolve()
        output = _output_path(args.output)
        records = [json.loads(line) for line in source.read_bytes().splitlines()]
        replayed = bounded_ndjson_bytes(replay_records(records))
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = output.with_name(f"{output.name}.tmp")
        temporary.write_bytes(replayed)
        temporary.replace(output)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        parser.error(str(exc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
