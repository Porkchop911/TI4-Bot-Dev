"""Bounded seeded-game observation for the pinned Python oracle."""

from __future__ import annotations

from typing import Any

from .cli import EXPORT_SCOPE, ORACLE_COMMIT, SCHEMA_VERSION, _load_oracle
from .projections.choice import choice_projection
from .projections.event import event_projection
from .projections.outcome import outcome_projection
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

    def __init__(self, seed: int, records: list[dict[str, Any]]) -> None:
        _, _, _ = _load_oracle()
        from engine.choice import SeededRandom, Table

        self._table = Table(default=SeededRandom(seed))
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


def bounded_game_records(scenario: str, seed: int, rounds: int) -> list[dict[str, Any]]:
    """Run a bounded seeded scenario and return its observable replay inputs/outputs.

    The original oracle exception is deliberately allowed to propagate.  Callers may use
    the error projector to persist it, but a failed or unfinished game is never reported
    as a completed outcome.
    """

    if rounds <= 0:
        raise ValueError("rounds must be positive")
    records: list[dict[str, Any]] = []
    table = _RecordingTable(seed, records)
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
