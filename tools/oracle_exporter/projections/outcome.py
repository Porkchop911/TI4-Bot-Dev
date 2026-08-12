"""Canonical finished-game outcome projection for the pinned Python oracle."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from engine.state import GameState


def _leader(state: GameState) -> str | None:
    """Mirror the oracle's finished-game initiative tie break (LRR 98.8)."""

    if not state.players:
        return None
    order = state.initiative_order
    best = max(player.victory_points for player in state.players)
    tied = [player.id for player in state.players if player.victory_points == best]
    return min(tied, key=lambda player_id: order.index(player_id) if player_id in order else 99)


def outcome_projection(state: GameState, reason: str) -> dict[str, Any]:
    """Return a JSON-ready outcome for a game that the oracle has actually finished."""

    if not state.finished:
        raise ValueError("cannot project an outcome from an unfinished game")
    if not isinstance(reason, str) or not reason:
        raise ValueError("outcome reason must be a non-empty string")

    return {
        "type": "outcome",
        "game_over": True,
        "winner": _leader(state),
        "victory_points": {
            player_id: state.player(player_id).victory_points
            for player_id in sorted(player.id for player in state.players)
        },
        "final_phase": state.phase.value,
        "final_turn": state.turn_seq,
        "final_round": state.round,
        "reason": reason,
    }
