"""Paired map-seed cluster bootstrap for clearance_eval per-seat VP TSVs."""

import json
import math
import random
import sys
import tempfile
from pathlib import Path


ORIGINAL_SIX = {"sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"}
GROUPS = ("original_six", "new", "table")
HEADER = "seed rotation faction cleared victory_points"
DRAW_COUNT = 1000
RNG_SEED = 20261011


def load(path):
    rows = {}
    with open(path, encoding="utf-8") as stream:
        if stream.readline().split() != HEADER.split():
            raise ValueError(f"{path}: expected exact header {HEADER!r}")
        for line_number, line in enumerate(stream, 2):
            fields = line.split()
            if len(fields) != 5:
                raise ValueError(f"{path}:{line_number}: expected five fields")
            seed, rotation, faction, cleared_text, vp_text = fields
            try:
                seed, rotation = int(seed), int(rotation)
                cleared = int(cleared_text)
                vp = float(vp_text)
            except ValueError as error:
                raise ValueError(f"{path}:{line_number}: invalid numeric field") from error
            if seed < 0 or rotation < 0 or cleared not in (0, 1) or not math.isfinite(vp):
                raise ValueError(f"{path}:{line_number}: invalid seed, rotation, clearance, or VP")
            key = (seed, rotation, faction)
            if key in rows:
                raise ValueError(f"{path}:{line_number}: duplicate seat-game key {key!r}")
            rows[key] = vp
    if not rows:
        raise ValueError(f"{path}: no seat-game rows")
    return rows


def validate_expected_eval(rows, seed_base, seed_count, rotations, seats):
    expected_seeds = set(range(seed_base, seed_base + seed_count))
    actual_seeds = {key[0] for key in rows}
    if actual_seeds != expected_seeds:
        raise ValueError("expected-eval seed range mismatch")
    expected_rotations = set(range(rotations))
    actual_rotations = {key[1] for key in rows}
    if actual_rotations != expected_rotations:
        raise ValueError("expected-eval rotation range mismatch")
    by_game = {}
    for seed, rotation, faction in rows:
        by_game.setdefault((seed, rotation), set()).add(faction)
    expected_games = seed_count * rotations
    if len(by_game) != expected_games or any(len(factions) != seats for factions in by_game.values()):
        raise ValueError(f"expected-eval requires exactly {seats} factions per seed/rotation")


def paired_report(baseline, candidate):
    if baseline.keys() != candidate.keys():
        only_base = len(baseline.keys() - candidate.keys())
        only_candidate = len(candidate.keys() - baseline.keys())
        raise ValueError(f"paired keys differ: {only_base} only in baseline, {only_candidate} only in candidate")

    sums = {group: {"baseline": 0.0, "candidate": 0.0, "count": 0} for group in GROUPS}
    for (seed, _rotation, faction), base_vp in baseline.items():
        cand_vp = candidate[(seed, _rotation, faction)]
        groups = ("table", "original_six" if faction in ORIGINAL_SIX else "new")
        for group in groups:
            arm = sums[group]
            arm["baseline"] += base_vp
            arm["candidate"] += cand_vp
            arm["count"] += 1

    # Rebuild per-seed numerator/denominator from paired rows; clusters with no rows in a faction group remain absent.
    cluster = {group: {} for group in GROUPS}
    for (seed, _rotation, faction), base_vp in baseline.items():
        cand_vp = candidate[(seed, _rotation, faction)]
        for group in ("table", "original_six" if faction in ORIGINAL_SIX else "new"):
            item = cluster[group].setdefault(seed, [0.0, 0])
            item[0] += cand_vp - base_vp
            item[1] += 1

    maps = sorted({key[0] for key in baseline})
    rng = random.Random(RNG_SEED)
    boot = {group: [] for group in GROUPS}
    for _ in range(DRAW_COUNT):
        sampled = [maps[rng.randrange(len(maps))] for _ in maps]
        for group in GROUPS:
            numerator = denominator = 0
            for seed in sampled:
                n, d = cluster[group].get(seed, (0.0, 0))
                numerator += n
                denominator += d
            if denominator == 0:
                raise ValueError(f"bootstrap produced zero denominator for {group}")
            boot[group].append(numerator / denominator)

    report = {"maps": len(maps), "seat_games": len(baseline), "bootstrap": {"resamples": DRAW_COUNT, "rng_seed": RNG_SEED, "cluster": "map seed", "interval": "95% percentile; evaluation-only"}, "groups": {}}
    for group in GROUPS:
        values = sorted(boot[group])
        arm = sums[group]
        if arm["count"] == 0:
            raise ValueError(f"zero denominator for {group}")
        report["groups"][group] = {
            "baseline_mean_vp": arm["baseline"] / arm["count"],
            "candidate_mean_vp": arm["candidate"] / arm["count"],
            "paired_delta_vp": (arm["candidate"] - arm["baseline"]) / arm["count"],
            "paired_delta_ci95_vp": [values[int(0.025 * DRAW_COUNT)], values[int(0.975 * DRAW_COUNT)]],
            "seat_games": arm["count"],
        }
    return report


