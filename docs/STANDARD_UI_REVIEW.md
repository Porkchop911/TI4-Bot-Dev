# Standard UI review

Review of the everyday screens of the web client (lobby, game shell, board, player sheet, log,
modals) and of the dedicated decision UIs that already exist. Written 2026-10-06 on branch
`smoke-playthrough`. Review only: no app code was changed and nothing was run.

This review complements and does not repeat:

- `docs/DECISION_UI_REVIEW.md`: decisions that fall back to the generic radio list, the turn
  menu, command-token decisions, the context header, the id-to-name rule for decision labels;
- `docs/MAP_SELECTION_SCREEN_PLAN.md`: the pre-game map picker;
- reaction dialogs (`docs/REACTION_UI_PLAN.md` does not exist yet; reaction windows are covered
  by H1 of the decision review).

Work already in progress elsewhere and treated as a given here: the persistent turn action bar,
command-token staging, the shared decision header, the hit-assignment panel.

## How this was done

- **Code:** every component under `web/src/components`, the presentation layer
  (`web/src/presentation`), the wire types (`web/src/protocol/types.ts`) and the server's
  projection (`crates/ti4-server/src/protocol/view.rs`, `projection.rs`, `protocol/server.rs`).
- **Rendered evidence:** the screenshot artifacts `web/e2e/screenshots/{A..H}/out/*.png`. They are
  rendered from synthetic two-player fixtures (`_shared/fixtures.ts`), not from engine states, and
  all at 1440x900.
- **Game evidence:** `nightly-reports/2026-10-05/report.md`, `2026-10-06/report.md` and
  `latest-summary.md` (35 smoke runs: what occurs in real games and where the proctors, who read
  the game state, got confused).
- **Markers.** **[seen]** means visible in a screenshot. **[code]** means read from source, not
  seen rendered. **[inferred]** means a likely consequence that I did not verify.
- **Format per finding:** *Needs* (what the player must know or do) / *Today* / *Problems* /
  *Proposal* / *Data* / *Effort and priority* / *Relates to*.
- **Scales.** Effort: S (under a day), M (a few days), L (a week or more). Priority: P0 (players
  misjudge the game or are misled), P1 (slows decisions noticeably), P2 (polish).

---

## Executive summary

### Top 15 improvements, ranked by impact on understanding and decisions per unit of effort

| # | Improvement | Area | Effort | Prio | Server change? |
|---|---|---|---|---|---|
| 1 | **Remove or fix the two misleading displays.** The action-card "Always / Never offer" switch is local React state that nothing reads (`PlayerSheet.tsx:204`, `:236-244`), yet artifact A says the game declines the card. The VP breakdown counts Mecatol Rex control as +1 VP (`PlayerSheet.tsx:63-73`), which is not a rule, and leaves out Custodians, Imperial, agendas and Support for the Throne. Seen: the opponent shows 2 VP with a breakdown summing to 0 (H/04). | Player sheet | S | P0 | no |
| 2 | **Show the state the server already sends but no screen draws:** `passed`, `active_player`, `leaders`, `relics`, `exhausted_relics`, `technologies` (only inside a modal), `table.laws`, `table.strategy_card_goods`, `table.unclaimed_strategy_cards` (grep: none of them is read by a component) | Player sheet, header | M | P0 | no |
| 3 | **Units on the map by owner and type.** A tile shows only "6 units" (`StandardOverlay.tsx:168-191`), so whose fleet, what kind and whether it is contested are invisible without clicking. | Board | M | P0 | no |
| 4 | **A "who / what / next" status strip** in place of today's header. It shows initiative order with passed markers, strategy cards and the speaker, plus a plain-language line on what the active player is doing. Other seats see nothing today when it is not their decision (`GameShell.tsx:645-651`), and the header text is raw (`TurnStatusBar.tsx:27-37`). | Shell | M | P0 | small (public stage text) |
| 5 | **Game-over screen** with final scores, the winner and the reason. `final_scores` is decoded but never shown, and the game ends in round 9 at 1 to 5 VP when the objectives run out, which confused the nightly proctors ten times. | Shell | S | P0 | no |
| 6 | **A pending or failed submission is always visible.** A click whose first submit got no reply turns every later click into a silent no-op (nightly 2026-10-05, finding 4; `protocol/client.ts` dedupe). Show "Sending...", and after a few seconds "No answer from the server, retry". | Shell, all decisions | S | P0 | no |
| 7 | **Readable planets and systems:** unique planet abbreviations (two "ARC" chips in one hex), resource and influence text at 7.5px, system names instead of "#26", home systems marked, legendary, trait and tech-skip glyphs | Board | M | P1 | no |
| 8 | **Command tokens and activation:** every owner's token as a visible coloured marker. "ACTIVATED" today refers only to the viewer's own token (`BoardTile.tsx:192`); others are 6px dots. Drop the "TARGET" text that is printed on about 30 hexes at once. | Board | S | P1 | no |
| 9 | **Event log that reads like a game log:** names instead of ids ("activated #26", "exhausted jord", "scored objective <id>", "moved sol_carrier2"), filters, the latest entry visible while collapsed, no raw ISO timestamp or `v123` | Log | M | P1 | yes (detail strings) |
| 10 | **Mount the auto-resolve toasts.** The component and hook exist, but only the dev gallery uses them (`dev/ToastGallery.tsx:20`). Extend them to "what just happened" toasts for other players' actions. | Shell | S | P1 | no |
| 11 | **Keep the map usable during map-related decisions.** Payment, movement and cargo open as modals over a blurred board (`GameShell.tsx:672-676`, `:747-764`). Payable planets are not highlighted on the map, and a pick there shows no mark (D/5b, 5c captions). | Decision UIs | M | P1 | no |
| 12 | **Missing ids-to-names and icons in the dedicated UIs:** cargo `group.unit` raw (`CargoLoadingTray.tsx:157`), production rows text-only (`ProductionBuilderDrawer.tsx:230-237`), "Origin: #id" in movement, "System 18" in combat and the invasion pill | Decision UIs | S | P1 | no |
| 13 | **Objectives and the VP race:** the objective deck remaining and the round-9 end, every VP source, a VP track in the header | Objectives | M | P1 | small (deck counts) |
| 14 | **Map navigation:** fit to view (the bottom row is cut off at 1440x900, seen), wheel and pinch zoom, keyboard pan, a memoised presentation model | Board | M | P1 | no |
| 15 | **Accessibility and tokens:** duplicate tooltips (native `title` and the custom tooltip together, seen H/09), hover-only board info, text-faint contrast of about 3.7:1, seat 8 black on a near-black canvas, no `prefers-reduced-motion`, 227 inline style blocks and 246 hex literals in components | All | M | P2 | no |

**The five that matter most:** 1 (the UI currently tells players things that are false), 2 and 3
(much of the game state is either not drawn or drawn only as a count), 4 (non-active players
cannot follow the game), and 5/6 (the game ends or stalls without telling anyone why).

### Quick wins (S effort, no server change)

