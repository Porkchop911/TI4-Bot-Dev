# web2: the start screen, the lobby and the setup of a game

Status: plan, 2026-10-12, second version. Phases 0, 1, 2 and 3 are built (2026-10-13, see
"Progress of phase 2" and "Progress of phase 3"). Phase 5 is built too (see "Progress of
phase 5": the rule bot in a crate, the setup in `ti4-view`, the exports of the wasm host), and
so is phase 6 (see "Progress of phase 6": single player on the screen of the lobby), and
phase 7 (2026-10-14, see "Progress of phase 7": the table in the game). Phase 4 (the map) is
skipped for now (owner, 2026-10-14) and is the one phase that is left.
**Phase 4 (the map) is postponed until after phase 6** (owner, 2026-10-13): the target is
single player on the shared setup. The order is 6, then 4, then 7. The text below it is of the
day of the plan:
nothing of web2 was built then. The first version was
written before the merge of `felix/main` (commit `6f0eb749`). That merge built a part of this
plan on the server, in another shape, so the decisions and the phases were made again with the
owner on the same day. "What the merge changed" says what is there now.

This is the plan that `2026-10-11-ONLINE_MULTIPLAYER_PLAN.md` names in its last line ("a screen
for the lobby of several humans is the next plan").

## Goal

A game is set up in web2, in single player and in multiplayer, on the same screen and with the
same code. A table of several people is made, filled and started from web2. Today `/` is a
local game on a fixed setup, `?online` starts a game of eight with seven random seats at once
(`dev/OnlineApp.tsx`), and web2 has no screen before a game.

What the owner asked for (2026-10-12):

1. The features of the lobby of `web/` (`web/src/components/Lobby.tsx`, `FactionPicker.tsx`,
   `MapPicker.tsx`, `hooks/useLobbySession.ts`), with a better UI.
2. The host chooses the number of players and the map, in the lobby.
3. The host chooses the set of strategy cards.
4. Factions are not bound to the seat.
5. Seats that the server plays can be added in the lobby, with a wait time that the host sets,
   so that a draft can be tested while the other seats "think".
6. Several seats of one game are played from several tabs of one browser.

## Decisions (owner, 2026-10-12)

Decisions 1, 2, 3, 6, 7, 11 and 16 were made after the merge. They replace the decisions of
the first version with the same numbers.

1. **A player chooses a faction in the lobby, or leaves it random.** This is what the server
   has (`POST …/lobby/seat-faction`): a person chooses for the own seat, the host for any
   seat, and the choice is public. A seat with no choice gets a faction from the seed of the
   game at Start. The first version had another design (two factions drawn after Start, the
   choice hidden); it is not built.
2. **The factions are the 24 of `ti4_engine::seating::LOBBY_FACTIONS`**: the base game and
   Prophecy of Kings. Keleres, the Obsidian and the factions of Thunder's Edge are not
   offered.
3. **The sets of strategy cards are `te` (default), `pok` and `base_game_codex1`**: what
   `ti4_server::card_set::OFFERED` has. `base_game` is not offered: the engine resolves its
   Diplomacy as the card of Codex I, so the text would not be the effect.
4. **The wait time** is one value for the table, for every seat that the server plays, of
   both kinds. The host sets it in the lobby and can change it while the game runs (phase 7).
5. **`/` is the choice between single player and multiplayer.** This replaces decision 5 of
   the online plan ("no mode picker for now"). See "The start screen".
6. **There is no "Ready", for every table.** The host starts the game when every seat is
   filled. `ready` goes from the server, and `web/` loses its Ready button.
7. **The seat order is always random, for every table.** It is drawn at Start from the seed of
   the game. No host moves a seat: `reorder` goes from the server, and `web/` loses its
   controls for it. The lobby lists the players in the order in which they came.
8. **Single player shares the setup of multiplayer**: the player count, the map, the strategy
   cards, the factions, the kinds of the other seats. It is the same screen and the same code
   (phase 6).
9. **Later, not in this plan** (see "Later"): the tables of this browser on the start screen,
   and the MLP bot.
10. **The lobby has a live connection** (a websocket), from the first phase. The page does not
    ask the server again and again. See "The connection of a table".
11. **There is one kind of table.** `web/` and web2 use the same lobby record and the same
    calls. The first version kept a field `setup` (`legacy` or `table`) so that the part of
    `web/` could be removed in one change; `web/` is developed further now, so nothing is
    split. What web2 adds (the socket, the settings) is there for `web/` too.
12. (Dropped. It was about a player who does not choose between two factions. A seat with no
    choice is random now, so nobody holds a table.)
13. **Saved games of single player stay as they are** (one for each address). A list of saved
    games is later.
14. **A visitor of a running game sees the table first** ("Watch", "Rejoin as …"), not the
    game at once.
15. **A table on the server can start from a scenario** ("Start from", as the Dev sheet of a
    local game has it), to test a screen of the late game with several people. It is in
    phase 2.
16. **The rule bot moves into a crate that the wasm host can use**, so that single player has
    it too (phase 5). Done: `crates/ti4-rule-bot`, used by `ti4-server` and `ti4-wasm`.
17. **The list of the maps scrolls**, for now. Six players have fourteen maps. The owner
    judges the list when it can be seen.

## How the lobby is built (asked by the owner, 2026-10-12)

- **The lobby looks like the game.** The game screen has the map at the left, a panel at the
  right, and one main button at the bottom of the panel. The lobby uses the same layout: the
  map of the table at the left, the players and the setup in the panel, "Start game" as the
  main button. It is not a form and it has no dialogs. On a phone it is the shell of the
  game too: one pane at a time with the tab bar, and the main button under every pane. See
  "The screen".
- **A change applies at once.** When the host sets the players to 5 or picks a map, or a
  player picks a faction, every player sees it at once. There is no "Apply" button. (In the
  game a choice is first marked and then sent with the main button; the setup of a table does
  not work like that, and `AGENTS.md` gets a line that says so.)

## Non-goals

- A list of all tables of the server, matchmaking, accounts, passwords for a table, chat.
- A spectator mode of its own. "Watch" opens the game with no seat, as today.
- The host history, the event log, restore, redo, the turn redo and the splice of `web/`
  (still postponed).
- Bans, a faction draft in turns, a slice draft (Milty), a choice between two drawn factions.
- Keleres, the Obsidian, the factions of Thunder's Edge, the set `base_game`.
- A map editor. The host chooses a map of the engine, or the random board.
- A list of saved games of single player (decision 13).
- The password screen, the Alpha notice and the feedback sheet of the public server (see
  "Open questions").

## What the merge changed

Read from the code after the merge. The tests were not run for this plan.

Built on the server, and used by `web/`:

- **Seats that the server plays.** `POST …/lobby/bots` `{ kind, count | fill, nickname,
  faction }`, for the host, with the kinds `random` and `smart` (the rule bot, named "Rule
  bot" in every text). `PlayerLobbyMember.bot` keeps the kind; `PlayerSlotView.bot` is public;
  `PlayerLobbyView.bot_kinds` lists what the server offers (`mlp` too when the bot service is
  configured). Such a seat is always "connected" and cannot be taken over. `remove-bot`
  removes it. `fill_seats: "random"` of `POST /api/games` adds the seats through the same
  function and starts.
- **The wait.** `RandomBotDecider::think` (`session/random_bot.rs`) waits before each answer
  of a seat of both kinds: in slices of 20 ms, with no lock held, with a jitter of 0.6 to 1.4,
  and the pick is drawn before the wait, so a game is the same with every wait. A replay never
  reaches it. The value is one for the process: `TI4_RANDOM_BOT_DELAY_MS`, 600 ms when it is
  not set, 30 s at most. `SessionConfig.random_bot_delay_ms` is there for a session, and the
  lobby does not set it.
- **Factions.** `seating::LOBBY_FACTIONS` (24), `seating::lobby_faction_draw(chosen, seed)`,
  `PlayerLobbyMember.faction`, `POST …/lobby/seat-faction { player_id, faction | null }`,
  `PlayerSlotView.faction` and `PlayerLobbyView.factions`. A faction that another seat has is
  refused (409). Every table gets its factions this way, also a table that web2 makes today:
  "seat 1 is Sol" is gone on the server. The map preview of a lobby seats the same factions
  (`maps::catalog::preview_seated`), by the place of the slot.
- **Strategy cards.** `strategy_card_set` of `POST /api/games`, kept in the lobby record and
  in `init.json`, shown in `PlayerLobbyView.strategy_card_set`. It is chosen when the table is
  made and nothing changes it. A table that names none plays `te`.
- **The speaker.** `speaker_order` of `POST /api/games`: `random` (default, from the seed) or
  `seat` (the first seat; fixtures and tests use it).
- **In `ti4-view`** (`map.rs`): `create_game_with_template_seated` and
  `create_game_with_map_seated` (the factions and the card set are given), `lobby_seat_factions`,
  `SpeakerOrder`. In `ti4-engine`: `start_game_seeded_with_card_set`, `seeded_speaker`. The
  wasm host uses none of them: a local game still has the factions by seat and the cards of
  the sources.
- **Maps.** 31 community maps beside the old ones (`maps/community_maps.json`): 6 for three
  players, 5 for four, 6 for five, 14 for six.
- **The public server** (`public/`, `TI4_PUBLIC=1`): a password with a cookie for `/api` and
  `/ws` (`/api/access`), limits of the rate for each address, 4 lobbies for each address, 200
  games, no dev scenarios, no `start_preset`, no MLP bot, a time after which a silent socket is
  closed (`ws_idle_timeout`). The server can serve one built page (`TI4_WEB_DIR`).
- **On the socket of a game**: the messages `reaction_intent_state` and `secondary_preview`.
  `wsTransport.ts` drops a message that it does not know, so they do no harm.

Not changed by the merge:

- A lobby has no socket. `web/` asks every two seconds.
- The player count is fixed when the table is made.
- `start` needs every member ready (`HumansNotReady`). `reorder` exists.
- A table has two phases, `lobby` and `running`.
- A running game does not know the nicknames.
- The server has no "Start from" of the scenarios of `ti4-view`.

What it means for web2 as it is today (phase 0): `?online` names no set and no seat order, so
its games now play the cards of Thunder's Edge with factions from the seed, a random speaker,
and seats that wait 600 ms. No file of `web2/src` names a card of `te`.

## Other findings that shape the plan

Server (`crates/ti4-server`):

- The lobby is HTTP (`http/games.rs`, `session/registry.rs`), with the header
  `x-ti4-player-session`. Under `/api/games/{id}/lobby`: `GET`, `join` (new, or `takeover`),
  `leave`, `heartbeat`, `ready`, `reorder`, `start`, `map`, `map-preview`, `bots`, `add-bot`
  (MLP), `seat-faction`, `remove-bot`. `GET /api/maps?player_count=N` and
  `GET /api/maps/{alias}/preview` are the map catalogue.
- The websocket (`/ws/games/{id}`) answers 404 while a table is a lobby.
- Every change of a lobby is written through one function (`save_player_lobby`), and
  `PlayerLobbyView.lobby_version` counts the changes. So one hook there can tell every
  connection of a table.
- Who is "connected" in a lobby comes from a heartbeat call and a grace time
  (`player_heartbeat`, `PRESENCE_GRACE`).
- `start` gives the players to the game in the order of the slots
  (`start_player_lobby`), and `seat_factions()` is in the same order.
- `ready` and `reorder` are used outside the server by `web/` (`Lobby.tsx`,
  `useLobbySession.ts`, `protocol/types.ts`, `protocol/decode.ts`, the e2e helpers, the smoke
  harness) and by `crates/ti4-bot-agent` (it calls `ready` after it joins).
- The rule bot is `session/smart_bot*.rs` (four files, 8 300 lines). Its entry is a function
  with no state of the server: `smart_bot::pick(choice, seen, rng)`. It uses
  `ti4_policy::fleet_strength` (178 lines, over `ti4-content` and `ti4-model` only). The
  random pick is `random_bot::pick_index` and `decision_rng`. Only `RandomBotDecider` (the
  wait, the pause of a history change) needs the session.
- The server has scenarios of its own (`dev/scenarios.rs`), which start a running game with no
  lobby. They are not the scenarios of a local game (`ti4-view/src/scenario.rs`).

web2:

- `dev/OnlineApp.tsx` is the whole entry of a game on the server, `dev/LocalApp.tsx` of a
  local game. `main.tsx` routes the addresses; `/` is the local game `DEFAULT_GAME`.
- **The player of a game is kept in `localStorage`** under `ti4.online.<game_id>.session`
  (`onlineKeys` in `wsTransport.ts`), and so are the draft, the plan and the staged movement.
  `localStorage` is shared by all tabs. So a second tab of the same game is the same player,
  and two seats in one browser cannot be played.
- The board is drawn from `BoardView` (`model/board.ts`, `game/board/GalaxyMap.tsx`), which is
  made from an update of a game. A preview is a list of `BoardTileView`.
- The frame of the game is `GameShell` over a `GameSession`. It is not checked yet how much of
  it (the header, the footer with the main button, the tab bar of a phone) is free of the
  session.
- A local game is `?local=<seed>&players=8&humans=<mask>` (`ti4_play(seed, players, mask)`),
  and it is saved as its seed and its answers (`SavedGame`, format 1).

## The start screen

`/` is the choice (decision 5): two rows, "Single player" and "Multiplayer", each the control,
with the keys 1 and 2.

- "Multiplayer" loads `?online`, which makes a table on the server and opens its lobby.
- "Single player" opens the same screen for a table in the page (phase 6). Until phase 6 it
  loads the local game of today.
- The header link of a game ("Local game" online, "Demo" in a local game stays) becomes
  "Start", to `/`. On a phone it is the last item of "Reference and more".
- What changes with it, and must be found before the change:
  - The installed app opens `/` (`public/manifest.webmanifest`, `start_url`): it opens the
    choice then.
  - `e2e/local.spec.ts`, `e2e/local.phone.spec.ts`, `e2e/entry.spec.ts` and the shots open the
    local game at `/`. They get its address (`?local=42&players=8&humans=1`, as
    `DEFAULT_GAME`).
  - The saved game of `/` is kept under the key of `DEFAULT_GAME`: check that the same game
    opens from its address, so nobody loses a saved game.
  - `AGENTS.md`: "Local play" ("the bare address `/` is that game") and "Online play" ("there
    is no mode picker") are rewritten.
- `?example=…`, `?gallery`, `?local=…` and `?scenario=…` are untouched. The quick start of a
  game on the server (eight seats, seven random, started at once) stays as
  `?online&fill=random`, for the row "Multiplayer" of the Dev sheet and the e2e.

## The screen

The lobby is one screen with the layout of the game: the board is the map that the table will
play, the list of the players is in the panel, and the panel has the setup with the main
button in the footer.

Desktop (1920×1080), eight seats, the two long names of the demo data:

```
┌ Lobby · 6 players ─────────────────────────── [Copy link] [Start] ┐
│                                        │ SETUP                    │
│                                        │ Players        − 6 +     │
│          the map of the table          │ Strategy cards   TE ▾    │
│     (six places of seats, not yet      │ Bots wait      − 1 s +   │
│          given to a player)            │──────────────────────────│
│                                        │ PLAYERS  3 of 6          │
│  ┌ maps for 6 players ──────────── ↻ ┐ │ ★ Ada (you)     Hacan ✎  │
│  │ ▸Standard                         │ │   Bartholomew…  Random   │
│  │  Hyperlanes                       │ │ ⚙ Rule bot 3    Sol   ×  │
│  │  Random                         ↕ │ │   Open   [Rule] [Random] │
│  └───────────────────────────────────┘ │──────────────────────────│
│                                        │ Waiting for 3 open seats │
│                                        │ [      Start game      ] │
└────────────────────────────────────────┴──────────────────────────┘
```

- **One screen, no dialog** (pattern 2). `web/` opens the map picker as a dialog, and the
  faction picker as a list over the page. Here the map is always in view, and so are the
  players and the setup.
- **A control is where the choice is made** (pattern 1). The list of the maps is on the board,
  under the map that it changes. The nickname and the faction are edited in the row of the
  viewer ("✎"). The kinds of a seat that the server plays are in the row of the open seat.
  "×" is in the row of the seat that it removes.
- **The same screen for everyone** (pattern 10). A player who is not the host sees the setup
  with its values and no controls. A visitor with no seat sees the same, with "Join" in the
  footer.
- **A state has one sign and a word on hover** (patterns 4 and 6), in a column of fixed width:
  "★" host, "⚙" the rule bot, "⚄" a random seat, "◇" not connected, an empty outline for an
  open seat. The list has no numbers and no seat colours: the seats are drawn at Start
  (decision 7).
- **The faction of a row** is a word in a column of fixed width: the name of the faction, or
  "Random". It is public (decision 1), so it is not hidden information.
- **Names** (pattern 13): a column of fixed width, cut with "…", the full name on hover; on a
  phone the name takes a second line.
- **Amounts change in steps of one** (pattern 8): the player count is a counter. Beside the
  open seats there is one button for the usual target: "Fill with rule bots".
- **The main button** is in the footer, and Enter is the main button: "Join" for a visitor,
  "Start game" for the host. A player who is not the host has no main button; the note says
  "Waiting for Ada to start". While Start is blocked, the note says why ("Waiting for 3 open
  seats"). Esc leaves the editor of the row.
- **A change of the host is told to the others once**, as a toast ("The host changed the
  map").
- No text that explains the lobby is visible. What the kinds of a seat and the wait time do is
  behind "?" (`Hint`).

Phone (412×923): the same blocks in the panes of the game shell, **Map · Players · Setup**,
with the footer under every pane, so the main button is always in reach. A pane may scroll
down. Nothing is cut at the side with eight seats and the long names.

### The choice of the faction

The row of the viewer opens in place into its editor, as the row of the viewer in the seat
list of a strategy card. It is the only row of a person that opens.

- The editor is one list: "Random" first, then the 24 factions in two groups, "Base game" and
  "Prophecy of Kings" (`LOBBY_BASE_FACTIONS` says where the second begins; the page gets the
  groups from the server or the wasm and writes no name of a faction). A row has the symbol
  and the name. The list scrolls inside the panel; it has too many rows for keys.
- A row is the control and the choice applies at once ("A change applies at once"). The
  chosen row has "▸".
- A faction that another seat has stays in its place, is dimmed and says who has it ("Ada").
  The server refuses it too (409); that answer is a toast.
- The sheet of a faction (the abilities, the leaders, the starting technologies, the starting
  fleet, the home system) is behind its name, as printed, from the catalogue. Rule text is
  not written in web2.
- **The host chooses for a seat that the server plays**: the row of such a seat opens for the
  host, into the same list. web2 gives the host no control over the faction of another person
  (pattern 11: a row of another player is not a control); the server allows it, and `web/`
  has it.
- After Start nothing can be changed. The game shows every faction.

## One table, two pages

`web/` and web2 use the same table (decision 11). So a change of what every table does is a
change of `web/` too, and it is made in one commit with the server.

- **Removed for every table** (decisions 6 and 7): the calls `ready` and `reorder`,
  `HumansNotReady`, `ready` of `PlayerLobbyMember` and of `PlayerSlotView`. A record on disk
  that still has `ready` must load (the field is read and dropped).
- **What goes in `web/` with it**: the Ready button and the controls that move a seat
  (`Lobby.tsx`), their calls (`useLobbySession.ts`), `ready` in `protocol/types.ts` and
  `protocol/decode.ts`, the e2e helpers (`lobbyHelpers.ts`, `lobby_lifecycle.spec.ts`) and the
  smoke harness, which mark every seat ready. `crates/ti4-bot-agent` calls `ready` after it
  joins: the call goes. The home of a seat in the map preview of `web/`
  (`MapPreviewBoard.tsx`: "Seat 2: Hacan") can no longer name a faction, see below.
- **Who changes `web/`** is open: Felix owns it. See "Open questions".
- **New code is in files of its own** where it can be: `session/table.rs` (the settings, the
  draw of the order, "Start from") and `ws/table.rs`. Both pages may use all of it.
- **The order of the seats in tests.** Many tests of the server and the start presets of the
  smoke harness expect that a player sits in the order of the join. `POST /api/games` gets
  `seat_order`: `random` (default) or `join`, the same way as `speaker_order` has `seat` for
  them. It is an argument of the call that makes the table, no page has a control for it, and
  "Start from" uses `join` (a case of a scenario is at a certain seat).

## One setup, two hosts

The setup of a game is decided in Rust, in `ti4-view`, and both hosts call it (decision 8, and
decision 6 of the online plan: one implementation). Most of the parts are there since the
merge; this section puts them behind one type.

- **`ti4-view/src/setup.rs`** (new, small):
  - `TableSetup { player_count, map, strategy_cards, seat_order, speaker_order, start_from,
    seed }`: what a host chooses.
  - `setup_options(player_count)`: the maps that build for the count, the sets of strategy
    cards with their names and the names of their cards, the factions in their groups, the
    kinds of a seat, the scenarios.
  - `preview(&TableSetup)`: the tiles of the board and the places of the seats, with no
    faction.
  - `draw(&TableSetup, seats)`: the seat order from the seed (a domain of its own in
    `GameRng`), then the factions with `lobby_faction_draw`. A chosen faction stays with its
    player.
  - `create(&TableSetup, drawn)`: calls `create_game_with_template_seated`, with the card set
    and the speaker.
- **The server** calls these from `start` and from the lobby calls. Its own code for the
  preview and the catalogue (`maps/catalog.rs`, `map.rs`) moves onto them.
- **The wasm host** gets exports for the same functions and a way to start a game from a
  setup (phase 5). `ti4_play(seed, players, mask)`, the scenarios and the fixtures keep the
  game of today (the factions by seat, the cards of the sources, the first seat as speaker),
  so no seeded game changes.
- **The page** has `Table` in `session/`, the seam of the setup as `Transport` is the seam of a
  game: the table as it is (the players, the setup, the phase), the preview, and the calls (set
  a value of the setup, add or remove a seat, choose a faction, start). `serverTable` talks to
  the server (a socket and the HTTP calls); `localTable` (phase 6) keeps the table in the page
  and asks the wasm. The screen and `select/lobby.ts` do not know which one they have, and
  `boundaries.test.ts` holds it.

## The connection of a table

A table has a websocket of its own from the moment it is made (decision 10):
`/ws/tables/<game_id>`. It is not the socket of a game. `web/` can use it in place of its
two-second call.

- The page sends `Subscribe` with its `player_session` (or none, as a visitor). The server
  answers with the table as that viewer may see it, and sends it again after every change:
  a player who came or left, a faction, a value of the setup, the phase. The message is the
  lobby view, whole; a table is small.
- A player is "connected" while a socket of that player is open. The heartbeat call is not
  used by web2.
- The calls of a table stay HTTP (`join`, `leave`, `settings`, `start`, …): they are there,
  they answer with the reason when they are refused, and the answer of a call and the push
  of the socket carry the same view with its `lobby_version`; the page keeps the newer one.
- When the phase is `running`, the page closes this socket and opens the socket of the game
  (`/ws/games/<game_id>`), as today. The sheet "Table" of a running game (phase 7) opens the
  socket of the table again while it is in view.
- Reconnect as the game does (`wsTransport.ts`: 0.5 s, then twice as long, up to 15 s), with
  the same one signal: "Offline · reconnecting" in the header. Take that code out into one
  helper that both sockets use.
- Server: `ws/table.rs`, and in the registry one channel for each table
  (`tokio::sync::watch` of the `lobby_version`), told from `save_player_lobby` and from the
  change of presence. `PROTOCOL_VERSION` is checked as on the game socket.
- **The public server.** The socket is under `/ws`, so the middleware `public::guard` must
  cover it as it covers the socket of a game (the password, the limits): a test in
  `tests/it/public_playtest.rs`. A lobby is silent for minutes, and `ws_idle_timeout` closes
  a silent socket: the page sends a ping in a shorter interval, as the game socket does.

## A seat for each tab

The address of a tab says which player the tab is: `?online=<game_id>&seat=<player_id>`. The
id of a player is public; the credential is not in the address.

- `localStorage` keeps every player that this browser has in a game:
  `ti4.online.<game_id>.players` is a map from the id of a player to its `player_session`.
- The draft, the plan, the staged movement and the draft of a secondary are kept for each
  player: `ti4.online.<game_id>.<player_id>.draft` and so on. Today they are kept for each
  game, so two seats of one browser would share a draft.
- A tab without `seat`: with one player of this browser in the game, the tab is that player and
  the address gets its `seat`; with none it is a visitor; with several the list marks them
  ("yours") and a click on such a row makes the tab that player.
- A row of an open seat has "Join in a new tab" for a player of the table: it joins as another
  player ("Ada 2") and opens the tab with that `seat`. This is the way to play several seats.
- A reload, a closed tab that is opened again and a bookmark are the same seat. A duplicated
  tab is the same player twice, which the server allows (two connections of one player).
- The key `ti4.online.<game_id>.session` of today is read once and moved to the map.
- The public server allows 4 lobbies for each address. Several seats of one table are one
  lobby, so the limit is not reached by this.

## Phases

Each phase leaves `?local=…`, `?online&fill=random` and `web/` working and green.

### Phase 0: web2 on the merged server (web2)

Before anything is built: the games of `?online` changed under web2 (the cards of `te`, the
factions from the seed, a random speaker, a wait of 600 ms).

1. Build the server and the wasm again (`cargo build -p ti4-server --bin server`,
   `scripts/build-wasm.sh`), then run `npm test` and the e2e. List what fails before fixing.
2. **The wait in tests.** `playwright.config.ts` and the server of `transportContract.test.ts`
   set `TI4_RANDOM_BOT_DELAY_MS=0`, or the suites wait 600 ms for each decision of seven
   seats.
3. **The cards of `te`.** Play a game of each of the three sets: the action picker, the seat
   list of a strategic action and the draft of a secondary must name `te4construction`,
   `te6warfare` and `base4` from the catalogue, not by an id. Check `session/select/strategy.ts`
   and `names.ts`.
4. **The 24 factions.** A game of eight on several seeds: the symbol and the colour of a seat,
   the name, the rows that only some factions have (faction technologies, the flagship, the
   mech). Play each faction once in a test of the selectors, so that no screen shows an id.
   The seat of the viewer is found by the player, never by the place (check
   `select/players.ts` and the mock data for "seat 1 is Sol"). The Ghosts of Creuss have their
   home off the map.
5. **The speaker** is not the first seat: check the first screen of a game (who the game waits
   for) and the tests that expect the page to draft first.
6. **The local game.** The wasm test of the merge has one answer less (the game no longer asks
   "negotiate, or continue" there). Check that a saved game of before still plays, or say in
   the page that it is of another engine (the page has that notice). Bring the line of
   `AGENTS.md` ("Local play") up to date.
7. Record the fixtures again with their commands where the facts changed.

### Phase 1: the start screen, the lobby, and its connection (web2, a small part of the server)

The server gets the socket of a table and nothing else. The lobby has what the server can do
today, which is more than before the merge.

1. **The start screen** ("The start screen"): `/` is the choice; "Single player" loads the
   local game of today. The tests that open the local game at `/` get its address.
2. **Routing.** `?online` makes a table (`POST /api/games` with no `fill_seats`; the player
   count is the last one that this browser used, else four, as `web/`) and goes to
   `?online=<game_id>&seat=<player_id>`. `?online=<game_id>` asks the table: the phase `lobby`
   shows the lobby, `running` mounts the game as today, 404 is "The server no longer knows
   this game."
3. **The socket of a table** on the server ("The connection of a table"): `ws/table.rs`, the
   channel of a table in the registry, presence from the open socket.
   `tests/it/table_socket.rs`: a second connection is told when a player joins, leaves, picks
   a faction and when the map changes; a visitor gets the view with no credential in it; a
   closed socket is "not connected" after the grace time; the public server asks for the
   password.
4. **`session/table.ts`**: the `Table` seam and `serverTable`: the socket for the table as it
   is, the HTTP calls that the server has for the changes. Of two views the one with the
   higher `lobby_version` is kept. A failed call is a toast with the reason; the lobby stays
   as it was. A lost socket is "Offline · reconnecting", and the page tries again by itself.
5. **`session/select/lobby.ts`** makes a `LobbyView` (in `model/`): the rows of the players
   with their signs and their faction, the setup rows with `editable`, the footer (the main
   button, the note). The components decide nothing from names (as `ScoreRowView.open`).
6. **The screen**: the list of the players, the setup block (the player count and the set of
   strategy cards are shown and cannot be changed in this phase), the footer, the header with
   "Copy link" and "Start". The map is the preview of `GET …/lobby/map-preview`, read-only,
   asked again when `map_revision` changes.
   - Check first how much of `GameShell` is free of a `GameSession` (the header, the footer,
     the tab bar of a phone). Take the frame out as a component that both use; do not copy it.
   - Check how `GalaxyMap` can draw a list of `BoardTileView` with no game. A `BoardView` with
     the tiles only, made in `select/lobby.ts`, is the aim; the camera and "Fit board" then
     work as in the game.
7. **Seats that the server plays.** The row of an open seat has one button for each kind of
   `bot_kinds` that is played in the process ("Rule bot", "Random"; `mlp` is not offered,
   see "Later"), and under the list "Fill with rule bots" (`POST …/lobby/bots`). Such a seat
   has its sign and "×" (`remove-bot`).
8. **A seat for each tab**, as the section above.
9. **The nickname**: kept in `localStorage` (`ti4.nickname`), sent with `POST /api/games` and
   `join`. In this phase it is edited before the join only; phase 2 adds the rename.
10. **No "Ready"** (decision 6). Until phase 2 the server asks for it, so the page marks its
    player ready when it joins and shows nothing of it.
11. **The start**: when the table says `running`, every tab mounts the game with its player.
    (In this phase the seats are in the order of the join.)
12. `AGENTS.md`: a section "Lobby" with the rules of "The screen", and the changed lines of
    "Online play" and "Local play".

Tests: `select/lobby.test.ts` (rows and signs for the host, a player, a visitor; the reasons of
a blocked start); `table.test.ts` over a socket of the test and a fake `fetch` (an older view
is dropped, a failed call, a lost socket); a test of the keys (two players of one browser do
not share a draft; the old key is moved); `e2e/lobby.spec.ts` against the real server: **one
browser context with three pages** makes a table of four from the start screen, joins from two
more tabs, adds a rule bot, starts, and each page is another seat of the running game; a tab
sees a player who joins with no reload and no wait; a reload of a page keeps its seat. A phone
case: nothing is cut at the side, and the main button is in view.

### Phase 2: no Ready, the random order, the settings of a table (Rust, `web/`, web2)

The first half changes every table and is one commit with `web/` ("One table, two pages").
After this phase a draft can be tested alone: one or two tabs, the other seats played by the
server, with time to draft.

Server, for every table:

1. **No "Ready"** (decision 6): `start` needs every seat filled and nothing else. The call
   `ready`, `HumansNotReady` and the field go. `web/`, the smoke harness and
   `ti4-bot-agent` are changed with it.
2. **The seat order is drawn at Start** (decision 7): the occupants of the slots are shuffled
   with a stream of the seed of the game (a domain of its own in `GameRng`, as
   `seating:lobby-faction-draw:v1`), then the factions are drawn for the new order; a chosen
   faction stays with its player. `reorder` goes. `seat_order: "join"` of `POST /api/games`
   keeps the order of the join for tests and presets.
3. **The preview names no faction.** A home of the preview is a place of a seat: nobody knows
   before Start who sits there. `SeatPreview.faction` and `faction_name` go, and the home tile
   is sent as a place. Read `maps/build.rs` (`placeholder_homes`, `preview_seated`): whether a
   template builds must not depend on which faction sits where, or the preview and the start
   must use the same rule (the Ghosts of Creuss change the board). `web/` shows the place
   with its number only.

Server, new:

4. **`POST …/lobby/settings`** (host, phase `lobby`), each field optional:
   - `player_count` (2 to 8). More seats are added open. Fewer seats: the open seats go first,
     then the seats that the server plays (with their chosen factions); a seat of a person is
     never removed (`409` with the names that are in the way). The map of a table is for one
     count: the choice becomes the default map of the new count, and `map_revision` goes up.
     The host's own choice of a random board stays.
   - `strategy_card_set` (one of `card_set::OFFERED`). Today it is set when the table is made
     only.
   - `bot_delay_ms` (0 to 30 000), see item 6.
   - `start_from`, see item 7.
   - `POST …/lobby/nickname` renames the player of the credential.
5. `PlayerLobbyView` gets `bot_delay_ms`, `seat_order` and `start_from`. The record on disk
   gets the same fields with a default, so a table of today loads.
6. **The wait of a table** (decision 4). The mechanism is there (`RandomBotDecider::think`).
   What is missing:
   - the value of the table: the lobby record keeps it, `start` gives it to
     `SessionConfig.random_bot_delay_ms`, and `init.json` keeps it, so a recovered game has
     it. A table with no value uses the value of the process, as today;
   - a value that can change while the session runs (phase 7 sets it): the decider reads it
     for each decision, not once when the session starts;
   - **what a seat waits for.** It waits before every answer today. The owner asked that a
     reaction window that the seat only declines does not wait. Read `answer` and `pick_index`:
     if the decider cannot tell such a decision from the others, measure a round of eight with
     a wait of one second before choosing a rule;
   - the update must say who the game waits for while a seat waits (`bot_deciding` of
     `SessionShared` is there; check what `TurnStatus` says then): the page says "Waiting for
     Rule bot 3", as for a person;
   - tests: the same game with a wait of 0 and of 50 ms; an undo while a seat waits is not
     held up (the pause of a history change is there, `pause_bots`).
7. **"Start from"** (decision 15). `start_from` of `lobby/settings`: none ("Round 1"), or the
   id of a scenario of `ti4-view` (`scenario::find`, `scenario::apply`), the same list that
   the Dev sheet of a local game has. `GET /api/setup-options` lists them with what each
   holds.
   - A scenario is a game of its own: it has its seed, its number of seats, its board and
     its factions. So the table takes the player count of the scenario, and the map, the
     strategy cards and the factions are not asked: their rows show the values of the
     scenario with no control.
   - The seats are not drawn: a player sits in the order of the join, the host first. Open
     seats are filled with seats that the server plays, as in every table.
   - Read how the wasm host starts a scenario (`ti4_play_scenario`) and use the same function
     of `ti4-view`; the server keeps the first state in `init.json` as for every game, so a
     recovered game is the scenario. The scenarios of `dev/scenarios.rs` are not touched.
   - It is refused on the public server, as `start_preset` is.
   - It is a tool for testing and it is not hidden: the row is in the setup of every host,
     and the lobby says "Start from: Late game" to every player.

web2:

8. The setup rows get their controls for the host: the counter of the players ("− 6 +"; "−" is
   blocked with the reason when a person is in the way), the list of the sets of strategy
   cards (the name; the cards that differ from the default behind hover; the names come from
   `GET /api/setup-options`, the page writes none), the counter of the wait in steps of half a
   second ("Off" at zero).
9. The setup row "Start from" is a list: "Round 1" and the scenarios, with what a scenario
   holds behind "?", as in the Dev sheet.
10. The nickname is edited in the row of the viewer at any time before Start.
11. The page no longer sends `ready`. `?online&fill=random` sends `fill_seats` as today, and
    can name `wait=<seconds>`.

Tests: `tests/it/lobby_settings.rs` (grow and shrink; a person in the way; the map follows the
count; the card set; only the host), `tests/it/seat_order.rs` (the order is the same for a
seed and another for another seed; a chosen faction stays with its player; `join` keeps the
order; a start with a seat that is not ready is no longer a thing), the tests of the wait in
`tests/it/random_bots.rs`, `tests/it/table_scenario.rs` (a table of two people and bots starts
the late game; each person has the seat of the join; a recovered game is the same; refused on
the public server); `player_lobby_admission.rs` and the other tests that call `ready` or
`reorder` are brought up to date; the fixture test, the lobby e2e and the smoke harness of
`web/` are green. web2: the selectors, and in `e2e/lobby.spec.ts` the case of the owner: two
tabs and four rule bots with a wait of one second, the secondary of a card is drafted in one
tab while the bots wait, and the toolbar says who the game waits for. A second case starts
"Late game" with two tabs and opens a sheet of the late game in each.

#### Progress of phase 2 (2026-10-13, not committed)

Phase 2 is built: the server, `web/` and web2. Nothing is committed: all of it is in the working
tree of `felix-merge-2026-10-12`, as one change ("One table, two pages"). What was run is under
"Gates"; what is left is under "Still open".

Done, server (`cargo c-web`, `cargo l-web`, `cargo fmt --all -- --check` clean; `cargo t-web`
green at the last whole run, the tests added after it were run by filter):

- Items 1 to 7 are built. New code is in `session/table.rs` (`SeatOrder`, `TableSettings`,
  `apply_settings`, `seated_players`, `setup_options`); the registry has
  `change_player_lobby_settings`, `rename_player` and `create_player_lobby_with_seating`.
- No "Ready": `ready` and `reorder` (calls, routes, registry functions, the fields of the
  member and of the slot view) are gone. A record on disk with `ready` loads
  (`StoredLobbyMember`). `start` answers `SeatsOpen` (409) while a seat is open.
- The seat order: `ti4_engine::seating::lobby_seat_order` (stream
  `seating:lobby-seat-order:v1`), drawn in `start_player_lobby`; the slots of the running table
  are then the seats. `seat_order` of `POST /api/games` and of the lobby view.
- The preview names no faction: a home is a tile `seat_N` with no planets
  (`maps/catalog.rs`). The test `a_preview_is_the_board_of_the_game_but_for_its_homes` holds
  with the Ghosts of Creuss at the table, on templates and on the random board. A chosen
  faction no longer moves `map_revision`.
- `POST …/lobby/settings`, `POST …/lobby/nickname`, `GET /api/setup-options`; the lobby view has
  `bot_delay_ms`, `seat_order`, `start_from`; `POST /api/games` takes `seat_order` and
  `bot_delay_ms`.
- The wait: a value of the table, kept in the lobby record and in `init.json`, read by the
  decider in every slice of a wait (`SessionShared::bot_delay_ms`,
  `GameSession::set_bot_delay_ms`), so a shorter value ends a running wait. A reaction window
  that the seat only declines does not wait (`is_reaction_window` and `is_decline`; no
  measurement was needed). While a seat waits, every subscriber gets a `turn_status` message
  (`broadcast_waiting_for`); a reaction window names no seat.
- "Start from": `ti4_view::scenario::start` is the one function for both hosts (the wasm host
  calls it now). The server keeps `start_from` in `init.json` and builds the map of the
  scenario again on recovery. Refused on the public server.
- Tests: `tests/it/lobby_settings.rs`, `seat_order.rs`, `table_scenario.rs`, `table_http.rs`
  (helpers), new cases in `random_bots.rs`, unit tests in `session/table.rs`,
  `session/random_bot.rs` and `maps/catalog.rs`. The tests that called `ready` or `reorder` are
  brought up to date; the reorder tests of `player_lobby_admission.rs` are removed.
- `crates/ti4-bot-agent`: the `ready` call is gone (it waits on the heartbeat); its unit tests
  pass. `tests/real_e2e.rs` is changed and was not run.

Decisions made while building, for the owner to confirm:

- `LobbyError::HumansNotReady` stays for the old seat lobby (`LobbyState`, not reachable over
  HTTP), which still has its own ready flag. The plan said that it goes.
- The registry helpers (`create_player_lobby`, …) draw the seats, as the HTTP default does. The
  test suites that index a seat by its place name `seat_order: "join"` when they make the
  table.
- A table with a start preset keeps the order of the join, as a table with a scenario does:
  a preset places its cases by seat.
- A scenario table plays the seed of the scenario (42), so the game is the one of a local
  game. Factions that were chosen before stay in the record and are not used; the view shows
  the factions of the scenario. Back to "Round 1" keeps the eight seats.
- `start_from: null` clears the scenario; a missing field leaves it.

Done, `web/`:

- The Ready button, the readiness text and the buttons that move a seat are out of `Lobby.tsx`;
  `setReady` and `reorder` are out of `useLobbySession.ts` and `App.tsx`; `ready` is out of
  `protocol/types.ts` and `protocol/decode.ts`. The map preview shows the number of a place only,
  with no faction and no "(you)" (`MapPreviewBoard.tsx`, `MapPicker.tsx`, `mapDecode.ts`,
  `mapPicker.ts`; `viewerSeat` and `viewerPosition` are gone).
- Its tests are brought up to date: `Lobby.test.tsx`, `App.test.tsx`, `LobbyMap.test.tsx`,
  `MapPicker.test.tsx`, `useLobbySession.test.tsx`, `useResumePolling.test.tsx`,
  `presentation/mapPicker.test.ts`, the fixture `dev/mapPickerFixtures.ts` (a home is a tile
  `seat_N` with no planets, as the server sends it).
- `e2e/lobbyHelpers.ts` no longer marks seats ready and makes its tables with
  `seat_order: "join"`. `e2e/lobby_lifecycle.spec.ts` has no Ready and no reorder, finds a
  player of a running table by its name (the seats are drawn), and compares the preview with
  the board without the home places.
- `e2e/screenshots/BG-nightly-web-bugs/capture-1-lobby-poll.ts` shows the failed action with
  Start on a full table (it used Ready). It was changed and not run. The other screenshot
  scenes still have `ready` in their mock lobbies; the decoder drops a field that it does not
  know, so they were left.

Done, web2 (items 8 to 11):

- `session/wire.ts`: no `ready`; `LobbyState` has `seat_order`, `bot_delay_ms`, `start_from`;
  `SetupOptions`; a place of the preview has no faction; `PublicTurnStatus` has
  `waiting_for_reactions`; the message `turn_status`.
- `session/table.ts`: `Table.setSettings` and `Table.rename`; the setup options are asked when
  the socket opens, until the server said them, and are a part of `TableState`; `createTable`
  takes the wait. The page sends no `ready`.
- `session/select/lobby.ts`: the setup rows with their controls (`LobbySetupControlView`: a
  counter with a reason for a step that cannot be taken, or a list), the names of the sets, the
  kinds of a seat and the scenarios from the setup options (`CARD_SETS` and `BOT_KINDS` are
  gone), `hostChange` (the toast for the players who are not the host).
- `game/lobby/LobbyScreen.tsx`: the counters, the lists (`Select` of `ui/`), "?" of a row, the
  name of the viewer edited in its row. `dev/OnlineApp.tsx` keeps the count of the host as the
  count of the next table. `dev/onlineStart.ts`: `&wait=<seconds>`.
- Who the game waits for: `wsTransport.ts` reads `turn_status` and tells it as the transport
  event `status`, which changes the status of the update and leaves what is staged
  (`useLiveSession.ts`); the toolbar says "Waiting for reactions" for a reaction window.
- Tests: `select/lobby.test.ts`, `table.test.ts`, `wsTransport.test.ts`, `liveOnline.test.tsx`,
  `onlineStart.test.ts`; in `e2e/lobby.spec.ts` three new cases (the host sets up the table and
  the other tab is told; a secondary is drafted in one tab while a rule bot waits; a table
  starts from "Late game" with two tabs), and the phone case uses the controls.
- `AGENTS.md` ("Lobby", "Online play", "Tests") and `NEXT.md`.

Decisions made while building the page, for the owner to confirm:

- **The case of the owner uses "Late game", not round 1.** Two tabs and six rule bots with a
  wait of one second: the host plays Leadership, and the other tab (the fourth seat) drafts its
  secondary while the first rule bot decides. From round 1 the strategy phase alone is longer
  than a test may be. The test ends when the draft is ready: a rule bot takes the wait for each
  of its answers (about six seconds for one secondary), so the seat is reached too late for a
  test of ten seconds.
- A list of the setup is the native `Select`, as the plan drew it ("TE ▾"). What a choice does
  is behind "?" of the row, for all choices of the row.
- A kind of seat is offered only when the server said its name: the page writes none.
- A rename is kept as the name of this browser (`ti4.nickname`).
- The row "Start from" is not shown where the server has no scenario (the public server).

Gates (2026-10-13):

- Rust: `cargo fmt --all -- --check`, `cargo l-web` and `cargo t-web` are clean (one whole run,
  with every test of this phase); `cargo t-web-slow` is green (41 tests); `cargo b-web` and
  `scripts/build-wasm.sh` were run before the suites of the pages.
- `crates/ti4-bot-agent`: `cargo check -p ti4-bot-agent --tests` and its unit tests pass.
  `tests/real_e2e.rs` compiles; it was not run (it needs the bot service).
- web2: `npm run typecheck`, `npm run lint`, `npm test` (327 tests) and the whole `npm run e2e`
  (50 tests, desktop and phone) are green. The four e2e tests that failed after the merge
  (`NEXT.md`, phase 0) pass. `npm run e2e:slow` and `npm run shots` were not run.
- `web/`: `npx tsc --noEmit` and `npm test` (1845 tests) are green; `e2e/lobby_lifecycle.spec.ts`
  is green; one run of the smoke harness (`npm run test:e2e:smoke`, 200 decisions) passed.
- The whole e2e of `web/` has 20 failing tests of 44, in the gallery, the combat, the movement
  and the research. The same 20 fail on the commit before this change (`9277a3eb`, built and
  run in a worktree of its own), so none comes from this phase. They were not looked at.
- `undo::a_seat_takes_back_its_action_and_not_the_decision_of_another_seat` (it failed once in
  a whole run on 2026-10-13): fifteen runs of `cargo t-web undo::` and two whole runs of
  `cargo t-web` passed. The cause of the one failure was not found.

Still open:

1. **The commit.** Phase 2 changes every table, so it is one commit with `web/`, and Felix
   knows of it before it is made ("Risks", open question 2). It is not made.
2. `web/` still lists "Position N" with seat colours in its lobby; after the draw a position
   of the lobby is not a seat. Felix decides what it shows.
3. The 20 failing e2e tests of `web/` (they are older than this phase).
4. A place of a seat on the board of the web2 lobby shows the id of its tile ("#seat_6").
5. Not in this phase and not built: a call that sets the wait of a running game and keeps it
   on disk (phase 7; `set_bot_delay_ms` is there for it).

### Phase 3: the choice of the faction (web2)

The server has all of it (`seat-faction`, `factions` of the view).

1. The editor of the row of the viewer, as "The choice of the faction": the list in two
   groups, "Random" first, a taken faction in its place with who has it.
2. The row of a seat that the server plays opens for the host, into the same list.
3. The sheet of a faction comes from the catalogue of the wasm (`Transport.catalog` has the
   abilities, the leaders and the technologies as printed); check what a faction sheet needs
   that the catalogue does not have (the starting fleet, the starting technologies, the home
   system, the commodities) and add it in `ti4-view`, from `ti4-content`.
4. The groups of the factions come from `GET /api/setup-options` (phase 2); until then from
   `PlayerLobbyView.factions`, as one list.

Tests: the selector of the editor (the viewer, the host over a bot, a taken faction, a player
who is not the host has no control over another row); `e2e/lobby.spec.ts`: two tabs pick two
factions, each tab sees the other's at once, a tab that picks a taken faction gets the toast,
a seat left at "Random" gets a faction that no other seat has, and each tab then plays the
faction that it chose. A shot with eight seats and the open editor at 1920×1080 with no
vertical scroll of the page, and on the phone.

#### Progress of phase 3 (2026-10-13)

Built, with its tests:

- Items 1 and 2: the faction of a row is the control that opens the choice under the row
  (`LobbyRowView.editor`, made in `select/lobby.ts`; `FactionEditor` in `LobbyScreen.tsx`).
  `Table.setFaction` is `POST …/lobby/seat-faction`.
- Item 3: `ti4_view::components::FactionSheet` has the leaders, the starting technologies (or
  the choice of them), the faction technologies, the starting fleet and the home planets, from
  `ti4-content`. `select/factionSheet.ts` makes the sheet; it is behind "?" of a row of the choice
  and behind the faction of a closed row.
- Item 4: the groups are `faction_groups` of the setup options; before the server said them,
  `factions` of the view as one list.
- Server: the answer for a taken faction names the faction as a table does ("Hacan is already
  taken by Ada"; it had the id).
- `Peek` of `ui/` has `tall`, for a text that can be higher than the window.

Decisions made while building, for the owner to confirm:

- The control is the faction of the row with "▾", as the lists of the setup, not a second "✎".
- The choice stays open after a choice (the chosen row has "▸"); Esc or the control closes it.
- Three columns on desktop, so the list and eight seats fit the panel with no scroll; two on a
  phone.
- A row of the choice has no symbol: web2 has no faction icons.
- The names of the list are those of the catalogue ("Hacan"), as the rows; the setup options
  have the printed names ("The Emirates of Hacan").
- The e2e gets the refusal of the server by holding the socket of the table back from one tab
  (`routeWebSocket`): with a live socket a taken faction is never offered.

Found and not changed: the engine gives no starting technology to a faction that chooses
(`NEXT.md`).

### Phase 4: the map (web2)

**Postponed until after phase 6** (owner, 2026-10-13). Until then every table, also a table of
single player, plays the default map of its player count (what `POST /api/games` and
`ti4_view::map` choose when no map is named), and the Map row of the setup shows its name with
no control. Phase 4 then puts one list on the board of both `serverTable` and `localTable`, so
the `Table` seam needs `maps(player_count)` and `setMap` from phase 6 on (the server has
`GET /api/maps` and `…/lobby/map`; the wasm host has the catalogue of `ti4-view/src/maps`).

1. The list of the maps on the board, for the host: one row for each map of
   `GET /api/maps?player_count=N` and "Random", with the name, the author and what
   `MapChoiceView` says (the number of systems, hyperlanes, "recommended"). The chosen one has
   "▸". A click saves it (`POST …/lobby/map`) and the board shows it. "↻" draws the open
   places again (the same call: each one draws a new seed). Read
   `web/src/presentation/mapPicker.ts` for the order and the words; do not import it.
2. **The list scrolls** (decision 17): it has a fixed height on the board and scrolls inside,
   the recommended maps first. This is an exception to pattern 9 until the owner has seen it
   with the fourteen maps of six players; groups ("Official", "Community") are the next thing
   to try.
3. A player who is not the host has no list: the name of the map is in the heading of the
   board. The map of the lobby is asked again when `map_revision` changes, and the change is a
   toast.
4. The board marks the home places of the seats. Before Start no player is on a place.
5. Looking at another map before choosing it (`GET /api/maps/{alias}/preview`) is not built:
   a choice applies at once and can be changed until Start, so the board is the preview.
6. The start preset of `web/` (dev, `combat`) is `&preset=` of the address that makes the
   table. No control. The public server refuses it.

Tests: the selector of the list (host and not host); `e2e/lobby.spec.ts`: the host chooses a
map, the other tab shows it and the toast, the started game has that map; a shot of the lobby
with eight seats and the long names at 1920×1080 with no vertical scroll of the page
(`e2e/shots.spec.ts`), a shot with six seats and the long list, and both on the phone.

### Phase 5: the setup in one place, and the bots in a crate of their own (Rust)

1. **`ti4-view/src/setup.rs`**, as "One setup, two hosts". The server moves `start`, the
   preview, the catalogue and `GET /api/setup-options` onto it; the draw of the order of
   phase 2 moves into `draw`. Guard: every fixture of web2 and of the server is the same byte
   for byte, and a table of a seed starts the same game as before the move.
2. **A crate for the seats that are played by code** (decision 16), `crates/ti4-bots`, over
   `ti4-model`, `ti4-content` and `ti4-engine`:
   - the rule bot (`smart_bot.rs`, `smart_bot_agents.rs`, `smart_bot_faction_plans.rs`,
     `smart_bot_objectives.rs`) and the random pick (`pick_index`, `weight`, `decision_rng`)
     move there, with their tests. The move changes no line of what a bot decides: the survey
     tests of the server (`smart_bots.rs`, `random_bots.rs`) are the guard;
   - `fleet_strength` is used from `ti4-policy` today. `ti4-policy` is not one of the web
     crates. Check that it builds for `wasm32`; if not, or if it is heavy, move the module
     into the new crate and let `ti4-policy` use it from there;
   - `RandomBotDecider` stays in the server: the wait, the pause of a history change and the
     counter are of a session;
   - the crate joins the aliases `c-web`, `l-web` and `t-web` (`.cargo/config.toml`) and the
     list of `AGENTS.md` ("Rust builds");
   - **Felix changes these files every day.** A move of 8 300 lines breaks every open branch
     of his. It is one commit with nothing else in it, made at a moment that he names.
3. **The wasm host** exports the functions of `setup.rs` (`ti4_setup_options`,
   `ti4_setup_preview`, `ti4_setup_draw`), starts a game from a setup (`ti4_play_setup`), and
   lets a seat that is not played in the page be played by the rule bot or by the random pick.
   `ti4_play(seed, players, mask)` stays as it is.
   - The same game on both hosts: a table of a seed, with the same seats played by the same
     kinds, is the same game in the wasm and on the server. A test plays one round of both.
   - Measure the size of `ti4.wasm` and the time of a decision of the rule bot in the browser
     before and after (`crates/ti4-wasm/README.md` has the numbers of today).

Tests: `ti4-view` (the same seed gives the same table; a chosen faction is kept; every seat has
another faction); `ti4-bots` (the tests that move); the test of both hosts.

#### Progress of phase 5 (2026-10-13)

Item 2 is built, in another shape than written (commit `f8213cef`):

- The crate is `crates/ti4-rule-bot`, not `ti4-bots`: `agents.rs`, `faction_plans.rs`,
  `objectives.rs`, `lib.rs` (the rule bot, `choose`) and `random.rs` (the random pick). Its
  79 unit tests moved with it. It is over `ti4-model`, `ti4-content` and `ti4-engine`, with no
  default features of `rand`, so it links into the wasm build.
- `fleet_strength` and `valuation` moved from `ti4-policy` into `ti4-engine`; `ti4-policy`
  re-exports both (it cannot enter the wasm build, so no new crate was needed for them).
- `RandomBotDecider` stays in the server, which re-exports the crate as
  `session::smart_bot` and `session::smart_bot_faction_plans`.
- The wasm host has `ti4_set_rule_seats(mask)` (README, "Rule seats"): the seats of the mask are
  played by the rule bot, decided from the seed, the seat and the number of the decision, as on
  the server; the other seats that the host does not play decide at random, as before. The
  mask stays until it is set again, so it is set before `ti4_play`. `ti4-wasm` has a test that
  a game with rule seats is the same twice, differs from a random one, and ends without a host.
- Raw size of `ti4.wasm`: 5.68 MB, +0.21 MB with the rule bot.

**The rest of phase 5 is built (2026-10-13, later the same day):**

1. **`ti4-view/src/setup.rs`** (item 1). `TableSetup` (the count, the map, the cards, the seat
   order, the speaker order, `start_from`, the seed) with `check`, `scenario` and `game_seed`;
   `setup_options(host, default_bot_delay_ms)`; `preview`; `seat_places`; `draw` (the order
   from the seed, then the factions for that order; a scenario keeps the order of the join);
   `create` (the seated template game with the card set and the speaker, or the scenario). The
   sets of strategy cards (`setup::card_set`), the kinds of a seat (`setup::bot_kind`),
   `SeatOrder`, `PLAYER_COUNTS` and `MAX_BOT_DELAY_MS` are there too.
   - The catalogue of the maps moved with it: `ti4-server/src/maps/catalog.rs` is now
     `ti4-view/src/maps/catalog.rs` (`MapChoice`, `MapPreview`, `preview`, `scenario_preview`,
     `catalog`, `describe`, `recommended_for`), with its tests. The server keeps `variant_seed`
     (the secret of the process) and re-exports the rest, so no caller changed.
   - The server is on it: `session/table.rs` has `setup_of(record)` and `seat_order_of(record)`;
     `start_player_lobby` calls `setup::draw` and `setup::create`; the preview of a lobby is
     `setup::preview`; `GET /api/setup-options` is `setup::setup_options("The server", …)`, with
     the scenarios taken out on the public server; `card_set` re-exports the sets and keeps
     `LEGACY` and `resolve`.
   - **A start preset stays in the server** (`crate::preset`, 2 500 lines of a tool for
     `web/`): a table with a preset still starts through `create_game_with_factions`. It is not
     a field of `TableSetup`.
   - Guard: `cargo t-web` was green after the move with no test changed but one name
     (`SeatOrder::of` is `seat_order_of`): the fixtures (`fixtures_verification`), `seat_order`,
     `lobby_all_factions`, `table_scenario`, `strategy_card_set`, `random_seats`. A test of
     `setup.rs` compares, field by field as JSON, the state of `create` with the state of the
     functions that the server called before.
2. **The exports** (item 3): `ti4_setup_options(players)` (the options, and `maps` for the
   count), `ti4_setup_preview`, `ti4_setup_draw`, `ti4_play_setup`. The request is
   `{ setup, seats }`; a seat has `kind` (`human`, `smart`, `random`) and `faction`. The player
   who came as number i has the id of the i-th letter, wherever it is seated. `ti4_play` is as
   it was. The README has the section "A table".
   - In the game of a table a random seat decides as on the server (seed, seat, number of the
     decision), not from the shared stream of `ti4_play`: without this the two hosts would not
     play the same game.
3. **The test of both hosts**: `ti4-server/tests/it/both_hosts.rs`
   (`slow_a_table_of_a_seed_is_the_same_game_in_the_page_and_on_the_server`): two tables (four
   rule seats; five seats with rule and random seats, two chosen factions and the `pok` cards)
   play a round in the wasm host and on the server, and every decision is the same. It takes
   11 s in a debug build, so it is in the slow tier; the server has `ti4-wasm` as a
   dev-dependency for it. It has no fast twin: the wasm host stops at a round, not at a
   decision.
4. **Measurements** (README, "Rule seats"): `ti4.wasm` is 5.78 MB raw, 1.47 MB with gzip and
   1.03 MB with brotli. In headless Chromium on two cores a table of six rule seats takes 6 to
   9 ms for a decision with the engine in the number, six random seats 3 to 4 ms. So the rule
   bot is no risk for single player, and the page needs no worker for it.
5. **The aliases**: `ti4-rule-bot` is in `c-web`, `l-web`, `t-web` and `b-web`, and in the list
   of `web2/AGENTS.md` (seven crates). Its tests run with `cargo t-web` now.
6. **web2** (`session/wasmTransport.ts`): `LocalPlay.ruleSeats` (the mask of
   `ti4_set_rule_seats`) and `LocalPlay.table` (the game is started with `ti4_play_setup`);
   `setupOptionsOf`, `setupPreviewOf` and `setupDrawOf` ask the engine with no game. The types
   are in `wire.ts` (`TableSetup`, `TableSeat`, `TableRequest`, `TableOptions`, `TablePreview`,
   `TableDraw`). Two tests in `wasmTransport.test.ts`. No screen uses them yet: that is
   `localTable` of phase 6.

Dropped from the first wording: "move `fleet_strength` into the new crate" (it went to
`ti4-engine`), and "Felix names the moment" (done; the move was one commit with nothing else).

**State of the checks (2026-10-13, stopped by the owner before the last run ended). Nothing
is committed: all of it is in the working tree of `dev`.**

Run and green:

- `cargo fmt --all -- --check` and `cargo l-web` (Clippy, with `ti4-rule-bot` in the set), after
  the last change of Rust.
- `cargo t-web`, whole, once: after the move of the setup and with the exports, before
  `both_hosts.rs`, the dev-dependency on `ti4-wasm` and the new alias were there (2974 engine,
  170 + 340 server, 108 view, 5 wasm).
- The two slow tests by filter, in a debug build: the host test of `ti4-wasm` with its new part
  for a table (72 s) and `both_hosts` (11 s).
- `scripts/build-wasm.sh` (release) and the measurements; `npm run typecheck` of web2.

Not run, or not green:

- **The last whole `cargo t-web` did not end.** The run before it (one job, after the target
  directory was cleared) got as far as the integration tests of the server: 339 passed and
  `undo::a_seat_takes_back_its_action_and_not_the_decision_of_another_seat` failed. It is the
  test that "Gates" of phase 2 names as failing once; `cargo t-web undo::` then passed three
  times. That run stopped there, so the tests of `ti4-view` and `ti4-wasm` were not run after
  the last changes. The next whole run sat with no CPU for more than ten minutes and was
  stopped; its output was not kept, so **where it hung is not known**. Start there:
  `cargo t-web 2>&1 | tee /tmp/t-web.log`, and look at the last "Running" line. Suspects, in
  order: the same undo test (it waits on a session), and the test binary of the server, which
  links `ti4-wasm` now (its statics are only touched by the slow test, but this was not
  checked).
- An earlier hang of this work was a deadlock in a test of mine (`ti4-wasm`, the host test: a
  table of six with five seats named, so the sixth was a person and the native host took a lock
  that the test held). It is fixed, and the test passed after it.
- `cargo t-web-slow` (the whole slow tier) was not run.
- `npm test` of web2 was not run with the new `ti4.wasm`: the two new cases of
  `wasmTransport.test.ts` (the rule seats of a mask; a table from its options to its game) have
  never run. `npm run lint`, the e2e and `cargo b-web` were not run either. The e2e needs the
  server built again (`cargo b-web`): its setup code moved.
- The machine: 3 GB and two cores. A cold build of the engine with its tests must be one job
  (`-j1`); two compilers at once were killed. 13 GB of old artifacts of the workspace crates were
  removed from `target/debug` (`deps`, `incremental`) because the disk was full.

Left for phase 6, found while building:

- `LocalGame` and `SavedGame` do not keep the table or the rule seats yet (format 2, item 3 of
  phase 6). Until then a caller of `LocalPlay.table` must give the same table again for a
  reload, and `game.humans` must be the mask of its `human` players (the transport reads it for
  "Play as").
- The wait of a table is not in the wasm host: `default_bot_delay_ms` of its options is 0. The
  page waits (item 1 of phase 6).
- A table of two seats passes `TableSetup::check` when a map builds for it; `ti4_play` still
  asks for three or more.

### Phase 6: single player uses the same setup (web2, a small part of the wasm)

1. **`localTable`** in `session/`: a `Table` that lives in the page. Its players are "You" and
   seats that the page plays (the rule bot, or random; `ti4_set_rule_seats`); the host is the
   page. It has the player count, the strategy cards, the factions, the wait time (the page waits
   before it lets a seat decide; check against "Other seats: Step" of the Dev sheet, which is
   the same idea). The map is the default of the count until phase 4 (see there). It has no
   link to copy, no nickname to edit and nobody to wait for.
2. **"Single player" of the start screen** opens the lobby screen over `localTable`, when
   this browser has no saved game of single player; with one it opens that game, as today
   (decision 13). "New game" of the Dev sheet opens the setup.
3. **The saved game** gets the setup: `SavedGame` format 2 has the setup beside the seed and
   the answers; format 1 is read as the game of today. The address of such a game names the
   save, not the setup (a setup does not fit an address). Export and Import keep working.
4. "Play as" and several seats in the page (the mask) stay tools of the Dev sheet.
5. The row "Multiplayer" of the Dev sheet stays the quick start.

Tests: the contract of `Table` runs against `serverTable` (a real server) and `localTable` (the
wasm), as `transportContract.test.ts` does for a game: the options, a change of the count, the
preview, a faction, a bot, the start. `savedGame.test.ts` (format 1 and 2).
`e2e/local.spec.ts`: from the start screen, a game of four on another map and the `pok` set
with three rule bots, a faction is chosen, the game is played for a turn, a reload continues
it.

#### Progress of phase 6 (2026-10-13)

First the checks that phase 5 left open, on a machine of 16 cores and 32 GB:

- `cargo t-web` ran whole and green, with no hang (2974 engine, 170 + 340 server, 108 view, 79
  rule bot, 5 wasm). The hang of the small machine did not come back and its cause is not known;
  the undo test passed in every run here.
- `cargo t-web-slow` is green (42 tests), with `both_hosts`.
- The two cases of `wasmTransport.test.ts` that had never run both failed, and both were wrong
  tests, not wrong code: the questions of seat a alone are the same with rule seats and with
  random seats (the test compares the view of the players now), and a place of a seat has no
  `planets` field (it was expected as an empty list).

Built, with its tests:

1. **`localTable`** (item 1, `session/localTable.ts`): a `Table` in the page over the setup
   exports. Every change is checked by the engine before it applies; a refusal is a `rejected`
   event with the reason. `TableState.local` marks it. The `Table` seam has `maps(players)` and
   `setMap(map)` now, on both tables (phase 4 needs them; no screen uses them).
2. **The wait** (item 1). `ti4_set_stepping(2)` of the wasm host: the host is asked before each
   decision of a seat that it does not play, but not before a reaction window that the seat
   only declines (the pick is read first, and the stream of the random seats does not move).
   The transport waits there (`LocalPlay.waitMs`, with the jitter of the server, 0.6 to 1.4)
   and shows the game. In one round of four rule seats 284 of 301 decisions wait.
3. **`?single`** (item 2, `dev/SingleApp.tsx`): the setup over `localTable`, or the game that
   the browser kept. "Single player" of the start screen goes there; a browser that has only
   the game of `DEFAULT_GAME` gets that game. "New game" of the Dev sheet of such a game opens
   the setup.
4. **`SavedGame` format 2** (item 3): `table` and `waitMs` beside the seed and the answers,
   under the one key `ti4.single`. Format 1 is read. A page writes format 2 for every game.
5. Items 4 and 5 are as they were. "Play as" says "Played by the page" for the other seats of a
   table.
6. `ti4_setup_draw` names the factions of a scenario (the factions of its seats), so the setup
   shows them before the start, as the lobby of the server does.

Tests: `tableContract.test.ts` (six cases on both tables, three more for the table in the page),
`savedGame.test.ts` (format 1 and 2), a case of the wait in `wasmTransport.test.ts`, the view of
a table in the page in `select/lobby.test.ts`, the addresses in `dev/localGame.test.ts`;
`e2e/local.spec.ts` (the setup, the start, a reload, "New game"; the wait), `e2e/entry.spec.ts`
(the game of before the setup), `e2e/local.phone.spec.ts` (eight seats and the long name). The
host test of `ti4-wasm` plays the same round with every mode of the stepping.

Decisions made while building, for the owner to confirm:

- The address is `?single`, and there is one game of single player: the save is the game.
  `?local=…` and `?scenario=…` stay as they are, with "Demo" as their way out and "New game" as
  the same game again; they are the addresses of the tests.
- A table of single player begins full: a rule bot in every other seat, and a rule bot for a
  seat that the count adds. The server adds open seats, because a person may come.
- A new table has six seats when the browser chose none before (multiplayer has four), and
  both keep the count under the same key.
- The player of the table has the name of the start screen; the row has "(you)" and no "✎".
- The e2e of the plan asked for "another map": phase 4 is postponed, so the test has the usual
  map, and the choice of a map is tested on the seam only (`tableContract.test.ts`).
- A save of single player that cannot be read is treated as none: the setup opens, and the
  start of a new game writes over it.

Still open:

- The wait cannot be changed while the game runs (phase 7 has the control, for both tables).
- The names of the rule seats ("Rule bot 2") are not in the game: a seat is named by its
  faction there, as online (phase 7).
- The contract suite does not start a scenario on the server (the server of the suite is not
  the public one, so it could).
- `LocalApp` and the setup each load the engine: the file is fetched once, and compiled twice.

Gates (2026-10-13): `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`,
`cargo t-web-slow`; `scripts/build-wasm.sh`, `cargo b-web`; `npm run typecheck`, `npm run lint`,
`npm test` (362 tests), `npm run e2e` (56 tests, desktop and phone). `npm run e2e:slow` and
`npm run shots` were not run.

### Phase 7: the table in the game (Rust + web2)

1. **Nicknames in the game.** The snapshot and the update get the seats of the table as a
   fact: the nickname and who plays the seat (a person, the rule bot, random), per player id.
   It is public. The page names a seat by its nickname everywhere it says "Seat PLAYER_1"
   today (the player table, the toolbar status, the seat list of a strategic action, the
   sheets), in the column of fixed width; a seat that the server plays has its sign.
2. **The sheet "Table"** in "Reference and more": the lobby screen, read-only (pattern 10),
   for the running game: who sits where, who is connected, the map by its name, the set of
   strategy cards. The question that it answers: "who is at this table, and how was it set
   up?" For the host it has the counter of the wait
   (`POST /api/games/{id}/settings`, host only, for a running game).
3. **A visitor of a running game** (`?online=<game_id>` with no player of this browser) gets
   the same screen in place of the game, with "Watch" as the main button and "Rejoin as …" in
   the row of a seat that can be taken over (`can_take_over`, `join` of kind `takeover`).
   Today such a visitor is a spectator at once (decision 14).
4. Update `NEXT.md` and `TODOS.md` (the line "Lobby, map picker with preview, nicknames, seats
   and bots"), and the list of what `web/` has and web2 does not (the online plan,
   "Postponed").

Tests: the redaction test gets the new fact (it holds no credential); a selector test of the
names; `e2e/lobby.spec.ts`: the names of three tabs are in the player table of each; the host
changes the wait in the running game; a tab that is closed is taken over from another tab
after the grace time (the server is started with a short one for the e2e, if it can be set).

#### Progress of phase 7 (2026-10-14)

Built before phase 4, which the owner skipped for now. Items 1 to 4 are built, with their tests.

1. **Nicknames in the game.** The fact is `seats` of the snapshot and of the update: for each
   player id the nickname and who plays the seat (`ti4_view::setup::SeatFact`, kind `human`,
   `smart` or `random`). It is public and the same for every viewer.
   - Server: the session has the names of its table (`SessionConfig.nicknames`), from the lobby
     record at the start and at recovery; a seat that is taken over has its new name
     (`GameSession::set_nicknames`), and a worker that an undo starts keeps the names. A session
     with no table says no `seats`.
   - Wasm host: a player of the table of the page has a name (`SeatRequest.name`,
     `TableSeat.name`), and the updates of its game say them. A table of an older page has no
     names, and its game says none.
   - Page: `SeatView.name` is the nickname (`select/players.ts`), so the player table, the sheet
     of a player and the Dev sheet have it; a seat that no person plays has its sign there
     (`SeatView.played`, "⚙" and "⚄" as in the lobby). The status of the toolbar ("Waiting for
     Ada"), the list of the seats of a strategic action and "… decides next" name the player too
     (`seatLabel` in `select/names.ts`). A draft is played on a copy of the game: its screen has
     the seats of the game.
2. **The sheet "Table"** is the last reference sheet, in a game that has a table. It is the
   lobby with nothing open (`TableSheet` in `game/lobby/LobbyScreen.tsx`, over the same
   `LobbyView`): the seats in their order, each with the symbol of its seat, its sign, its
   player and its faction, then the rows of the setup. `select/lobby.ts` gives a running table
   no control but the wait of the host. The sheet reads the table while it is in view
   (`dev/GameTable.tsx`): the socket of the table online, `localGameTable` in single player.
   - `POST /api/games/{id}/settings` (`{ "bot_delay_ms": … }`, host only, running only): the
     session is told, and the lobby record keeps the value, so a recovered game has it.
     `Table.setSettings` sends it for a running table.
   - Single player: the transport has the capability `wait`, and the saved game keeps the value.
   - The lobby view of a running table has the faction of every seat, also of a seat that chose
     none (`GameSession::factions`).
3. **A visitor of a running game** is shown the lobby screen of the table, with its board,
   "Watch" as the main button and "Rejoin" in the row of a seat that can be taken over
   (`Table.takeOver`, `join` of kind `takeover`, under the name of the seat). The browser keeps
   the player and the page is loaded as that player. A visitor who watches has the sheet
   "Table" with the same button.
4. `NEXT.md`, `TODOS.md`, `AGENTS.md` and the online plan ("Postponed") are brought up to date.

Tests: `crates/ti4-server/tests/it/table_in_game.rs` (the fact holds no credential and is the
same for a spectator; the factions of the running view; the wait: only the host, its range, kept
through a recovery; the name after a take-over); `select/shell.test.ts` (the names),
`select/lobby.test.ts` (the running table for the host, a player and a visitor),
`table.test.ts` (the take-over, the wait of a running game, the board of a running table),
`wasmTransport.test.ts` (the names of a table of the page, a wait that changes while the game
runs), `LobbyScreen.test.tsx` (the sheet); `e2e/lobby.spec.ts` (the names of three tabs in the
player table of each; the sheet and the wait in two tabs; a visitor: the table, "Rejoin",
"Watch"), `e2e/lobby.phone.spec.ts` (the same on the phone, with eight seats and the long names).

Decisions made while building, for the owner to confirm:

- **The button is "Rejoin", not "Rejoin as Ada".** It is in the row of the seat, which has the
  name; a name of 32 letters made a button that a phone has no room for. The name is in its
  tooltip.
- The seat is taken over under the name that it has. A visitor is asked no name.
- The texts that state the game keep the faction (the trigger of a reaction window, the
  exploration, the Objectives sheet). The item of the plan names the player table, the status of
  the toolbar, the list of a strategic action and the sheets; these have the name of the player.
- The sheet has the players first and the setup second: its question begins with "who".
- A long name in the status of the toolbar pushed the menu of a phone out of view: the status
  has the room that is left now, and is cut.
- The call of the running game refuses a table that is still a lobby (409, `NotRunning`): the
  lobby has its own call.
- The e2e does not play a take-over against the grace time (see `NEXT.md`): the server of the
  e2e cannot have a short one, because the page pings every ten seconds.

Still open: see `NEXT.md`, "the table in the game".

## Later

Not in this plan (decision 9).

- **The tables of this browser on the start screen**: one row for each game that
  `localStorage` has a player for and the server still knows, so a closed tab finds its table
  again. The map `ti4.online.<game_id>.players` of phase 1 is what it reads.
- **The MLP bot.** `web/` adds it through the bot service of the server (`add-bot`, a process
  of its own, the advisor, libtorch; not on the public server). web2 does not offer it.
- A list of the saved games of single player.
- **A choice between two drawn factions**, as the first version of this plan had it, as a
  setting of a table.
- **More of the setup** (ideas, owner 2026-10-12: nice to have). Each is one more row of the
  setup block and one more field of `TableSetup`; none is checked against the engine yet.
  - The points that win (10, or 14 for a long game).
  - The speaker (`speaker_order` is there: `random` or the first seat).
  - The content of the game: with or without Thunder's Edge and the codices. The engine has
    the source sets `BASE`, `POK` and `FULL`; every host starts a game with `POK` today.
  - The factions that can be drawn (leave some out), Keleres.
  - House rules of the seats: whether the undo of a seat is allowed, whether a visitor may
    watch.

## Checks that every phase must pass

- Rust (phases 1, 2, 5, 7): `cargo c-web`, `cargo t-web`, `cargo fmt --all -- --check`,
  `cargo l-web`; server tests with `cargo test -p ti4-server --test it <file>::`. Do not build
  other crates, except `ti4-bot-agent` in phase 2 (it calls `ready`) and `ti4-policy` in
  phase 5 (`fleet_strength`).
- `PROTOCOL_VERSION`: a phase adds optional fields, calls and a socket. Phase 2 removes
  `ready` from the lobby view and two calls; the lobby view is HTTP and `web/` is changed in
  the same commit. Check `web/src/protocol/decode.ts` for a decoder that refuses a field that
  it does not know or misses.
- The records on disk (`PlayerLobbyRecord`, `PlayerGameInitRecord`) refuse unknown fields: a
  new field has a default, a removed field is still read, and a table of today loads.
- `web/` stays green: its unit tests, its lobby e2e and one run of the smoke harness after
  phase 2.
- Rebuild `session/ti4.wasm` (`scripts/build-wasm.sh`) in phase 0 and after phases 5 and 6.
- web2: `npm test`, `boundaries.test.ts` (the calls of a table are in `session/`; `game/` and
  `ui/` do not know the server), the e2e on desktop and phone. A changed screen is checked with
  eight seats and the two long names. Format only the files that changed (`npx oxfmt <files>`).
- The owner looks at the lobby on the phone after each phase.

## Risks

- **A change of every table breaks `web/` and Felix's branches.** Phase 2 (no Ready, the
  random order, the preview with no faction) and phase 5 (the move of the bots) touch code
  that he works on. Each is one commit that he knows of before it is made.
- **Hidden information before the game.** The seed of a table never leaves the server: it
  tells the seat order and the faction of every seat that is left at "Random". The seed is in
  the address only for development (`&seed=`), and the public server must refuse it there if
  web2 is served by it. A field of the lobby view is read by a visitor with no credential.
- **Tests that expect the order of the join.** After phase 2 the first player is not the
  first seat. `seat_order: "join"` is for them; a test that should have the real behaviour
  reads its seat from the view.
- **A wait that holds a lock** would stop the drafts that it is there to test. The decider
  waits with no lock today; the test "an undo while a seat waits" and the e2e of the secondary
  draft are the guard when the value becomes one of the table.
- **Two tabs, one player.** A duplicated tab is the same player twice, and both write the same
  draft key. The server allows it; the page does not guard against it. Say so in `AGENTS.md`.
- **Seeded games change** if the setup of today changes. The fixtures prove that it does not;
  `ti4_play`, the scenarios and `seat_order: "join"` keep the old games.
- **Twenty-four factions and three sets on every screen.** web2 was looked at with the six
  factions by seat and the cards of `pok`. Phase 0 plays each faction and each set once.
- **The rule bot in the browser.** It was written for the server: its size in the wasm and
  its time for a decision are not known. Phase 5 measures both before single player depends
  on it.

## Open questions (owner)

1. **Is web2 served by the public server?** Open (owner, 2026-10-12). If yes, web2 needs the
   password screen (`/api/access`), and the plan must refuse `&seed=` there. What the plan
   does in any case: the socket of a table is behind the password and the limits, and
   "Start from" and the preset are refused on the public server.
2. **Who changes `web/` in phase 2**, and when: Felix, or this branch with his agreement? The
   same for the move of the rule bot in phase 5.
3. **The list of the maps** (decision 17): the owner judges the scrolling list when phase 4
   shows it.

## Order and size

Phase 0 comes first: it says whether web2 works on the server of today. Phase 1 is next: every
other phase puts a control on its screen. Phase 2 gives the owner the tools to test drafts,
and it needs the agreement about `web/`; its second half (the settings) does not, and can be
built first if that agreement takes time. Phases 3 and 4 are web2 only and independent of each
other. Phase 5 is Rust only and can run beside them. Phase 6 needs 5. Phase 7 is last.

**Order since 2026-10-13 (owner): 5 (built), 6 (built), then 4, then 7.** Phase 4 does not
block single player: the setup of phase 6 uses the default map of the count, and phase 4 adds
the list for both tables at once. Phase 4 loses its "independent of 5" place and comes after 6
so that its list is built once over the `Table` seam, not twice.

| Phase                                                                 | Where                                        | Size                                                                                              |
| --------------------------------------------------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| 0 web2 on the merged server                                           | web2                                         | small to medium (not known before the tests are run)                                              |
| 1 the start screen, the lobby, its socket, bots, a seat for each tab  | web2, ti4-server (the socket of a table)     | large (two new screens, the frame taken out of `GameShell`, the keys, the addresses of the tests) |
| 2 no Ready, random order, settings, the wait of a table, "Start from" | ti4-server, `web/`, ti4-bot-agent, web2      | large (many tests of the server and of `web/` change)                                             |
| 3 the choice of the faction                                           | web2, ti4-view (facts of a faction sheet)    | medium                                                                                            |
| 4 the map (after phase 6)                                             | web2                                         | medium                                                                                            |
| 5 one setup, the crate of the bots, the wasm exports                  | ti4-view, ti4-rule-bot, ti4-server, ti4-wasm | built                                                                                             |
| 6 single player on the same setup                                     | web2, ti4-wasm                               | built                                                                                             |
| 7 the table in the game                                               | ti4-server, ti4-view (a fact), web2          | medium                                                                                            |
