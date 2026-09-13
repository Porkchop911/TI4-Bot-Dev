"""Select complete captured games into an immutable training corpus.

The input remains a temporary, complete capture. Outcome-derived selection reasons live only in
the selection index; game and decision records are copied byte-for-object without adding quality
labels to the raw trajectories.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import os
import random
from collections import defaultdict
from pathlib import Path

import zstandard


SCHEMA = "ti4-offline-selfplay-selection-v1"
GAMES = "games.jsonl.zst"
DECISIONS = "decisions.jsonl.zst"
SELECTION = "selection.jsonl.zst"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def read_jsonl(path: Path):
    with path.open("rb") as source:
        with zstandard.ZstdDecompressor().stream_reader(source) as decoded:
            with io.TextIOWrapper(decoded, encoding="utf-8") as text:
                for number, line in enumerate(text, 1):
                    try:
                        yield json.loads(line)
                    except json.JSONDecodeError as error:
                        raise ValueError(f"{path}:{number}: {error}") from error


class JsonlWriter:
    def __init__(self, path: Path):
        self._raw = path.open("wb")
        self._compressed = zstandard.ZstdCompressor(level=9).stream_writer(self._raw)
        self._text = io.TextIOWrapper(self._compressed, encoding="utf-8", newline="\n")
        self.count = 0

    def write(self, value) -> None:
        self._text.write(json.dumps(value, separators=(",", ":"), ensure_ascii=False))
        self._text.write("\n")
        self.count += 1

    def close(self) -> int:
        self._text.flush()
        self._text.detach()
        self._compressed.close()
        self._raw.close()
        return self.count


def total_vp(game: dict) -> int:
    return sum(seat["final_progress"]["victory_points"] for seat in game["seats"])


def maximum_vp(game: dict) -> int:
    return max(seat["final_progress"]["victory_points"] for seat in game["seats"])


def select(games: list[dict], seed: int) -> tuple[dict[str, set[str]], dict]:
    complete = [game for game in games if game["completed"] and game["error"] is None]
    if not complete:
        raise ValueError("the source corpus contains no complete games")
    quota = max(1, math.ceil(len(complete) * 0.10))
    reasons: dict[str, set[str]] = defaultdict(set)

    strongest = sorted(complete, key=lambda game: (-total_vp(game), game["game_id"]))[:quota]
    weakest = sorted(complete, key=lambda game: (total_vp(game), game["game_id"]))[:quota]
    for game in strongest:
        reasons[game["game_id"]].add("strong_table_top_10_percent")
    for game in weakest:
        reasons[game["game_id"]].add("weak_table_bottom_10_percent")
    for game in complete:
        if maximum_vp(game) >= 6:
            reasons[game["game_id"]].add("absolute_standout_6vp")

    policy_games: dict[str, list[dict]] = defaultdict(list)
    for game in complete:
        for policy_id in {seat["policy"]["policy_id"] for seat in game["seats"]}:
            policy_games[policy_id].append(game)
    for policy_id, candidates in sorted(policy_games.items()):
        if any(game["game_id"] in reasons for game in candidates):
            continue
        chosen = max(candidates, key=lambda game: (total_vp(game), game["game_id"]))
        reasons[chosen["game_id"]].add(f"policy_diversity:{policy_id}")

    unselected = [game for game in complete if game["game_id"] not in reasons]
    control_quota = min(len(unselected), math.ceil(len(complete) * 0.05))
    rng = random.Random(seed)
    for game in rng.sample(unselected, control_quota):
        reasons[game["game_id"]].add("random_control_5_percent")

    metrics = {
        "complete_games": len(complete),
        "strong_table_quota": quota,
        "weak_table_quota": quota,
        "random_control_quota": control_quota,
        "strong_table_minimum_total_vp": min(map(total_vp, strongest)),
        "weak_table_maximum_total_vp": max(map(total_vp, weakest)),
    }
    return reasons, metrics


def run(source: Path, destination: Path, seed: int) -> None:
    if destination.exists():
        raise ValueError(f"{destination} already exists; selected corpora are immutable")
    manifest_path = source / "manifest.json"
    source_manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    games = list(read_jsonl(source / GAMES))
    if len(games) != source_manifest["records"]["games"]:
        raise ValueError("source game count does not match its manifest")
    reasons, metrics = select(games, seed)
    selected_ids = set(reasons)
    if not selected_ids:
        raise ValueError("selection retained no games")

    staging = destination.with_name(f".{destination.name}.staging-{os.getpid()}")
    if staging.exists():
        raise ValueError(f"staging path already exists: {staging}")
    staging.mkdir(parents=True)
    try:
        game_writer = JsonlWriter(staging / GAMES)
        selection_writer = JsonlWriter(staging / SELECTION)
        expected_decisions = 0
        selected_games = []
        for game in games:
            game_id = game["game_id"]
            if game_id not in selected_ids:
                continue
            game_writer.write(game)
            expected_decisions += game["decision_count"]
            selected_games.append(game)
            selection_writer.write(
                {
                    "game_id": game_id,
                    "reasons": sorted(reasons[game_id]),
                    "table_total_vp": total_vp(game),
                    "maximum_faction_vp": maximum_vp(game),
                }
            )
        game_count = game_writer.close()
        selection_count = selection_writer.close()

        decision_writer = JsonlWriter(staging / DECISIONS)
        decisions_by_game: dict[str, int] = defaultdict(int)
        for decision in read_jsonl(source / DECISIONS):
            game_id = decision["game_id"]
            if game_id in selected_ids:
                decision_writer.write(decision)
                decisions_by_game[game_id] += 1
        decision_count = decision_writer.close()
        if decision_count != expected_decisions:
            raise ValueError(
                f"selected decisions {decision_count} != game metadata {expected_decisions}"
            )
        for game in selected_games:
            if decisions_by_game[game["game_id"]] != game["decision_count"]:
                raise ValueError(f"incomplete trajectory for {game['game_id']}")

        shards = {
            name: sha256(staging / name) for name in (GAMES, DECISIONS, SELECTION)
        }
        manifest = dict(source_manifest)
        manifest.update(
            {
                "schema": SCHEMA,
                "parent_manifest_sha256": sha256(manifest_path),
                "parent_corpus": str(source.resolve()),
                "selection": {
                    "strong_table": "top 10% by total table VP",
                    "weak_table": "bottom 10% by total table VP",
                    "absolute_standout": "any faction final VP >= 6",
                    "strong_faction_relative": None,
                    "winning_margin": None,
                    "policy_diversity": "at least one retained game per generated policy variant",
                    "random_control": "5% of otherwise rejected complete games",
                    "seed": seed,
                    **metrics,
                },
                "games": game_count,
                "records": {
                    "games": game_count,
                    "decisions": decision_count,
                    "selection": selection_count,
                },
                "shards": shards,
            }
        )
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )
        staging.rename(destination)
    except Exception:
        # Leave staging in place for diagnosis. Never publish a partial destination.
        raise

    print("offline corpus selection passed")
    print(f"  source games       {len(games)}")
    print(f"  retained games     {game_count}")
    print(f"  retained decisions {decision_count}")
    print(f"  destination        {destination}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=1_026_091_300)
    args = parser.parse_args()
    try:
        run(args.source, args.destination, args.seed)
    except Exception as error:
        raise SystemExit(f"REFUSED: {error}") from error


if __name__ == "__main__":
    main()
