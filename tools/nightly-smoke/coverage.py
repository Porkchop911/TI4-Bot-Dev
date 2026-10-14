#!/usr/bin/env python3
"""Which decision subtypes did the smoke runs never offer?

    coverage.py [--repo DIR] [--min-runs N] <reports-dir> [<reports-dir> ...]

Collects every literal decision subtype the engine can offer (the third argument of each
`DecisionContext::new(...)` in crates/*/src) and diffs it against the `subtypes` counters of every
`report.json` found under the report directories. Prints the subtypes no run ever offered, and
those offered in fewer than `--min-runs` runs (default 2), so presets and steering can be aimed at
them. Subtypes built at run time (`format!`, a variable) are not listed.

    coverage.py --agenda-runs <night-dir>     prints "<runs with votes> <runs read>"
    coverage.py --agenda-line <night-dir>     prints the one-line "agenda phase: N of M runs" status
                                              (loud when no run reached the agenda phase)
"""

import argparse
import collections
import glob
import json
import os
import re
import sys

CALL = "DecisionContext::new("
IDENT = re.compile(r"^[a-z0-9_]+$")


def split_args(text, start):
    """The top-level arguments of the call whose '(' is just before `start`, or None."""
    depth, i, args, current = 1, start, [], []
    in_string = False
    while i < len(text):
        ch = text[i]
        if in_string:
            current.append(ch)
            if ch == "\\":
                i += 1
                if i < len(text):
                    current.append(text[i])
            elif ch == '"':
                in_string = False
        elif ch == '"':
            in_string = True
            current.append(ch)
        elif ch in "([{":
            depth += 1
            current.append(ch)
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                args.append("".join(current).strip())
                return args
            current.append(ch)
        elif ch == "," and depth == 1:
            args.append("".join(current).strip())
            current = []
        else:
            current.append(ch)
        i += 1
    return None


def literal_subtypes(source):
    """Literal subtype strings of every DecisionContext::new call in one Rust source text."""
    found = set()
    pos = 0
    while True:
        at = source.find(CALL, pos)
        if at < 0:
            return found
        pos = at + len(CALL)
        args = split_args(source, pos)
        if args and len(args) >= 3:
            match = re.fullmatch(r'"([^"]+)"', args[2])
            if match and IDENT.match(match.group(1)):
                found.add(match.group(1))


def engine_subtypes(repo):
    found = set()
    for path in glob.glob(os.path.join(repo, "crates", "*", "src", "**", "*.rs"), recursive=True):
        try:
            with open(path, encoding="utf-8") as handle:
                found |= literal_subtypes(handle.read())
        except OSError:
            continue
    return found


def report_files(root):
    """Every report.json under root, without walking into the git worktrees a night leaves there
    (fixer-N, slot-K/repo: whole checkouts), node_modules, hidden or target directories."""
    for here, dirs, files in os.walk(root):
        dirs[:] = sorted(d for d in dirs if not d.startswith(".") and d not in ("node_modules", "repo")
                         and not d.startswith(("fixer-", "target")))
        if "report.json" in files:
            yield os.path.join(here, "report.json")


def run_counts(report_dirs):
    """subtype -> number of runs that offered it, and the number of runs read."""
    runs = collections.Counter()
    total = 0
    for root in report_dirs:
        for path in report_files(root):
            try:
                with open(path) as handle:
                    report = json.load(handle)
            except (OSError, ValueError):
                continue
            subtypes = report.get("subtypes")
            if not isinstance(subtypes, dict):
                continue
            total += 1
            for name, count in subtypes.items():
                if count:
                    runs[name] += 1
    return runs, total


# A run reached the agenda phase when the agents were asked to vote.
VOTE_SUBTYPES = ("cast_vote", "vote_tiebreak")


def agenda_runs(night_dir):
    """(runs that were asked to vote, runs read) in one night's directory."""
    voted = total = 0
    for path in report_files(night_dir):
        try:
            with open(path) as handle:
                subtypes = json.load(handle).get("subtypes")
        except (OSError, ValueError, AttributeError):
            continue
        if not isinstance(subtypes, dict):
            continue
        total += 1
        if any(subtypes.get(name) for name in VOTE_SUBTYPES):
            voted += 1
    return voted, total


def agenda_line(night_dir):
    voted, total = agenda_runs(night_dir)
    if total and not voted:
        return (f"**NO AGENDA PHASE HAPPENED TONIGHT: 0 of {total} runs were asked to vote** "
                "(check the `+agenda` preset pick and the custodians)")
    return f"agenda phase: {voted} of {total} runs voted"


def main(argv):
    if len(argv) == 2 and argv[0] in ("--agenda-runs", "--agenda-line"):
        if argv[0] == "--agenda-runs":
            print("%d %d" % agenda_runs(argv[1]))
        else:
            print(agenda_line(argv[1]))
        return 0
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--repo", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    parser.add_argument("--min-runs", type=int, default=2)
    parser.add_argument("reports", nargs="+")
    args = parser.parse_args(argv)
    engine = engine_subtypes(args.repo)
    runs, total = run_counts(args.reports)
    never = sorted(engine - set(runs))
    rare = sorted((n for n in engine & set(runs) if runs[n] < args.min_runs), key=lambda n: (runs[n], n))
    print(f"# Decision subtype coverage over {total} runs\n")
    print(f"{len(engine)} literal engine subtypes; {len(never)} never offered, "
          f"{len(rare)} offered in fewer than {args.min_runs} runs.\n")
    print("## Never offered\n")
    print(", ".join(f"`{n}`" for n in never) or "none")
    print(f"\n## Offered in fewer than {args.min_runs} runs\n")
    print(", ".join(f"`{n}` ({runs[n]})" for n in rare) or "none")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
