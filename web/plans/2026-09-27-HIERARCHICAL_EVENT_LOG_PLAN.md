# Hierarchical player-decision log

## Goal

Make the event log readable from the scale of a whole game down to a single player choice:

**Round → Phase → Action (for example, Tactical Action) → Action stage (Movement, Combat, Invasion, Production, …) → Player decisions.**

Show **decisions made by players**, including reactions by other players during an action. Do not add automatic engine effects such as dice rolls as leaf entries. Keep every decision available for history/undo, while making the actor immediately recognizable by their seat symbol and Okabe–Ito color. Hovering or focusing that symbol should identify the player by their current name.

On reconnect, display only round headings plus the expanded path to the **currently active action**. Do not mount decision rows for collapsed rounds, phases, actions, or stages. A user can expand any older branch to inspect it.

Keep the useful event number, time, state version, and decision text. Each eligible decision has one unobtrusive **Undo** button; no three-dot actions menu or "Undo from action start" option.

## Current State

- `web/src/components/EventLog.tsx` renders a mostly flat list. `foldLogEvents` collapses contiguous decisions sharing a batch ID, with a summary and per-decision undo links. Phase transitions and game initialization are rows rather than parents. The simplified current controls show an inline Undo action rather than a three-dot menu; event number, time, version, and text remain useful.
- The server log (`crates/ti4-server/src/protocol/server.rs`) has initialization, phase-transition, decision-resolved, and game-finished events. A decision may have public `detail`, actor-only `private_detail`, `movement`, `batch_id`, and a best-effort `action_id`/start cursor. It does **not** persist the decision's actor, round, phase, action type, or tactical stage as explicit log fields.
- Ordinary decisions are recorded in `crates/ti4-server/src/session/worker.rs`; batch decisions are built separately in `crates/ti4-server/src/session/registry.rs`. The current action ID is inferred from the last `"action phase"` prompt; this is useful but not a complete action-lifecycle guarantee. Engine `DecisionRecord.context` already carries actor, round, phase, and a stable subtype, but that context is not part of the viewer's event entry.
- `web/src/protocol/client.ts` keeps only the latest 500 events, although the server retains and projects the complete log in an initial snapshot. This can clip a round or action and prevent browsing earlier decisions. Live event updates also scan existing IDs and copy the array.
- `web/src/presentation/playerDisplay.ts` defines the eight fixed seat symbols and Okabe–Ito colors. `PlayerIdentityProvider` resolves current names from the lobby, including renamed players; `SeatBadge` provides an existing shape/color treatment. The log currently uses player names mainly inside generated prose, not as a consistent actor marker.
- The dev decision gallery at `web/src/dev/DecisionGallery.tsx` renders `GameShell` with `events={[]}`. The actual event log is mounted by `GameShell`; the gallery has no representative log data to preview.

## Target Architecture / Design

### Authoritative event data

Keep the server's **ordered, persisted event stream** as the source of truth. Add optional, explicitly public grouping fields to decision entries: decision actor, round, phase, action identity/type/actor, and stage identifier. Use stable machine identifiers for grouping and separate display labels for headings. Keep `batch_id` as a distinct grouping/undo concept, not as a replacement for an action or stage. The server should publish the same projected fields to live subscribers and in reconnect snapshots, using the same projection for player and spectator views.

Do not send a whole private `DecisionRecord` or its option payload to other players. Derive safe public labels and stage assignments from verified engine decisions and known context subtypes; retain the existing public/actor-only fact projection. Unknown or historical decisions get an **Other decisions** stage rather than a guessed one. An action with no stage-specific choices may put its decisions directly under the action.

The top-level round and phase boundaries come from initialization/phase-transition events, with explicit round/phase metadata on new decision entries to make grouping robust when a boundary event and a decision share a cursor. The action-selection decision belongs to its own action. A reaction by another player belongs under the ongoing action but displays **that reaction's decider**, not the action's owner. Non-action phases can hold phase-level decisions directly; introduce an action-like group there only when the game actually has a meaningful action boundary.

### Readable, lazy tree

Build an ordered tree of rounds, phases, actions, stage **segments**, and decision leaves from visible events. Preserve event order; if the same stage resumes later, create a new segment rather than moving its later decisions above intervening stages. Summaries can aggregate known public facts (such as repeated moves); incomplete actions should not be labelled finished. Show small counts and useful headings at each level, with a neutral fallback for older events whose metadata or prose is missing.

Render round headings first. Render children **only when their parent is expanded**, recursively: collapsing a round unmounts its phases and all deeper rows. On an initial snapshot/reconnect or history replacement, open only the round → phase → action → stage path identified as current by authoritative state/metadata; leave every other branch collapsed. If the game is between actions, open the current round and phase only. If there is no reliable current action marker, stop at the last known current phase rather than opening a stale action. While connected, new decisions can extend the open current path without overriding branches the user opened or closed manually. Reset expansion deliberately on a new snapshot/history generation, not on every ordinary state update.

### Player identity

Put a compact actor symbol beside **each decision leaf**, including reactions and batched decisions. Obtain its symbol, color, and name through `usePlayerIdentity()` / `playerDisplay()`; do not derive colors from player IDs or embed names in stored events. Provide the current player name (and seat position if needed for duplicate names) on hover **and keyboard focus**, with an accessible label. Use the existing seat styling/contrast conventions, especially a visible outline for black. Group headings may show the action owner, but that badge must not suggest every child decision was made by that owner. Legacy events with no actor get a neutral unknown-actor marker or no badge; never infer an actor from a movement sentence or private text.

## Implementation Details

