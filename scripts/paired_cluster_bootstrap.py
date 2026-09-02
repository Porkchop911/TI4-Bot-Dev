"""Paired cluster bootstrap over map seeds, for two policies measured on identical seeds.

Why this rather than a confidence interval on each percentage, or a plain McNemar test:

- The two measurements are *paired*. Every seat-game was played from the same seed, rotation and
  faction under both policies, so the difference is a within-pair quantity and treating the two
  percentages as independent throws that away.
- The pairs are *not independent of each other*. One base map seed contributes 36 seat-games -- six
  rotations by six seats -- which share topology, slice and opponents. McNemar assumes discordant
  pairs are independent and would be optimistic here.

So the resampling unit is the map seed, and every one of its 36 paired seat-games travels with it.
Rotations and seats are never deduplicated: they are correlated observations, not duplicates, and
the bootstrap is how that correlation is paid for rather than assumed away.

Usage:
    python scripts/paired_cluster_bootstrap.py out/paired/champion.txt out/paired/cloned.txt
"""

import collections
import random
import sys


def load(path):
    """seat-game key -> cleared, from a `clearance_eval --per-seat` file."""
    rows = {}
    with open(path, encoding="utf-8") as handle:
        header = next(handle)
        if not header.startswith("seed rotation faction cleared"):
            raise SystemExit(f"{path}: unexpected header {header!r}")
        for line in handle:
            seed, rotation, faction, cleared = line.split()
            rows[(int(seed), int(rotation), faction)] = int(cleared)
    return rows


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    baseline_path, candidate_path = sys.argv[1], sys.argv[2]
    baseline, candidate = load(baseline_path), load(candidate_path)

    # A missing or extra seat-game means the two runs did not cover the same games, and a paired
    # analysis over them would be silently comparing different things.
    if baseline.keys() != candidate.keys():
        only_a = len(baseline.keys() - candidate.keys())
        only_b = len(candidate.keys() - baseline.keys())
        raise SystemExit(
            f"the two files do not cover the same seat-games: {only_a} only in the first, "
            f"{only_b} only in the second"
        )

    # Group the paired differences by map seed, which is the cluster.
    by_map = collections.defaultdict(list)
    for key, cleared in baseline.items():
        by_map[key[0]].append(candidate[key] - cleared)

    maps = sorted(by_map)
    per_map = {seed: sum(diffs) / len(diffs) for seed, diffs in by_map.items()}
    seat_games = sum(len(d) for d in by_map.values())
    observed = sum(sum(d) for d in by_map.values()) / seat_games * 100.0

    base_rate = sum(baseline.values()) / len(baseline) * 100.0
    cand_rate = sum(candidate.values()) / len(candidate) * 100.0

    print(f"  maps (clusters)      {len(maps)}")
    print(f"  seat-games per map   {seat_games // len(maps)}")
    print(f"  seat-games total     {seat_games}")
    print()
    print(f"  {baseline_path:<34} {base_rate:6.2f}%")
    print(f"  {candidate_path:<34} {cand_rate:6.2f}%")
    print(f"  paired difference                  {observed:+6.2f} pp")
    print()

    draws = 10_000
    rng = random.Random(20_260_902)
    means = []
    for _ in range(draws):
        # Resample whole maps with replacement; each carries all 36 of its paired seat-games.
        total = 0.0
        for _ in range(len(maps)):
            total += per_map[maps[rng.randrange(len(maps))]]
        means.append(total / len(maps) * 100.0)
    means.sort()
    low = means[int(0.025 * draws)]
    high = means[int(0.975 * draws)]
    print(f"  map-cluster bootstrap 95% CI       [{low:+.2f}, {high:+.2f}] pp   ({draws} draws)")

    # Companion sign-flip test at the cluster level: under the null, a map's difference is as
    # likely to have come out negative. This is a randomisation test, so it makes no distributional
    # assumption beyond exchangeability of the per-map differences.
    values = [per_map[seed] for seed in maps]
    actual = abs(sum(values) / len(values))
    at_least = 0
    for _ in range(draws):
        flipped = sum(v if rng.random() < 0.5 else -v for v in values) / len(values)
        if abs(flipped) >= actual:
            at_least += 1
    print(f"  cluster sign-flip test             p = {(at_least + 1) / (draws + 1):.4f}")

    if low > 0.0:
        print("\n  The interval excludes zero.")
    else:
        print("\n  The interval includes zero: not established.")


if __name__ == "__main__":
    main()
