#!/usr/bin/env python3
"""Per-objective score rates of a stored `selfplay compare` run: candidate seats against baseline
seats, for the aliases given. Unconditional (scored in a seat-game / seat-games) and, for games
stored with the held-secret ids (`timeline.snaps[*].players[*].secret_ids`), conditional on the
seat having been dealt the secret (held in an end-of-round snapshot or scored).

  tools/rulebot/backward_rates.py results/<key>.jsonl uf baf fwp mew
"""
import json
import math
import sys

path, aliases = sys.argv[1], sys.argv[2:]
cand = {a: 0 for a in aliases}
base = {a: 0 for a in aliases}
dealt_c = {a: 0 for a in aliases}
dealt_b = {a: 0 for a in aliases}
scored_dealt_c = {a: 0 for a in aliases}
scored_dealt_b = {a: 0 for a in aliases}
n_c = n_b = 0
for line in open(path):
    try:
        g = json.loads(line)
    except ValueError:
        continue
    if "scored" not in g:
        continue
    for seat, scored in enumerate(g["scored"]):
        is_c = g["candidate_seats"][seat]
        if is_c:
            n_c += 1
        else:
            n_b += 1
        held = set()
        has_ids = False
        for snap in (g.get("timeline") or {}).get("snaps", []):
            seats = snap.get("seats", [])
            if seat < len(seats) and "secret_ids" in seats[seat]:
                has_ids = True
                held.update(seats[seat]["secret_ids"])
        for a in aliases:
            if a in scored:
                (cand if is_c else base)[a] += 1
            if a in held or a in scored:
                if has_ids:
                    (dealt_c if is_c else dealt_b)[a] += 1
                    if a in scored:
                        (scored_dealt_c if is_c else scored_dealt_b)[a] += 1
print(f"seat-games: candidate {n_c}, baseline {n_b}")
for a in aliases:
    pc, pb = cand[a] / max(n_c, 1), base[a] / max(n_b, 1)
    se = math.sqrt(pc * (1 - pc) / max(n_c, 1) + pb * (1 - pb) / max(n_b, 1))
    print(f"{a:6s} candidate {cand[a]:4d} ({100*pc:5.2f}%)  baseline {base[a]:4d} ({100*pb:5.2f}%)  diff {100*(pc-pb):+.2f} pp  (+-{196*se:.2f})")

if any(dealt_c.values()) or any(dealt_b.values()):
    print("conditional on being dealt the secret (held in a snapshot or scored):")
    for a in aliases:
        dc, db = dealt_c[a], dealt_b[a]
        pc = scored_dealt_c[a] / max(dc, 1)
        pb = scored_dealt_b[a] / max(db, 1)
        se = math.sqrt(pc * (1 - pc) / max(dc, 1) + pb * (1 - pb) / max(db, 1))
        print(f"{a:6s} candidate {scored_dealt_c[a]:3d}/{dc:3d} ({100*pc:5.1f}%)  baseline {scored_dealt_b[a]:3d}/{db:3d} ({100*pb:5.1f}%)  diff {100*(pc-pb):+.1f} pp (+-{196*se:.1f})")