| Where | Change | Evidence |
|---|---|---|
| `PlayerSheet.tsx:328-329` | "You (You)": show the nickname, with "(you)" only when the nickname is not "You" | [seen] C/1 |
| `PlayerSheet.tsx:331` | faction shows the raw id ("sol", "hacan"); add a faction-name catalog | [seen] |
| `PlayerSheet.tsx:366-374` | "Tokens: 3/3/2": label it "Tactic 3 · Fleet 3 · Strategy 2" with icons | [seen] |
| `PlayerSheet.tsx:204-244` | hide the reaction switch until it is wired to the server, or wire it (open question 2) | [code] |
| `PlayerSheet.tsx:63-78` | VP breakdown: drop the Mecatol row, add "Other (+n)" = total minus the known sources so the rows always add up | [seen] H/04 |
| `StandardOverlay.tsx:189` | "1 units" should read "1 unit" | [seen] |
| `TurnStatusBar.tsx:31-32` | drop the parenthesised stage. On a real server it is the constant "Waiting for player" (`projection.rs:594`), so the banner reads "Waiting for Bob (Waiting for player)" | [code] + [inferred] |
| `TurnStatusBar.tsx:90-92` | hide `v40` (game version) outside dev mode | [seen] |
| `App.tsx:440-444` | the stand-alone `TechnologyModal` gets no `viewerSeat`, so it falls back to `playerList[0]` (`TechnologyModal.tsx:74-84`); a spectator or seat 2 sees seat 1's techs as "owned" | [code], verify |
| `App.tsx` | mount `AutoResolveToastContainer` with `useAutoResolveToasts` | [code] |
| `EventLog.tsx:373-374` | raw ISO timestamp and `v<version>` on every row: show a relative time ("2 min ago") with the absolute time in a tooltip, and hide the version | [code] |
| `SpaceCombatOverlay.tsx:964`, `:992` | title "System 18" should use the system name; "Combat complete" is printed three times on the result screen | [seen] F/1 |
| `GameShell.tsx:595` | the invasion pill reads "View invasion · System 26"; use the name | [code] |
| `CargoLoadingTray.tsx:157` | `group.unit` raw: use `getUnitDisplayName` and `UnitIcon` (both exist in `UnitIcon.tsx`) | [code] |
| `ProductionBuilderDrawer.tsx:237` | add the unit icon, the cost and a reason when "+" is disabled | [code] |
| `TacticalMovementOverlay.tsx:741`, `:813`, `:859` | "Destination: system 26" and "Origin: #12": use names | [code] |
| `PaymentDrawer.tsx:349-351` | show the other value on each planet: "Jord +4 Res (loses 2 Inf)" | [seen] D/5a |
| `playerDisplay.ts:12` | seat 8 is `#000000` on a `#090d16` canvas: use a light outline or a different colour | [code] |
| `index.css` | add `@media (prefers-reduced-motion: reduce)` for `target-pulse`, `invasion-pulse`, `pulse-ready` and the toast slides | [code] |
| `PlayerSheet.tsx:398` and the other stats | remove the native `title` where a `Tooltip` already wraps the element (two tooltips show at once) | [seen] H/09 |
| `PlayerSheet.tsx:273-302` | move the sound toggle from the "Players" heading to a settings menu in the header | [seen] |

### Cross-cutting recommendations

1. **Design tokens, not literals.** `index.css:1-29` defines good tokens, but components bypass
   them: 227 `style={{...}}` blocks and 246 hex colour literals in `components/*.tsx` (grep).
   `TurnStatusBar.tsx` is all inline. Add semantic tokens (`--res`, `--inf`, `--tg`, `--comm`,
   `--vp`, `--danger-soft`, `--seat-1..8`) and move inline styles to classes. This is also what
   makes a light theme or a high-contrast mode possible later.
2. **One icon set for game quantities, used everywhere.** Today resources are a gold diamond and
   influence a blue crown in the player sheet (`PlayerSheet.tsx:116-140`). Elsewhere they are
   text: "Res", "Inf", "R/I", "+4 Res", "6 v", "TG", "Comm", a coin emoji in the toolbar, and
   rocket, shield and crossed-swords emoji in the combat overlay. Make `<Qty kind="res" n={4}/>`
   cover resources, influence, trade goods, commodities, tactic, fleet and strategy tokens, VP,
   action cards and secret objectives. It renders icon plus number plus an accessible name, and
   every drawer, bar, tooltip and the log uses it.
3. **Unit and planet legibility rules.** A unit is always icon plus count plus owner colour or
   symbol. A planet is always its name (abbreviated only on the map, unique per hex) plus
   resources and influence plus ready or exhausted state. A system is always "Name (#26)", with
   the bare number only as secondary text. Put this in one presentation module next to the
   id-to-name rule from the decision review (its cross-cutting point 4).
4. **A glossary and rules-tooltip system.** Wrap terms in `<Term id="nebula">`: anomalies,
   wormholes, fleet supply, capacity, production, sustain damage, the token pools,
   exhausted/ready, speaker, custodians, laws. Each shows a two-line rule summary and a
   "Rules ref" number. Reuse the existing `Tooltip` primitive (keyboard-accessible,
   `Tooltip.tsx:85-96`). The board tooltip already names anomalies without saying what they do.
5. **Onboarding for new players:** a dismissible "What can I do now?" line in the status strip
   ("Your turn: take a tactical or strategic action, or pass"), a map legend popover (tile
   colours, token markers, planet chip anatomy), and first-game coach marks for the player sheet,
   the overlays and the log.
6. **Submission state is part of every decision UI** (top 15, #6): one shared hook that exposes
   `idle | sending | slow | failed`, rendered the same way in every bar and drawer.
7. **A screen gallery for the standard screens**, like the decision gallery
   (`dev/decisionGalleryCases.ts` has 18 cases, all decisions). Add the lobby (empty, full, bots),
   the player sheet with 6 players, a mid-game board, the log with 200 entries, game over,
   disconnected and spectator, at 1440, 1024 and 390 px wide. Mobile has no screenshots today.
8. **Prefer bars and side panels over modals** for anything that needs the map (cross-cutting
   point 9 of the decision review applies to the dedicated UIs as well; see section 15).

---

## 1. Game shell, status header and banners

**Needs:** whose turn it is, what they are doing, the round and phase, the turn order and who has
passed, the speaker, whether I am connected, and when something needs me.

**Today:**
- The header is `TurnStatusBar` plus two buttons (`App.tsx:327-353`). It shows a connection dot,
  `v{gameVersion}`, "Round 2 • Action Phase • Speaker: You", and a banner such as "YOUR TURN:
  Awaiting your choice (activate_system)" [seen C/1].
- The banner text strings are built at `TurnStatusBar.tsx:23-39`. "Active Turn: You (Round 2,
  action)" repeats the round and phase shown on the left [seen F/1].
- Other seats get no decision UI at all unless the choice is a planet pick or a combat
  (`GameShell.tsx:633-651`). `SystemActivationBar` has a "Waiting for X to activate a system"
  branch (`SystemActivationBar.tsx:33-45`), but the dispatcher returns `null` before it is reached.
- The game-over state is the banner text "Game Over! Winner: X" (`TurnStatusBar.tsx:36-37`).
  `GameOverMsg.final_scores` is decoded (`decode.ts:192`) and then ignored.
- Loading is plain text: "Loading game state..." (`App.tsx:390`) and "Loading lobby..." /
  "Unable to load lobby." (`App.tsx:154-160`).
- Errors from history changes are a top `session-error` div (`App.tsx:320-324`). Undo
  confirmations use `window.confirm` (`App.tsx:244-247`).
- Disconnection shows only as the dot colour and the word (`TurnStatusBar.tsx:69-87`). The
  client retries on its own (`protocol/client.ts:610-620`), but nothing tells the user that a
  decision cannot be sent while the connection is down.
