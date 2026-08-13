"""Canonical projection of the board a bounded game was played on.

The board is not part of ``GameState`` — in the oracle it hangs off ``Game.galaxy``, and in the
native engine off ``Game`` likewise — so the state projection never carried it. That omission set
the ceiling on differential replay: without a map the native engine offers no tactical action at
all, so a replayed script diverges the first time the oracle took one, and a tactical action is
most of what a game of this is.

Emitted as its own record rather than folded into the state, for two reasons. The state schema is
pinned and consumed by existing fixtures, and the board does not change during a bounded run, so
repeating it on every snapshot would be noise that grows with the trace.

Positions are exported as axial hex coordinates. Adjacency is *derived* from them by the consumer
rather than listed here: a stored neighbour list can disagree with the positions it was computed
from, and then two engines reading the same trace disagree about the board while both look right.
"""

from __future__ import annotations

from typing import Any

MAP_SCHEMA_VERSION = "1.0.0"


def map_projection(galaxy: Any) -> dict[str, Any]:
    """Return a JSON-ready projection of tile placement.

    ``galaxy`` may be ``None`` — a game set up without a board is legal, and says so explicitly
    rather than being omitted, so a consumer can tell "no map was exported" from "this game had
    no map".
    """

    if galaxy is None:
        return {
            "type": "map",
            "schema_version": MAP_SCHEMA_VERSION,
            "present": False,
            "tiles": [],
        }

    tiles = []
    for system_id in sorted(galaxy.coords):
        hex_position = galaxy.coord_of(system_id)
        tiles.append(
            {
                "system": system_id,
                "q": hex_position.q,
                "r": hex_position.r,
            }
        )

    return {
        "type": "map",
        "schema_version": MAP_SCHEMA_VERSION,
        "present": True,
        "wormholes_off": bool(getattr(galaxy, "wormholes_off", False)),
        "wormholes_all_linked": bool(getattr(galaxy, "wormholes_all_linked", False)),
        "tiles": tiles,
    }
