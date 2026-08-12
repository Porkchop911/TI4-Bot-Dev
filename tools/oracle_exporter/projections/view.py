"""Canonical per-player projection through the oracle's redacted view boundary."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from .state import state_projection

if TYPE_CHECKING:
    from engine.views import GameView


def view_projection(view: GameView) -> dict[str, Any]:
    """Return a JSON-ready projection of one oracle ``GameView``.

    Reading ``view.state`` is intentional: ``GameView`` owns the redaction boundary.
    The exporter must never reconstruct private holdings from the underlying game.
    """

    state = view.state
    projection = state_projection(state)
    projection["type"] = "view"
    projection["viewer"] = view.viewer

    players_by_id = {player["id"]: player for player in projection["players"]}
    for player_id in state.initiative_order:
        player = state.player(player_id)
        exported = players_by_id[player_id]
        exported["action_cards"] = list(player.action_cards)
        exported["secret_objectives"] = list(player.secret_objectives)

    return projection
