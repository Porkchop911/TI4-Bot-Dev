# web2: rules for agents

Read this before you change the UI in `web2/`. These are decisions of the project owner (2026-10-07).

## Consistency

- The UI must feel consistent. A new UX feature must work the same way on every screen where a player can expect it.
- Before you add an interaction (a key, a gesture, a navigation, a kind of control), check each other screen: does it apply there, and does it mean the same thing there?
- If it applies to one screen or a few screens only, do not add it. Use an interaction that the UI already has, or find one that applies everywhere.
- An exception needs a reason that a player can see on the screen, and a line in this file.

## Patterns

These rules hold on every screen. The feature sections below give the specifics and the exceptions.

1. **Controls are where the choice is made.** A control is next to the thing that it changes, not in another place that only has the room. Cases: the payment controls are on the map; the editor is in the row of the viewer in the seat list; the selected technology cell is the control.
2. **Everything that a choice needs is on one screen, with no click.** Prefer one overview to a view for each item. A view that needs a click for each item is the last resort. Case: the seat list shows what every player did.
3. **A staged change shows "now → after" in game values.** Cases: pools "3 → 5", "Trade goods 2 → 5", "Strategy pool 2 → 1".
4. **Colour is never the only carrier of a state.** A state also has a sign, a word or a shape. Cases: ▶ ✓ – ✎ · in the seat list; "+" and the empty outline on tokens; hollow prerequisite marks; "⚠ 1 wasted".
5. **An amount against a cost has three states, always the same.** Short is red and blocks the main button. Exact is green. More than the cost is orange with "⚠", and it is allowed.
6. **One signal for one state.** A state is a pill or a sign. Do not add a frame, a tint and a badge for the same state, and do not repeat it in several places of one screen.
7. **An element that never changes and repeats other content is removed.** Case: the pills for the stages of a strategy card.
8. **An amount changes in steps of one.** A button for the usual target can stand next to the counter ("Pay rest", "Auto-pay"). There is no all-or-nothing button for an amount.
9. **When a screen does not fit: move, restructure, make dense. Do not scroll.** In this order: put a summary into a row that exists on every screen (the footer, the heading, the header of a block); remove what repeats; make secondary rows dense. Only then ask the owner for an exception. Cases: the payment line in the footer; the chosen technologies in the header of the tree; the 24px rows of the seat list.
10. **Live, Draft and the event log use the same screen.** Read-only is the same screen with nothing open, not another view.
11. **Hidden information.** A choice of another player is visible when it is resolved, never before. A row of another player is not a control.
12. **Symbols follow the physical game** where the game has a convention. Case: command tokens are triangles, and the fleet pool points up.
13. **Names and long texts.** A name is in a column of fixed width and is cut with "…"; the full name is on hover. Check every screen with 8 players, two long player names (the table of eight of `e2e/lobby.spec.ts` and `e2e/lobby.phone.spec.ts`), and the rows that only some factions have (faction technologies).

Checklist for a new screen or a changed screen:

- Where does the player make the choice? Are the controls there?
- What must the player see to decide? Is all of it on this screen?
- Does every state have a sign or a word, and only one signal?
- Does it fit at 1920×1080 with 8 players and the long names?
- On a phone: is nothing cut at the side, and is the main button in view?
- Does a read-only state show the same screen with nothing open?

## Screen size

There are two layouts, and no others.

- **Desktop** is the design target: a viewport of 1920×1080 or larger. Every rule in this file is written for it.
- **Phone** is a portrait phone, checked on a Pixel 9 (412×923, the installed app with no browser bars). The owner added it on 2026-10-08. It shows the same views with the same blocks; only the shell differs.
- Tablets, landscape phones and narrow desktop windows are a **non-goal**. They get whichever of the two layouts their width selects.
- There is one breakpoint: `phone:` in `src/styles/theme.css`, and `usePhone()` in `src/ui/usePhone.ts` with the same width. Do not add another breakpoint or a container query.
- `touch:` (a coarse pointer) is only for the size of a control under a finger. It does not change the layout.
- Desktop screenshots and layout tests use 1920×1080. Phone tests use a Pixel 9 in portrait. The projects are in `playwright.config.ts`: `desktop` and `phone` for the real game, and `slow` and `slow-phone` for its long tests (see "Tests" in `docs/agents/testing.md`).

### Phone

