"""Fleet doctrine audit of sol_round1_pilot census snapshots.
Usage: python -I audit_fleet_doctrine.py <results.json> <units.json> <new summary.json>"""
import json, os, statistics, sys
from collections import defaultdict

SOAK = {"fighter", "destroyer"}
HEAVY = {"dreadnought", "warsun"}


def unit_table(path):
    units = json.load(open(path, encoding="utf-8"))
    units = units if isinstance(units, list) else list(units.values())
    by_id = {u["id"]: u for u in units}
    upgrades = defaultdict(list)  # base id -> [(required tech, upgraded unit)]
    for u in units:
        if u.get("upgradesFromUnitId") and u.get("requiredTechId"):
            upgrades[u["upgradesFromUnitId"]].append((u["requiredTechId"], u))
    return by_id, upgrades


def stats(type_id, techs, by_id, upgrades):
    u = by_id.get(type_id)
    if u is None:
        return None
    for tech, up in upgrades.get(type_id, []):
        if tech in techs:
            u = up
    return u


def seat_metrics(seat, by_id, upgrades):
    techs = set(seat["technologies"])
    space, ground = seat["fleet"]["space"], seat["fleet"]["ground"]
    dice = defaultdict(float)
    capacity = cargo = bombard = 0
    unknown = 0
    for tid, n in space.items():
        u = stats(tid, techs, by_id, upgrades)
        if u is None:
            unknown += n
            continue
        base = u.get("baseType", tid)
        if base in ("infantry", "mech"):
            cargo += n  # ground forces riding in space
            continue
        if base == "fighter":
            cargo += n
        # expected hits per round: dice x P(hit)
        p = max(0, 11 - (u.get("combatHitsOn") or 11)) / 10
        dice[base] += n * (u.get("combatDieCount") or 0) * p
        capacity += n * (u.get("capacityValue") or 0)
        bombard += n * (u.get("bombardDieCount") or 0)
    hits = sum(dice.values())
    ships = sum(n for t, n in space.items() if (stats(t, techs, by_id, upgrades) or {}).get("baseType") not in ("infantry", "mech"))
    soak = sum(n for t, n in space.items() if (stats(t, techs, by_id, upgrades) or {}).get("baseType") in SOAK)
    return {
        "hits": round(hits, 2),
        "fighter_hit_share": None if not hits else round(dice["fighter"] / hits, 3),
        "heavy_hit_share": None if not hits else round(sum(dice[b] for b in HEAVY) / hits, 3),
        "flagship_hit_share": None if not hits else round(dice["flagship"] / hits, 3),
        "soak_share": None if not ships else round(soak / ships, 3),
        "capacity": capacity, "cargo": cargo,
        "cargo_fill": None if not capacity else round(min(cargo / capacity, 1.5), 3),
        "bombard_dice": bombard,
        "ground_on_planets": sum(ground.values()),
        "ship_systems": seat["fleet"]["ship_systems"], "largest_stack": seat["fleet"]["largest_stack"],
        "board_tokens": seat["fleet"]["board_tokens"],
        "has_breakthrough": seat.get("breakthrough") is not None,
        "unknown_units": unknown,
        "carriers": sum(n for t, n in space.items() if (stats(t, techs, by_id, upgrades) or {}).get("baseType") == "carrier"),
        "dreadnoughts": sum(n for t, n in space.items() if (stats(t, techs, by_id, upgrades) or {}).get("baseType") == "dreadnought"),
        "fighters": sum(n for t, n in space.items() if (stats(t, techs, by_id, upgrades) or {}).get("baseType") == "fighter"),
    }


def mean(xs):
    xs = [x for x in xs if x is not None]
    return None if not xs else round(statistics.fmean(xs), 3)


def main(src, units_path, dst):
    if os.path.exists(dst):
        sys.exit("summary must be a new file")
    by_id, upgrades = unit_table(units_path)
    report = json.load(open(src))
    rows = defaultdict(list)  # (faction, round) -> metrics
    hero = defaultdict(lambda: defaultdict(int))
    games = 0
    for r in report["results"]:
        if r.get("error") or r["job"]["repeat"]:
            continue
        games += 1
        for snap in r["snapshots"]:
            if snap["label"] != "end-action":
                continue
            for seat in snap["seats"]:
                rows[(seat["faction"], snap["round"])].append(seat_metrics(seat, by_id, upgrades))
        final = r["snapshots"][-1]
        for seat in final["seats"]:
            heroes = {k: v for k, v in seat["leaders"].items() if k.endswith("hero")}
            for status in heroes.values():
                hero[seat["faction"]][status] += 1
    keys = ["hits", "fighter_hit_share", "heavy_hit_share", "flagship_hit_share", "soak_share", "carriers",
            "dreadnoughts", "fighters", "capacity", "cargo", "cargo_fill", "bombard_dice", "ground_on_planets",
            "ship_systems", "largest_stack", "board_tokens", "has_breakthrough", "unknown_units"]
    table = {f"{f}/r{rnd}": {k: mean([m[k] for m in ms]) for k in keys} | {"n": len(ms)} for (f, rnd), ms in sorted(rows.items())}
    summary = {"source": os.path.abspath(src), "games": games, "by_faction_round": table,
               "hero_status_at_cutoff": {f: dict(v) for f, v in hero.items()}}
    with open(dst, "x") as f:
        json.dump(summary, f, indent=1)
    print("games", games)
    cols = ["hits", "fighter_hit_share", "heavy_hit_share", "soak_share", "carriers", "dreadnoughts", "fighters", "cargo_fill", "bombard_dice", "ship_systems", "largest_stack", "has_breakthrough"]
    print("key".ljust(14), *[c[:10].rjust(10) for c in cols])
    for k, v in table.items():
        if k.endswith(("/r2", "/r4")):
            print(k.ljust(14), *[str(v[c]).rjust(10) for c in cols])
    print("hero at cutoff", summary["hero_status_at_cutoff"])


if __name__ == "__main__":
    main(*sys.argv[1:4])
