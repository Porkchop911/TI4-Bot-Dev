# I-command-tokens

Captures: `capture-*.ts` (one per screenshot, importing `../_shared`). Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it (self-contained, ready to publish as an artifact).

Rerun: `cd web && npm run screenshots -- I`

The runner starts the vite dev server on a free port through Playwright, runs this folder's captures against the dev decision gallery (no backend), builds `index.html` and stops the server.
