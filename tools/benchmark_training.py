"""Compare Rust and Python training throughput under the fixed M00-012 protocol.

Runs the same workload — one training generation: play the games, credit the decisions, apply one
update — in both implementations, interleaved, and reports whether the difference is measurable.

What the protocol fixes, and this obeys:

* 30 timed samples per implementation, none discarded.
* A deterministic balanced order: Python first on even pairs, Rust first on odd ones, so thermal
  drift and background load fall on both sides equally instead of on whichever ran second.
* The same seed for both sides of a pair.
* Monotonic nanoseconds.
* Variance thresholds fixed in advance (training throughput: stdev/mean <= 10%, (p95-p50)/median
  <= 20%). A run outside them is rejected, not narrated.

Two honesty constraints are built in rather than left to the reader.

**Games, not seeds.** The Python trainer builds one rollout per seed *and seat rotation*, so three
factions turn four seeds into twelve games. Asking both sides for "four" would have compared four
Rust games against twelve Python ones. The orchestrator matches the total game count instead.

**Per decision, not only per generation.** The two sides raise different numbers of decisions in
the same game, and the reason is worth stating precisely because two plausible explanations are
both wrong.

Measured on one seed, four rounds, three seats: Rust raises 188 decisions, the oracle about 358.
Both play rounds 1 to 4, so the games are not shorter. The learned policy records 188 of 188, so
nothing is being answered without the policy seeing it. And it is not action-card availability: a
player with fewer playable cards still faces the same "play a card?" question, only with fewer
options on it.

What it actually is, in two parts. Swapping only the policy — blank learned for the authored
scored bot, same engine and seed — takes Rust from 188 decisions to 245, and makes `load which
unit` (22) and `commit ground forces` (12) appear where there had been none. A uniformly-random
policy declines to load and declines to move, so it never reaches committing, production or
payment. The rest is engine surface: against the oracle's per-game figures this side raises no
production, payment, scoring, ability, development or agenda decisions at all, and 18 trade
decisions against 66.

So per game compares equal games in which the two sides do unequal amounts of work, and per
decision compares unequal mixes of work by two policies that do not behave alike. Both are
printed. Neither is "the speedup", and a clean like-for-like figure needs parity.

Usage:
    python tools/benchmark_training.py --games 12 --seats 3 --pairs 30
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_BINARY = REPO_ROOT / "target" / "release" / "examples" / "bench_generation.exe"
PYTHON_RUNNER = REPO_ROOT / "tools" / "bench_generation.py"

#: M00-012e, training throughput row.
MAX_STDEV_OVER_MEAN = 0.10
MAX_P95_MINUS_P50_OVER_MEDIAN = 0.20


def _run(command: list[str]) -> dict:
    finished = subprocess.run(
        command, capture_output=True, text=True, cwd=REPO_ROOT, check=False
    )
    for line in reversed(finished.stdout.strip().splitlines()):
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
    return {"gate": "fail", "nanos": 0, "games": 0, "decisions": 0, "error": finished.stderr[-400:]}


def rust_sample(seed: int, games: int, seats: int) -> dict:
    return _run([str(RUST_BINARY), "--seed", str(seed), "--games", str(games), "--seats", str(seats)])


def python_sample(seed: int, games: int, seats: int) -> dict:
    # Seeds, not games: the trainer turns each seed into one game per seat rotation.
    seeds = max(1, games // max(1, seats))
    return _run([sys.executable, str(PYTHON_RUNNER), "--seed", str(seed), "--games", str(seeds), "--seats", str(seats)])


def percentile(sorted_values: list[float], fraction: float) -> float:
    if not sorted_values:
        return 0.0
    rank = max(1, min(len(sorted_values), int(-(-fraction * len(sorted_values) // 1))))
    return sorted_values[rank - 1]


def summarise(samples: list[dict]) -> dict:
    times = sorted(float(s["nanos"]) for s in samples)
    decisions = sum(int(s["decisions"]) for s in samples)
    games = sum(int(s["games"]) for s in samples)
    mean = statistics.fmean(times) if times else 0.0
    stdev = statistics.pstdev(times) if len(times) > 1 else 0.0
    median = statistics.median(times) if times else 0.0
    p95 = percentile(times, 0.95)
    stable = bool(
        times
        and mean > 0
        and median > 0
        and stdev / mean <= MAX_STDEV_OVER_MEAN
        and (p95 - median) / median <= MAX_P95_MINUS_P50_OVER_MEDIAN
    )
    return {
        "samples": len(samples),
        "games": games,
        "decisions": decisions,
        "min_nanos": min(times) if times else 0,
        "max_nanos": max(times) if times else 0,
        "mean_nanos": mean,
        "median_nanos": median,
        "stdev_nanos": stdev,
        "p95_nanos": p95,
        "p99_nanos": percentile(times, 0.99),
        "nanos_per_decision": (sum(times) / decisions) if decisions else 0.0,
        "nanos_per_game": (sum(times) / games) if games else 0.0,
        "stable": stable,
        "execution_gate": "pass" if samples and all(s.get("gate") == "pass" for s in samples) else "fail",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--games", type=int, default=12, help="total games per sample, both sides")
    parser.add_argument("--seats", type=int, default=3)
    parser.add_argument("--pairs", type=int, default=30, help="the protocol fixes this at 30")
    parser.add_argument("--seed", type=int, default=0, help="manifest seed; pair i uses seed + i")
    parser.add_argument("--warmup", action="store_true", default=True)
    args = parser.parse_args()

    if not RUST_BINARY.exists():
        print(
            f"build the Rust side first:\n"
            f"  cargo build -p ti4-training --example bench_generation --release\n"
            f"expected at {RUST_BINARY}",
            file=sys.stderr,
        )
        return 2

    print(f"workload: one training generation, {args.games} games, {args.seats} seats, stage 2")
    print(f"protocol: {args.pairs} interleaved pairs, seed {args.seed}+i, nothing discarded\n")

    if args.warmup:
        print("warmup (10 unmeasured iterations each, not reported)...", flush=True)
        subprocess.run(
            [str(RUST_BINARY), "--seed", "900000", "--games", str(args.games),
             "--seats", str(args.seats), "--warmup", "1"],
            capture_output=True, cwd=REPO_ROOT, check=False,
        )
        subprocess.run(
            [sys.executable, str(PYTHON_RUNNER), "--seed", "900000",
             "--games", str(max(1, args.games // max(1, args.seats))),
             "--seats", str(args.seats), "--warmup", "1"],
            capture_output=True, cwd=REPO_ROOT, check=False,
        )
        time.sleep(5)  # the protocol's idle period before timed samples begin

    rust: list[dict] = []
    python: list[dict] = []
    for pair in range(args.pairs):
        seed = args.seed + pair
        # Balanced order: neither side is always second, so drift is shared.
        if pair % 2 == 0:
            first = python_sample(seed, args.games, args.seats)
            second = rust_sample(seed, args.games, args.seats)
            python.append(first)
            rust.append(second)
        else:
            first = rust_sample(seed, args.games, args.seats)
            second = python_sample(seed, args.games, args.seats)
            rust.append(first)
            python.append(second)
        print(
            f"  pair {pair + 1:>2}/{args.pairs}: "
            f"rust {rust[-1]['nanos'] / 1e6:8.1f} ms  python {python[-1]['nanos'] / 1e6:9.1f} ms",
            flush=True,
        )

    reports = {"rust": summarise(rust), "python": summarise(python)}
    print("\n" + "=" * 78)
    for name, row in reports.items():
        print(
            f"{name:7} mean {row['mean_nanos'] / 1e6:9.1f} ms   median {row['median_nanos'] / 1e6:9.1f} ms   "
            f"stdev/mean {row['stdev_nanos'] / max(row['mean_nanos'], 1):.3f}   "
            f"{'stable' if row['stable'] else 'UNSTABLE'}  execution={row['execution_gate']}"
        )
        print(
            f"        {row['games']} games, {row['decisions']} decisions   "
            f"{row['nanos_per_game'] / 1e6:.2f} ms/game   {row['nanos_per_decision'] / 1e3:.1f} us/decision"
        )

    execution_valid = all(row["stable"] and row["execution_gate"] == "pass" for row in reports.values())
    # A clean process and a non-empty trajectory are execution gates, not semantic parity.  The
    # engines currently raise different legal decision windows and this benchmark carries no
    # decision-boundary trace proving equivalent work.  It may report timings, but it must not turn
    # them into a migration speedup claim.
    semantic_parity = False
    print("=" * 78)
    if execution_valid and semantic_parity:
        by_decision = reports["python"]["nanos_per_decision"] / max(reports["rust"]["nanos_per_decision"], 1)
        by_game = reports["python"]["nanos_per_game"] / max(reports["rust"]["nanos_per_game"], 1)
        print(f"speedup, per decision: {by_decision:.1f}x   per game: {by_game:.1f}x")
        print(
            "read the per-decision figure first: the engines are not at parity, so the Rust side\n"
            "plays shorter games and part of any per-game gap is missing content rather than speed."
        )
    elif execution_valid:
        print(
            "TIMING DIAGNOSTIC ONLY: both runners were stable and executed successfully, but\n"
            "semantic parity was not established. No Rust/Python speedup is claimed. Run the\n"
            "Stage-1 solved-profile and decision-boundary gates first."
        )
    else:
        print(
            "NOT COMPARABLE: a report failed its variance threshold or its semantic gate.\n"
            "The protocol says repeat one fresh run; if that also fails the result is rejected."
        )

    out = REPO_ROOT / ".backup" / "benchmark_training.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps(
            {
                "schema_version": "1.0.0",
                "benchmark_id": "training_generation",
                "workload": {"games": args.games, "seats": args.seats, "pairs": args.pairs, "seed": args.seed},
                "reports": reports,
                "execution_valid": execution_valid,
                "semantic_parity": semantic_parity,
                "raw": {"rust": rust, "python": python},
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    print(f"\nraw samples: {out}")
    return 0 if execution_valid else 1


if __name__ == "__main__":
    raise SystemExit(main())
