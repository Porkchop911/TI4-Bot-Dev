"""Canonical public-state projection for the pinned Python oracle.

This module deliberately has no import from the oracle at module load time.  Callers
provide a state they created through the oracle, so importing the exporter cannot
alter the oracle filesystem or process state.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from engine.state import GameState, Player, SystemState
    from engine.units import Unit


def _unit_projection(unit: Unit) -> dict[str, Any]:
    return {
        "type_id": unit.type_id,
        "owner": unit.owner,
        "damage": unit.sustained_damage,
    }


def _ordered_units(units: tuple[Unit, ...]) -> list[dict[str, Any]]:
    return [
        _unit_projection(unit)
        for unit in sorted(units, key=lambda unit: (unit.owner, unit.type_id, unit.sustained_damage))
    ]


def _system_projection(system_id: str, system: SystemState) -> dict[str, Any]:
    return {
        "id": system_id,
        "units": _ordered_units(system.units),
        "command_tokens": sorted(system.command_tokens),
        "planet_control": dict(sorted(system.planet_control.items())),
        "planet_units": {
            planet_id: _ordered_units(units)
            for planet_id, units in sorted(system.planet_units.items())
        },
    }


def _player_projection(player: Player, state: GameState) -> dict[str, Any]:
    controlled_planets = [
        planet_id
        for _, system in sorted(state.board.items())
        for planet_id, owner in sorted(system.planet_control.items())
        if owner == player.id
    ]
    return {
        "id": player.id,
        "faction": player.faction,
        "vp": player.victory_points,
        "techs": sorted(player.technologies),
        "command_tokens": {
            "tactic": player.tactic_tokens,
            "fleet": player.fleet_tokens,
            "strategic": player.strategic_tokens,
        },
        "trade_goods": player.trade_goods,
        "commodities": player.commodities,
        "home_system": player.home_system,
        "home_planets": list(player.home_planets),
        "controlled_planets": controlled_planets,
        "strategy_cards": list(player.strategy_cards),
        "exhausted_strategy_cards": sorted(player.exhausted_strategy_cards),
        "passed": player.passed,
        "action_cards_count": len(player.action_cards),
        "secret_objectives_count": len(player.secret_objectives),
        "relics": list(player.relics),
    }


def state_projection(state: GameState) -> dict[str, Any]:
    """Return a JSON-ready state projection with explicit canonical ordering.

    The current oracle has no ``GameState.turn`` field; ``turn_seq`` is its monotonic
    action-turn counter and is therefore exported as ``turn``.  The projection does
    not expose card or secret-objective identities that are hidden from other players.
    """

    return {
        "type": "state",
        "turn": state.turn_seq,
        "round": state.round,
        "phase": state.phase.value,
        "active_player": state.active,
        "speaker": state.speaker,
        "seating_order": list(state.seating_order),
        "initiative_order": list(state.initiative_order),
        "players": [_player_projection(state.player(player_id), state) for player_id in state.initiative_order],
        "systems": [
            _system_projection(system_id, system)
            for system_id, system in sorted(state.board.items())
        ],
        "unclaimed_strategy_cards": list(state.unclaimed_strategy_cards),
        "strategy_card_goods": dict(sorted(state.strategy_card_goods.items())),
        "agenda": {
            "custodians_removed": state.custodians_removed,
            "laws": dict(sorted(state.laws.items())),
        },
        "game_over": state.finished,
    }
