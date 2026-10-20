# Project notes

## UI work

- Every new UI implementation must also ship an artifact visualizing it: screenshots of the new
  layout (all meaningful variants), published as a private Artifact and linked in the report.
  If screenshots can't be taken, say so rather than faking them.
- The screenshots must be reproducible: add `capture-*.ts` scripts in a new folder under
  `web/e2e/screenshots/` (next free letter, with `manifest.json`), following the existing folders
  and `_shared/` helpers (`openMockedGame`, `shot`, `build-artifact.mjs`). Run them with
  `cd web && npm run screenshots -- <letter>`; see `web/e2e/screenshots/README.md`. Publish the
  artifact from the generated page, not from ad-hoc screenshots.
