# Project notes

## How to work here

- The main session stays brief: answer questions, plan, coordinate and report. Most or all actual
  work (investigation, code changes, tests, screenshots, research, merges) is delegated to
  subagents, each in its own git worktree and branch. Only do a task inline when it is a
  one-liner or a quick lookup.

## UI work

- Every new UI implementation must also ship an artifact visualizing it: screenshots of the new
  layout (all meaningful variants), published as a private Artifact and linked in the report.
  If screenshots can't be taken, say so rather than faking them.
- The screenshots must be reproducible: add `capture-*.ts` scripts in a new folder under
  `web/e2e/screenshots/` (next free letter, with `manifest.json`), following the existing folders
  and `_shared/` helpers (`openMockedGame`, `shot`, `build-artifact.mjs`). Run them with
  `cd web && npm run screenshots -- <letter>`; see `web/e2e/screenshots/README.md`. Publish the
  artifact from the generated page, not from ad-hoc screenshots.

## Alpha feedback

- Testers' reports from the public alpha are files in `/var/lib/ti4-public/feedback/<date>/`
  (the in-game feedback button is built); read them with `tools/public/list_feedback.sh`. See
  `docs/FEEDBACK_BUTTON_NOTES.md`. When triaging, replay the game id from the report's context.

## Time

- We use Berlin time (Europe/Berlin). Whenever you talk about "today", "tonight" or the nightly
  sweep, check the clock first (`TZ=Europe/Berlin date`) instead of assuming. Nightly names
  (`nightly-fixes-<night>`) use the date the sweep started, 20:30 Berlin.
- The nightly sweep exists to test the LATEST version: whatever lands in the live checkout's
  branch before the start (20:30 Berlin), even a minute before, is part of that night's sweep.
