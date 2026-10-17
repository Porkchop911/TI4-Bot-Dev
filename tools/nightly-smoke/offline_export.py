#!/usr/bin/env python3
"""Wrap an offline run's save as a game file the offline page can import.

    offline_export.py <run-dir> > game.json

The page's start screen has "Import a game" (docs/OFFLINE.md, "Saves, export and import"); the game then
resumes at the last question the run recorded, so a developer can look at the position in a browser. The
file needs the engine identity of the run (trace/report.json); an engine that differs from the page's
makes the page ask what to do (view only is safe).
"""
import json
import os
import sys
import time


def main():
    run = sys.argv[1].rstrip("/")
    save = json.load(open(os.path.join(run, "trace", "save.json")))
    report = json.load(open(os.path.join(run, "trace", "report.json")))
    engine = report.get("engine") or {}
    json.dump({
        "kind": "ti4-offline-game",
        "version": 1,
        "exportedAt": int(time.time()),
        "game": {
            "nickname": "Sweeper",
            "humanFaction": report.get("humanFaction"),
            "createdAt": int(time.time()),
            "engine": {
                "build": engine.get("build") or (save.get("engine") or {}).get("build") or "unknown",
                "contentDigest": engine.get("content_digest") or (save.get("engine") or {}).get("content_digest") or "unknown",
                "wasmSha256": engine.get("wasmSha256") or "unknown",
            },
            "save": save,
        },
    }, sys.stdout)


if __name__ == "__main__":
    main()
