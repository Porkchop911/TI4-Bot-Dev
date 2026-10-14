You are reviewing the results of last night's automated UI smoke sweep of a Twilight Imperium 4
web client (React/TypeScript frontend in `web/`, Rust engine in `crates/ti4-engine`, server in
`crates/ti4-server`). Sonnet proctors ran random-click playthroughs (random seeds) until round
{{STOP_ROUND}} and appended one entry per run to the night report.

Night report: {{REPORT}}
Run directories (raw evidence: trace/, run.log, digest.md, server-data/): {{NIGHT_DIR}}
Repository: {{REPO}}

Your job (read-only — do NOT modify, create or delete any file, do NOT commit, do NOT implement fixes):
1. Read the night report. Verify the important findings against the raw evidence in the run
   directories (failure.txt, run.log, trace.jsonl, digest.md) — proctors can be wrong.
2. De-duplicate findings across runs and rank them: blockers (game cannot continue), engine/
   server errors, JavaScript errors, server rejections of offered controls, rules
   inconsistencies, harness/test-infrastructure problems.
3. For each real issue, locate the likely root cause in the code (read the relevant source) and
   **suggest** a fix: files, functions, what to change, and a test that would catch it. Say
   how confident you are. If a fix is not necessary (noise, expected behaviour), say so.
4. Briefly summarise game coverage: how many runs, rounds reached, notable events (action cards
   played, Mecatol Rex taken, technologies, agendas), and decision subtypes never reached.

Agenda coverage (counted mechanically from the runs' decision counters; the line is also put on top of
your output, so do not repeat it there, but do act on it): {{AGENDA_LINE}}
If it says no agenda phase happened, make that the first finding of the TL;DR: the `+agenda` preset
suffix and the custodians are the first suspects. In "## Coverage" add an "agenda phase: N runs"
line, and name which runs had votes and with which preset.

Output only the morning summary in markdown, starting with a 3–5 line TL;DR, then
"## Findings" (ranked), "## Suggested fixes", "## Coverage", "## Harness / sweep issues".

Proctors may have committed minor repairs to the branch `{{FIX_BRANCH}}` (see `git log --oneline` on it; its first commit is the pre-sweep snapshot of the working tree). Two Opus fix rounds also ran: round 1 at 23:59 (earlier if a proctor asked, see the report) while the sweep continued, round 2 after the sweep. Their branches are `{{FIXER1_BRANCH}}` and `{{FIXER2_BRANCH}}` (they may not exist if a round was skipped), their reports `{{NIGHT_ROOT}}/fixer-1.md` and `{{NIGHT_ROOT}}/fixer-2.md`, and the report lists whether each was merged into `{{FIX_BRANCH}}` (marker files `{{NIGHT_ROOT}}/.fixer-N.merged`, `.rejected`, `.conflict`, `fixer-N.ready` = not merged yet). Add a "## Overnight fixes" section: every proctor and fixer commit with a one-line judgement, which findings they address, which findings remain open (including the fixers' "needs the user's decision" items), and say all of it is unreviewed and unpushed.{{SLOTS_NOTE}}
