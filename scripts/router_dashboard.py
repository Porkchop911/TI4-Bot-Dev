#!/usr/bin/env python3
"""Terminal view of per-faction batch progress, read from <run dir>/router_status.json.

Usage: python scripts/router_dashboard.py <run dir> [--interval 2]
Ctrl+C exits.
"""
import argparse
import json
import math
import os
import sys
import time
import re
from datetime import datetime, timezone

CLEAR = "\x1b[2J\x1b[H"
SEATS, FACTION_SEATS = 32, 6  # a faction sits in about 6 of 32 seats


def render(status):
    now = datetime.now(timezone.utc)
    try:
        # The trainer writes UTC with nanoseconds; Python parses at most microseconds.
        stamp = re.sub(r"(\.\d{6})\d+", r"", status["updated"])
        age = f"{(now - datetime.fromisoformat(stamp)).total_seconds():.0f}s old"
    except (KeyError, TypeError, ValueError):
        age = "age ?"
    lines = [
        f"updated {status.get('updated', '?')} ({age})  updates_done={status.get('updates_done', 0)}  "
        f"games_finished={status.get('games_finished', 0)}  "
        f"held={status.get('held', 0)}/{status.get('max_held', 0)}  "
        f"queued_batches={status.get('queued_batches', 0)}",
        "",
        f"{'faction':<12}{'pending/batch':>15}{'to next':>9}{'est games':>11}{'updates':>9}{'mean_vp':>9}  bar",
    ]
    batch = status.get("batch", 0) or 0
    factions = status.get("factions", {})
    rows = []
    for name, f in factions.items():
        pending = f.get("pending", 0)
        remaining = batch - pending
        rows.append((remaining, name, f))
    for remaining, name, f in sorted(rows, key=lambda r: r[0]):
        pending = f.get("pending", 0)
        est = math.ceil(remaining * SEATS / FACTION_SEATS)
        filled = min(20, int(20 * pending / batch)) if batch > 0 else 0
        bar = "#" * filled + "." * (20 - filled)
        lines.append(
            f"{name:<12}{f'{pending}/{batch}':>15}{remaining:>9}{est:>11}"
            f"{f.get('updates', 0):>9}{f.get('mean_vp', 0.0):>9.2f}  [{bar}]"
        )
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir")
    ap.add_argument("--interval", type=float, default=2.0)
    args = ap.parse_args()
    path = os.path.join(args.run_dir, "router_status.json")
    try:
        while True:
            try:
                with open(path, encoding="utf-8") as fh:
                    body = render(json.load(fh))
            except (OSError, json.JSONDecodeError):
                body = f"waiting for {path}"
            sys.stdout.write(CLEAR + body + "\n")
            sys.stdout.flush()
            time.sleep(args.interval)
    except KeyboardInterrupt:
        sys.stdout.write("\n")
        return 0


if __name__ == "__main__":
    sys.exit(main())
