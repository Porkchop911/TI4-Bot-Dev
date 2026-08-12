"""Read-only, deterministic NDJSON export wiring for the pinned Python oracle."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

from .projections.choice import choice_projection
from .projections.state import state_projection
from .projections.view import view_projection

REPO_ROOT = Path(__file__).resolve().parents[2]
ORACLE_ROOT = Path(r"D:\Projects\ti4-engine")
ORACLE_COMMIT = "37061c511a4780d4c0719e0342533a498cd4b457"
SCHEMA_VERSION = "1.0.0"
EXPORT_SCOPE = "initial_setup"


def _verify_oracle() -> None:
    """Refuse a moved or dirty oracle before importing any of its modules."""

    try:
        head = subprocess.run(
            ["git", "-C", str(ORACLE_ROOT), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        dirty = subprocess.run(
            ["git", "-C", str(ORACLE_ROOT), "status", "--porcelain"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RuntimeError(f"unable to verify oracle at {ORACLE_ROOT}") from exc
    if head != ORACLE_COMMIT:
        raise RuntimeError(f"oracle commit mismatch: expected {ORACLE_COMMIT}, got {head}")
    if dirty:
        raise RuntimeError("oracle worktree is dirty; refusing read-only export")


def _load_oracle() -> tuple[Any, Any, Any]:
    _verify_oracle()
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    sys.dont_write_bytecode = True
    oracle_path = str(ORACLE_ROOT)
    if oracle_path not in sys.path:
        sys.path.insert(0, oracle_path)
    from engine import content
    from engine.game import Game, start_game

    return content, Game, start_game


def _parse_seats(value: str) -> tuple[str, ...]:
    seats = tuple(item.strip() for item in value.split(",") if item.strip())
    if len(seats) < 2:
        raise ValueError("at least two non-empty seats are required")
    if len(set(seats)) != len(seats):
        raise ValueError("seat identifiers must be unique")
    return seats


def _parse_sources(value: str, full_sources: frozenset[str]) -> frozenset[str]:
    if value == "full":
        return full_sources
    sources = frozenset(item.strip() for item in value.split(",") if item.strip())
    if not sources:
        raise ValueError("sources must be 'full' or a non-empty comma-separated list")
    unknown = sorted(sources - full_sources)
    if unknown:
        raise ValueError(f"unknown sources: {', '.join(unknown)}")
    return sources


def setup_records(game_id: str, seed: int, seats: tuple[str, ...], sources_text: str) -> list[dict[str, Any]]:
    """Build the bounded initial-setup export without advancing the game."""

    if not game_id:
        raise ValueError("game_id must be non-empty")
    content, Game, start_game = _load_oracle()
    sources = _parse_sources(sources_text, content.FULL)
    state = start_game(seats, sources=sources, deck_seed=seed)
    game = Game(state)
    records: list[dict[str, Any]] = [
        {
            "type": "header",
            "schema_version": SCHEMA_VERSION,
            "oracle_commit": ORACLE_COMMIT,
            "game_id": game_id,
            "seed": seed,
            "seats": list(seats),
            "sources": sorted(sources),
            "export_scope": EXPORT_SCOPE,
        },
        state_projection(state),
    ]
    records.extend(view_projection(game.view_for(player_id)) for player_id in state.seating_order)
    choice = game.legal_options()
    if choice is not None:
        records.append(choice_projection(choice))
    validate_records(records)
    return records


def validate_records(records: Iterable[dict[str, Any]]) -> None:
    """Validate the deterministic initial-setup stream shape before writing it."""

    materialized = list(records)
    if len(materialized) < 3:
        raise ValueError("initial setup export must contain header, state, and views")
    header = materialized[0]
    required_header = {
        "type": "header",
        "schema_version": SCHEMA_VERSION,
        "oracle_commit": ORACLE_COMMIT,
        "export_scope": EXPORT_SCOPE,
    }
    for key, expected in required_header.items():
        if header.get(key) != expected:
            raise ValueError(f"invalid header {key}")
    if not isinstance(header.get("game_id"), str) or not header["game_id"]:
        raise ValueError("invalid header game_id")
    if not isinstance(header.get("seed"), int):
        raise ValueError("invalid header seed")
    if not isinstance(header.get("seats"), list) or len(header["seats"]) < 2:
        raise ValueError("invalid header seats")
    if not isinstance(header.get("sources"), list) or not header["sources"]:
        raise ValueError("invalid header sources")
    if materialized[1].get("type") != "state":
        raise ValueError("initial setup export must place state after header")
    seats = header["seats"]
    views = materialized[2 : 2 + len(seats)]
    if [view.get("viewer") for view in views] != seats or any(view.get("type") != "view" for view in views):
        raise ValueError("views must be ordered by the declared seats")
    remaining = materialized[2 + len(seats) :]
    if len(remaining) > 1 or (remaining and remaining[0].get("type") != "choice"):
        raise ValueError("only the initial legal choice may follow setup views")


def ndjson_bytes(records: Iterable[dict[str, Any]]) -> bytes:
    """Encode canonical NDJSON with a terminal newline."""

    materialized = list(records)
    validate_records(materialized)
    return b"".join(
        json.dumps(record, ensure_ascii=True, separators=(",", ":"), sort_keys=True).encode("ascii") + b"\n"
        for record in materialized
    )


def _output_path(value: str) -> Path:
    path = Path(value).resolve()
    try:
        path.relative_to(REPO_ROOT)
    except ValueError as exc:
        raise ValueError(f"output path must be inside {REPO_ROOT}") from exc
    return path


def write_setup_export(output: Path, records: Iterable[dict[str, Any]]) -> None:
    """Atomically write an already-validated export inside the Rust repository."""

    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(f"{output.name}.tmp")
    temporary.write_bytes(ndjson_bytes(records))
    temporary.replace(output)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Export deterministic initial oracle setup as NDJSON")
    parser.add_argument("--game-id", required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--seats", default="p1,p2")
    parser.add_argument("--sources", default="full")
    parser.add_argument("--output", required=True)
    parser.add_argument("--validate-schema", action="store_true")
    args = parser.parse_args(argv)
    try:
        seats = _parse_seats(args.seats)
        records = setup_records(args.game_id, args.seed, seats, args.sources)
        if args.validate_schema:
            validate_records(records)
        write_setup_export(_output_path(args.output), records)
    except (RuntimeError, ValueError) as exc:
        parser.error(str(exc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
