# D-planet-selection

Captures: `capture-*.ts` (one per screenshot, importing `../_shared`; `payment.ts` holds the payment fixture and the Pick on map step shared by the 5x captures). Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it (self-contained, ready to publish as an artifact).

Rerun: `cd web && npm run screenshots -- D`

The runner starts the vite dev server on a free port through Playwright, runs this folder's captures against a mocked game (no backend), builds `index.html` and stops the server.

Shots 5a to 5e show paying on the map: the list, payable planets highlighted with their value, a partial payment, Auto-pay, and the list agreeing with the map.
