"""Bounded seeded-game observation for the pinned Python oracle."""

from __future__ import annotations

import itertools
import json
from typing import Any, Iterable

from .cli import EXPORT_SCOPE, ORACLE_COMMIT, SCHEMA_VERSION, _load_oracle
from .projections.choice import choice_projection
from .projections.event import event_projection
from .projections.outcome import outcome_projection
from .projections.map import map_projection
from .projections.state import state_projection


def _scenario_game(scenario: str, seed: int, table: Any) -> tuple[Any, list[str], str]:
    """Construct one supported deterministic Save 52/54 scenario."""

    content, _, _ = _load_oracle()
    from engine import sim
    from engine.game import cards_per_player, seated_game

    if scenario == "save54_base":
        seats = dict(sim.SAVE_54_SEATS)
        sources = content.BASE
        galaxy = sim.save_54_galaxy(tuple(seats.values()), tile_seed=seed)
    elif scenario == "save54_te":
        seats = dict(sim.SAVE_54_SEATS)
        sources = content.FULL
        galaxy = sim.save_54_galaxy(tuple(seats.values()), tile_seed=seed)
    elif scenario == "save52_base":
        seats = dict(sim.DEFAULT_SEATS)
        sources = content.BASE
        galaxy = sim.save_52_galaxy(tuple(seats.values()))
    elif scenario == "save52_te":
        seats = dict(sim.DEFAULT_SEATS)
        sources = content.FULL
        galaxy = sim.save_52_galaxy(tuple(seats.values()))
    else:
        raise ValueError(f"unknown bounded-game scenario: {scenario}")
    game = seated_game(
        seats,
        table=table,
        dice_seed=seed,
        sources=sources,
        galaxy=galaxy,
        cards_per_player=cards_per_player(len(seats)),
    )
    return game, list(seats), "base" if sources == content.BASE else "full"


class _RecordingTable:
    """Delegate generated legal choices while preserving the exact selected option ID."""

    def __init__(
        self, seed: int, records: list[dict[str, Any]], decisions: Iterable[str] | None = None
    ) -> None:
        _, _, _ = _load_oracle()
        from engine.choice import Scripted, SeededRandom, Table

        default = SeededRandom(seed) if decisions is None else Scripted(decisions)
        self._table = Table(default=default)
        self._records = records

    def ask(self, choice: Any) -> Any:
        option = self._table.ask(choice)
        record = choice_projection(choice)
        record["selected"] = option.id
        self._records.append(record)
        return option

    @property
    def log(self) -> Any:
        return self._table.log

    def seat(self, player: str, decider: Any) -> None:
        self._table.seat(player, decider)


def _dice_entropy(game: Any) -> dict[str, Any]:
    return {
        "type": "entropy",
        "stream": "dice",
        "seed": game.dice.seed,
        "rolls": [
            {
                "reason": roll.reason,
                "faces": list(roll.faces),
                "hits_on": roll.hits_on,
                "rerolled": sorted(roll.rerolled),
            }
            for roll in game.dice.history
        ],
    }


def bounded_game_records(
    scenario: str, seed: int, rounds: int, decisions: Iterable[str] | None = None
) -> list[dict[str, Any]]:
    """Run a bounded seeded scenario and return its observable replay inputs/outputs.

    The original oracle exception is deliberately allowed to propagate.  Callers may use
    the error projector to persist it, but a failed or unfinished game is never reported
    as a completed outcome.
    """

    if rounds <= 0:
        raise ValueError("rounds must be positive")
    # Event IDs are allocated from an oracle module-global iterator. An export must be
    # independent of earlier exports in this interpreter, so establish the documented
    # per-trace origin before constructing any event-producing game objects.
    _load_oracle()
    from engine import timing

    timing._uid_counter = itertools.count(1)
    records: list[dict[str, Any]] = []
    table = _RecordingTable(seed, records, decisions)
    game, seats, sources = _scenario_game(scenario, seed, table)
    records.extend(
        [
            {
                "type": "header",
                "schema_version": SCHEMA_VERSION,
                "oracle_commit": ORACLE_COMMIT,
                "scenario": scenario,
                "seed": seed,
                "rounds": rounds,
                "seats": seats,
                "sources": sources,
                "export_scope": "bounded_game",
            },
            state_projection(game.state),
            # The board the game was played on. Emitted once, before any decision: it does not
            # change during a bounded run, and without it a consumer cannot rebuild the galaxy and
            # so can never replay a tactical action.
            map_projection(getattr(game, "galaxy", None)),
        ]
    )
    original_emit = game._emit

    def observed_emit(event_type: str, **payload: Any) -> Any:
        event = original_emit(event_type, **payload)
        records.append(event_projection(event, game.state))
        return event

    game._emit = observed_emit
    game.run(rounds=rounds)
    records.append(state_projection(game.state))
    if game.state.finished:
        reason = next(
            (
                record["payload"]["reason"]
                for record in reversed(records)
                if record["type"] == "event" and record["event_type"] == "GAME_ENDED"
            ),
            "unknown",
        )
        records.append(outcome_projection(game.state, reason))
    records.append(_dice_entropy(game))
    return records


def bounded_ndjson_bytes(records: Iterable[dict[str, Any]]) -> bytes:
    """Encode a bounded-game stream canonically, including its terminal newline."""

    return b"".join(
        json.dumps(record, ensure_ascii=True, separators=(",", ":"), sort_keys=True).encode("ascii") + b"\n"
        for record in records
    )


def replay_records(records: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    """Rebuild a bounded run from its stable selected option IDs."""

    materialized = list(records)
    if not materialized or materialized[0].get("type") != "header":
        raise ValueError("replay input must start with a header")
    header = materialized[0]
    if header.get("export_scope") != "bounded_game":
        raise ValueError("replay input is not a bounded-game export")
    decisions = [record["selected"] for record in materialized if record.get("type") == "choice"]
    return bounded_game_records(header["scenario"], header["seed"], header["rounds"], decisions)
