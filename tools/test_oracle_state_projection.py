"""Focused read-only verification for M00-009b's state projection."""

from __future__ import annotations

import json
import os
import sys
import unittest
from dataclasses import replace
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ORACLE_ROOT = Path(r"D:\Projects\ti4-engine")
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
sys.dont_write_bytecode = True
sys.path.insert(0, str(REPO_ROOT))
sys.path.insert(0, str(ORACLE_ROOT))

from engine.game import start_game  # noqa: E402
from engine.state import SystemState  # noqa: E402
from engine.units import Unit  # noqa: E402
from oracle_exporter import state_projection  # noqa: E402


class StateProjectionTests(unittest.TestCase):
    def test_projection_is_canonical_and_hides_private_card_identities(self) -> None:
        state = start_game(("beta", "alpha"), deck_seed=17)
        state = replace(
            state,
            board={
                "z": SystemState(
                    units=(Unit("fighter", "beta"), Unit("carrier", "alpha")),
                    command_tokens=frozenset({"beta", "alpha"}),
                    planet_control={"zeta": "beta", "alpha": "alpha"},
                    planet_units={"zeta": (Unit("infantry", "beta"),)},
                ),
                "a": SystemState(units=(Unit("dreadnought", "alpha", True),)),
            },
        )

        projection = state_projection(state)

        self.assertEqual(projection["turn"], 0)
        self.assertEqual(projection["phase"], "strategy")
        self.assertEqual([system["id"] for system in projection["systems"]], ["a", "z"])
        self.assertEqual(
            projection["systems"][1]["units"],
            [
                {"type_id": "carrier", "owner": "alpha", "damage": False},
                {"type_id": "fighter", "owner": "beta", "damage": False},
            ],
        )
        self.assertEqual(projection["players"][0]["id"], "beta")
        self.assertEqual(projection["players"][0]["controlled_planets"], ["zeta"])
        self.assertNotIn("action_cards", projection["players"][0])
        self.assertNotIn("secret_objectives", projection["players"][0])

    def test_same_seed_state_serializes_byte_identically(self) -> None:
        first = state_projection(start_game(("p1", "p2"), deck_seed=42))
        second = state_projection(start_game(("p1", "p2"), deck_seed=42))

        first_json = json.dumps(first, separators=(",", ":"), ensure_ascii=True)
        second_json = json.dumps(second, separators=(",", ":"), ensure_ascii=True)
        self.assertEqual(first_json, second_json)


if __name__ == "__main__":
    unittest.main()
