"""Canonical legal-choice projection for the pinned Python oracle."""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from engine.choice import Choice, Option


def _canonical_value(value: Any) -> Any:
    """Return a JSON value or reject a payload that cannot be exported faithfully."""

    # Event payloads can contain a resolved `engine.dice.Roll`, notably the unit-ability
    # reroll event. Preserve its replay-relevant fields rather than refusing an otherwise
    # complete bounded trace.
    from engine.dice import Roll

    if isinstance(value, Roll):
        return {
            "faces": list(value.faces),
            "hits_on": value.hits_on,
            "reason": value.reason,
            "rerolled": sorted(value.rerolled),
        }
    if value is None or isinstance(value, (str, bool, int)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("choice payload contains a non-finite float")
        return value
    if isinstance(value, (list, tuple)):
        return [_canonical_value(item) for item in value]
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise TypeError("choice payload maps must have string keys")
        return {key: _canonical_value(value[key]) for key in sorted(value)}
    raise TypeError(f"choice payload value is not JSON-compatible: {type(value).__name__}")


def _option_projection(option: Option) -> dict[str, Any]:
    return {
        "id": option.id,
        "kind": option.kind,
        "label": option.label,
        "payload": _canonical_value(option.payload),
    }


def choice_projection(choice: Choice) -> dict[str, Any]:
    """Return a JSON-ready projection preserving the oracle's legal option order."""

    return {
        "type": "choice",
        "player": choice.player,
        "prompt": choice.prompt,
        "options": [_option_projection(option) for option in choice.options],
    }