- The shell shows one pane at a time: **Map · Players · Action**, with a tab bar at the bottom. The toolbar stays on top.
- The footer of the action (note, payment line, main button) is under every pane, so the main button is always in reach. This is how pattern 2 holds on a phone: the choice and the button that sends it are on one screen, the rest is one tap away.
- Exception (owner, 2026-10-10): while the fleet of a system is open on the map, the footer is not shown. The sheet has its room. The "✓" of the sheet closes it, and the footer is back. The landings of an invasion are such a sheet too.
- The pane changes with a tap on its tab, and when a choice of the viewer opens the viewer's next step in another pane (owner, 2026-10-16): "Tactical" in the action opens the map, because the activation is on the board, and an undo that takes the viewer back to a step on the map opens the map. What does not follow from a choice of the viewer never switches the pane: another player, a bot, a window that the game opens, or a decision that comes after the viewer waited for the game. Then the tab of the pane where the open decision of the viewer is made has a blinking dot while another pane is in view: a choice on the board is Map, a choice of a player is Players, a window (a reaction window, an ability that asks in the middle of an action) is Map, where it is a sheet over the board with its system marked, every other decision is Action. A game opens on the pane of its choice. A choice is an intent of the viewer that the game answers, sent while a decision is open: an answer, "Tactical" or a draft that starts, a change of the draft (redo, start over, a removed entry), an undo (also while the viewer waits). What is only staged on the screen (a count, a toggle, an option before its button) is no choice. A choice switches the pane at the next change of the pane of the decision, and only until the game answers it (a new question of the viewer, the answer to the draft, a refused answer: `ShellView.reply`), whether or not the pane changed (`GameShell`).
- A pane may scroll down. Pattern 9 and "the open step must not need vertical scroll" are desktop rules. Nothing may be cut at the side: the test of each screen in `e2e/local.phone.spec.ts` checks this.
- The player table has two columns of cards (owner, 2026-10-16): the strategy cards are the top row of a card, then the turn, the symbol, the name and the victory points, then the faction with "You", "Passed" and the speaker, then the values in a grid of three columns. A tap on a card opens the sheet of the player; there is no hover sheet.
- A phone has no hover and no keys. A shortcut key is not shown. A name that desktop cuts with "…" takes a second line where the full name is only on hover (the technology tree, the action picker).
- The map zooms with two fingers; the "+" and "−" buttons are not shown under a finger. "Fit task" and "Fit board" stay, in the row of the map views.
- Exception to "no short variants of labels" (owner, 2026-10-10): the map views and the two fit buttons are icons on a phone, so that they are one row. The name is the tooltip and the accessible name. Desktop keeps the words.
- The reference sheets, the draft menu and (in a local game) Dev are one menu in the toolbar. A sheet covers the pane.
- Exception (owner, 2026-10-11): while a sheet covers the pane (a reference sheet, the sheet of a player, Dev), the footer and the tab bar are not shown. The sheet has their room, and its "×" gives them back. A sheet on the map (a fleet, the landings, a build) keeps the tab bar: the Action pane says what is staged.
- The app can be installed (`public/manifest.webmanifest`): fullscreen, locked to portrait. There is no service worker: the game is live, and the CDN already caused stale bundles once. The icons come from `scripts/make-icons.mjs`.
- A new screen is checked in both layouts. Short variants of labels are still not allowed; restructure the row instead.

## Action panel

- The action panel has one fixed width. It does not change with the step.
- There is no control to change the panel width.
- The top of the panel has two rows: the bar of the game (round, phase, turn and the status, "Your decision" or whom the game waits for; Live, Draft and the menu) and the heading of the open step. The heading row has the heading, beside it in muted text what the heading does not say (the kind of action, its system and its seat: "Tactical action · #26 · Hacan", "Action card · Hacan", "Turn ended"), the pill of its state ("Your turn" in the action picker, "Reaction", "Resolved"; never "Waiting for …": whom the game waits for is the status of the bar, "Turn of Hacan", "Waiting for Hacan" or "Waiting for reactions"), "Not sent" while a choice is staged, "?" with the rules, and the trail. The pill does not say again what the bar of the game says: a pill that is the status of the bar is not shown, and "Your decision · Reaction" is "Reaction" (`pillBeyond` in `ActionPanel.tsx`). The heading, the context, the pill, "Not sent" and "?" are one line; the context is cut with "…" first. A tactical action has its step tabs between the two rows.
- The board for 8 players (61 systems) must stay usable in the space that remains. Improve the board; do not make the panel narrower.

## Text

