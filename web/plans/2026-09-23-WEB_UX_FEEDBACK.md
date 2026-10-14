# Web client UX review

Review of the current `web/` implementation (source-level walkthrough, 2026-09-23). This is design feedback, not a browser usability study; the observations below are grounded in the cited code and should be checked in a real multiplayer session before treating the proposed solutions as validated.

## What already works well

- The lobby makes occupancy, readiness, connection state, and the reason Start is disabled visible (`web/src/components/Lobby.tsx:80-95`). It also keeps credentials out of shared URLs (`web/src/App.tsx:14-28`).
- A consistent position/color/name vocabulary carries from the lobby into the board and player sheets, including duplicate-nickname disambiguation (`web/src/components/Lobby.tsx:55-57`, `web/src/components/Board.tsx:169-173`, `web/src/components/PlayerSheet.tsx:38-75`).
- Legal targets are highlighted on the board, while dedicated payment, movement, combat, trade, agenda, and production views offer more context than a raw list of option IDs (`web/src/components/GameShell.tsx:49-79`). The generic dialog supports search for longer lists and a minimize-to-inspect path (`web/src/components/PendingChoiceModal.tsx:128-189,305-323`).

## Prioritized changes

### P0 — Make “Exit Game” safe and explicit

`Exit Game` calls `onForget`, which deletes the tab's saved player session before returning home (`web/src/App.tsx:28,44-50`). A player may read “exit” as closing the game view, but after clicking it a reload of the invitation URL has no credential to resume the seat. In a running game, the lobby only exposes takeover when the seat becomes eligible (`web/src/components/Lobby.tsx:96-99`). **Recommendation:** separate “Back to home” from “Leave this seat / forget this session”; preserve the credential for navigation and require a clearly worded confirmation before forgetting it. Acceptance check: exit the view, revisit the URL in the same tab, and verify that the original player can resume without a takeover.

### P1 — Show failures where recovery happens

When the lobby cannot load, the route renders only “Unable to load lobby.” even though the hook exposes `error` (`web/src/App.tsx:31-41`). When game snapshot loading fails, the board continues to say “Loading game state...” and `lastError` is only passed to the choice renderer, which renders nothing when there is no choice (`web/src/App.tsx:44-50`, `web/src/components/GameShell.tsx:119,245-256`). A broken connection can therefore look like an endless wait. **Recommendation:** give the lobby and game-loading screens explicit retry, invalid-session, and server-unavailable states; surface connection errors persistently at the top of the game view. Acceptance check: stop the backend before joining and during play, then confirm the screen says what happened and how to retry.

### P1 — Make multi-step commitments legible

“Confirm Payment,” “Commit Moves,” “Confirm Casualties,” and “Cast Votes” build queues of separate authoritative submissions (`web/src/components/PaymentDrawer.tsx:125-151`, `web/src/components/TacticalMovementOverlay.tsx:152-183`, `web/src/components/CombatResolutionModal.tsx:119-145`, `web/src/components/AgendaBallotModal.tsx:93-106`). If a later option disappears, the queue stops after earlier actions have already happened (`web/src/hooks/usePipelineRunner.ts:42-76`). The interface currently presents a single confirm action without explaining partial completion, and payment does not display the pipeline error returned by the hook (`web/src/components/PaymentDrawer.tsx:42,326-334`). **Recommendation:** say “Apply selections in order,” display step-by-step progress and an explicit “Some choices were applied” failure state, refresh the draft from the current pending decision, and show pipeline errors in every workflow. Do not imply an all-or-nothing transaction unless the server supports one. Acceptance check: interrupt a batch after the first accepted step and verify that the remaining choices and already-spent resources are unmistakable.

### P1 — Use the exact action selected in the system inspector

The inspector renders a button for each `availableActions` entry and passes its `optionId` (`web/src/components/SystemInspector.tsx:286-309`), but `Board` wires the callback without forwarding that ID (`web/src/components/Board.tsx:527-537`). The app then searches the pending options by system/planet and picks the first match (`web/src/App.tsx:47-50`). Two actions targeting the same system can thus select a different choice from the one clicked. **Recommendation:** carry the exact legal option ID through inspector → board → selection, and use system/planet matching only for clicking the map itself. Acceptance check: provide two legal options on one system and verify each inspector button selects its corresponding option.

### P1 — Give players enough information to choose before they commit

The board shows a single total-units badge per system and abbreviated planet names, while owner/type breakdown lives behind the inspector (`web/src/components/Board.tsx:380-473`, `web/src/components/SystemInspector.tsx:195-253`). The always-visible player sheet has VP, economy, strategy cards, and private cards but little of the broader public situation (for example controlled planets and technologies) (`web/src/components/PlayerSheet.tsx:79-128`). In the trade-answer view, the player sees the prompt and option labels, not a structured offer summary (`web/src/components/TradeDeskModal.tsx:142-177`); the combat view accepts a dice feed but `GameShell` never supplies it (`web/src/components/CombatResolutionModal.tsx:16-35`, `web/src/components/GameShell.tsx:81-84`). **Recommendation:** prioritize an at-a-glance owner/type summary on selected or contested hexes and show the actual offer, relevant holdings, or combat context next to consequential choices. Test with a new player asked to explain _why_ they chose an option, without requiring them to infer facts from opaque labels.

### P2 — Remove avoidable lobby friction

The creation page says “Other players and bots join from the shared URL,” but the visible lobby offers join/watch/rejoin, not a bot-join control (`web/src/components/Lobby.tsx:31-37,88-99`). A host who chooses eight positions may be stuck waiting for every slot to be occupied and ready (`web/src/components/Lobby.tsx:66-82,95`). **Recommendation:** either expose the supported bot-join path in the lobby or change the copy to reflect the actual flow; explain before creation that starting requires every position to be filled and ready. Keep the clear waiting reason once the lobby is open.

### P2 — Improve narrow-screen decision ergonomics

The game header has connection, version, round, phase, speaker, a long turn banner, and Exit in one flex row with no narrow-screen wrapping rule (`web/src/components/TurnStatusBar.tsx:53-129`, `web/src/index.css:70-84,281-358`). The movement tray is absolutely positioned at the bottom and limits only its list height (`web/src/index.css:506-509`); on short mobile screens its header/gauges/actions may compete with the board and fixed Players/Events buttons. **Recommendation:** test 320–390 px wide and landscape/short-height viewports, collapse secondary status into a details affordance, and give each decision surface a scrollable body with a reliably visible primary action and resume control.

### P2 — Reduce unexplained shorthand and noisy status

The player sheet uses “TG,” “Comm,” a three-number token string, and “Secret Obj” without inline explanations (`web/src/components/PlayerSheet.tsx:79-82,124-128`). The generic choice view shows internal `context.subtype` strings (`web/src/components/PendingChoiceModal.tsx:258-262`), and the header displays a protocol-like `v{gameVersion}` next to essential turn status (`web/src/components/TurnStatusBar.tsx:89-102`). **Recommendation:** spell out economy and token-pool labels, move technical subtype/version details into an optional diagnostics section, and let the decision title answer “What am I doing now?” in ordinary language.

## Suggested validation order

1. Run a two-player journey: create → copy URL → join → ready → start → make a decision → leave view → return in same tab. Check credential continuity and recovery.
2. Repeat with the backend unavailable and with a disrupted multi-step choice; note exactly what each participant sees after one successful submission.
3. Inspect a dense system with multiple legal actions, then repeat the decision on a narrow mobile viewport and with keyboard navigation.

No application code or tests were changed for this review.
