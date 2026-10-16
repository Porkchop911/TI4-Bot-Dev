# Remaining todos (written 2026-10-06, 20:10 Berlin)

State at this point: `smoke-playthrough` is pushed (f073cfa) and the working tree is clean. The first
nightly sweep starts at 20:30 Berlin; the first Opus fix round is at 23:59 (or earlier if a proctor
requests it), round 2 follows the sweep (ends 06:00), then the Opus summary. Do not edit the checkout
while the sweep runs: games are built and served from it.

## Morning checklist (2026-10-07)

1. Read `nightly-reports/2026-10-06/summary.md` and the fixer reports (`fixer-1.md`, `fixer-2.md`);
   the checkout will be on branch `nightly-fixes-2026-10-06`, the original branch name is in
   `nightly-reports/2026-10-06/orig_branch`. Review the proctor and fixer commits (unreviewed,
   unpushed), then merge or cherry-pick what you want into `smoke-playthrough` and switch back.
2. The sweep is the first real-world test of the turn action bar, command-token panels, hit panel,
   reaction dialog, map picker, payment on the map, corner toasts and the new decision UIs: expect
   findings there.
3. Run the full test suites once (web, engine, server) on the merged result.
4. A stale branch with tonight's name would be checked out by the sweep instead of a fresh snapshot
   (this happened with the accidental 14:15 sweep; fixed by deleting the branch). Keep this in mind
   if a night is ever re-run.

## Needs your decision

1. Reaction "Never offer" in a window with several cards: today only that card is hidden and the
   window still opens for the others. Decline the whole window instead?
2. Map picker: a rejected Random pick (7 or 8 players, HTTP 422) shows no feedback because the error
   is behind the dialog. Proposed fix: show it in the dialog. Not yet approved.
3. `SHIP_MOVED` reaction timing (parked): the engine emits the typed event only when a cargo hold
   opens, so reactions "after a ship moves" are not offered for ships without capacity. Rules call.
4. More auto-resolved decisions (decided 2026-10-07, UI only): a lone strategic action and a lone system
   activation are answered by the web client for the player (engine unchanged, answer journaled as a
   normal choice, toast "Only one choice: ...", per-player setting, never after undo/redo/load). See
   `docs/AUTO_RESOLVE_CASES.md`. The last strategy card is auto-resolved by the server, which only
   happens in 4-player drafts.
5. Standard UI review (`docs/STANDARD_UI_REVIEW.md`, parked): which items to build first (game-over
   screen, units on map tiles, other seats following play), whether phones are a target, whether the
   server view may grow (promissory notes, commodity cap, VP sources, objective deck counts, bot
   flag), whether undo stays host-only.
6. Feedback button (parked): chat webhook choice (Discord or Telegram) and whether hidden information
   may be sent. Notes: `docs/FEEDBACK_BUTTON_NOTES.md`.

## Known gaps in what was built

1. Leadership panel: Archon's Gift faces and The Triad are not planned (the server rejects such a
   plan). Payment can be changed, but not by clicking planets on the map (the panel has no pending
   payment options for the map to highlight).
2. Reaction dialog on phones: the Play/Pass row flows below the content instead of sticking to the
   bottom.
3. Decision UIs: the Jamming system pick is not matched by the new system picker (only subtypes
   ending in `_pick_system`, `_choose_system`, `_recall_token`); no map highlight for Orbital Drop,
   Transit Diodes, Sling Relay; no gallery case for `place_structure` with units on the planet; the
   replenish text says "refills to full value" because the client has no per-faction commodity cap.
4. Influence for agenda votes still uses the ballot's own list, with no confirm bar.
5. Map selection not built (as agreed): balance numbers, host transfer, player vote. Tables of 7 or
   8 players cannot start at all (six factions in scope; the seventh seat reuses Sol's home tile).
6. Sling Relay's single-ship skip is not converted to the auto-resolve note mechanism.
7. Reaction plan phase 4 (`docs/REACTION_UI_PLAN.md`): public waiting status with the trigger for
   other seats, an optional structured log fact, and a real Pass/back option on the inner multi-card
   decision (an engine behaviour change that bumps the sim baseline).
8. Remaining decision UI review items (`docs/DECISION_UI_REVIEW.md`, section 1): #17 the dedicated
   `activate_system` screen (tactic pool, contested status, planets), and the cross-cutting items not
   yet done (cost and net effect on each option row, no raw ids anywhere).

## Tests and build

1. `InvasionLandingTray` "uses the engine's classified defender..." has failed since the M20 work.
2. `decision_delivery_inventory` (Rust) fails without any change.
3. `ti4-sim` `fixture_capture_is_deterministic` cannot find its pool file.
4. Timing flakes seen once under a parallel run: two `ws_lifecycle` tests and a server history test
   (`session_deterministic_replay` also flaked once). They passed on rerun.
5. The replayer and review one-liners added with the reaction work do not compile on this machine
   (libtorch and gtk are missing): compile them on a full machine.
6. Screenshot capture is not fully byte-identical: 2 of 18 images rechecked differed at pixel level.

## Nightly harness (items from the first nightly summary that may still be open)

Check these against the 2026-10-06 reports before working on them:
1. `server-data/` in a run directory is not the final server state (copied before the game ends).
2. No backend log in `run.log`, so server hangs cannot be diagnosed from the evidence.
3. `error-context.md` shows seat 1's page instead of the stuck actor's.
4. The digest maps VP and objectives to seats unclearly (should map player ids to factions) and misses
   data when there is no `final-snapshot.json`.
5. Two long Opus sessions per night cost real usage; `NIGHTLY_FIXERS=` (empty) disables both rounds.

## Housekeeping (parked by the user)

1. The unreviewed proctor commits and the nightly snapshot commits are in `smoke-playthrough` history.
2. Local refs `nightly-fixes-2026-10-05` and `fixes-2026-10-06` are fully merged and can be deleted.
