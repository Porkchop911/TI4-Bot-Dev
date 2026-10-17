#!/usr/bin/env python3
"""Turn the evidence of one offline run into a night-report entry.

    offline_entry.py <run-dir> [--terminated REASON]

Reads meta.json, trace/{report.json,failure.txt,events.jsonl,trace.jsonl}, exit_code and run.log
of a run played by offline_game.sh. Writes <run-dir>/digest.md (the mechanical facts) and
<run-dir>/entry.md (the entry in the shape the proctors write, so the fix rounds and the morning
summary read it unchanged) and prints the entry.
"""

import collections
import json
import os
import re
import sys

KEYWORDS = re.compile(
    r"custodian|Mecatol|agenda|law|relic|research|scored|objective|combat|retreat|destroy|played|explor|"
    r"produced|trade|promissory|vote|won|victory|game over",
    re.I,
)
# Severity ranks for the verdict and the order of the findings.
RANK = {"blocker": 0, "high": 1, "medium": 2, "low": 3}


def load(path, default=None):
    try:
        with open(path) as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return default


def jsonl(path):
    rows = []
    try:
        with open(path) as handle:
            for line in handle:
                line = line.strip()
                if line:
                    try:
                        rows.append(json.loads(line))
                    except ValueError:
                        pass
    except OSError:
        pass
    return rows


def read(path, limit=4000):
    try:
        with open(path, errors="replace") as handle:
            return handle.read(limit)
    except OSError:
        return ""


def bugs_of(report, failure, exit_code, terminated):
    """[(severity, category, text)] sorted by severity."""
    out = []
    outcome = report.get("outcome", "no report")
    if terminated:
        out.append(("high", "harness", f"the run was terminated mid-game ({terminated}); decisions so far: {report.get('decisions', '?')}"))
    elif not report:
        out.append(("high", "harness", f"the Playwright run wrote no report.json (exit code {exit_code}); see run.log"))
    for panic in report.get("panics", []):
        out.append(("blocker", "engine error", f"engine trap or crash: {panic}"))
    if outcome == "stuck":
        out.append(("blocker", "stuck game", failure.strip().splitlines()[0] if failure.strip() else "the game stopped advancing"))
    elif outcome == "setup-failed":
        out.append(("blocker", "setup", failure.strip().splitlines()[0] if failure.strip() else "the offline game did not start"))
    elif outcome in ("decision-cap", "time-cap"):
        out.append(("medium", "harness", f"the game did not end ({outcome}): {report.get('decisions')} decisions in {report.get('seconds')} s, "
                                       f"round {(report.get('finalStatus') or {}).get('round')}"))
    for text in report.get("findings", []):
        sev = "blocker" if text.startswith("stuck") else ("high" if text.startswith("resume") else "medium")
        cat = ("stuck game" if text.startswith("stuck") else "resume" if text.startswith("resume") else "layout" if text.startswith("layout")
               else "JavaScript" if "error banner" in text else "UI")
        out.append((sev, cat, text))
    for text in report.get("pageErrors", [])[:10]:
        out.append(("high", "JavaScript", f"uncaught page error: {text}"))
    for text in report.get("consoleErrors", [])[:10]:
        out.append(("high", "JavaScript", f"console error: {text}"))
    for text in report.get("failedRequests", [])[:10]:
        out.append(("low", "harness", f"failed request (offline play should only fetch the built files): {text}"))
    out.sort(key=lambda row: RANK[row[0]])
    return out


