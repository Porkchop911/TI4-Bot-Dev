"""Validate and summarize production diagnostics; never emit training labels.

Usage: python -B tools/analyze_strategy_probe.py report.json new-summary.json
"""

import collections
import hashlib
import json
import math
from pathlib import Path
import sys


def analyze(path):
    report = json.loads(path.read_text(encoding="utf-8"))
    assert report["schema"] == 1
    assert report["status"] == "production-subcase-diagnostic-not-training-labels"
    results = report["results"]
    expected_cases = {
        ("hacan-production-bottleneck", "funded"),
        ("hacan-production-bottleneck", "more-money"),
        ("hacan-production-bottleneck", "extra-dock"),
        ("jolnar-opening-infantry", "shortage"),
        ("jolnar-opening-infantry", "stocked"),
        ("jolnar-opening-infantry", "no-cash"),
    }
    assert results, "empty probe output"
    streams = {r["stream"] for r in results}
    assert 1 <= len(streams) <= 16
    assert streams == set(range(61016001, 61016001 + len(streams)))
    expected = {(f,v,s,b) for f,v in expected_cases for s in streams for b in ("mlp", "infantry-first", "stop-first")}
    assert {(r["family"],r["variant"],r["stream"],r["branch"]) for r in results} == expected, "missing or unexpected case/stream/branch"
    assert len({(r["family"], r["variant"], r["stream"], r["branch"]) for r in results}) == len(results)
    families = {f["id"]: f["faction"] for f in report["catalog"]["families"]}
    groups = collections.defaultdict(list)
    failures = []
    head_counts = collections.Counter()
    for result in results:
        assert result["family"] in families
        if "error" in result:
            failures.append(result)
            continue
        assert result["faction"] == families[result["family"]]
        assert result["final_fleet_excess"] == result["final_capacity_excess"] == 0
        assert result["replay_equal"] is True
        assert result["inference_decisions"] == len(result["decisions"])
        root = [d for d in result["decisions"] if d["observation"]["root"]]
        assert len(root) == 1
        for decision in result["decisions"]:
            head_counts[decision["head"]] += 1
            options = decision["options"]
            ids = [o["option"]["id"] for o in options]
            observation = decision["observation"]
            assert decision["proposed_id"] == observation["proposed_id"]
            assert observation["proposed_id"] in ids and observation["executed_id"] in ids
            assert all(math.isfinite(o["logit"]) for o in options)
            assert all(math.isfinite(o["probability"]) and 0 <= o["probability"] <= 1 for o in options)
            assert abs(sum(o["probability"] for o in options) - 1) < 1e-8
            for option in options:
                facts = option["facts"]
                assert len(facts) == len(option["columns"]) == len(option["values"])
                assert [f["column"] for f in facts] == option["columns"]
                # Actual sparse input casts factual f64 values to f32.
                assert all(math.isclose(f["value"], v, rel_tol=1e-6, abs_tol=1e-7) for f, v in zip(facts, option["values"]))
        root = root[0]
        options = sorted(root["options"], key=lambda o: (-o["logit"], o["option"]["id"]))
        infantry = [o for o in options if "infantry" in o["option"]["payload"].get("unit", "")]
        produced = collections.Counter(unit for unit, _ in result["produced"])
        # Home contains only its deployed owner's units in these fixtures; retaining all
        # identities avoids assuming that newly produced interchangeable units survive.
        final_ships = collections.Counter(u["type_id"] for u in result["final_home"]["units"])
        initial_ships = collections.Counter(u["type_id"] for u in result["initial_home"]["units"])
        root_facts = [f for o in options for f in o["facts"]]
        groups[(result["faction"], result["variant"], result["branch"])].append({
            "stream": result["stream"], "capacity": result["capacity"],
            "discount": result["discount"], "unused_capacity": result["unused_capacity"],
            "produced": dict(produced), "root_selected": root["observation"]["executed_id"],
            "final_ships": dict(final_ships), "initial_ships": dict(initial_ships),
            "units_removed_by_limits": result["units_removed_by_limits"],
            "root_mlp_proposed": root["proposed_id"],
            "root_logit_ranking": [{"id": o["option"]["id"], "logit": o["logit"], "probability": o["probability"]} for o in options],
            "root_infantry_probability": sum(o["probability"] for o in infantry),
            "root_infantry_best_rank": min((i + 1 for i, o in enumerate(options) if o in infantry), default=None),
            "root_fleet_supply": root["details"].get("fleet_supply"),
            "root_unknown_names": sorted({f["name"] for f in root_facts if not f["assigned"]}),
            "root_feature_occurrences": len(root_facts),
            "root_unknown_occurrences": sum(not f["assigned"] for f in root_facts),
            "trade_goods_remaining": result["after"]["trade_goods"],
            "infantry_before": result["before"]["infantry"], "infantry_after": result["after"]["infantry"],
        })
    grouped = []
    for (faction, variant, branch), rows in sorted(groups.items()):
        assert len({r["stream"] for r in rows}) == len(rows)
        grouped.append({"faction": faction, "variant": variant, "branch": branch, "rows": rows})
    # A forced root must be the only divergence before the first production outcome.
    successful = {(r["family"],r["variant"],r["stream"],r["branch"]): r for r in results if "error" not in r}
    for (family, variant, stream, branch), result in successful.items():
        base = successful.get((family, variant, stream, "mlp"))
        if base is None:
            continue
        assert result["initial_home"] == base["initial_home"]
        assert result["initial_public_objectives"] == base["initial_public_objectives"]
        assert result["initial_own_secrets"] == base["initial_own_secrets"]
        assert result["decisions"][0]["options"] == base["decisions"][0]["options"]
        if branch == "stop-first":
            assert not result["produced"]
        if branch == "infantry-first":
            assert result["produced"] and "infantry" in result["produced"][0][0]
    return {"schema": 1, "status": "diagnostic-not-training-labels", "input": str(path),
            "input_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "branch_results": len(results), "successful": len(results)-len(failures),
            "failures": failures, "ordinary_decisions": dict(sorted(head_counts.items())),
            "groups": grouped,
            "limits": "Synthetic production windows; streams share states. Not independent maps, strategic labels, objective gains or gameplay strength. Unknown vocabulary names alone do not establish a defect."}


if __name__ == "__main__":
    source, destination = map(Path, sys.argv[1:])
    summary = analyze(source)
    with destination.open("x", encoding="utf-8") as output:
        json.dump(summary, output, indent=2, ensure_ascii=False, allow_nan=False)
        output.write("\n")
    print(f'{summary["successful"]}/{summary["branch_results"]} successful; all recorded decisions validated')
    for group in summary["groups"]:
        row = group["rows"][0]
        print(group["faction"], group["variant"], group["branch"], "capacity", row["capacity"],
              "produced", row["produced"], "unused", row["unused_capacity"],
              "removed", row["units_removed_by_limits"], "kept", row["final_ships"],
              "infantry_probability", round(row["root_infantry_probability"], 6),
              "infantry_rank", row["root_infantry_best_rank"])