1. **Define metadata and boundaries.** Extend `GameEvent` and the matching TypeScript type in `web/src/protocol/types.ts` with optional, backward-compatible decision actor/round/phase/action-type/stage fields. Define the allowed action and stage identifiers in one server-side mapping; cover tactical activation/movement (including cargo), combat, invasion, production, strategic actions and secondaries, component actions, passing, and relevant reactions. Start with an explicit mapping of known `DecisionContext.subtype` values and an `other` fallback. Preserve `decision_count`, `action_start_cursor`, and batch start/end cursors for undo. Avoid treating a new player turn or a batch as proof that the previous action has completed.

2. **Emit identical facts for manual and batched play.** Populate metadata from the actual recorded choice/context and the verified offered option in both `worker.rs` and `registry.rs`; ideally share one fact-building function near `protocol/server.rs`. Carry the action ID across other players' decisions in its reaction windows. Where prompt-derived IDs cannot correctly describe an action's lifecycle, add an authoritative active-action boundary/marker rather than silently merging unrelated decisions. Store metadata with the event so replay, undo/redo, and reconnect do not need to reconstruct it from React state. Old stored events remain readable with missing fields.

3. **Identify the current path.** Expose an optional current round/phase/action marker with snapshots (and update it through normal state updates) so the browser does not equate the _last logged_ action with the _currently active_ one. Resolve the open stage from the current decision context when safe, otherwise from the latest stage associated with that active action. A newly opened action with no resolved leaves should still be representable; if no action is active, stop auto-expansion at the phase. For finished games, show round headings by default and allow manual exploration.

4. **Build the tree separately from rendering.** Replace batch-only `foldLogEvents` with a pure event-to-tree function in/near `EventLog.tsx`. Group only events visible to that viewer, preserve same-cursor event order, attach initialization/transition/finish markers to their appropriate round/phase, and de-duplicate multiple visible facts for one decision cursor **without losing an actor-only detail**. Retain each leaf's real decision cursor and each valid action/batch start cursor. Make missing metadata fall back to a clearly labelled round/phase/action bucket, without incorrectly attributing decisions to a different round. Use stable IDs based on persisted event/action/boundary IDs, not array positions.

5. **Render and style the hierarchy.** Give each level a clear indentation and expandable summary in `EventLog.tsx` and `web/src/index.css`. Use controlled expansion keyed by stable IDs so reconnect and manual navigation behave predictably; conditionally render descendants instead of hiding an already-mounted full list. Preserve event number, timestamp, state version, text, drawer controls, redo controls, and private/referee indicators. Show a single inline **Undo** control for each eligible decision (including a batch decision when expanded), using its real cursor; do not reintroduce the three-dot menu, action-start undo, or a separate stack of row-level history actions. Ensure keyboard toggles and actor tooltips work as well as pointer interactions.

6. **Keep complete client history.** Remove the 500-entry truncation in `serverEventLog` and its fragment-discard logic, so a round is not silently incomplete. The server already returns all projected events on reconnect. Keep live event-ID deduplication and history-generation replacement, but use a set/index or another efficient approach rather than repeatedly scanning the whole log as it grows. Build/group the tree efficiently (one pass over ordered entries where possible); with collapsed branches unmounted, only open paths incur leaf DOM cost. Measure long-game reconnect payload and tree-building time before introducing server pagination.

7. **Preview and verify.** Add a synthetic multi-round, multi-player event fixture to the dev gallery (or a dedicated dev log preview) so the real `GameShell` log can be inspected without a server. Test nested ordering, round-first reconnect state and current-path expansion, manual expand/collapse, stage repetition, tactical batches, reactions with different actors, private details for actor vs spectator, old events missing metadata, undo cursors after restore/redo, and long logs beyond 500 entries. Verify live events and initial snapshots produce the same tree for each viewer; run the focused web/server tests and the web build.

## Completion Criteria

- A reconnect shows round headings and only the active round/phase/action/stage path expanded; collapsed branches have no decision-leaf DOM nodes.
- Any earlier round can be explored down to every recorded **player decision**, even beyond 500 entries, with correct ordering and undo cursor.
- Every new decision identifies its actual decider with the assigned symbol/color and a name available on hover and focus, without leaking private choices.
- Live play, batches, history restore, and reconnect group equivalent decisions the same way; older saved games remain usable.

## Progress (implemented)

- [x] Persist optional actor, round, phase, action type/owner and stage on manual and batch decision events. Stage identifiers come from an explicit allowlist of engine subtypes; unknown choices use the neutral **Other decisions** stage. The viewer projection still strips seat-only details for everyone else.
- [x] Publish a current round/phase/action/stage path in both initial snapshots and state updates. An action-phase turn prompt does not reopen the previous action; finished games have no auto-open path.
- [x] Replace flat batch rows with an ordered round/phase/action/stage-segment tree. The renderer unmounts collapsed descendants, de-duplicates same-cursor visible facts without losing private detail, and gives every eligible leaf its own cursor-based Undo. Legacy entries get neutral headings.
- [x] Resolve actor badges from the live player roster, with seat symbols/colors and a name on hover and keyboard focus. Simplify the visual tree to indentation, compact rows and minimal borders.
- [x] Keep complete event history beyond 500 entries with live ID indexing. Add a multi-round, multi-player dev gallery log and regression coverage for stage repetition, reactions, private facts, current-path expansion and long history.
- [x] Validate with `cargo test -p ti4-server`, `npm test`, `npm run build` (TypeScript check included), and `npm run test:e2e`. The landing pipeline needed a small continuity fix when the pending offer briefly clears between steps.

Follow-up: collect real long-game reconnect payload and tree-build timing before deciding whether server pagination is warranted.
