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
from oracle_exporter import (
    choice_projection,
    event_projection,
    outcome_projection,
    state_projection,
    view_projection,
)  # noqa: E402


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

    def test_view_projection_preserves_only_the_viewers_private_identities(self) -> None:
        state = start_game(("viewer", "opponent"), deck_seed=23)
        cards = state.action_card_deck
        state = state.with_player(
            "viewer", action_cards=cards[:2], secret_objectives=("viewer-secret",)
        )
        state = state.with_player(
            "opponent", action_cards=cards[2:4], secret_objectives=("opponent-secret",)
        )

        from engine.game import Game

        projection = view_projection(Game(state).view_for("viewer"))
        players = {player["id"]: player for player in projection["players"]}

        self.assertEqual(players["viewer"]["action_cards"], list(cards[:2]))
        self.assertEqual(players["viewer"]["secret_objectives"], ["viewer-secret"])
        self.assertEqual(players["opponent"]["action_cards"], ["?", "?"])
        self.assertEqual(players["opponent"]["secret_objectives"], ["?"])
        self.assertNotIn(cards[2], json.dumps(projection, separators=(",", ":")))
        self.assertNotIn("opponent-secret", json.dumps(projection, separators=(",", ":")))

    def test_same_seed_view_serializes_byte_identically(self) -> None:
        from engine.game import Game

        first = view_projection(Game(start_game(("p1", "p2"), deck_seed=42)).view_for("p1"))
        second = view_projection(Game(start_game(("p1", "p2"), deck_seed=42)).view_for("p1"))

        self.assertEqual(
            json.dumps(first, separators=(",", ":"), ensure_ascii=True),
            json.dumps(second, separators=(",", ":"), ensure_ascii=True),
        )

    def test_choice_projection_preserves_option_order_and_canonicalizes_payload_maps(self) -> None:
        from engine.choice import Choice, Option

        choice = Choice(
            "sol",
            "choose a tactical action",
            (
                Option("second", "action", "second option", {"z": 2, "a": ("x", {"b": 1, "a": 0})}),
                Option("first", "decline", "decline", {"count": 1}),
            ),
        )

        projection = choice_projection(choice)

        self.assertEqual([option["id"] for option in projection["options"]], ["second", "first"])
        self.assertEqual(
            projection["options"][0]["payload"],
            {"a": ["x", {"a": 0, "b": 1}], "z": 2},
        )

    def test_choice_projection_is_byte_identical_and_rejects_unserializable_payloads(self) -> None:
        from engine.choice import Choice, Option

        choice = Choice("sol", "pick", (Option("x", "action", payload={"b": 2, "a": 1}),))
        first = json.dumps(choice_projection(choice), separators=(",", ":"), ensure_ascii=True)
        second = json.dumps(choice_projection(choice), separators=(",", ":"), ensure_ascii=True)
        self.assertEqual(first, second)

        invalid = Choice("sol", "pick", (Option("x", "action", payload={"bad": object()}),))
        with self.assertRaises(TypeError):
            choice_projection(invalid)

    def test_event_projection_captures_post_resolution_context_and_canonical_payload(self) -> None:
        from engine.timing import Event, Phase

        state = start_game(("p1", "p2")).with_(phase=Phase.ACTION, round=3, turn_seq=4)
        event = Event("UNIT_MOVED", {"z": 2, "a": {"b": 1, "a": 0}}, cancelled=True, uid=17)

        projection = event_projection(event, state)

        self.assertEqual(projection["id"], 17)
        self.assertEqual(projection["event_type"], "UNIT_MOVED")
        self.assertEqual(projection["phase"], "action")
        self.assertEqual(projection["turn"], 4)
        self.assertEqual(projection["round"], 3)
        self.assertEqual(projection["payload"], {"a": {"a": 0, "b": 1}, "z": 2})

    def test_event_projection_is_byte_identical_and_rejects_unserializable_payloads(self) -> None:
        from engine.timing import Event

        state = start_game(("p1", "p2"))
        first = event_projection(Event("E", {"b": 2, "a": 1}, uid=3), state)
        second = event_projection(Event("E", {"a": 1, "b": 2}, uid=3), state)
        self.assertEqual(
            json.dumps(first, separators=(",", ":"), ensure_ascii=True),
            json.dumps(second, separators=(",", ":"), ensure_ascii=True),
        )

        with self.assertRaises(TypeError):
            event_projection(Event("E", {"bad": object()}, uid=4), state)

    def test_outcome_projection_requires_a_finished_game_and_uses_initiative_tie_breaking(self) -> None:
        from engine.game import Game

        game = Game(start_game(("second", "first")))
        with self.assertRaises(ValueError):
            outcome_projection(game.state, "victory_points")

        game.state = game.state.with_player("second", victory_points=10)
        game.state = game.state.with_player("first", victory_points=10).with_(finished=True)
        projection = outcome_projection(game.state, "victory_points")

        self.assertTrue(projection["game_over"])
        self.assertEqual(projection["winner"], "second")
        self.assertEqual(projection["victory_points"], {"first": 10, "second": 10})
        self.assertEqual(projection["reason"], "victory_points")

    def test_outcome_projection_is_byte_identical(self) -> None:
        first_state = start_game(("p1", "p2")).with_player("p1", victory_points=10).with_(finished=True)
        second_state = start_game(("p1", "p2")).with_player("p1", victory_points=10).with_(finished=True)

        first = outcome_projection(first_state, "victory_points")
        second = outcome_projection(second_state, "victory_points")
        self.assertEqual(
            json.dumps(first, separators=(",", ":"), ensure_ascii=True),
            json.dumps(second, separators=(",", ":"), ensure_ascii=True),
        )


if __name__ == "__main__":
    unittest.main()
