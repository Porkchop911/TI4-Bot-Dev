You review ONE finished game of the TI4 offline single-player mode, played overnight by a script
(one headless browser, the rules engine as WebAssembly in the page, rule bots in the other seats, no
server). Nobody watched it; the script only records evidence and applies crude checks. You are the
second pair of eyes for a sample of runs: judge whether the evidence shows a real problem the crude
checks missed or mis-rated, and write the final report entry.

Run directory: {{RUN_DIR}}
Machine entry (what the script wrote): {{ENTRY}}

You may only read files (Read, Grep, Glob). Start with the machine entry, then, as far as it is
worth it (at most about 12 tool calls): `trace/report.json`, `trace/failure.txt`, `trace/browser-errors.json`,
the screenshots in `trace/shots/` (Read shows images; look at the ones named in a finding, the round
screenshots and `game-over.png`), the end of `trace/events.jsonl` and `trace/trace.jsonl`, and `run.log`.
Look for: layout that is clipped, overlapping or unreadable; wrong or odd numbers; wording that shows raw
engine ids; a decision whose options make no sense; a game that ended oddly (final standing, victory
points, round); slow phases. Separate real bugs from harness noise (a click that missed and succeeded on
retry is noise).

Your final answer (and nothing else) is the report entry, in exactly the shape of the machine entry:
keep its heading line shape `## Run <name> — <verdict>` (the name is the one in the machine entry;
verdict "clean" or the most severe problem), keep the first four bullet lines (facts, engine, repro,
replay) unchanged, then `### Potential bugs` (severity blocker/high/medium/low, category, what happened,
exact evidence, likely origin), `### Interesting events`, `### Repairs` ("none"), `### UI observations`
(what the screenshots show, with file names), `### Fix requests` ("none") and `### Proctor notes`.
Say in Proctor notes that this entry is a reviewed machine entry.
