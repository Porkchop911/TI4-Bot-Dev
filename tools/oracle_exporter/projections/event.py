"""Canonical resolved-event projection for the pinned Python oracle."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from .choice import _canonical_value

if TYPE_CHECKING:
    from engine.state import GameState
    from engine.timing import Event


def event_projection(event: Event, state: GameState) -> dict[str, Any]:
    """Return the resolved event with the phase context in which it was observed.

    Call this after the oracle resolver has completed its WHEN/AFTER windows; those
    windows may mutate the payload or cancel the event.
    """

    return {
        "type": "event",
        "event_type": event.type,
        "payload": _canonical_value(event.payload),
        "cancelled": event.cancelled,
        "id": event.uid,
        "phase": state.phase.value,
        "turn": state.turn_seq,
        "round": state.round,
    }
