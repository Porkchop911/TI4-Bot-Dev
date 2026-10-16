# Q-turn-redo

The turn redo UI: the event log's "Redo my last turn" controls and the status strip for a redo in flight (`GET/POST /api/games/{id}/turn-redo`, mocked in `redoMock.ts`).
Captures: `capture-*.ts`, one per screenshot, importing `../_shared`. Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it.

Rerun: `cd web && npm run screenshots -- Q`