- The users are experts of the game.
- Text that states a rule or explains the UI is not visible by default. Put it behind hover/click (`Hint` in `src/ui/overlays.tsx`).
- Exception: a list where the player chooses one card (the list of action cards) shows the printed text of each card. The list has the room, and the text is what the player chooses by.
- The action picker itself keeps its rule text behind hover/click. Inline text on every row was tried (2026-10-07) and was too much.
- Visible text states the game state: costs, counts, errors, and who the game waits for.
- The open step must not need vertical scroll at 1920×1080. The test of each screen in `e2e/local.spec.ts` checks this.
- Do not abbreviate labels when there is room.
- The player table is one card for each seat in two columns (8 seats in four rows; `src/game/players/PlayerTable.tsx`). Which seat has which strategy card is the anchor of a card (owner, 2026-10-16): the strategy cards are the left column of every card, in the same place, the number large in gold with the name under it (dimmed with "✓" when used); two cards (four players or fewer) are stacked in the same place, and a seat with none has an empty place of the same size. Right of it, the first line is the identity: ▶ for the turn, the symbol of the seat, the name of the player and the faction, "You" on the card of the viewer, "Passed", the speaker (gavel) and the victory points (star). Under it the values are a fixed grid of four columns, the same place for each value on every card: resources, influence (ready/total), planets, trade goods; commodities (held/limit), the command tokens as triangles (tactic · fleet · strategy; the fleet pool points up), action cards, secret objectives (scored/held). An icon is the short form of a label: the full name is its tooltip and its accessible name, and "?" in the header of the table is the legend of the icons. The hover sheet of a card is left of the cards, over the board.
- Exception to pattern 6 (owner, 2026-10-16): the card of the active player has ▶ and a frame in the accent, and the card of the viewer has "You" and a tint, so that both are found at a glance. In the action phase the header of the table has the turn ("Turn 19").
- The header of the table sorts the cards: "Initiative" (the engine's order of the turns in the action phase, otherwise the lowest strategy card; a seat with none last) or "VP" (the most first); a tie keeps the seat order. With no choice the order follows the phase: initiative in the action phase, victory points otherwise. The choice of the viewer is kept in the browser for each seat (`localStorage`).
- **Rules that a player may need to look up are in the UI, behind hover or click** (owner, 2026-10-11). These are: every text that is printed only on a component and is not in the rule book (a unit, a card, a technology, a leader, a faction ability), and special abilities that are not used often. General rules are not in the UI: the flow of the game, and the rules of every game such as combat dice or the unit keywords (SUSTAIN DAMAGE, BOMBARDMENT). A keyword is shown with its values and no explanation. These are examples, not a list: decide by "would an expert look this up?".
- Rule text is never written in web2. It comes from `ti4-content` through the engine (a fact of the update, or the catalogue).

## Gates

The full rules are in `docs/agents/testing.md` ("Tests" and "Rust builds"). Read it before you run a build or a suite.

- Rust: only the aliases `cargo l-web`, `cargo t-web`, `cargo b-web`, and `cargo wire-web` to write the generated wire types of `web-shared` after a change of a Rust wire type (never `-p`, `--workspace` or `cargo c-web`). Before a commit of Rust: `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`; when `ti4-host` or `ti4-wasm` changed, also `scripts/build-wasm.sh` (the shared build of both apps, `web-shared/scripts/build-wasm.mjs`) and `npm test`; when the server changed, also `cargo b-web` and `npm test`.
- A change of `web2/` only: `npm run typecheck`, `npm run lint`, `npm run format:check` (oxfmt over web2 and web-shared; `npx oxfmt .` fixes it), `npm test`, then `npm run e2e -- -g "<the screen>"`; the whole `npm run e2e` before the commit. Not for each change: `npm run e2e:slow`, `cargo t-web-slow`.
- With less than 4 GB of memory: one build or one suite at a time.

## Deployment

The dev host serves web2 with Vite from the working tree. To bring it up to date with a branch (the engine of the page, the server, the MLP models, the restarts and the checks): `DEPLOYMENT.md`.

## Topics

The rules of each feature are in `docs/agents/`. Before you change a feature, read the file of every topic that the change touches. A new rule goes into its topic file; only a rule for every screen goes into this file.

| Read before you change…                                                                                                                                                      | File                            |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| A decision, the decision list, a reaction window, the action picker, shortcut keys                                                                                           | `docs/agents/decisions.md`      |
| A reference sheet: Cards, Secrets, Agendas, Objectives, the sheet of a player (`src/game/shell/Reference.tsx`)                                                               | `docs/agents/sheets.md`         |
| A strategy card, its primary or secondary, the draft of a secondary, the technology tree                                                                                     | `docs/agents/strategy-cards.md` |
| The map, a system, the inspector, the payment bar, exploration (`src/game/board/`)                                                                                           | `docs/agents/board.md`          |
| Movement, invasion, production, the battle table, the space odds (`src/game/steps/`, `src/game/combat/`)                                                                     | `docs/agents/tactical.md`       |
| Online play, the server transport, undo online, staged work of a seat (`wsTransport.ts`, `seatStore.ts`, `OnlineApp.tsx`; in a local game also `local-play.md`)              | `docs/agents/online.md`         |
| The lobby, single player, the setup of a table, the Table sheet (`src/game/lobby/`, `table.ts`, `localTable.ts`)                                                             | `docs/agents/lobby.md`          |
| A local game, `Transport`, the Dev sheet, drafts of a tactical action, the saved game and the save list of the start screen (`src/session/`, `LocalApp.tsx`, `StartApp.tsx`) | `docs/agents/local-play.md`     |
| Tests, fixtures, screenshots, Rust builds, the wasm build                                                                                                                    | `docs/agents/testing.md`        |
| The code shared with web/: the engine driver (`@ti4/shared`, `../web-shared/`)                                                                                               | `../web-shared/AGENTS.md`       |
