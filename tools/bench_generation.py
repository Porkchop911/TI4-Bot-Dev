"""One training generation in the Python oracle, timed to the M00-012 protocol.

The counterpart to `cargo run -p ti4-training --example bench_generation`. Emits a single JSON
sample on stdout in the same shape, so an orchestrator can interleave the two implementations and
compare them under the fixed protocol rather than by running each to completion in turn.

The unit is a whole generation — play the games, credit the decisions, apply one update — because
that is what a training run is made of. Measuring only rollouts would flatter whichever side has
the cheaper gradient, and measuring only the update would flatter whichever has the slower engine.

Never writes to the oracle. It imports the pinned oracle read-only with bytecode writing disabled,
and takes no argument that would cause it to save a profile or a checkpoint.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
import time
from pathlib import Path

ORACLE_ROOT = Path(r"D:\Projects\ti4-engine")
TRAINER = ORACLE_ROOT / "tools" / "train_stage1_policy_gradient.py"

#: The six factions this project plays. Faction scope is a separate decision from content scope,
#: and the benchmark has to seat the same table as the Rust side or it is not a comparison.
IN_SCOPE_FACTIONS = ("sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x")


def _load_trainer():
    """Import the oracle's trainer without writing anything into the oracle."""

    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    sys.dont_write_bytecode = True
    if str(ORACLE_ROOT) not in sys.path:
        sys.path.insert(0, str(ORACLE_ROOT))
    spec = importlib.util.spec_from_file_location("bench_trainer", TRAINER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load the oracle trainer at {TRAINER}")
    module = importlib.util.module_from_spec(spec)
    # Registered before execution: the trainer's dataclasses resolve their own module on creation,
    # and fail with an unhelpful AttributeError if it is not in sys.modules yet.
    sys.modules["bench_trainer"] = module
    spec.loader.exec_module(module)
    return module


def generation(trainer, seed: int, games: int, seats: int) -> tuple[int, int] | None:
    """Play, credit, update. Returns (games, decisions), or None if the sample is invalid."""

    from engine import learned_policy

    factions = tuple(IN_SCOPE_FACTIONS[:seats])
    # The explicit-head schema, not the flat hashed one: the trainer keys its updates by
    # (faction, head) and raises KeyError on a flat profile. This is the same requirement that put
    # per-head weights on the Rust `Profile`.
    profiles = {
        faction: learned_policy.blank_explicit_profile(faction) for faction in factions
    }
    seeds = tuple(range(seed, seed + games))

    episodes = trainer.rollouts(
        profiles,
        factions,
        seeds=seeds,
        workers=1,
        vary_maps=False,
        capture=True,
        horizon=4,
    )
    if not episodes:
        return None

    decisions = sum(
        len(episode.get("trajectory") or ())
        for game in episodes
        for episode in game.values()
    )
    if decisions == 0:
        return None

    reward = trainer.Reward(stage=2)
    trainer.update_profiles(
        profiles,
        episodes,
        learning_rate=0.05,
        entropy=0.01,
        gradient_clip=1.0,
        reward=reward,
    )
    return len(episodes), decisions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--games", type=int, default=4)
    parser.add_argument("--seats", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=0)
    args = parser.parse_args()

    trainer = _load_trainer()

    if args.warmup:
        # The protocol's ten unmeasured iterations, same shape, not reported.
        for index in range(10):
            try:
                generation(trainer, args.seed + index, args.games, args.seats)
            except Exception:  # noqa: BLE001 - a failed warmup is reported, not raised
                break
        print(json.dumps({"warmup": 10}))
        return 0

    started = time.perf_counter_ns()
    try:
        outcome = generation(trainer, args.seed, args.games, args.seats)
    except Exception as error:  # noqa: BLE001 - a failure is a failed sample, not a crashed run
        outcome = None
        print(f"sample failed: {type(error).__name__}: {error}", file=sys.stderr)
    nanos = time.perf_counter_ns() - started

    if outcome is None:
        sample = {
            "pair": 0,
            "seed": args.seed,
            "nanos": nanos,
            "games": 0,
            "decisions": 0,
            "gate": "fail",
        }
    else:
        played, decisions = outcome
        sample = {
            "pair": 0,
            "seed": args.seed,
            "nanos": nanos,
            "games": played,
            "decisions": decisions,
            "gate": "pass",
        }
    print(json.dumps(sample))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