def main():
    args = sys.argv[1:]
    terminated = ""
    if "--terminated" in args:
        index = args.index("--terminated")
        terminated = args[index + 1]
        del args[index:index + 2]
    severity_only = "--max-severity" in args
    args = [a for a in args if a != "--max-severity"]
    run_dir = args[0].rstrip("/")
    name = os.path.basename(run_dir)
    meta = load(os.path.join(run_dir, "meta.json"), {})
    trace_dir = os.path.join(run_dir, "trace")
    report = load(os.path.join(trace_dir, "report.json"), {})
    failure = read(os.path.join(trace_dir, "failure.txt"))
    trace = jsonl(os.path.join(trace_dir, "trace.jsonl"))
    events = jsonl(os.path.join(trace_dir, "events.jsonl"))
    exit_code = read(os.path.join(run_dir, "exit_code"), 20).strip() or "still running"
    bugs = bugs_of(report, failure, exit_code, terminated)

    rounds = report.get("roundStarts") or {}
    last_round = max(map(int, rounds), default=(trace[-1].get("round") if trace else "?"))
    engine = report.get("engine") or {}
    shots = (report.get("shots") or {}).get("names", [])
    subtypes = report.get("subtypes") or collections.Counter(row.get("subtype") for row in trace)

    if severity_only:
        # "<blocker|high|medium|low|none> <first finding>" for the runner (request_fix.sh)
        print(f"{bugs[0][0]} {bugs[0][1]}: {bugs[0][2]}" if bugs else "none")
        return
    if bugs:
        sev, cat, text = bugs[0]
        verdict = f"{sev}: {cat}: {text}"
        verdict = re.sub(r"\s+", " ", verdict)[:140]
    else:
        verdict = "clean"
    if terminated:
        verdict = f"sweep terminated mid-run ({terminated}); mechanical digest only"

    seat = report.get("humanSeat")
    head = (
        f"- offline run (no server, wasm engine, rule bots): seed {meta.get('game_seed')} (click seed {meta.get('click_seed')}), "
        f"{meta.get('players')} players, human seat {seat if seat else '?'} ({report.get('humanFaction') or 'dealt by the seed'}), "
        f"policy {meta.get('policy')}, viewport {meta.get('viewport')}, rounds reached {last_round}, "
        f"decisions {report.get('decisions', len(trace))}, duration {report.get('seconds', '?')} s, "
        f"outcome {report.get('outcome', 'none')}, exit status {exit_code}"
    )
    meta_line = (
        f"- engine {engine.get('wasmSha256', '?')} (build {engine.get('build', '?')}), commit {str(meta.get('commit', ''))[:8]}"
        + (f" (+{meta['dirty_files']} uncommitted files)" if meta.get("dirty_files") else "")
        + f", final decisions hash {report.get('finalDecisionsHash') or '?'}"
    )

    out = []
    p = out.append
    p(f"## Run {name} — {verdict}")
    p(head)
    p(meta_line)
    p(f"- repro: `{meta.get('repro', '?')}`")
    p(f"- exact replay of the saved game (native, same engine code): `{meta.get('replay', '?')}` (`trace/save.json` is the save blob: seed, setup, every answer, decisions hash)")
    p("")
    p("### Potential bugs")
    if bugs:
        for sev, cat, text in bugs:
            p(f"- **{sev}** ({cat}): {text}")
        p("")
        p("Evidence: `trace/failure.txt`, `trace/report.json`, `trace/browser-errors.json`, `trace/shots/` (the screenshot named in a finding), `run.log`, `trace/save.json`.")
    else:
        p("none")
    p("")
    p("### Interesting events")
    standings = report.get("finalStandings") or []
    if standings:
        p(f"- final standing: {', '.join(standings)}")
    p(f"- seats: {', '.join(report.get('factions') or []) or '?'}")
    interesting = []
    seen = set()
    for event in events:
        detail = str(event.get("detail", ""))
        if KEYWORDS.search(detail) and detail not in seen:
            seen.add(detail)
            interesting.append(f"r{event.get('round')} {detail}")
    p(f"- {len(events)} game events logged; {len(interesting)} distinct matching: " + ("; ".join(interesting[:30]) or "none"))
    votes = sum(v for k, v in subtypes.items() if k in ("cast_vote", "vote_tiebreak"))
    p(f"- agenda votes cast by the human: {votes}")
    combat = sum(v for k, v in subtypes.items() if "combat" in str(k) or "casualty" in str(k) or k in ("sustain_damage", "announce_retreat", "retreat_to"))
    p(f"- combat-related decisions: {combat}; productions: {subtypes.get('produce_unit', 0)}; transactions: {subtypes.get('propose_transaction', 0) + subtypes.get('answer_transaction', 0)}")
    common = [(k, v) for k, v in sorted(subtypes.items(), key=lambda kv: -kv[1]) if v > 3]
    rare = [(k, v) for k, v in sorted(subtypes.items(), key=lambda kv: -kv[1]) if v <= 3]
    p(f"- decision subtypes: {dict(common)}")
    if rare:
        p(f"- rare decision subtypes (3x or fewer): {dict(rare)}")
    p("")
    p("### Repairs")
    p("none (offline runs have no proctor and make no repairs)")
    p("")
    p("### UI observations")
    layout = [f for f in report.get("findings", []) if f.startswith(("layout", "error banner", "reaction text"))]
    if layout:
        for text in layout:
            p(f"- {text}")
    else:
        p("none seen by the checks (sideways scroll, decision panels outside the window, error banners, raw ids in reaction text)")
    p(f"- screenshots in `trace/shots/` ({len(shots)}): {', '.join(shots[:12])}{' ...' if len(shots) > 12 else ''}")
    p("")
    p("### Fix requests")
    p(read(os.path.join(run_dir, "fix-request.txt"), 600).strip() or "none")
    p("")
    p("### Proctor notes")
    p(f"- machine entry (no proctor). Pace: {report.get('decisionsPerMinute', '?')} decisions/min, engine/bot wait {round(report.get('engineWaitMs', 0) / 1000)} s in total, worst single wait {round(report.get('worstWaitMs', 0) / 1000, 1)} s, JS heap {report.get('heapMb', '?')} MB at the end, recoveries (escape/reload) {report.get('recoveries', 0)}.")
    resume = report.get("resume")
    if resume:
        p(f"- resume check: reloaded the page at decision #{resume.get('at')} ({resume.get('answers')} answers); the game "
          f"{'came back with the same question' if resume.get('ok') else 'did NOT come back as it was'} after {resume.get('ms')} ms")
    elif meta.get("resume_at"):
        p(f"- resume check was planned at decision #{meta.get('resume_at')} but the game ended or stopped before it")
    rss = [int(x) for x in read(os.path.join(run_dir, "rss_kb.log"), 100000).split() if x.isdigit()]
    if rss:
        p(f"- peak memory of the run's browser, engine worker and node (PSS): {round(max(rss) / 1024)} MB")
    p("")
    entry = "\n".join(out)

    digest = [f"### Run digest (offline) — {name}", "", head, meta_line, ""]
    digest.append("#### Findings and errors")
    digest.extend([f"- {sev} ({cat}): {text}" for sev, cat, text in bugs] or ["- none detected"])
    if failure.strip():
        digest += ["", "#### failure.txt", "```", failure.strip()[:3000], "```"]
    digest += ["", "#### Round starts (decisions before each round)", json.dumps(rounds), "", "#### Decision subtypes", json.dumps(dict(subtypes))]
    if events:
        digest += ["", "#### Last events"] + [f"- r{e.get('round')} {e.get('detail')}" for e in events[-15:]]
    with open(os.path.join(run_dir, "digest.md"), "w") as handle:
        handle.write("\n".join(digest) + "\n")
    with open(os.path.join(run_dir, "entry.md"), "w") as handle:
        handle.write(entry + "\n")
    print(entry)


if __name__ == "__main__":
    main()
