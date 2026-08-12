"""Deterministic error projection for the pinned Python oracle."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from engine.state import GameState


def _optional_id(value: str | None, name: str) -> str | None:
    if value is not None and not isinstance(value, str):
        raise TypeError(f"{name} must be a string or None")
    return value


def error_projection(
    exc: Exception,
    state: GameState,
    *,
    player: str | None = None,
    option_id: str | None = None,
    card_id: str | None = None,
) -> dict[str, Any]:
    """Return a deterministic error record without a machine-specific traceback.

    The exception remains a failure for the caller to handle; this projection exists
    to record that failure in a replayable export, not to convert it into success.
    """

    if not isinstance(exc, Exception):
        raise TypeError("exc must be an Exception")
    return {
        "type": "error",
        "error_type": type(exc).__name__,
        "message": str(exc),
        "context": {
            "turn": state.turn_seq,
            "round": state.round,
            "phase": state.phase.value,
            "player": _optional_id(player if player is not None else state.active, "player"),
            "option_id": _optional_id(option_id, "option_id"),
            "card_id": _optional_id(card_id, "card_id"),
        },
        "stack_trace": None,
    }
