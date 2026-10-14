# F-space-combat-result

Captures: `capture-*.ts` (one per screenshot, importing `../_shared`). Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it (self-contained, ready to publish as an artifact).

Rerun: `cd web && npm run screenshots -- F`

The runner starts the vite dev server on a free port through Playwright, runs this folder's captures against a mocked game (no backend), builds `index.html` and stops the server.