- There is no way to leave a running game or go back to the start page: `GameViewContainer`
  receives `onLeave` and drops it (`App.tsx:217`).

**Problems:**
- In a real game the stage is the constant "Waiting for player" (`projection.rs:594`), so the
  banner says nothing about *what* is being decided. The screenshots show subtypes only because
  the fixtures fill them in.
- Opponents and spectators cannot follow an action as it happens: no "Hacan is moving ships into
  Lodor", no combat preview until the overlay opens.
- Turn order, passed players and strategy cards, the core of the action phase, are not on screen
  at all (see section 5).
- When the game ends, the board stays as it is and the result is one line. The nightly proctors
  repeatedly flagged "winner at 2 VP in round 9" as a bug; it is correct (rule 81.2,
  `latest-summary.md` finding 10), and the UI should say so.

**Proposal:** a two-row header.

```
 ● Connected      Round 3 · Action phase · Speaker ● Ana            [Tech] [Objectives] [⚙]
 ┌──────────────────────────────────────────────────────────────────────────────────────┐
 │ 1 Leadership ● Ana 7VP │ 3 Politics ▲ Bob 5VP ✓passed │ 5 Trade ■ Cy 6VP ◀ ACTIVE │ ... │
 └──────────────────────────────────────────────────────────────────────────────────────┘
  Cy is taking a tactical action in Lodor (#26): moving ships            (your turn next)
```

- Row 2 is the initiative order: strategy card, seat symbol and colour, name, VP, a passed tick
  and an active marker. Clicking a chip scrolls the player sheet to that player. On mobile it
  collapses to "Cy (5 Trade) · you are next".
- Line 3 is the "what is happening" line. Build it from public facts the server already has:
  `GameView.active_player`, `board.active_system`, `board.combat`, `board.invasion`, and the
  latest public log detail. For your own turn it reads "Your turn: choose a system to
  activate". Never show raw subtypes.