def self_test():
    def fixture(rows):
        temp = tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", delete=False)
        temp.write(HEADER + "\n" + "\n".join(rows) + "\n")
        temp.close()
        return Path(temp.name)

    files = []
    try:
        base = ["1 0 sol 0 2", "1 0 newfaction 1 4", "2 0 sol 1 3", "2 0 newfaction 0 5"]
        same_a, same_b = fixture(base), fixture(base)
        files += [same_a, same_b]
        result = paired_report(load(same_a), load(same_b))
        assert all(item["paired_delta_vp"] == 0 for item in result["groups"].values())

        shifted = fixture([row.rsplit(" ", 1)[0] + " " + str(int(row.rsplit(" ", 1)[1]) + 1) for row in base])
        files.append(shifted)
        result = paired_report(load(same_a), load(shifted))
        assert all(item["paired_delta_vp"] == 1 for item in result["groups"].values())

        mismatch = fixture(base[:-1])
        files.append(mismatch)
        try:
            paired_report(load(same_a), load(mismatch))
            raise AssertionError("mismatched keys accepted")
        except ValueError as error:
            assert "paired keys differ" in str(error)

        duplicate = fixture(base + [base[0]])
        files.append(duplicate)
        try:
            load(duplicate)
            raise AssertionError("duplicate row accepted")
        except ValueError as error:
            assert "duplicate seat-game" in str(error)

        shape_rows = {
            (seed, rotation, faction): 1.0
            for seed in (100, 101)
            for rotation in (0, 1)
            for faction in ("sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x")
        }
        validate_expected_eval(shape_rows, 100, 2, 2, 6)
        try:
            validate_expected_eval(dict(list(shape_rows.items())[:-1]), 100, 2, 2, 6)
            raise AssertionError("truncated expected-eval input accepted")
        except ValueError as error:
            assert "exactly 6 factions" in str(error)
        no_rotation = {key: value for key, value in shape_rows.items() if key[1] == 0}
        try:
            validate_expected_eval(no_rotation, 100, 2, 2, 6)
            raise AssertionError("missing expected rotation accepted")
        except ValueError as error:
            assert "rotation range mismatch" in str(error)
    finally:
        for path in files:
            path.unlink(missing_ok=True)
    print("self-test: PASS")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    args = sys.argv[1:]
    expected_eval = len(args) == 3 and args[2] == "--expected-eval"
    if len(args) not in (2, 3) or (len(args) == 3 and not expected_eval):
        raise SystemExit(f"Usage: python {Path(sys.argv[0]).name} [--self-test | baseline.tsv candidate.tsv [--expected-eval]]")
    baseline, candidate = load(args[0]), load(args[1])
    if expected_eval:
        for rows in (baseline, candidate):
            validate_expected_eval(rows, 900000000, 600, 6, 6)
    report = paired_report(baseline, candidate)
    print(json.dumps(report, indent=2, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
