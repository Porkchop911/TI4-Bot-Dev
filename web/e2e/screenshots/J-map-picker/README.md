# J-map-picker

Captures: `capture-*.ts` (one per screenshot, importing `../_shared`). Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it.

Rerun: `cd web && npm run screenshots -- J`

The captures render the real lobby and its MapPicker (`_shared/mapLobby.ts`); only the server's HTTP answers (map catalog, lobby, map choice, map preview) are routed fixtures. Each map choice bumps the revision and re-rolls the pictured board.