- **Game over:** a full-width result panel listing the winner, final scores per player with a VP
  source breakdown, the reason ("No public objective left to reveal in round 9: highest VP wins;
  ties go to initiative order"), and buttons to view the final board and the log.
- **Connection:** when not connected, a sticky amber bar: "Reconnecting... your last action may
  not have been sent". Disable confirm buttons while it shows. In the same place, the
  "no answer from server" state from top 15, #6.
- **Loading and errors:** a skeleton board and sheet instead of text. Errors carry an action
  ("Retry", "Back to lobby").
- Add a "Leave game" item to the settings menu (`⚙`), which also holds sound, theme and help.

**Data:** `active_player`, `active_system`, the passed flags and the strategy cards are all in
`GameView` today. A public "what is happening" phrase needs either a client mapping from the
latest public event or a server-side public stage string in `PublicTurnStatus`; the client
mapping is enough to start. Game-over data: `final_scores` is already there; the reason needs one
field (`end_reason`) or can be inferred client-side (round 9 and no objective left).

**Effort / priority:** header strip M, P0; game over S, P0; connection and submission state S,
P0; loading and errors S, P2.

**Relates to:** the persistent turn action bar (the strip and the bar should share one row on
small screens); the decision review's context header.

## 2. Lobby and game creation

**Needs (host):** create a table, share it, fill seats with people or bots, set order, start.
**Needs (guest):** see who is there and what the game will be, join, become ready.

**Today:** `CreateLobby` has player count 2-8, nickname and an optional seed
(`Lobby.tsx:52-100`). `LobbyStatus` lists the positions with a seat badge, nickname, "(Host)",
ready and connected state, reorder arrows and "Add Bot" (`Lobby.tsx:229-314`).

**Problems:**
- Nothing says what game this is: no factions (assigned at start, invisible until then), no
  victory-point target, no expansion or map choice. The map picker plan covers the map only.
- Bots are not marked. A bot seat looks like any player, both in the lobby and in the game
  (no "bot" anywhere outside `Lobby.tsx`). The "✕" remove button appears for every non-host
  occupant, humans included (`Lobby.tsx:279-291`), labelled "Remove player".
- The add-bot modal is a hand-built `div` (`Lobby.tsx:315-395`), not the `Dialog` primitive, so it
  has no focus trap and no Escape handling [inferred from code]. The bot password is kept in
  `localStorage` in plain text by default (`Lobby.tsx:147-151`, `:165-169`).
- The host must press "Mark ready" as well as "Start game", which is redundant for the host.
- The start page cannot rejoin games: there is no list of "your games" (sessions live in
  `sessionStorage` per tab, `App.tsx:44-55`).

**Proposal:**

```
 Game lobby · 6 players · 10 VP · map: standard           [Copy invite link]
 ● 1  Ana (host)        ✔ ready   ● online        faction: random
 ▲ 2  Bot 2  [BOT]      ✔ ready                   faction: random     [✕ remove bot]
 ■ 3  (open)            [Invite] [Add bot]
 ...
 [Start game]  (waiting for 1 open seat)
```

Show the settings once they exist (VP target, factions). Add a bot badge, wired from a slot flag
the server already knows when it adds a bot. Use `Dialog` for the bot form, default "remember
password" to off, and let the host start without pressing ready. On the start page, list the
games of this browser.

**Data:** a slot `is_bot` flag (server); game settings once configurable.
**Effort / priority:** S, P2 (the bot badge P1, because it also matters in game, see section 11).
**Relates to:** `MAP_SELECTION_SCREEN_PLAN.md`.

## 3. The board

### 3a. Units and fleets

**Needs:** for each system, whose ships and ground forces are there, what kind, how many and
whether damaged; whether a system is contested; where the threats are.

**Today:**
- Standard view: a "N units" pill per tile, the same for every owner (`StandardOverlay.tsx:167-191`)
  [seen C/1: "6 units" on Mecatol Rex, "5 units" on the home system].
- Space control is computed (`boardPresentation.ts:594-609`), but drawn only as a slightly lighter
  grey border (`#475569` instead of `#334155`), with no player colour.
- Unit types, owners and damage appear only in the system inspector after a click
  [seen D/2: "2 × Dreadnoughts (1 damaged), 2 × Fighters, 1 × Infantry"].
- The hover tooltip says "Units: 6 | Command Tokens: 0" (`BoardTooltip.tsx:110-112`).
- The space combat overlay shows rocket and shield emoji with counts and "⚔ 0.4" average hits
  per tile, with no owner colour [seen C/4]. On #16, "2 rockets 1 shield" does not say who
  owns what.

**Problems:** this is the biggest legibility gap on the main screen. A player cannot see where
the enemy fleets are, how strong they are, or which systems are contested, without clicking
every hex. Combat starts as a surprise.

**Proposal:** per-owner unit stacks on the tile.

```
      #26 Lodor
   ┌───────────────┐
   │ ●▲2 ◆1   (Ana)│  <- owner chip in seat colour + symbol: carrier icon 2, dreadnought icon 1
   │ ▲▲3      (Bob)│  <- second owner => tile gets a red "contested" corner flag
   │   (LOD 3/1)   │
   │  inf 2 on LOD │  <- ground forces drawn on/next to the planet chip in owner colour
   └───────────────┘
```

- Up to two owners per tile, with the top 2-3 unit types as icon plus count. Anything more
  collapses to "+n". Ground forces sit on the planet chip.
- A tile with ships of two owners gets a red flag. A tile with your ships gets your colour on
  its border; this replaces the grey control stroke.
- Damaged units get a small crack mark. Fighters and infantry are summarised ("✈6", "inf 4").
- At zoom below 0.8, fall back to one owner-coloured count per owner.
- Space combat overlay: owner-coloured numbers, and a split "Ana 4 vs Bob 3" for contested
  tiles; drop the emoji.

**Data:** all present: `SystemView.units` with `unit_type`, `owner`, `planet` and `damaged`.
`UnitIcon` already draws every unit type (`UnitIcon.tsx`).
**Effort / priority:** M, P0.

### 3b. Planets, systems, anomalies, wormholes

**Needs:** planet names, resources and influence, who controls a planet and whether it is
exhausted, traits, tech skips, legendary abilities; system names; anomaly and wormhole effects.

**Today:**
- Planet chip: the first three letters of the id, upper-cased (`StandardOverlay.tsx:146`), and
  resources/influence at 7.5px (`:150-161`). [seen C/1: two "ARC" chips in one hex,
  "EXH" for Exhausted World.]
- The owner is shown by fill colour plus a symbol at 12px (`:107`, `:120-134`). Exhausted is a red
  stroke only (`:111-112`), which is colour-only signalling.
- Every system except Mecatol Rex is labelled "#nn" (`BoardTile.tsx:229-235`), including home
  systems [seen: Sol's home is "#1"].
- Anomalies get a coloured fill plus an 8px label ("NEBULA", "GRAVITY RIFT"). Wormholes are a 16px
  circle with α or β (`BoardTile.tsx:254-273`) [seen].
- Traits, tech specialties and legendary status appear only in the inspector
  (`SystemInspector.tsx`, planet block).

**Problems:** at the default 1440x900 the planet text is below legible size. Abbreviations
collide. The rule effects of anomalies (move stops in a nebula, +1 move or a lost die in a
gravity rift, no movement through a supernova) appear nowhere.

**Proposal:**
- Planet chip: unique 3-4 letter abbreviations from the content catalog ("ARC P", "ARCH"), and
  the full name when zoom ≥ 1.25.
- Resources and influence as two small coloured pips (gold ◆ 2, blue ♛ 3) at 10px minimum.
- Exhausted shown by a diagonal hatch plus lower opacity, not only by red.
- Corner glyphs for a tech skip (coloured dot) and for legendary (star).
- System title is the name of its first planet, or the anomaly or "Empty"; "#26" becomes
  secondary text. Home systems get the owner's colour band and a "Home" label.
- The hover tooltip gains one rule line per anomaly or wormhole ("Nebula: ships that move in
  must stop; defender gets +1 on combat rolls"), using the glossary from the cross-cutting
  recommendations.

**Data:** `PlanetMetaView` already has `label`, `traits`, `tech_specialties`, `legendary`.
Home-system ownership needs the faction's home system id (content catalog) [verify].
**Effort / priority:** M, P1.

### 3c. Command tokens and activation markers

**Today:**
- Tokens are 6px circles in the owner colour with the seat symbol at 8px, along the bottom edge
  of the hex (`BoardTile.tsx:306-329`) [seen: small orange dots].
- "ACTIVATED" and a dimming are drawn only when the viewer's own token is present
  (`BoardTile.tsx:191-217`).
- In activation mode every candidate hex shows a reticle plus a "TARGET" label (`:123-189`)
  [seen C/1: about 30 "TARGET" labels].

**Problems:** you cannot tell at a glance where opponents have committed tokens, which is a key
read in TI4 (who is boxed in, who can still move where). "TARGET" on nearly every hex adds noise
without adding information.

**Proposal:** token markers twice the size, in the owner colour with symbol, stacked in a fixed
corner, with a tooltip "Bob's command token". An "ACTIVATED" ribbon for any owner's token placed
this round, plus a subtle "your token" dim for your own. In activation mode, dim the non-targets
instead of labelling the targets, and keep the reticle only on hover or focus.
**Data:** present (`SystemView.command_tokens`). **Effort / priority:** S, P1.

### 3d. Overlay toolbar and hover tooltip

**Today:** the toolbar offers Standard, Res / Inf, Space Combat, Ground Combat and Tech Skips
(`MapOverlayToolbar.tsx`).
- The Res / Inf overlay replaces units and planets with per-tile totals [seen C/3]. Mecatol shows
  "2/3 2/5 Exhausted", which is ambiguous: is it ready/total or resources/influence?
- The toolbar's tooltip is overlapped by the seat legend [seen C/3: "& Influence (Ready vs
  Exhausted)" with its start hidden behind the "Player 2" chip].
- The hover tooltip is a fixed box (`.board-tooltip`), not anchored to the hex, and appears only
  on mouse hover (`Board.tsx:257-273`).

**Proposal:** label overlay numbers ("R 2/3 · I 2/5", with the icon set). Move the seat legend
under the toolbar, or merge it into the status strip of section 1, which makes it redundant. Show
the tile tooltip on keyboard focus as well as on hover, and anchor it near the hex. Add a small
overlay legend in the corner. **Effort / priority:** S, P2.

### 3e. Zoom, pan and performance

**Today:**
- Buttons +, − and reset only, with scale clamped to 0.5-2.5 (`Board.tsx:114-118`). No wheel zoom,
  no pinch, no keyboard pan, no fit-to-view.
- The default view cuts off the bottom row at 1440x900 [seen C/1, D/5b].
- Any pointer-down on the board starts a pan (`Board.tsx:95-100`); no `touch-action` is set
  [inferred: on touch screens this fights the page and taps].
- `buildBoardPresentationModel` runs on every render (`Board.tsx:81`), and every hover sets state
  (`:257-273`), so the whole board model is rebuilt on each mouse move over a new hex.
- Several infinite animations (`target-pulse` on every candidate hex with an SVG glow filter,
  `index.css:1690-1706`) run with no reduced-motion fallback.

**Proposal:**
- Fit-to-view on load and on resize, and make reset mean "fit".
- Wheel and pinch zoom around the pointer, arrow keys to pan, `+`/`-` keys, `touch-action: none`
  on the SVG, and pan only after a drag threshold.
- `useMemo` the presentation on `[board, seatingOrder, players, pendingChoice, viewerSeat,
  selectedSystemId]`, and keep hover state inside the tile or in CSS.
- Pause pulse animations when there are more than 10 targets, or under reduced motion.

**Effort / priority:** M, P1 (fit-to-view alone is S).

### 3f. Player colours and symbols

The Okabe-Ito palette with seat symbols (`playerDisplay.ts:4-14`) is a good, colour-blind-aware
choice, and the symbols back up the colours. Problems:
- Seat 8 is pure black on the `#090d16` canvas.
- Seat 1 orange (`#E69F00`) is close to the warning orange of "YOUR TURN" and to the resource
  gold.
- Seat 4 yellow is close to the selection yellow `#facc15` (`BoardTile.tsx:54`).

Keep the palette, but give every seat-coloured fill a light outline, change seat 8 to a light
grey with a dark symbol, and make selection a white double outline rather than yellow.
**Effort / priority:** S, P2.

## 4. System inspector and card details

**Needs:** everything about one system: planets with values, owner and state, units by owner,
tokens, anomaly and wormhole rules, and what I can do there.

**Today:**
- `SystemInspector` in a `DetailPanel` at the top right of the board (`Board.tsx:283-288`). It
  lists anomalies and wormholes as badges, planets with resources and influence, control and
  traits, units grouped by owner for space and per planet, and command tokens
  (`SystemInspector.tsx`).
- The title is repeated: "System Lodor #26" in the panel header, then "SYSTEM DETAILS / Lodor
  #26" below it (`:28`, `:36-50`) [seen C/2].
- The panel covers about a third of the map, including hexes next to the selected one [seen C/2:
  #33, #27 and #79 hidden].
- `CardDetails` uses the same panel slot (`App.tsx:413-418`).

**Problems:**
- Anomaly badges carry no rule text.
- Tech specialties and legendary abilities are not explained.
- There is no capacity or fleet-supply status per owner, and no hint of what activating the
  system would trigger (space cannon in range, enemy PDS).
- On a 1024px tablet the panel and the decision bar overlap [inferred from the fixed positions in
  `index.css:1571-1580`].

**Proposal:** one header line ("Lodor · #26 · α wormhole"). A "Rules here" block fed by the
glossary. Per owner: "Ana: 3 ships (capacity 4/6), 2 infantry on Lodor". If the viewer has a
decision about this system, its primary action appears at the bottom. Make the panel dockable
(right side on desktop, bottom sheet on mobile) and pan the map so the selected hex stays visible.
**Effort / priority:** S-M, P2.
**Relates to:** the decision review E1 (activation information), which can reuse this block.

## 5. Player sheet and player list

**Needs:**
- for every player: VP, strategy cards and whether they are used, passed or not, trade goods and
  commodities (against the cap), command tokens per pool, ready and total resources and
  influence, technologies (count by colour), leaders (unlocked, exhausted, purged), relics,
  promissory notes in play, scored secrets, action-card and secret counts;
- for me, also my hand, my secrets with progress, and my promissory notes.

**Today:** `PlayerSheet.tsx` renders per player:
- seat badge, label plus "(You)", the raw faction id, VP with a breakdown tooltip (`:325-353`);
- "TG: n | Comm: n" and "Tokens: a/b/c" (`:356-375`);
- a stats row of resources, influence, systems, planets and production capacity, each with an
  icon and a tooltip (`:378-510`);
- strategy-card buttons, with an exhausted class (`:513-551`);
- action-card and secret counts, scored secrets;
- for the viewer, the private hand with full text and a per-card switch, and the held secrets
  (`:581-805`). [seen C/1, A/3]

**Problems:**
1. **Hidden state** (top 15, #2). Technologies, leaders, relics, `passed`, laws owned and the
   active player are never drawn, though `PlayerView` carries them (`view.rs:14-44`). Promissory
   notes and relic fragments are not in the view at all.
2. **Misleading displays** (top 15, #1): the reaction switch (`:204`, `:236-244`, `:685-708`) and
   the Mecatol VP row (`:63-73`).
3. **Density and labels:** "Tokens: 3/3/2" has no pool names. The icons in the stats row are not
   explained without hovering. Commodities have no cap ("Comm: 0" — out of 3? 4?).
4. **The private hand dominates:** four action cards with full text take most of the column
   [seen C/1]. The second player's card is pushed below the fold at 900px height, so with 6
   players the opponent cards are a long scroll.
5. **Ordering:** "you first, then turn order" (`:208-230`) makes the list stable, but it hides
   who acts next.
6. **Inline styles and hover handlers** that write `style` directly (`:292-299`).

**Proposal:** a compact opponent row and an expanded "me" card.

```
 ┌ ● Ana (you) · Sol ·  7 VP ───────────────────────── 5 Trade (used) ┐
 │ ◆ 8/9  ♛ 7/10   TG 2  Comm 0/4   Tactic 3 · Fleet 3 · Strategy 2  │
 │ Tech: ●2 ●1 ●0 ●1 (blue, green, red, yellow)   Relics: —           │
 │ Leaders: Agent ready · Commander locked · Hero locked              │
 │ [Hand 4 ▸] [Secrets 1 ▸] [Promissory 3 ▸]                         │
 └────────────────────────────────────────────────────────────────────┘
 ▲ Bob · Hacan · 5 VP · 3 Politics ✓ · PASSED         TG 1 · AC 2 · SO 1   ▸
 ■ Cy  · Letnev · 6 VP · 7 Warfare · ACTIVE           TG 0 · AC 4 · SO 2   ▸
```

- Opponents are single rows that expand on click, in initiative order, with active and passed
  markers.
- The hand and secrets of the viewer are collapsible lists. Card text appears on expand or hover,
  so a 7-card hand fits.
- Secrets show progress where the engine computes it (`objective_progress` covers public ones;
  see section 6).
- The VP badge breakdown always sums to the total (an "Other" row), and lists Custodians,
  Imperial, agenda and Support for the Throne when the data exists.
- The reaction switch: either wire it (server-side auto-pass per card) or remove it. Never ship
  a toggle that does nothing.

**Data:** present: techs, leaders, relics, passed, laws, strategy cards. Missing in the view:
promissory notes (held and in play), relic fragments, commodity cap, and the VP source ledger.
Custodians and Support for the Throne VPs are otherwise unknowable on the client. These are
server changes to `PlayerView` (a protocol version bump).

**Effort / priority:** layout and hidden state M, P0. Server fields M, P1. Misleading items S, P0.
**Relates to:** the status strip in section 1 (same data, two densities).

## 6. Objectives modal

**Needs:** which objectives are out, what each requires, who scored what, how close each player
is, how many objectives are left, when the game ends.

**Today:** `ObjectivesModal.tsx` shows a matrix of objectives by players, grouped into Stage I,
Stage II and Other. Each cell carries scored, selected or ready state and a progress bar
`have/threshold` (`:240-330`, `:390-460`). Scoring mode uses the same matrix with a footer
(`:464-510`). This is one of the better screens [code; no screenshot of the browse mode].

**Problems:**
- No count of unrevealed objectives, and no "the game ends after round N if the deck runs out".
  The nightly games all ended in round 9 at 1-5 VP, and ten proctor entries called it a rules
  bug (`latest-summary.md` finding 10).
- Secret objectives scored by others are not in the matrix, so their VP are invisible here.
- No VP total column. The modal is only reachable from a header button, with no badge when you
  can score.

**Proposal:** add a header line "Stage I: 5 revealed, 0 left · Stage II: 3 revealed, 2 left ·
the game ends when no objective can be revealed (round 9 at this pace)". Add a VP column and a
"Secrets: n scored" row per player. Show a badge on the Objectives button when the viewer meets
an unscored objective (`objective_progress.satisfied`). Echo a VP track in the status strip.
**Data:** deck sizes per stage (server, small); the rest is present.
**Effort / priority:** S-M, P1.

## 7. Technology modal (browse and research)

**Needs (browse):** who has which tech, the prerequisites, and what each does. **Needs
(research):** what I can research now, what skips I have, the cost.

**Today:**
- The tech tree, a roster of players with tech markers, a faction-tech section, and in research
  mode the planet-skip toggles and a selection of up to two (`TechnologyModal.tsx:404-560`).
- Prerequisites are checked client-side (`checkTechPrerequisites`), and the research cost is
  derived at `:252`.

**Problems:**
- The stand-alone modal in `App.tsx:440-444` gets neither `viewerSeat` nor `board`, so in browse
  mode the "viewer" is `choice.actor` or `playerList[0]` (`:74-84`). For a spectator, or for seat
  2 browsing, the highlights may be someone else's [code; verify in the browser].
- Exhausted technologies are tracked (`technologyData.ts:126`) but not shown on the player sheet.
- No unit-upgrade comparison (base vs upgraded stats), which is the common reason to open the
  modal.

**Proposal:** pass `viewerSeat` and `board`, and add a "compare with player" selector in browse
mode. For unit upgrades, show the stat delta ("Carrier II: move 2 (+1), capacity 6 (+2)").
**Effort / priority:** S, P1 (the viewer bug), M, P2 (comparison).

## 8. Event log, history and undo

**Needs:** what just happened (especially while I was not looking), a way to find things (combat
results, cards played, trades), and, for the host, a safe undo.

**Today:**
- A bottom drawer, collapsed by default, labelled "Event Log n" [seen C/1].
- Expanded, it is a tree of round, phase, action marker, stage marker and entry
  (`EventLog.tsx:117-244`).
- Each entry shows `#n`, the actor's seat symbol in colour, the text (with action-card names
  underlined and their text in a tooltip, `:65-114`), "Private" or "Referee", the raw timestamp,
  `v<version>` and, for the host, an "Undo" button (`:324-389`).
- Redo has three buttons: one, batch, action (`:486-503`).
- Undo is host-only (`App.tsx:424`) and confirmed with `window.confirm` (`App.tsx:244-247`).

**Problems:**
- **Ids in text.** The server builds details such as "{actor} activated #{id}", "{actor}
  exhausted {planet}", "{actor} placed {unit} on {planet}" and "scored objective {option.id}"
  (`protocol/server.rs:305`, `:351-358`, `:505-510`, `:779`). The client writes "moved {unit}
  from #a to #b" with the raw unit id (`EventLog.tsx:321-323`). Unrecognised choices become
  "Decision resolved" (`:323`).
- **No filtering**, no search, no "since my last turn" marker. The collapsed bar shows a count,
  not the latest event.
- **Developer data on every row:** the ISO timestamp, the version, and the entry number taken
  from the id (`:317`).
- **Undo on every row** sits next to normal reading. It is easy to misclick, and the stakes
  ("for everyone") appear only in a browser dialog. Non-hosts cannot request an undo.
- Action-card matching scans every card name over every text fragment on every render
  (`:65-114`), O(entries × cards) [inferred: noticeable with long logs].

**Proposal:**

```
 Log  [All] [Mine] [Combat] [Cards] [Trade] [Agenda]   🔍        ▾
 ── Round 3 · Action ─────────────────────────────────────────────
  ● Ana   activated Lodor (#26)                          2 min ago
  ● Ana   moved Carrier ×2, Infantry ×2 → Lodor
  ▲ Bob   played Sabotage ▸ (cancels Direct Hit)
  ⚔ Combat in Lodor: Ana won · Bob lost 2 Cruisers
 ─ since your last turn ─
 Collapsed bar:  Log (214) · ▲ Bob passed · 10 s ago                 ▴
```

- Names through the shared id-to-name presentation.
- Combat and invasion results as one summarised entry with a link to the result (reuse
  `CombatResultSummary`).
- Filters by kind and actor, and a "since your last turn" divider.
- Timestamps relative, with the absolute time in a tooltip. Hide the version.
- History controls move to a "History" menu in the log header ("Undo last action", "Undo to
  here..." from a row's context menu) with an inline confirm that names the affected players.
  Non-hosts get "Ask host to undo".
- Precompute card-name matches once per entry (memoised).

**Data:** the server's public detail strings should carry names (or structured ids the client
names). The current ids come from `protocol/server.rs`. An "undo request" message is new.
**Effort / priority:** names and filters M, P1; dev data and the collapsed preview S, P1; undo
redesign S-M, P2.

## 9. Trade desk

**Today:** `TradeDeskModal.tsx` has "YOU OFFER" and "YOU RECEIVE" sections, item labels such as
"Promissory Note: {note} for {price} TG" and "Action Card: {card}", a "Net Value (You)" figure,
and offer tabs (`:72-180`, `:330-370`). Labels come from `tradeDecoder.ts`, which passes the ids
through (`:77`, `:108`) [code; verify whether the ids are mapped later].

**Problems:**
- Promissory notes and action cards may appear as ids.
- The "Net Value" number has no explanation (what is a card worth?).
- No view of the partner's public resources (TG, commodities, commodity cap), which is what the
  trade depends on.
- Covered in part by the decision review (A1, I: one Trade button). This review adds the content
  of the desk.

**Proposal:** names and card text on hover. Replace "Net value" with explicit columns ("you
give: 2 commodities · you get: 2 TG"). A partner strip with their TG, commodities/cap and
neighbour status. **Effort / priority:** S, P2.

## 10. Toasts and notifications

**Today:**
- `AutoResolveToast` and `useAutoResolveToasts` are complete and pretty [seen G/1-3], but only
  `dev/ToastGallery.tsx` uses them (grep `showToast`).
- `ChoiceOptionDto.auto_resolved` exists in the protocol (`types.ts:72`) and nothing reads it.
- The only real-game notifications are the turn sound (`useTurnSound`) and the banner colour.

**Proposal:**
- Mount the container in the game view and fire a toast when the server auto-resolves a choice
  for the viewer.
- Add "while you were away" toasts for public events that affect the viewer: you were attacked,
  a card targeted you, an agenda passed, someone scored.
- Rate-limit them to three on screen and link each to its log entry.
- Add a tab-title badge ("(!) Your turn — TI4") for background tabs.

**Effort / priority:** S, P1.

## 11. Spectators, other seats and bot seats

**Today:**
- Spectators get `viewerSeat` undefined. For a non-combat choice the dispatcher's guard
  `viewerSeat !== undefined && ...` (`GameShell.tsx:645-649`) is then false, so a spectator could
  be shown the acting player's decision UI if a pending choice ever reached them. The server
  projects pending choices only to their seat (`projection.rs`, `project_pending_choice`), so in
  practice spectators see nothing [code + inferred].
- Some workflows carry a `spectatorNotice` ("Observing payment in progress for ...",
  `PaymentDrawer.tsx`; production, trade), but these paths are not reachable for non-actors.
- Bots are invisible as bots in game.

**Proposal:** treat "watching" as a first-class mode. Show the status strip of section 1, the
active system highlighted on the map, live combat and invasion overlays (already spectator-capable
for combat, `GameShell.tsx:614-631`), and a "Spectating" chip in the header. Bot seats get a small
robot badge next to the name everywhere `PlayerIdentity` renders (and "thinking..." while a bot
holds the decision).
**Data:** the bot flag from the lobby slot. **Effort / priority:** S-M, P1.

## 12. Mobile and tablet

**Today:**
- Below 720px the shell becomes one column. Players and Events are bottom drawers opened by two
  fixed buttons (`GameShell.tsx:963-988`, `index.css:1620-1670`). The detail panel and tooltip
  are repositioned (`index.css:1571-1580`). Choice banners stretch across the screen.
- There are no mobile screenshots, so everything below is inferred from CSS.

**Problems [inferred]:**
- The header is a single non-wrapping flex row (`TurnStatusBar.tsx:57-66`) with two buttons next
  to it (`App.tsx:327-353`), so at 390px the banner text and the buttons overflow.
- The board has no pinch zoom (3e).
- The 330px player sheet is a fixed width in desktop mode (`PlayerSheet.tsx:251`). On a
  1024px tablet the board loses a third of the width, and the event log drawer is offset by
  `right: 330px` (`index.css:113-120`).
- Decision modals at 960px width (see decision review point 10).

**Proposal:** a mobile header of "Round · Phase · [active player chip] · [⚙]", with the strip from
section 1 as a swipeable row. Bottom-sheet decision bars. A tablet layout (720-1180px) where the
player sheet collapses to the compact opponent rows of section 5. Add 390, 768 and 1024 widths to
the screen gallery. **Effort / priority:** M, P2 (P1 if phones are a target; see open questions).

## 13. Keyboard and accessibility

**Good:**
- Primitives with focus trap, overlay stack and Escape (`primitives/core`, `dialog/Dialog.tsx`).
- Accessible tooltips (`Tooltip.tsx:85-96`).
- SVG buttons with labels (`BoardTile.tsx:87`).
- `aria-live` on the header (`TurnStatusBar.tsx:56`).
- Global `:focus-visible` styling (`index.css:1682-1685`).

**Problems:**
- **Colour-only signalling:** exhausted planets (red stroke), resources and influence on the
  overlay (gold and blue boxes without labels), the connection dot (paired with a word, which is
  fine), and ready/not-ready in the combat tally (`data-valid` green and red borders,
  `index.css:2110-2120`).
- **Contrast:** `--color-text-faint #64748b` on `--color-surface #0f172a` is about 3.7:1 (below
  4.5:1 for small text). It is used for `v40`, empty states and hints.
- **SVG text sizes** of 7.5-9px at scale 1 (`StandardOverlay.tsx:142`, `:156`; `BoardTile.tsx:182`,
  `:211`).
- **Tab order:** every hex and every candidate planet is a tab stop (`SvgButton isInteractive`),
  about 37 hexes plus planets, with no roving tabindex and no skip link to the decision bar.
  The tile tooltip appears only on mouse hover.
- **Duplicate tooltips:** native `title` plus the custom `Tooltip` on the same element
  (`PlayerSheet.tsx:394-399` and the other stats) [seen H/09]. Screen readers read the text
  twice.
- **Modals not using the primitive:** the add-bot form (`Lobby.tsx:315`).
- **Motion:** no `prefers-reduced-motion`.
- `window.confirm` for undo cannot be styled and is easy to dismiss by reflex.

**Proposal:**
- A roving tabindex on the map: arrow keys move between hexes, Enter selects, `i` inspects.
- Shortcuts with a `?` help sheet: `t` techs, `o` objectives, `l` log, `p` players, Space
  pass/end (shared with the action bar).
- Add a pattern or text to every colour signal.
- Raise the faint token to about `#7c8aa0`.
- Minimum SVG text of 10px at fit zoom.
- Remove the duplicate `title`s, use `Dialog` everywhere, and add reduced-motion.

**Effort / priority:** M, P2 overall. The duplicate tooltips and reduced-motion are S.

## 14. Theming, consistency and performance feel

- **Mixed styling systems.** Tokens in `index.css`, five component CSS files, and inline styles
  with hard-coded colours in most components (`TurnStatusBar.tsx`, `PlayerSheet.tsx`,
  `SystemInspector.tsx`, `BoardTooltip.tsx`, `Board.tsx`). The same "panel title" appears as
  `h2` 16px `#94a3b8` (`PlayerSheet.tsx:270`), as an uppercase 11px label
  (`SystemInspector.tsx:36-44`), and as a `DecisionHeader`. Pick one type scale (12/14/16/20) and
  one panel header component.
- **Button language** is inconsistent across flows: "Confirm Activation", "Confirm", "Cast votes
  (+8 staged)", "Done Voting", "Stage 5 more to pay", "Done", "Finish", "Resume decision",
  "Resume: <prompt>". Recommended: the primary button names the effect ("Pay 5", "Activate
  Lodor", "Cast 8 votes FOR"); the secondary is "Cancel" or "Skip"; "Done" means "I am finished
  with this step and nothing is staged".
- **Emoji as icons** (🚀 🛡 ⚔ 🪐 💰 🤖 ⚔️ in the toolbar, overlays, lobby and invasion pill) render
  differently per OS and cannot be coloured by owner. Replace them with the SVG icon set.
- **Re-render flicker [inferred]:** the board model is rebuilt per hover (3e); the event log tree
  is rebuilt per new event and re-runs card matching; `GameShell` keys the dispatcher on history
  generation and invasion sequence (`GameShell.tsx:1021`), so any undo remounts every open
  decision UI, which is correct but visible. Measure with the React profiler before optimising.
- **Theme:** only dark. A light or high-contrast theme becomes cheap once colours are tokens.
  Not a priority.

## 15. Dedicated decision UIs: information still hidden

These exist and work. The findings are about missing information, not about routing (the
decision review's subject). Effort and priority are per item.

### 15.1 PaymentDrawer

**Today:** a modal with "Pay 5 Resources", owed vs staged, a progress bar, ready planets with
checkboxes and "+n Res", a trade-good stepper with "(Available: 2)", and "Done" and "Stage n more
to pay" buttons [seen D/5a]. Map picking requires minimising the modal. Payable planets are not
highlighted on the map, and a pick shows no mark there (D/5b-5c captions).

**Missing:**
- what the payment is for, beyond the prompt text;
- the opportunity cost of each planet (its other value, for example influence for a resource
  payment; this matters for the agenda phase);
- an "optimal" suggestion that pays the exact amount while keeping the most influence;
- the leftover after payment.

The "Done" vs "Stage 5 more to pay" pair reads as two finishing buttons. A nightly bug (two
variants of one planet staged, run 32) suggests the variant rows also need clearer labels.

**Proposal:**

```
 Pay 5 resources — for: Carrier ×1, Fighter ×2 at Jord      staged 5/5 ✓
  [Suggest: Lodor + Quann + 1 TG (keeps Jord's 2 influence)]
  ☑ Lodor     3 res   (1 inf)
  ☐ Jord      4 res   (2 inf)
  ☑ Quann     1 res   (1 inf)
    TG  [-] 1 [+]  of 2
  After paying: TG 1 · ready influence 9
  [Pay 5]   [Cancel production]
```

Make it a side drawer rather than a modal, so the map stays usable, with payable planets ringed on
the map as the planet bar does. **Data:** planet influence is in the board meta; "for what"
needs `details.purpose` (server, small). **Effort:** M. **Priority:** P1.

### 15.2 ProductionBuilderDrawer

**Today:** fleet-supply, production-capacity and resources counters, steppers per offered unit
with a text label, Reset and Produce (`ProductionBuilderDrawer.tsx:169-300`). Fleet supply comes
from `context.details.fleet_supply` and otherwise shows nothing (`:125-127`).

**Missing:** unit icons and stats (cost, combat, move, capacity), units left in reinforcements
(plastic limits), why "+" is disabled (capacity, resources, supply), and where the units appear
(space or which planet). **Proposal:** a unit card grid ("Carrier · 3 ◆ · 2 units of capacity ·
4 left"), and a disabled reason in the tooltip. **Data:** unit stats are in the catalog;
reinforcement counts need the server. **Effort:** S (icons, reasons), M (reinforcements).
**Priority:** P1.

### 15.3 TacticalMovementOverlay and CargoLoadingTray

**Today:** a modal tray titled "Move Units" with "Destination: system 26", a fleet-supply gauge,
groups per origin "Origin: System #12", ship steppers with icon, name, available and capacity,
carryable cargo steppers, and "Moves and cargo loading are committed sequentially"
(`TacticalMovementOverlay.tsx:725-1050`). The cargo tray lists `group.unit` raw.

**Missing:**
- the destination's contents (enemy ships, PDS, planets), so the player sees the combat they are
  walking into;
- the path and the anomalies on it (the board has `MovementVectorsOverlay`, but the modal covers
  the board);
- the capacity totals across the selected ships while loading;
- the combat-odds preview the advisor already computes for the combat overlay;
- a summary of what stays behind in each origin.

The nightly reports show three classes of server rejection here (en-route cargo, Gravity Drive,
mid-move reactions, `latest-summary.md` findings 1-3). Each surfaced to the player as a generic
error banner.

**Proposal:** a side panel with the board visible. The destination card sits at the top ("Lodor:
Bob 2 Cruisers, PDS ×1 → space combat, space cannon 1 die"). Origins are named. Moving ships draw
vectors on the map. When the engine offers a ship only through a limited ability (Gravity Drive),
the row says so ("1 via Gravity Drive"). Errors are translated ("Your carrier can pick up ground
forces in Lodor: choose cargo or skip"). **Effort:** M. **Priority:** P1.

### 15.4 SystemActivationBar

Already covered by the decision review E1 (tactic tokens, enemy presence, reachability). Two
additions:
- the non-actor branch (`SystemActivationBar.tsx:33-45`) is unreachable through the dispatcher
  (section 1);
- "Not Targetable ... cannot be activated in this action" should say why (no token left, already
  activated, home system of another player).

**Effort:** S. **Priority:** P2.

### 15.5 PlanetSelectionBar

**Today:** good. The card name, "mine Lodor (3R/1I)", chips per candidate, and a map reticle
[seen D/1a-1b, D/2]. Labels still carry lower-case ids: "Place pds on jord", "Place spacedock on
jord" [seen D/2]. For a Mining Initiative or an Elect Planet pick, the bar does not show the
planet's current controller or what the effect yields ("+3 TG").

**Proposal:** structure names and icons, and the yield preview from the card text. **Effort:** S.
**Priority:** P2.

### 15.6 AgendaBallotModal

**Today:** the agenda card with both outcomes, outcome buttons with "n votes cast", Abstain, a
planet list with "6 v", Reset, "Done Voting", and "Cast votes (+8 staged)"
[seen B/1-3]. The text "Votes are submitted one decision at a time. Later planets and finishing
must be offered again; the sequence stops if interrupted." is implementation detail shown to
players [seen B/2].

**Missing:**
- who voted what so far and who is still to vote, in order;
- the viewer's total available votes (the decision review G already lists this);
- vote modifiers (Xxcha, Argent, action cards);
- the laws already in play (`table.laws` is never shown, section 5);
- the consequence of each outcome for the viewer (for example "you would lose 2 VP");
- the board, hidden behind the blur, although many agendas are about planets.

The fixture card uses YES/NO while the buttons say FOR/AGAINST [seen B/1]; check the real data.

**Proposal:** a vote table ("Ana FOR 8 · Bob AGAINST 5 · Cy (voting now) · Dee —"), "You have 14
votes (9 ready influence + 5 from …)", "6 v" written as "6 votes", and the implementation note
removed. **Effort:** S-M. **Priority:** P1.

### 15.7 InvasionOverlay and InvasionLandingTray

**Today:**
- Landing staging with a default target ("★ Default"), "Already on planet", per-unit steppers,
  "Remaining draft: infantry → jord", "Reset to Defaults", "Confirm landings" and "Done
  com..." [seen H/17-18].
- The minimised pill says "View invasion · System 26" (`GameShell.tsx:595`).

**Missing:**
- planet names, capitalised (`jord` [seen H/18]);
- the defender's forces and PDS on each planet in the landing tray (they are in the overlay but
  not next to the stepper);
- "Odds unavailable" with no reason [seen H/17];
- what is lost if no ground forces land.

The last button is cut off at the tray width ("Done com…") [seen H/18].

**Proposal:** a per-planet card "Jord: Bob 2 infantry, PDS (shield) · you land 3 → 68% win", and
buttons that fit or wrap. **Effort:** S. **Priority:** P1.

### 15.8 SpaceCombatOverlay and CombatResolutionModal

**Today:** an arena dialog with attacker and defender fleet cards, die-roll badges with
tooltips [seen H/16], odds from the advisor with a fallback estimate
(`SpaceCombatOverlay.tsx:355-460`), action cards held, and the result summary [seen F/1]. The
board is fully hidden behind an opaque overlay [seen F/1].

**Missing or unclear:**
- "System 18" instead of Mecatol Rex;
- "0 + 1 damaged" (previously plus newly damaged, `:774-788`) is unclear without hovering;
- the result screen says "Combat complete" three times;
- the viewer's own action-card names appear on the attacker card. That is fine for the owner,
  but check that a spectator never sees them (they come from `held_action_cards`, which is
  redacted for others);
- no reminder of what happens next (invasion, retreat already announced).

**Proposal:** "Damaged: 1 (new this round)", the system name in the title, one "Combat complete"
line, and a "Next: invasion of Lodor" line. Keep a small map inset so the location is clear.
**Effort:** S. **Priority:** P2. **Relates to:** the hit-assignment panel (in progress).

### 15.9 TechnologyModal in research mode

See section 7. In addition, the research footer should state the cost and what pays it
("Technology primary: free · second tech costs 6 resources"), and the skips row should say which
planets are being counted ("🪐 Tech Skips" uses an emoji and no explanation of exhausting).
**Effort:** S. **Priority:** P2.

---

## Suggested order of work

1. **Stop misleading players (S):** the reaction switch, the VP breakdown, the duplicate header
   stage, `viewerSeat` for the tech modal, game version hidden.
2. **Make the state visible (M):** the player sheet with passed, active, techs, leaders, relics
   and laws; the status strip; the game-over panel; the submission and connection state; mount
   the toasts.
3. **Make the map readable (M):** per-owner unit stacks, token markers, planet chips, system
   names, fit-to-view and zoom.
4. **Names everywhere (S-M):** one id-to-name module shared with the decision review, applied to
   the log (server detail strings), cargo, production, movement, combat and invasion.
5. **Map-linked decisions as side panels (M):** payment, movement and cargo with the board visible
   and highlights.
6. **Server view additions (M):** promissory notes, relic fragments, commodity cap, VP ledger,
   objective deck counts, bot flag.
7. **Accessibility, tokens, mobile (M, ongoing),** each step with a screen-gallery case.

## Open questions for the user

1. **Devices.** Are phones a real target, or desktop and tablet only? This decides whether
   section 12 is P1 or P2.
2. **The per-card "Always / Never offer" switch.** Should it become a real server-side auto-pass
   preference, or be removed? Today it does nothing, and artifact A describes it as working.
3. **Protocol additions.** May `PlayerView` and `TableView` grow promissory notes, relic
   fragments, the commodity cap, a VP ledger, objective deck counts and an `is_bot` flag (a
   protocol version bump)? Or must the client derive what it can?
4. **Undo policy.** Is undo host-only by design? Should other players be able to request one?
5. **Information policy for opponents.** Should non-active players see a live "what X is doing"
   line, for example "Bob is moving ships into Lodor" before the move is confirmed? Or only
   committed events?
6. **Visual direction.** Keep the neon sci-fi look and only fix legibility, or move to a calmer,
   denser "board-game table" style? Is a light theme wanted at all?
7. **Factions and game settings in the lobby.** Are faction choice and the VP target planned
   for the lobby, or always random and fixed?
