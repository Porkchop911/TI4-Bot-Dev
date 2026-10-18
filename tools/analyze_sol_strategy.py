"""Validate paired Sol replays and describe outcomes; produce no teacher labels."""
import collections
import hashlib
import json
import math
from pathlib import Path
import sys


def analyze(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["schema"] == 1 and data["status"] == "sol-reachable-diagnostic-not-training-labels"
    assert data["pool_role"] == "train" and data["sources"] == "DEFAULT/FULL (includes TE)"
    expected = {(s, d) for s in range(data["seed_base"], data["seed_base"] + data["seeds"]) for d in (False, True)}
    key = lambda r: (r["seed"], r["diplomacy"])
    baseline = {key(r): r for r in data["baseline"]}
    assert len(baseline) == len(data["baseline"]) and set(baseline) == expected
    selected = {(r["seed"],r["diplomacy"],t["index"]):t for r in data["selections"] for t in r["targets"]}
    assert len(selected) == sum(len(r["targets"]) for r in data["selections"]), "duplicate selection"
    wanted = {(s,d,i,a) for (s,d,i),t in selected.items() for a in (t["take"],t["skip"])}
    branches = {(r["seed"],r["diplomacy"],r["intervention"][0]["index"],r["intervention"][1]):r for r in data["branches"]}
    assert len(branches) == len(data["branches"]) and set(branches) == wanted
    controls = {(r["seed"],r["diplomacy"],r["target"]):r["equal"] for r in data["natural_controls"]}
    assert len(controls) == len(data["natural_controls"]) and set(controls) == set(selected)
    integrity_failures = []
    for (s,d,i),t in selected.items():
        b = baseline[s,d]
        c = branches[s,d,i,t["natural"]]
        equal = c.get("error") is None and c.get("initial_hash") == b.get("initial_hash") and c.get("outcome") == b.get("outcome") and c.get("timeline") == b.get("timeline")
        assert controls[s,d,i] == equal, "recorded control disagrees with raw comparison"
        if not equal:
            integrity_failures.append({"seed":s,"diplomacy":d,"index":i,"reason":"natural-action replay did not reproduce baseline"})
    failures = []
    for label, runs in (("baseline",data["baseline"]),("branch",data["branches"])):
        for run in runs:
            if run.get("error") is not None:
                failures.append({"stage":label,"seed":run["seed"],"diplomacy":run["diplomacy"],"intervention":run.get("intervention"),"error":run["error"]})
                continue
            assert run["outcome"]["reached_cutoff"] or run["outcome"]["finished"]
            assert all("Ok" in status and status["Ok"] > 0 for status in run["inference"])
            for root in run["roots"]:
                ids = [o["option"]["id"] for o in root["actor"]["options"]]
                assert root["target"]["natural"] in ids and root["executed"] in ids
                assert root["actor"]["chosen"] == root["target"]["natural"]
                assert abs(sum(o["probability"] for o in root["actor"]["options"]) - 1) < 1e-8
                for option in root["actor"]["options"]:
                    assert math.isfinite(option["logit"]) and math.isfinite(option["probability"])
                    assert 0 <= option["probability"] <= 1
                    assert [f["column"] for f in option["facts"]] == option["columns"]
                    assert len(option["facts"]) == len(option["values"])
                    assert all(math.isclose(f["value"],v,rel_tol=1e-6,abs_tol=1e-7) for f,v in zip(option["facts"],option["values"]))
    comparisons = []
    metrics = ("planets_gained","capacity_ships","infantry","victory_points","scoreable_public","scoreable_secret","technologies_gained","trade_goods")
    for (seed,dip,index), target in selected.items():
        take = branches[(seed,dip,index,target["take"])]
        skip = branches[(seed,dip,index,target["skip"])]
        base = baseline[(seed,dip)]
        root = next(r for r in base["roots"] if r["target"]["index"] == index)
        row = {"seed":seed,"diplomacy":dip,"round":target["round"],"kind":target["kind"],"cost":target["cost"],"index":index,
               "natural":target["natural"],"take":target["take"],"skip":target["skip"],"has_gravity_drive":root["view"]["has_gravity_drive"],
               "root_resources":root["view"]["resources"],"root_strategic_tokens":root["view"]["strategic_tokens"],"complete":take.get("error") is None and skip.get("error") is None and controls[seed,dip,index]}
        row["root_card"] = root["choice"].get("details",{}).get("card")
        row["root_costs_token"] = root["choice"].get("details",{}).get("costs_token")
        for run, answer in ((take,target["take"]),(skip,target["skip"])):
            if run.get("error") is not None:
                continue
            assert run["initial_hash"] == base["initial_hash"]
            actual = next(r for r in run["roots"] if r["target"]["index"] == index)
            assert actual["target"] == target and actual["executed"] == answer and run["target_applied"]
        if row["complete"]:
            take_final = take["outcome"]["snapshots"][-1]
            skip_final = skip["outcome"]["snapshots"][-1]
            row["take_final"] = take_final
            row["skip_final"] = skip_final
            row["take_minus_skip"] = {m:take_final["progress"][m]-skip_final["progress"][m] for m in metrics}
            row["take_round1_gd"] = any(s["label"] == "end-action/pre-status" and s["round"] == 1 and "gd" in s["technologies"] for s in take["outcome"]["snapshots"])
            row["skip_round1_gd"] = any(s["label"] == "end-action/pre-status" and s["round"] == 1 and "gd" in s["technologies"] for s in skip["outcome"]["snapshots"])
            ck = lambda snap: (snap["label"],snap["round"],snap["phase"])
            ts = {ck(s):s for s in take["outcome"]["snapshots"] if s["round"] >= target["round"]}
            ss = {ck(s):s for s in skip["outcome"]["snapshots"] if s["round"] >= target["round"]}
            row["matched_checkpoints"] = [{"label":k[0],"round":k[1],"phase":k[2],"take_progress":ts[k]["progress"],"skip_progress":ss[k]["progress"],"take_minus_skip":{m:ts[k]["progress"][m]-ss[k]["progress"][m] for m in metrics},"take_technologies":ts[k]["technologies"],"skip_technologies":ss[k]["technologies"],"take_public_progress":ts[k]["public_progress"],"skip_public_progress":ss[k]["public_progress"]} for k in sorted(ts.keys() & ss.keys())]
            row["take_after_root"] = [t for t in take["timeline"] if t["index"] >= index and t["round"] == target["round"]]
            row["skip_after_root"] = [t for t in skip["timeline"] if t["index"] >= index and t["round"] == target["round"]]
        comparisons.append(row)
    round1 = []
    gd_timing = []
    for b in data["baseline"]:
        snapshots = b.get("outcome",{}).get("snapshots",[])
        r1 = next((s for s in snapshots if s["label"] == "end-action/pre-status" and s["round"] == 1),None)
        if r1:
            round1.append({"seed":b["seed"],"diplomacy":b["diplomacy"],"has_gd":"gd" in r1["technologies"],"strategy_cards":[t["executed"]["id"] for t in b["timeline"] if t["round"] == 1 and t["executed"]["kind"] == "strategy_card"]})
        if b.get("error") is None:
            first = next((s["round"] for s in snapshots if s["label"] == "end-action/pre-status" and "gd" in s["technologies"]),None)
            gd_timing.append({"seed":b["seed"],"diplomacy":b["diplomacy"],"first_end_action_with_gd":first})
    return {"schema":1,"status":"sol-reachable-diagnostic-not-labels","mode":data.get("mode","research"),"input_sha256":hashlib.sha256(path.read_bytes()).hexdigest(),
            "baseline_attempts":len(baseline),"baseline_failures":sum(b.get("error") is not None for b in baseline.values()),
            "selected_gates":len(selected),"paired_attempts":len(branches),"exact_natural_controls":sum(controls.values()),"failures":failures,"integrity_failures":integrity_failures,
            "gate_round_counts":dict(collections.Counter(str(c["round"]) for c in comparisons)),"comparisons":comparisons,
            "baseline_round1":round1,"baseline_gd_timing_complete_games":gd_timing,
            "missing_gate_origins":[{"seed":s,"diplomacy":d,"error":baseline[s,d].get("error")} for s,d in sorted(expected) if not any(k[:2]==(s,d) for k in selected)],
            "limits":"Small conditional diagnostic; gates/rounds/diplomacy share origin maps. Take-minus-skip is one decision under unchanged subsequent policies, not optimal expansion or proof a universal guide rule wins. Failure selection remains explicit; cutoff VP is not a win."}


if __name__ == "__main__":
    source,destination = map(Path,sys.argv[1:])
    summary = analyze(source)
    with destination.open("x",encoding="utf-8") as out:
        json.dump(summary,out,indent=2,ensure_ascii=False,allow_nan=False)
        out.write("\n")
    print({k:v for k,v in summary.items() if k not in ("comparisons","input_sha256","limits","baseline_round1","baseline_gd_timing_complete_games")})
    for c in summary["comparisons"]:
        print(c["seed"],c["diplomacy"],"R",c["round"],c["kind"],"has_gd",c["has_gravity_drive"],"natural",c["natural"],"take",c["take"],c.get("take_minus_skip","FAILED"))
