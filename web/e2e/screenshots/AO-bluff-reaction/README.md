# AO-bluff-reaction

Captures: `capture-*.ts` (importing `../_shared`, plus `_bluff.ts` here). Output: `out/*.png`, listed with captions in `manifest.json`; `index.html` is generated from it (self-contained, ready to publish as an artifact).

Rerun: `cd web && npm run screenshots -- AO`

The captures run the real app against a mocked server (no backend). `_bluff.ts` makes the mock answer `set_reaction_intent` like the real server does (the whole set is accepted and locked until the next round); the hold itself is shown by pushing the same `turn_status` message the server sends. The banner capture asserts that a bluff stall and a real wait render identical markup.
