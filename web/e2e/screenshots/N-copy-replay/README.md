# N-copy-replay

The event log's "Copy replay" button (`GET /api/games/{id}/replay`, any seated player).
Captures: `capture-*.ts`, one per screenshot, importing `../_shared`. Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it.

Rerun: `cd web && npm run screenshots -- N`
