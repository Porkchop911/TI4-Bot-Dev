"""Classify Sol's status-phase scoring windows from sol_round1_pilot snapshots.
Usage: python -I classify_scoring_windows.py <results.json> <arm> [<new summary.json>]

Window k = status phase k, judged from the end-action snapshot of round k (just before it):
  scored          a revealed public Sol had not scored appears in the next snapshot's scored list
  satisfied-missed nothing scored although a revealed public was satisfied
  near-miss       nothing satisfied; best public one step short (count) or >= 75% of cost
  out-of-reach    everything else
"""
import json, os, sys
from collections import Counter, defaultdict


def near(p):
    gap = p["threshold"] - p["have"]
    if p["family"].startswith("cost_"):
        return p["have"] >= 0.75 * p["threshold"]
    return gap <= 1


def main(src, arm, dst=None):
    report = json.load(open(src))
    games = [x for x in report["results"] if not x["error"] and not x["job"]["repeat"] and x["job"]["arm"] == arm]
    windows = defaultdict(Counter)
    near_family = Counter()
    secret_missed = Counter()
    for x in games:
        snaps = [s for s in x["snapshots"] if s["label"] == "end-action"]
        nxt = snaps[1:] + [x["snapshots"][-1]]
        for a, b in zip(snaps, nxt):
            k = a["round"]
            before = set(a["scored_public"])
            open_ = [p for p in a["objectives"] if p["alias"] not in before]
            gained = set(b["scored_public"]) - before
            if any(p["alias"] in gained for p in open_):
                kind = "scored"
            elif any(p["satisfied"] for p in open_):
                kind = "satisfied-missed"
            elif any(near(p) for p in open_):
                kind = "near-miss"
                for p in open_:
                    if near(p):
                        near_family[p["family"]] += 1
            else:
                kind = "out-of-reach"
            windows[k][kind] += 1
            if any(s["satisfied"] and s["alias"] not in gained and s["alias"] not in before for s in a.get("secrets", [])):
                secret_missed[k] += 1
    n = len(games)
    print(f"{arm}: {n} games")
    kinds = ["scored", "satisfied-missed", "near-miss", "out-of-reach"]
    print("status".ljust(8), *[k.rjust(17) for k in kinds], "secret satisfied+unscored".rjust(27))
    for k in sorted(windows):
        print(str(k).ljust(8), *[f"{windows[k][t] / n:17.2f}" for t in kinds], f"{secret_missed[k] / n:27.2f}")
    print("near-miss families:", near_family.most_common(10))
    if dst:
        if os.path.exists(dst):
            sys.exit("summary must be a new file")
        with open(dst, "x") as f:
            json.dump({"source": os.path.abspath(src), "arm": arm, "games": n,
                       "windows": {k: dict(v) for k, v in windows.items()},
                       "near_miss_families": dict(near_family), "secret_satisfied_unscored": dict(secret_missed)}, f, indent=1)


if __name__ == "__main__":
    main(*sys.argv[1:4])
