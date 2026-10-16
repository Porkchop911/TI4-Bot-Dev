"""Paired read-out of sol_round1_pilot reports. Usage: python -I analyze_sol_round1_pilot.py <report.json> <new summary.json>"""
import json, os, random, statistics, sys
from collections import Counter, defaultdict

COMPARE = [("tech-gd", "tech-elsewhere"), ("tech-elsewhere", "control"), ("trade", "control"), ("military-support", "control"), ("carrier-doctrine", "control"), ("spend-down", "control"), ("no-early-pass", "control")]
EFFECT = 0.3  # VP difference the full-run size estimate is for


def end_action(r, rnd):
    return next((s for s in r["snapshots"] if s["label"] == "end-action" and s["round"] == rnd), None)


def metrics(r):
    final = r["snapshots"][-1]
    m = {"vp": final["vp"], "objectives": len(final["scored_public"])}
    for rnd in (1, 2):
        s = end_action(r, rnd)
        m[f"gd_r{rnd}"] = None if s is None else int(s["has_gd"])
    s = end_action(r, 2)
    m["planets_r2"] = None if s is None else s["planets"]
    m["resources_r2"] = None if s is None else s["resources"]
    seats = final["seats"]
    m["rank"] = 1 + sum(o["vp"] > final["vp"] for o in seats[1:])
    return m


def boot(origin_means, n=10000):
    rng = random.Random(20261016)
    k = len(origin_means)
    draws = sorted(statistics.fmean(rng.choices(origin_means, k=k)) for _ in range(n))
    return [draws[int(0.025 * n)], draws[int(0.975 * n) - 1]]


def main(src, dst):
    if os.path.exists(dst):
        sys.exit("summary must be a new file")
    report = json.load(open(src))
    runs = [r for r in report["results"] if not r["job"]["repeat"]]
    status = Counter()
    ok = {}
    for r in runs:
        j = r["job"]
        st = "error" if r.get("error") else "unavailable" if r.get("unavailable") else "complete"
        status[(j["arm"], st)] += 1
        if st == "complete":
            ok[(j["arm"], j["origin"], j["dice"])] = r
    arms = report["arms"]
    per_arm = {}
    for a in arms:
        rs = [r for (arm, _, _), r in ok.items() if arm == a]
        ms = [metrics(r) for r in rs]
        mean = lambda k: None if not ms or any(m[k] is None for m in ms) else round(statistics.fmean(m[k] for m in ms), 3)
        per_arm[a] = {
            "complete": len(rs), "error": status[(a, "error")], "unavailable": status[(a, "unavailable")],
            **{k: mean(k) for k in ("vp", "objectives", "rank", "gd_r1", "gd_r2", "planets_r2", "resources_r2")},
            "overrides_per_game": round(statistics.fmean(len(r["overrides"]) for r in rs), 2) if rs else None,
            "followed_tech": Counter(json.dumps(r["followed_tech"] and r["followed_tech"]["offered_yes"]) for r in rs),
            "gd_forced": sum(r["gd_forced"] for r in rs),
            "reserve_leaks": sum(len(r["leaks"]) for r in rs),
            "ms_opened": sum(sum(1 for e in r["ms_log"] if e.get("opened")) for r in rs),
            "ms_ends": Counter(e["end"] for r in rs for e in r["ms_log"] if "end" in e),
            "ms_responses": Counter(e["response"] for r in rs for e in r["ms_log"] if "response" in e),
        }
    comparisons = []
    for a, b in COMPARE:
        if a not in arms or b not in arms:
            continue
        by_origin = defaultdict(list)
        for (arm, o, d), r in ok.items():
            if arm == a and (b, o, d) in ok:
                ma, mb = metrics(r), metrics(ok[(b, o, d)])
                by_origin[o].append({k: (None if ma[k] is None or mb[k] is None else ma[k] - mb[k]) for k in ma})
        pairs = [p for ps in by_origin.values() for p in ps]
        row = {"arm": a, "versus": b, "pairs": len(pairs), "origins": len(by_origin)}
        if pairs:
            for k in ("vp", "objectives", "rank", "gd_r1", "gd_r2", "planets_r2", "resources_r2"):
                vals = [p[k] for p in pairs if p[k] is not None]
                row[k] = round(statistics.fmean(vals), 3) if vals else None
            om = [statistics.fmean(p["vp"] for p in ps) for ps in by_origin.values()]
            row["vp_ci95_origins"] = [round(x, 3) for x in boot(om)] if len(om) > 1 else None
            vp = [p["vp"] for p in pairs]
            sd = statistics.stdev(vp) if len(vp) > 1 else None
            row["vp_pair_sd"] = None if sd is None else round(sd, 3)
            row["vp_sign"] = {"+": sum(v > 0 for v in vp), "-": sum(v < 0 for v in vp), "0": sum(v == 0 for v in vp)}
            # Paired normal approximation, 80% power, two-sided 5%: n = ((1.96+0.84) sd / effect)^2.
            row[f"pairs_for_{EFFECT}vp"] = None if not sd else round((2.8 * sd / EFFECT) ** 2)
        comparisons.append(row)
    summary = {"source": os.path.abspath(src), "determinism": report["determinism"], "per_arm": per_arm, "comparisons": comparisons,
               "note": "diagnostic; dependent dice seeds within origin; CI resamples origins; not training labels"}
    with open(dst, "x") as f:
        json.dump(summary, f, indent=1, default=dict)
    print(json.dumps({"determinism_ok": all(d["equal"] for d in report["determinism"])}))
    for a, v in per_arm.items():
        print(a, {k: v[k] for k in ("complete", "error", "unavailable", "vp", "gd_r1", "gd_r2", "planets_r2", "overrides_per_game")})
    for c in comparisons:
        print(c)


if __name__ == "__main__":
    main(*sys.argv[1:3])
