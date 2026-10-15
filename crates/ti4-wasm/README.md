# ti4-wasm

The rules engine compiled to WebAssembly, for the games in a page: web's offline game and web2's
local play ("The hosted game").

It began as a spike (2026-10-09) that answered three questions: how large the wasm file is, what
could make it smaller, and whether the engine's blocking decisions work in a browser. `web2` now
plays a game through it (see "Local play in web2").

## Result

- The engine compiles to `wasm32-unknown-unknown` with one change: `rand` without default
  features in `ti4-engine` (`getrandom` has no backend for this target).
- The file is **4.30 MB, 0.80 MB with brotli**, with the projections (`ti4_host::projection`) included.
- A seeded game of three to eight players gives the same state in wasm and natively.
- A decision of a player can suspend the engine and wait for a promise (JSPI). The engine needs
  no change for it.
- The host gets what a client of the server gets: the redacted view, the pending choice and the
  turn status (`SessionUpdate` from `ti4_host::projection`, the shared part of the server's `StateUpdateMsg`).

## Size

The size of `ti4_wasm.wasm`; brotli at quality 11.

| Build | raw | gzip | brotli |
|---|---|---|---|
| `--release` (the workspace profile) | 6.19 MB | 1.89 MB | 1.23 MB |
| `--profile wasm-release` | 4.20 MB | 1.08 MB | 0.78 MB |
| `--profile wasm-release`, then `wasm-opt -Oz` | 3.49 MB | 1.14 MB | 0.83 MB |
| `--profile wasm-release`, with the projections | **4.30 MB** | 1.11 MB | **0.80 MB** |

- `wasm-release` is `opt-level = "z"`, `lto = "fat"`, `panic = "abort"` (workspace `Cargo.toml`).
- `wasm-opt` makes the file smaller but the download larger. Do not use it.
- The `--release` and `wasm-opt` rows were measured before the fixed buffers (see Memory); the
  `wasm-release` row was 4.22 MB then, so the rows compare.
- The projections cost 0.10 MB raw and 0.02 MB of download. The first three rows and the
  breakdown below are without them.
- With the map templates and their builder the file is 4.52 MB raw and 1.13 MB with gzip
  (brotli not measured). The table and the numbers above are from before that.
- With the draft (the planning checks of `ti4_host::planning`, the request and draft buffers) the file is
  4.65 MB raw and 1.17 MB with gzip (2026-10-10; brotli not measured).
- With the MLP bot (`ti4-infer`, the decision pipeline of `ti4-policy` and `ti4_host::bots`,
  2026-10-15) the `wasm-release` file is 7.55 MB raw, 1.91 MB with gzip and 1.31 MB with brotli;
  without it, at the commit before, 6.89 MB, 1.73 MB and 1.20 MB.
- Speed: 2000 steps take about 7–12 s with `wasm-release` and about 6 s with `--release`, in
  Node. That is 3–6 ms for a step.

### Where the bytes are

From `twiggy` on an unstripped `wasm-release` build.

| Part | Size | |
|---|---|---|
| Code | 3.1 MB | |
| – `ti4_engine` | 1.27 MB | the faction modules are 0.43 MB of it |
| – `alloc` and `core` | 1.52 MB | generic code, compiled once for each type that uses it |
| – `serde_json`, `serde_core` | 0.17 MB | all serde-related code together is 0.2–0.3 MB |
| – all other crates | 0.1 MB | `ti4_model`, `ti4_content`, `rand`, `sha2`, the allocator (8 KB) |
| Data | 1.13 MB | |
| – content JSON | 0.96 MB | `ti4-content/content/*.json`, embedded as written |
| – other strings | 0.17 MB | |

The 1.52 MB of `alloc` and `core` is mostly `Vec`, slices and sorting (about 0.6 MB), `BTreeMap`
and `BTreeSet` (about 0.5 MB) and iterator adapters (about 0.25 MB). The split is by function
name and is approximate.

## What can be cut

| Candidate | Saves raw | Saves brotli | Verdict |
|---|---|---|---|
| Third-party dependencies | 0 | 0 | Nothing to cut. No crate is larger than 125 KB. |
| Minify the content JSON | 270 KB | 4 KB | Cheap, small. |
| Also drop flavour text, authors, short names, emoji fields | 330 KB | 24 KB | Small. Not checked: which of these fields Rust reads. |
| Also drop the map templates | 420 KB | 28 KB | Small. |
| Engine modules that hotseat does not use | 0 | 0 | The linker already removes them. |
| `Debug` implementations | 7 KB | – | Not worth it. |
| Fixed data structures in the engine (next section) | up to 1.1 MB | not measured | Large, and a rewrite. |

- These crates are in the manifests of `ti4-model`, `ti4-content` or `ti4-engine` and are not
  used there: `anyhow`, `tracing`, `globset`, `itertools`, `bitflags`, `smallvec`, `hashbrown`,
  `indexmap`. They cost no wasm bytes. Removing them saves build time only.
- `opening`, `observation`, `fixtures`, `fingerprint` and `registry` link to 0 bytes. `synergy`,
  `preview` and `decision_context` are 8 KB together.

## Later: an engine with fixed data structures (non-goal now)

The largest part that is not rules is the generic container code: 1.1 MB of `Vec`, `BTreeMap`
and iterator code. An arena alone does not remove it, because a `Vec<T>` in an arena is still
one copy of `Vec` for each `T`. What removes it is data structures made for this game:

- ids as small integers, not strings (the guess is that string ids, cloned and used as map keys,
  are the first thing to change; this is not profiled);
- arrays indexed by those ids in place of maps;
- bit sets for sets of players, systems, technologies;
- lists of a fixed capacity where the game has a limit (units of a kind, cards in a hand).

With such a layout the whole game state is one block of a known size. It is allocated once at
the start, or from an arena, and the engine does not allocate while it plays.

Expected effects, none of them measured:

- Size: up to 1.1 MB raw. A few hundred KB of download is a fair guess.
- Speed: probably faster than today. The state is compact and stays in the cache, a copy of the
  state is one `memcpy`, and a step does not allocate.
- Debugging: the state has a fixed address and a fixed layout, and a snapshot is a copy of bytes.

This is a rewrite of the engine and is a non-goal until hotseat works. The rule for new code in
the meantime is below.

## Memory in this crate

New code uses data structures that fit the use, with fixed or arena memory, so that it does not
have to be migrated when the engine changes.

- The exports that need no game (the setup of a table, the scenarios) use fixed
  static buffers (`src/buffer.rs`): the result of an export (`RESPONSE_CAPACITY`,
  256 KiB) and the request of the host (`REQUEST_CAPACITY`, 64 KiB). A game speaks through the
  input and output of `host_api` (JSON messages, see "The hosted game"), which grow as needed.
  Until 2026-10-14 web2 had exports of its own with a fixed update buffer and a draft buffer;
  they are gone (`web2/plans/2026-10-14-DEDUPLICATE_HOSTS_PLAN.md`, step 5).
- Such a buffer never grows and never moves. The host reads every result at the same address.
- A value is serialized directly into its buffer. No intermediate `String` or `Value` is built.
- A value that does not fit fails the call at once. The error says what was too large, its size,
  the capacity, and the constant to raise:

  ```text
  ti4-wasm buffer full: the pending choice is 876 bytes, but PENDING_CAPACITY is 512 bytes.
  Raise PENDING_CAPACITY in crates/ti4-wasm/src/buffer.rs and rebuild, or make the pending
  choice smaller.
  ```

  The JS side throws it as an `Error`. A buffer is never cut short and never reallocated: the
  limits are there to be found.
- What still allocates in this crate is what the engine's interface asks for (owned ids, the
  boxed decider, the setup lists) and the text of an error. The engine itself allocates on every
  step, so the global allocator stays for now.
- The views allocate too. A decider is given no game state, so the run keeps a copy of the state
  and refreshes it after each step and at each choice that the engine offers with its position,
  and `ti4_host::projection` builds a `GameView` of maps and vectors from it. Both are shaped
  by the engine's types and change with them.
- The checkpoints for undo are a fixed number (`CHECKPOINTS`, 32), not a fixed size: each is a
  copy of the game made by the engine's `Game::fork`, on the heap. The oldest is dropped when
  a new one is made. This is not an error: the host then takes an answer back by a replay.
- Serializing goes through `dyn Write`. One copy of every serializer for each writer type cost
  39 KB.

## The boundary

Plain C ABI, no `wasm-bindgen`. The crate has its own `[lints]` because the boundary needs
`unsafe` attributes and the workspace forbids `unsafe`.

A game is played through one host, for both pages: the hosted exports `ti4_host_*` (see "The
hosted game" below), with `ClientMessage` in and `ServerMessage` out, as on the server. web2
reads the messages with the code of its server transport (`web2/src/session/serverMessages.ts`).
The host also answers the queries that need no game (`ti4_host_query`: the catalogue, the odds of
a space or a ground combat). The setup of a table and the scenarios are exports of their own:

| Export | |
|---|---|
| `ti4_run_seeded(seed, players, max_steps) -> status` | Random deciders on all seats; the result is the state. |
| `ti4_setup_options(request_len) -> status` | What a host can choose for a table, as JSON in the response buffer: the options of the server's `GET /api/setup-options` and, as `maps`, the maps that build for `players` seats (0: every map). The request (`OptionsRequest`) is `{"players", "models": [{"name", "label"}]}`: each model is the bot kind `mlp:<name>`, labelled, after the rule bot and the random seat. A model need not be loaded to be listed. |
| `ti4_setup_preview(request_len) -> status` | The board of the table in the request buffer, with a place for each seat and no faction, and `map`: what the table says of its map. |
| `ti4_setup_draw(request_len) -> status` | What the start draws for the table in the request buffer: `players` (the ids by seat), `order` and `factions`. |
| `ti4_scenarios() -> status` | The scenarios, as a JSON list in the response buffer. |
| `ti4_request_ptr()` | Where the host writes a request (`REQUEST_CAPACITY`, 64 KiB) before the export that reads it. |
| `ti4_response_ptr()` | The address of the result or the error of the last export. |

The result of `ti4_run_seeded` has the state and, as `view`, what seat `a` is shown of it.

A status of 0 or more is the length of a JSON result. A status below 0 is the negated length of
an error text.

The update of web2 is the part of a `state_update` that it reads (`updateOf`): `viewer`,
`view`, `pending_choice`, `turn_status` and the facts (`tactical`, `exploration`,
`action_cards`, `secret_objectives`, `score`, `agendas`, `secondary`, `reaction`, `seats`).
The viewer is the seat that is asked. The nonce of a choice is its number in the game.

### The seats of the page

The game of web2 has more than one seat of a person (`humans`, a mask: bit 0 is seat `a`), and
tools for testing that the server does not have. They are local commands of `HostSession`, not
exports of their own.

- **Stepping.** The start request has `stepping`: 0 (the other seats decide with no stop), 1
  (the game waits before each decision of a seat that the page does not play) or 2 (before each
  that takes the wait of a table, not before a reaction window that the seat only declines, as
  a seat on the server does not wait there). The local command `{"type":"stepping","mode"}`
  changes it at any time. While it waits, the game sends an `ask` envelope with `waiting` (the
  seat) and a `state_update` with no `pending_choice`; `turn_status` names the seat. `{"type":
  "step"}` lets the seat decide. A step is not a choice: it has no number, makes no checkpoint
  and is not in a saved game. The seats decide the same in every mode, so the game is the same
  game. The page takes the wait of a table itself, with a timer, before it sends `step`.
- **Switching seats.** `{"type":"switch_seat","seat"}` while the game waits: from the open
  decision on, the page plays that seat in place of the seat that it is shown as. A switch is
  `(decision, humans)`: from the decision with this number on (every decision of the game
  counts, also of the other seats), the page answers the seats of the mask. The decision is
  asked again, of the seat's new decider. An undo does not go back before a switch. The save
  keeps the switches; a game that is played again is given them before it starts.
- **Bot seats.** `local.bots` names the kind of the bot of each seat where the page does not
  play it: `smart` (the rule bot), `mlp:<name>` (a model), or `null`/`random`; a seat with none
  decides at random from one seeded stream. A save before `bots` has the mask `rule_seats`,
  read as `smart` on those seats. A table names the kind of each seat. A bot decides as on the
  server, with `ti4_host::bots::choose`: from the seed, the seat and the number of the decision.
  The rule bot keeps nothing between two decisions; an MLP seat keeps its decision pipeline (the
  fleet plan of the activation it moves, its power maps, the table of its activation). A
  checkpoint keeps that carry of every seat and an undo restores it; a copy of the game for a
  draft plays on with copies of the bots; a saved game that is played again gives each saved
  decision of an MLP seat to its bot (`Bot::follow`), so the game goes on as the game that was
  saved (`crates/ti4-server/tests/it/both_hosts.rs` cuts a saved game before the planned moves
  of an MLP seat).
- **MLP models.** `ti4_model_load` loads a `.ti4m` into the registry of the engine
  (`ti4_host::bots::register`), by its name. `ti4_host_start` refuses a game with a seat of the
  kind `mlp:<name>` whose model is not loaded, with the name.
- The view is of the seat that was asked last while the page still plays it, else of the first
  seat of the mask. A draft is of that seat.

### The game of a request

`ti4_host_start` takes one of three games (`host_setup::Start`):

- `setup`: the form of `web/` (`host_setup::Setup`, below).
- `table`: the table of web2's single player, `{ "setup": TableSetup, "seats": [...] }` (also
  the request of the setup exports). `setup` is `ti4_host::setup::TableSetup`: `player_count`
  and `seed`, and optional `map` (`{"kind":"template","alias"}` or `{"kind":"random"}`, the
  default), `strategy_cards` (`te` by default, `pok`, `base_game_codex1`), `speaker_order`
  (`random` or `seat`) and `start_from` (the id of a scenario). `seats` are the players in the
  order of their seats: `kind` (`human`, `smart` for the rule bot, `random`, `mlp:<name>` for a
  model), `faction` (or
  none, for one drawn from the seed) and `name`. The player of seat i has the id of the i-th
  letter (`a`, `b`, …). The game is started as the server starts the table of a lobby
  (`ti4_host::setup::{draw, create}`), and a seat that the page does not play decides as on the
  server, the random seats too, by its place, not its id (`ti4_host::bots::choose`): a table of a
  seed is the same game on both hosts (`crates/ti4-server/tests/it/both_hosts.rs`). With a name for every
  player, each update says who sits where (`seats`).
- `local`: a game of a fixed setup, `{ "seed", "players", "humans", "scenario"?,
  "bots"? }`: web2's `?local=` and `?scenario=`. The game is on the recommended map
  template for its count, with the factions of the seats in order; a scenario has its own seed,
  count and map (`ti4_host::view::scenario::start`).

A new game of a scenario with an opening (any of the three) plays the opening first: the
answers of its seat up to the question of the scenario, when the page plays that seat. A game
that is played again from a save, or a new run after a change of its history, does not: its
answers are in the save.

`modes` (the mode of each reaction card, by key) and `stepping` are given with the request.

## Decisions with JSPI

The engine asks for a decision with a blocking call (`Decider::choose`). The server blocks a
thread for it. A browser cannot block its main thread, so the host wraps the import in
`WebAssembly.Suspending` and the export in `WebAssembly.promising` (`js/host.mjs`). The wasm
stack is parked until the promise of the answer settles.

- No threads, no `SharedArrayBuffer`, no cross-origin isolation headers.
- The glue is about ten lines of JS, written by hand.
- While the game is parked, other exports of the same instance can be called. They must not
  touch the game, which the parked stack has borrowed. They read a copy that was stored when the
  choice was offered (`LATEST`, and the checkpoints for a draft).
- Such a call must return before the game is resumed. Rust keeps a second stack in linear
  memory, and the calls share it.
- Undo cannot rewind a parked stack. The parked run goes back to a checkpoint, or the game is
  played again from the seed and the decision log.
- Checked in Chromium 156 only. Firefox and Safari are not checked. The fallback is a Web Worker
  that blocks in `Atomics.wait`; it needs cross-origin isolation, and a blocked worker cannot
  answer queries.

## Local play in web2

`web2/?local=<seed>&players=8&humans=<mask>` plays a game of this engine in the shell of web2.
`humans=1` is seat `a` against seven random seats. `humans` is a mask of the seats that are
played in the page.

- `web2/scripts/build-wasm.sh` builds the file and copies it to `web2/src/session/ti4.wasm`
  (not in the repository). It and web's `web/scripts/build-wasm.mjs` both run
  `web-shared/scripts/build-wasm.mjs <out.wasm> [--dev]`: one cargo command
  (`--profile wasm-release`, or `wasm-dev` with `--dev`), so both apps share one build.
- `web2/src/session/` is the client side: `Transport` (updates in, `submitChoice` out),
  `messageTransport.ts` (the `ServerMessage`s of a link as the events of the page),
  `wasmTransport.ts` (the game in a Web Worker over `engineLink` of `web-shared/src/link/`, and
  the questions that need no game), `wsTransport.ts` (the server over `socketLink`),
  `serverMessages.ts` (what both read of a `ServerMessage`), the selectors from an update to the
  view models of web2, and `useLiveSession`.
- The activation, the movement and the landings of a tactical action have their screens (see
  "A tactical action" below). Every other decision is one list in the action panel.
- `tests/it/protocol_fixtures.rs` records the update of one choice of a game for each file of
  `fixtures/protocol/update-*.json`: the game, the number of the choice, and the option ids that
  answer the first choices so that the game reaches it. The tests of web2 read these files. The
  test fails while a file differs from what the engine plays; `cargo wire-web` writes them again.

### A tactical action

The engine asks for one ship at a time (`move|<system>|<index>`), then for each unit that the
ship loads, and it moves the ship before it asks for the next one. The screen of web2 stages the
whole movement first. Two things connect them.

**Facts.** For the choice of a system to activate, for the choice of a ship to move and for the
choice of a ground force to land, the update has one more field, `tactical`
(`ti4-host/src/view/tactical.rs`). It says what the choice itself does not: for the activation, how
many ships are in range of each system; for the movement, each ship of the seat with its path,
the gravity rifts on it, why it cannot move (a command token, or the range), and the units that
it can load, also on its way; for the landings (`kind: "invasion"`), the ground forces by kind
and place with their counts, and every planet of the active system with the forces that can
land there; for the production (`kind: "production"`, the choice `produce_unit`), each unit of a
production with its printed cost and values, how many one purchase makes, what is left in the
supply and its places, and why the seat cannot produce it when the game does not offer it (a
unit that the seat only cannot pay for has no reason), the production limit, the discount of this use, the ready planets that pay
resources with their worth, the trade goods, and the fleet pool and the capacity of the system.
The landing facts and the offered units of the production are read from the options of the
choice itself. A production whose places the facts do not know (by an ability, or in another system)
has no facts and stays a list. `ti4-wasm` fills
the field when it offers a choice to a seat of the page; the random seats do not pay for it.

- Nothing in `ti4-engine` was changed for this. Every fact is the answer of a public function
  of the engine, asked as `Game::begin_one_move` asks it. What a ship can load is read from the
  `load` options of the engine's own `CargoWindow`. A test plays a move and compares the two.
- The server does not send the field yet (`project_session_update` leaves it empty).

**A plan.** "Move fleet" sends the staged movement as steps that name what is moved and
loaded, not option ids (`web2/src/session/movementPlan.ts`). `Transport.submitPlan` takes them;
this is the contract of the server's batch (`ti4-server/src/session/batch.rs`), so the
websocket transport can post the same steps. The wasm transport answers each choice of the
engine with the option whose payload matches the next step, with the three rules of the
batch: a hold that closed itself needs no "done loading"; a hold that the plan does not load
is declined; a reaction window or a choice of another seat stops the plan, and the rest is
staged again when the movement goes on.

- A step that the engine does not offer takes the whole plan back, from the checkpoints. The
  server gets the same by playing the game again for each batch; here that would cost up to
  the whole engine time of the game.
- Each answer of a plan is an answer of the saved game, so the save, the replay and the
  checkpoints did not change.
- One undo takes a movement back whole: after an undo, the transport goes on while the choice
  that is open again is inside a movement (a hold, or a ship has already moved). It needs no
  stored groups, so it also holds after a reload.
- The transport keeps what each answer chose (the option with its payload, as `RecordedDecision`
  of the server), and a game that is played again fills that list again. The update after an
  undo tells the steps of the movement that were taken back (`undone`), and the page stages
  them again as the draft.

**The landings.** The engine asks for one ground force at a time (`commit|<n>|<planet>`) and
then for the end (`done_committing`). "Commit ground forces" sends the staged landings through
the same `submitPlan`, as steps `land` (planet, unit, damaged, and the place of a unit that is
not in the space area) and `done_committing`.

- When the last ground force has landed, the engine does not ask for the end. A plan whose
  only step left is `done_committing` is done at any other question.
- One undo takes the landings back whole: the transport goes on while the open choice is a
  landing and the answer before it landed a unit.
- The server's batch has no landing step yet.

**The production.** The engine asks for one purchase (`build|<unit>|<count>`), then for what
pays it, one planet or trade good at a time (`pay_resources`; what is paid over is kept for the
next purchase), then for the place when there are two or more (`place_unit`), then for the next
purchase, and at last for the end (`done_producing`). "Produce units" sends the staged build
and its payment through `submitPlan`, as steps `produce` (unit), `place` (destination), `pay`
(the id of the option) and `done_producing`. The purchases are in the order that the player
staged them, each with its place; the payments follow them.

- A payment is not in the place of the plan where the game asks for it. `plan_answer` answers a
  `pay_resources` question after a purchase of the plan with the first payment of the plan that
  the game offers, and moves that step to where the plan is. With none, the plan is refused
  whole: the staged payment does not cover the cost.
- What the game settles itself is left out: a place when there is one only, a payment that it
  did not need or took without a question (one option), the end when the limit is used.
- One undo takes a production back whole: the transport goes on while the answers before the
  open choice are a purchase and what paid and placed it.
- The cost and the limits of a staged build are sums in TypeScript over the facts
  (`productionDraft.ts`): the purchases at their printed cost, less the discount and the credit.
  The fleet pool and the capacity are plain sums, so they do not know what a rule excuses.
- A recording of `tests/it/protocol_fixtures.rs` plays a scenario when its game names one
  (`update-production`: the scenario `late-game`, answers `tactical 72 done_moving decline`).

Limits:

- The engine uses Gravity Drive only for a ship that cannot arrive without it, and it chooses
  the path. The screen shows that; the player cannot give Gravity Drive to another ship.
- The matching of a step to an option is in TypeScript here and in Rust in the server. One
  shared implementation means taking the movement part out of `batch.rs`.
- Not shown yet, because they need the event history: the roll at a gravity rift, units that
  are removed after the movement, space cannon, and the read-only view of a past movement.
  Units that stay behind with no ship that carries them get no warning.
- The other steps (space cannon, combat, bombardment, what follows the landings) are still
  lists.

### The exploration

Every update has the field `exploration` (`ti4-host/src/view/exploration.rs`):

- `decks`: the card ids that each of the four decks still holds, sorted. The order of a deck is
  hidden; what left a deck was shown to the table, so its content is public.
- `drawn`: the cards that were drawn, in order, with the seat, the planet and what came of it
  (`GameState::exploration_log`). The outcome is empty while the card is being resolved: a
  decision that the game asks then is of that card, and web2 shows the card on top of it.
- `explores`: the planets that no player controls, with the decks that they are explored into
  (`ti4_engine::planets::traits_now`). A seat that takes control of one explores it (35.1).

An update names cards and attachments by id. The catalogue (`{"query":"catalog"}` of
`ti4_host_query`, see "The hosted game") has what is printed on them: `content` (the digest of the
corpus, which a page compares with the one of a server before it plays there), and in
`exploration` the name, the deck, the kind and the text of each exploration card, the cards
that each deck begins with, and for each attachment its name, what it adds to the planet, and
the text of the card that attaches it. `ti4-content` is compiled into the engine, so this costs
no bytes.

Its second part, `components` (`ti4_host::view::components`), is what is printed on the cards of a
play area: the abilities of each faction, the leaders with how each is unlocked, the
technologies, the relics and the ability cards of the legendary planets. A technology, a relic
or a legendary card has `used`: its text has an `ACTION` or it exhausts. The player view says
which of them a seat has and which are exhausted (`leaders`, `exhausted_technologies`,
`exhausted_relics`, `exhausted_legendary`).

Its third part, `records` (`ti4_host::view::records`), is every record of the corpus that a page
names by id, of every source (not only those of the game): the strategy cards, the secret and
public objectives, the action cards, the technologies, the exploration cards, the planets, the
attachments, the factions, the units, the leaders, the faction abilities, the promissory notes,
the relics and the breakthroughs. Each kind is a list in the order of the corpus, each record
with its id and the fields that a page shows, as printed (`UnitRecord` has the numbers of a unit
card). It is about 250 KiB of the 410 KiB of the catalogue; a list, not a map by id, because 15
maps cost 110 KB of wasm (2026-10-15).

The server serves the same JSON at `GET /api/catalog` (`ti4_server::http::content`), so a page
without an engine reads the same catalogue; `content` is the digest of its corpus. Both apps load
it with `loadCatalog` of `web-shared/src/content/catalog.ts`: web2 from its engine, web from the
server online and from its engine offline. `fixtures/catalog/catalog.json` is the answer of the
engine, for the tests of web (`cargo wire-web` writes it, `cargo t-web` fails while it is stale).

The relic fragments of a seat are in its player view (`relic_fragments`, by deck), and the
attachments of a planet in its planet view. The values of a planet (`resources`, `influence`)
are still the printed ones: the value after the attachments is not in the view yet.

### A draft

A draft is the next tactical action of a seat, tried on a copy of the game: its activation, its
movement, the landings of its invasion and its production. `web2` shows it in the Draft
workspace.

- The page sends `run_draft` (`{"request_id", "script": [...]}`) and is answered with a
  `draft_outcome`. With `"scope": {"kind":"secondary","primary":…,"card":…}` the script is of the secondary of the
  seat, for the strategic action that is resolved (`secondary` of the update names it and says
  whether the seat may draft); its entries are answers by id. A
  step of the script is `{"kind":"choose","option_id":…}` or a step of a plan (`PlanStep` in
  `ti4-host/src/planning/steps.rs`: `move`, `load`, `done_loading`, `done_moving`, `land`,
  `done_committing`, `produce`, `place`, `pay`, `done_producing`). The script of a tactical
  action is `choose tactical`, `choose <system>`, then the movement, the landings and the
  production. A `choose` between them answers what the game asks the seat there (AI Development
  Algorithm before the production).
- The steps that follow each other are answered as a plan (`plan_answer`), also when the page
  sends a plan to the game (`apply_draft`, see "A plan that is sent"): a hold that is not
  loaded is declined, a payment is found anywhere in the plan, an end that the game does not ask
  for is left out. The pause between two steps of the action ("negotiate, or continue") is
  passed.
- The run forks the game at its last finished step, gives the seat a hypothetical turn
  (`Game::prepare_hypothetical_turn`), answers from the script, and stops at the first question
  that the script does not answer. Nothing is kept between two runs: a changed script is run
  again from its start. So the command returns at once and needs no second stack, and undo and
  redo of a draft are a list of scripts in the page.
- The outcome (`DraftOutcome`) has `update`, where the run ended, in the shape of a live update
  with `tactical`; `movement`, `invasion` and `production`, the update at the first question of
  that step, which the whole step is staged against; `consumed`, how many steps fit; and
  `stop`: `open` (a question that the script does not answer; steps of the script may be left),
  `complete` (the tactical action is over), `refused` with the step, `stopped` or `failed`.
- The run ends where something random lies ahead: dice (a gravity rift, space cannon, a
  combat), a card that is drawn (a landing on a planet that nobody explored), ships of another
  seat in the active system, a question of another seat. It is `stopped` there, with the reason.
- What a draft may show is checked as on the server (`ti4-host/src/planning/audit.rs`): only
  audited offers, no dice, nothing that the seat did not know. The other seats take no optional
  reactions. These checks and the recorded-decision adapters were moved from `ti4-server`,
  which runs the same `run_draft` on a copy of its game, guarded by the same code
  (`ti4-host/src/planning/fork.rs`).
- The game at its last finished step is not kept: the checkpoints are made only after a step
  that asked the host. The base of a draft is the last checkpoint with the steps of the random
  seats since then played again on a copy of their stream, as an undo does. It is made for the
  first draft at a choice and dropped when the game goes on. The checkpoints are in a
  `thread_local` for this (a `Game` is not `Send`); the game that is parked is not touched.
- A test plays the same answers live and compares: the movement that the draft shows is the
  movement that the game then shows.
- The seat of a draft is the seat that was asked last (with stepping: also while another seat
  decides).
- A draft is also run in the strategy phase: the copy is put into the action phase as it
  stands (`Game::prepare_hypothetical_turn`). The status and agenda phases have no draft.

Limits:

- A question of the seat before its turn menu (a start-of-turn ability) stops the draft at its
  first step. A question of an ability in the middle of a step (not before it) has no place in
  the script of the page.
- A landing on a planet that nobody explored draws a card, so a production after it is not
  drafted.
- An option of the landings with `from_system` or `from_planet`, and the exchange of a
  production, are not audited: the draft stops there.
- `batch.rs` in the server has its own steps.

### A plan that is sent

The page sends a movement, the landings, a production or a whole draft to the live game as one
request (`Transport.submitPlan`, `Transport.applyDraft`): `apply_draft` with the script, as on
the server. How a step answers a question of the game is decided in Rust, in the function that
answers a run of a draft (`ti4_host::planning::draft::script_answer`): one copy of the rules,
for the draft, for the live game and for the server.

- The session answers each choice of the page from the script while it runs
  (`HostSession::scripted`), and the page is not asked. The runs are the server's
  (`ti4_host::planning::script::ScriptRuns`, one per seat); what differs is
  `OnMismatch::TakeBack` below.
- First the script is played on a copy of the game, as on the server
  (`ti4_host::planning::script::check_script`): the copy is the game at its last finished step
  (`script_copy` in `lib.rs`, the last checkpoint and the steps since it played again), with
  the decisions since then played again. A script that the copy refuses is answered at once with
  `draft_applied` (refused, nothing applied), and the game is not resumed.
- All or nothing: a step that the game does not offer all the same (the copy stops at a decision
  of another seat) takes the whole script back (the run goes back to the checkpoint of the choice
  the script began at, as a failed `submit_batch` does), and `draft_applied` says it was refused,
  with nothing applied. A question that is not of the
  script (another seat, also another seat played by the page, a reaction window) stops it with
  its earlier steps in the game (`interrupted`), and so does the end of its action or turn.
- A window that the Never cards of the page decline is declined before the script sees it, so
  it does not stop the script.
- Before a decision of another seat (stepping) the choice is that seat's, and a script never
  answers it.

### Never cards

A player can set an action card to "never offer". The engine applies the setting, with the
wrapper that the server uses (`ti4_host::view::reaction_modes::NeverOffer`); the page has no copy
of the rule.

- `set_reaction_mode` sets a card by its printed name, or the card of an ability by its key.
  The modes are given with the start request, and a change is sent while the game waits.
- A window that offers only Never cards is declined without asking. It is still a choice of
  the page with an answer: it has a number (the nonce after it is higher by one), the next
  update has a note with `passed` (one for each window), and a checkpoint gives the answer
  again after an undo.
- An undo does not stop at such a window: it opens the last choice that the page answered
  itself.
- A window with another card is asked without the Never cards. The index of the answer is the
  place in the options that the page was shown.
- A saved game is played again below the wrapper, as a replay on the server is: the decisions
  of its log are given whatever the cards are set to ("The saved game"). So a game that was
  played with other settings is the same game.
- The engine asks some windows twice in a row (after an activation). Each is passed.

### Scenarios

A scenario is a board that a game only has after hours of play, or a question that it only
reaches after several answers, for testing the screens (`ti4-host/src/view/scenario/`, the one
scenario system of both hosts; the server's `/api/dev/scenarios` lists the same). It is not a
stored state: it is a function over the state of a new game, so a game that starts from one is
saved, played again and taken back like any other. `web2/?scenario=<id>&humans=<mask>` plays
one, and the Dev sheet lists them.

- The scenarios of three seats (`three_seats.rs`: `research_tech_skips`, `tactical_action`, the
  three productions, `space_combat`, `ongoing_combat`, `ongoing_combat_four_views`, the three
  invasions, `endgame`, the two scores, `full_game`) are Sol, Hacan and Letnev in round 1 on the
  seeded random board, and play on the seed of a table (`any_seed`; `?scenario=` plays seed 42).
  Most have an opening: Sol's answers up to the question of the scenario (the research, the
  production at Jord, the first retreat of the battle, the first question of the invasion). It
  is played when the page plays seat `a`; the answers are ordinary decisions of the save.

- `late-game`: round 4, seat `a` (Sol) has the turn with ten fleets, every ship type, every ship
  upgrade and Gravity Drive. Around them are a gravity rift, a nebula, a supernova, an asteroid
  field (the map of eight seats has none, so one tile is replaced), a wormhole pair, fleets of
  other seats, cargo on planets and in space on the way, and a system with a command token of
  the seat. The other seats have command tokens on the board; one system has seven. Next to
  the fleet in the north is a system with two planets and no units (38), for landings without
  a fight. For a production, system 72 has a space dock on each of its two planets (a ground
  force has a choice of places) and the home has one; the seat has Space Dock II, War Sun, Spec
  Ops II, Sarween Tools and AI Development Algorithm, 6 trade goods, and one exhausted planet
  (Quann). The exploration left its marks: seat `a` has a Dyson Sphere on Vega Major, two
  research facilities (Velnor, which has a technology specialty, and Lisis, which has none) and
  three cultural relic fragments and one hazardous; seat `b` has a Paradise World on Loki and a frontier fragment. These
  nine cards are out of their decks. Planets of each type are free.
- `late-game-deflectors`: the same, with Light/Wave Deflector and Antimass Deflectors.
- The tests of `scenario/mod.rs` ask the facts of a tactical action for each case (a move that needs
  Gravity Drive, a rift roll, the move of 1 out of the nebula, the way around the supernova, the
  wormhole, a pickup on the way, three places to load, the command token, out of range). A new
  case is added there with its test.
- Found with it: the engine gives Antimass Deflectors no effect
  (`MovementRules::asteroid_fields_open` is only set in a test), so the asteroid field is closed
  in both scenarios. A test pins that, and fails when the engine changes.
- The save of a scenario is kept under the slot `scenario.<id>.<humans>`, and names the
  scenario.

### The saved game

A game is not kept as its state. It is kept as what plays it again: the save of `HostSession`
(`save_state`, format 2, see "Save and resume"), with its game, **the option id of every
decision, of every seat, in order** (`decisions`), the answers of the page among them
(`answers`), the switches of its seats (`switches`), and the times of its events. The random
seats and the dice follow the seed. The server keeps a game the same way: an init record and the
decision log. The apps keep it in the save store of `web-shared/src/storage/` (IndexedDB
`ti4-offline`), with the SHA-256 of the engine file.

- Where: web2 writes it at each question of the page, under the slot of the address of the game
  (`local.<seed>.<players>.<humans>`, `scenario.<id>.<humans>`, `single`;
  `web2/src/session/savedGame.ts`). The Dev sheet has New game, Export and Import (the export
  file of the store).
- **Playing it again.** The save is the `resume` of the start request. Every decision is given
  again, whoever the seat is: no seat decides, no bot runs, nothing is projected, no seat waits
  and no card setting is applied. What the host counts is counted as in the game that was never
  saved, so the first choice after the replay has the same number, and an undo goes back into
  the saved decisions. A seat that draws from the random stream moves the stream, so the seats
  decide the same after the replay. `progress` envelopes say how far it is (every 20
  decisions). A save of another format is not read.
- A decision of the log that the game does not offer ends the run: the `end` envelope has
  `diverged`, which names its number, its id, the prompt and the ids that are offered.
- **Undo.** One undo takes back one answer of the page, or a movement, the landings or a
  purchase whole, back to the last switch of the seats. The answers are kept in the server's
  track (`ti4_host::planning::undo::UndoTrack`) under `UndoRules::Local`: a die, a shuffle, a
  revealed card and the decisions of the other seats do not cut it, only a switch. The engine keeps a checkpoint (`Game::fork` and the position of the
  random seats' stream) after each step that asked the page, the last 32 of them; when they
  reach back far enough, the run goes back one answer at a time (each without asking the page)
  and opens the choice again. Else, and when the game is over, the session replies `restart`
  with the log cut at the first answer that it takes back: the page plays the game again from
  its seed with that log, in a new instance. The first snapshot after it says the steps that
  were taken back (`undone`).
- A saved game of another engine file is not replayed before the player says so: another
  engine may play the same answers differently, and not every difference gives an illegal id.

### Measurements

Measured on 2026-10-12 with the former exports of web2 (`ti4_play`, a replay of the answers of
the page only; the bench was removed with them): one full game in Chromium, seed 3, eight seats,
seat `a` answers option `7n mod options` at its n-th choice. The game has 296 choices of seat
`a` and 2619 decisions in all. A replay of a saved log gives every decision and runs no bot, so
it is faster than the replays of this table.

| What | Time |
| --- | ---: |
| Random seats between two choices: median | 9 ms |
| the same, 90% of the choices | under 0.15 s |
| the same, 99% | under 0.51 s |
| the same, longest | 0.57 s |
| Whole game | 14.6 s |
| of that, reading the 296 updates in JS | 0.14 s |
| Replay up to answer 50 (resume or undo there) | 1.8 s |
| Replay up to answer 150 | 5.6 s |
| Replay up to answer 296 | 14.2 s |
| Undo at answer 50, from a checkpoint | 8 ms |
| Undo at answer 150 | 25 ms |
| Undo at answer 290 | 42 ms |
| Writing the save, 296 answers (3.7 KiB) | 0.08 ms |
| Writing the save, 5000 answers (59 KiB) | 0.7 ms |
| Writing the save, 50000 answers (589 KiB) | 6.6 ms |

- Writing the whole save after each answer is not a bottleneck. No game comes near 5000 answers.
- The replay is the bottleneck: it costs all the engine time of the game so far. The page
  shows how far it is. Only a reload pays it; an undo comes from a checkpoint.
- A checkpoint after every step would make the game a third slower (19.7 s in place of
  14.7 s). After the steps that asked the page it costs nothing that can be measured, and an
  undo then costs what the wait for that choice cost.
- The time is the engine's, not the boundary's. The same game with random deciders only (2678
  decisions) takes 8.3 s natively (`--release`), 12.4 s in this file, and 7.9 s in a wasm file
  built with `opt-level = 3` (6.8 MB before compression in place of 4.5 MB). That is about
  3 ms for each decision natively.
- A CPU profile of that game (Node, a build with names) has no single hot function. The time is
  allocation, `String` and `Vec` clones and `BTreeMap` walks. By caller: `Game::apply_choice`
  26%, `Game::advance_turn` 25%, `Game::legal_options` 20%;
  `transactions::may_transact` 18% (it computes the neighbours of a seat each time);
  `fleet::enforce_everywhere` 12%, nearly all of it `units::catalogue`, which builds its map
  on each call; `Resolver::checkpoint` 9%; the copy of the state for the host at each offer 6%.
  The shares overlap.

### What a checkpoint is

`Game::fork` in the engine copies a game at a step boundary: the state, the open windows, the
dice streams, the counters and the resolver. The drafts of both hosts use it. The
test `a_fork_at_a_step_boundary_plays_the_same_game` checks it as a checkpoint for local play,
on eight seats, seeds 3 and 11, 700 steps each, a copy every 7 steps, each played 30 steps on
with a copy of the random seats' stream:

| A copy of the game | Plays the same game |
| --- | ---: |
| `Game::fork` | 192 of 192 |
| Made again from the `GameState` alone, at any step boundary | 54 of 192 |
| the same, only where no window is open | 54 of 61 |

- A window is open at 72% of the step boundaries (no window at 390 of 1400): mostly a trade,
  the end of a turn, a strategy secondary or a tactical action.
- The state alone is not a checkpoint, also where no window is open: the dice streams start
  again. This is what the server's `snapshot.json` holds, so it can check a replay but not
  replace one.
- One fork takes 2.6 ms in a debug build. The state is 37 KiB as JSON.
- A fork lives in memory only. A checkpoint that outlasts a reload, or the server process,
  needs the same parts stored: of the window types only `DiplomacyWindow` can be serialized
  today. Not done: a reload pays the replay once.

Limits:

- The random seats play in the Worker between two choices. The page responds in that time, but
  its next question waits for them (see the table).
- A reload late in a game takes as long as the game took the engine so far. So does an undo
  of an answer older than the 32 checkpoints, or after the game is over.
- With `humans` naming more than one seat, the page shows the hand of the asked seat at once.
  There is no hand-over screen.
- The view has ids, not names. web2 shows technologies by id and has a small table for the
  names and commodity values of the eight factions (`session/select/names.ts`).
- Systems outside the map (the wormhole nexus, the fracture) are not on the board of web2.
- The view does not say whose home a system is, and has no combat strength.

## Eight players

A game is set up on the recommended map template for its player count (`ti4_host::maps`, source set
Prophecy of Kings). Seven and eight players work through three marked hacks (search for `HACK`):

- `ti4-engine/src/seating.rs`, `EXTRA_SEAT_FACTIONS`: seats seven and eight play Sardakk N'orr
  and Yin, which are not fully ported. They are not in `IN_SCOPE_FACTIONS`.
- `ti4-host/src/maps/build.rs`: a hyperlane tile that a template places a second time is
  replaced by another hyperlane tile. Hyperlane paths are not modelled, so nothing is lost today.
- `ti4-host/src/maps/build.rs`: when the filler pool (planet systems) is empty, the open slots
  get the other numbered tiles (anomalies, empty space, wormholes) in tile order.

The templates `7pStaticEq` and `8pStaticEq` still do not build: they name a tile `0g` that the
content does not have.

## Not done

- `ti4-server` does not compile to wasm (tokio, axum, threads, files). The views, the code
  that makes them from a state, the messages, the decision facts and event history, the presets
  and the maps are in `ti4-host`, which both sides use. Still in the server: storage, sockets
  and the lobby.
- The view of a seat is as old as the last finished step, apart from the combat and invasion
  boundary, which is current. The server has the same limit.
- The UI queries are the `tactical` field of the update and the draft. Undo and resume are replays from the seed; a stored
  position that can be resumed would need the engine to stop at step boundaries only.

## The hosted game (`web/` offline play, `web2/` local play)

A game in a page, speaking the protocol of the server: the offline game of `web/` (one person
against rule bots) and the local play of `web2/` (one seat or several; see "The seats of the
page" and "The game of a request"). Both run it in a Web Worker with the driver of
`web-shared/src/engine/` (`Host`, `HostRunner`), and read it through `engineLink`
(`web-shared/src/link/`). The code: `src/host_api.rs` (exports, JSON envelopes), `src/host_session.rs`
(`HostSession`: what the server's `SessionShared` does around the engine), `src/host_setup.rs`
(the request of a table → `ti4_host::setup::TableSetup`, the form's data). `js/host.mjs` is a working reference host and a smoke test.
Chrome and Edge 137 and later only (JSPI); Node 24 has it too.

### Exports

All strings are UTF-8 JSON. A status of 0 or more is the length of the output; below 0 it is the
negated length of an error text. The output is at `ti4_host_out_ptr()` for `ti4_host_out_len()`
bytes: read both again after every call (the buffer can move).

| Export | |
|---|---|
| `ti4_host_in(len) -> ptr` | Reserve the input buffer; write `len` bytes of request there. |
| `ti4_host_out_ptr()`, `ti4_host_out_len()` | The last output. |
| `ti4_host_set_time(unix_ms: f64, monotonic_ms: f64)` | Tell the engine the time. Call before `ti4_host_start` and before each resume (events carry the time of day). |
| `ti4_host_query(len)` | No game needed: `{"query":"describe"}`, `{"query":"map_preview","choice":{"kind":"random"} or {"kind":"template","alias"},"player_count","variant"?}`, `{"query":"catalog"}` (the printed cards, the JSON of `GET /api/catalog` of the server; about 410 KiB, in the output of `host_api`), `{"query":"space_odds",...SpaceOddsRequest}` (`BattleResponse`: 2000 rollouts of `ti4_calc` from a fixed seed, so the same sides give the same numbers; `in_progress: true` starts at a combat round, with no space cannon or barrage before it), `{"query":"ground_odds",...GroundOddsRequest}` (`GroundOddsResponse` of `ti4_calc::odds::ground_odds`). Reads and changes nothing of a game: ask it with no game, or while the game is parked at a question (the Worker asks the instance of the running game). Both apps ask the odds with `spaceOdds`/`groundOdds` of `web-shared/src/engine/odds.ts`. |
| `ti4_host_start(len)` | **`WebAssembly.promising`.** Plays a game: `{"setup" \| "table" \| "local": ..., "resume": Save?, "modes"?, "stepping"?, "staged"?}` (with none of the three, the game of the save). Returns when the game ends or is left; the output is an `end` envelope. |
| `ti4_host_command(len)` | A command while the game is parked (or after it ended). The output is a `command` envelope. |
| `ti4_model_load(len)` | Load an MLP model: the input holds the bytes of a `.ti4m` file. Its SHA-256 trailer is checked, it runs on the fastest deterministic backend (`simd128` in the page), and it is kept by its name in place of a model of the same name (the file is not kept twice: the input buffer is emptied). The output is `{"name","label","sha256","backend"}`, `sha256` being the SHA-256 of the whole file (`sha256sum`, the address `/api/models/<sha256>.ti4m` of a server). A game whose seat is of the kind `mlp:<name>` starts only when the model of that name is loaded. |
| `ti4_infer_check(len)` | A test of the MLP forward of the build: a request of `ti4_infer::synthetic::check` (`{"seed","width","capacity","blocks","cases":[{"seed","options","columns","faction","head"}]}`) gives `{"backend","digest","logits"}`, each logit the `u32` of its bits. `web2/src/session/inferCheck.test.ts` compares them with `fixtures/infer/synthetic_logits.json`, which the native build writes. |

The import is `host.ask() -> i32` (`WebAssembly.Suspending`).

One instance plays one game at a time (statics). Make a new instance for a new game, or end
the run (`leave`) and wait for the `ti4_host_start` promise before starting another.

### `Setup` (`host_setup::Setup`)

`seed` (number, up to 2^53), `players` (`ti4_host::setup::PLAYER_COUNTS`, 2 to 8; the form of
`web/` offers 3 to 6), `human_seat?` (0-based; left out: drawn from the seed), `human_faction?`
(id of `describe().faction_groups`; left out: drawn), `card_set?` (`te` default, `pok`,
`base_game_codex1`), `map?` (template alias, `"random"`, left out: the recommended template),
`speaker?` (`seat`, `random`), `bots?` (`smart` default, also `rule`; `random`; `mlp:<name>`), `game_id?`
(default `"local"`), `preset?` and `scenario?` (test tables). It becomes a `TableSetup` (seats in
the order of the join) and the game is `ti4_host::setup::{draw, create}`, as on the server.
`describe()` is `setup_options` (with the models that the engine loaded) with the maps, presets and engine identity added. Seats are the
ids `a` to `h`; the person is `ViewerRole::Player(<their seat>)`. The result of `resolved()` is
stored in the save, so a save is self-contained.

### Output: the envelope

```json
{ "kind": "ask" | "progress" | "end" | "command",
  "messages": [ServerMessage...],      // the shapes of crates/ti4-host/src/protocol/server.rs
  "progress": {"done","total"},        // progress: a saved game is being played again
  "waiting": "c",                      // ask: the game waits before a decision of this seat
  "resume": -1 | -2 | -3 | -4 | <option index>,  // command: what the parked game does now
  "restart": {"answers","redo","generation","version","log"?,"switches"?,"undone"?,"staged"?},  // play again with these
  "batch": {...},                      // how a submit_batch ended (ask or command)
  "save": {...},                       // command save_state
  "resume_check": {"ok",...},          // first ask after a replayed save
  "end": {"finished","left","error","decisions","answers","diverged"} }  // end
```

### Flow

1. `ti4_host_set_time`, then `ti4_host_start(config)` (do not await it yet).
2. The game runs (bots, dice) until the person must decide. It calls `host.ask`. The output is an
   `ask` envelope: forward `messages` to the UI (the same objects `GameSessionClient` gets from
   the socket; `reduceServerMessage` takes them as they are). The first `ask` of a run starts
   with an `initial_snapshot`; later ones carry `event` messages, then one `state_update` with
   `pending_choice`. Then **the import returns a promise** that stays pending.
3. While parked, call `ti4_host_command(...)` as often as needed (queries do not resume). A
   command whose envelope has `resume` is the answer: call `ti4_host_set_time`, then settle the
   pending promise with that number. `-2` is an undo, `-1` leaves.
4. A `progress` envelope (a save is being replayed) is answered at once with `-5` (`CONTINUE`);
   forward `progress` to the UI. It is sent every 20 replayed answers.
5. The game over: `ti4_host_start` settles with an `end` envelope (`game_over` is in its or the
   last `ask`'s messages). `command` still works afterwards (`save_state`, `subscribe`,
   `query_reach`).

### Commands (input of `ti4_host_command`)

`ClientMessage` JSON as the page sends it today (`protocol_version: 4`, `game_id`):
`subscribe` (a fresh `initial_snapshot`), `submit_choice` (`action_accepted` or `action_rejected`,
checked as on the server by `ti4_host::protocol::check_submission`; resumes with the index of the
option), `apply_draft` (a script; `draft_applied` when it ended),
`run_draft` (`draft_outcome`), `undo` (resumes `-2`), `query_reach` (`reach_into`),
`set_reaction_mode` (`state_update` with `reaction_modes`), `ping` (`pong`), `put_staged` (see
"Staged work"), `preview_secondary` (`secondary_preview` for the seat the page is shown as, on
the position it sees, as the server answers it: `ti4_host::preview`; `busy` once the game is
over). `set_reaction_intent` and `pass_reaction_hold` do nothing. Local commands:

- `{"type":"submit_batch","nonce","expected_version","plan": BatchPlan}`: the `BatchPlan` of
  `web/` (`tactical_movement`, `production`, `payment`, `casualties`, `tokens`,
  `agenda_vote_planets`, `ready_planets`), applied as the engine asks with the logic of the
  server's batch (`ti4_host::batch::LiveBatch`). A refused first step: `batch.status:"refused"`
  in the reply, nothing resumes. A later step that fails takes the game back to the question the
  plan began at (a checkpoint rollback; the history does not change) and the next `ask` says
  `batch.status:"refused"`. A reaction window between steps pauses it: `status:"interrupted"`
  with `remaining_steps` and `waiting`. Done: `status:"applied"`.
- `{"type":"history","action": ...}` with `"undo"`, `"undo_batch"`, `"undo_pipeline"`, `"redo"`,
  `"redo_batch"`, `"redo_pipeline"`, `{"restore_cursor":{"cursor"}}` or
  `{"restore":{"event_id"}}`. `undo*` is the checkpoint undo (resume `-2`; the next `ask` has an
  `initial_snapshot` with `history.generation + 1` and `undone` steps). The others reply
  `restart` and `resume: -1`: the page waits for `ti4_host_start` to settle and starts again with
  `resume = restart` (`{answers, redo, generation, version}`, in a new instance; `version` keeps the
  game versions rising, so a client that keeps its state does not drop the new snapshot as stale). A rewind lands on the person's
  next question at or after the cursor.
- `{"type":"save_state"}`: reply `save`.
- `{"type":"leave"}`: resume `-1`.
- `{"type":"step"}`: while the game waits before a decision of another seat, resume `-3`.
- `{"type":"switch_seat","seat"}`: resume `-4` (see "The seats of the page").
- `{"type":"stepping","mode"}`: only a flag of the play loop; nothing resumes.

`undo` (and `history` `undo*`) take back one answer, or a movement, the landings or a purchase
whole. Through the checkpoints the reply resumes `-2`; when they do not reach, or the game is
over, it replies `restart` (with `log`) and `resume: -1`, and the page plays the game again.

### Staged work

What each seat has staged and not sent (`ti4_host::staged`: the staged movement, landings or
production, the private draft of a tactical action, the draft of a secondary) is kept by
`HostSession`, in its save and in every `restart`, as the server keeps it for the seat. A page
that starts the game with `"staged": true` (web2), or subscribes with `staged`, is one
connection of the seat that it is shown as (`connection` 1):

- After each snapshot, and when the seat that the page is shown as changes (another seat of the
  page is asked), it is sent `seat_staged` with `connection`: the slots of that seat, which
  replace what the page holds.
- `put_staged {slot, base_revision, value?}` writes a slot of that seat by compare-and-set; the
  write is told back as `seat_staged` with `writer` 1, a stale one with what the seat has.
  Revisions count up for the whole page, so a write that was meant for another seat is stale.
- A draft of a secondary that is ready is sent by the session when the game asks the seat that
  secondary, and when the seat marks it ready while the game asks it (then the `put_staged`
  reply resumes the game): the draft is played on a copy of the game (`run_draft` with the
  scope of the secondary, `ti4_host::staged::ready`), it begins as a script, and its slot is
  cleared. One that no longer fits is written back with `ready: false`, and the seat is asked.
  These writes of the session have no `writer`.
- `switch_seat` clears the slots of the seat that is given up.

A page that does not ask for it (`web/`) is sent no `seat_staged`.

### Save and resume

`save` (`HostSession::save_state`) has format 2:

| Field | |
|---|---|
| `format` | `2`. A start whose `resume` has another `format` is refused: old saves are not read. |
| `seed`, `players` | The seed the seats decide from; the count, `ti4_host::setup::PLAYER_COUNTS` (2 to 8). |
| `game` | The game as it was asked for, with the choices that were left open filled in: `{"setup"}`, `{"table"}` or `{"local"}` (`host_setup::GameRequest`). |
| `table`? | The `TableSetup` the game was made from (a game of `setup` or `table`; not of `local`). |
| `scenario`? | The scenario the game starts from. |
| `humans` | The seats the page plays at the start, a mask (bit 0 is seat `a`). |
| `decisions` | The option id of every decision of the game, of every seat, in order. An undo cuts it at the answer that it takes back. |
| `answers` | The page's answers among them. |
| `switches` | The changes of the seats of the page: `{decision, humans}` each. |
| `redo`, `never`, `staged`, `event_times`, `event_versions`, `version`, `generation`, `decisions_hash` | What the run goes on with: the redo list, the Never cards, the staged work of each seat (see "Staged work"), the time and version of each event, the counters, and the hash that the replay is checked against. |
| `finished`, `round`, `phase` | Where the game is. |
| `engine` | `{build, content_digest}`. |

Resume: `ti4_host_start({"resume": save})`; the game is `save.game` (a request that names its own
game, as web2's does, plays that one). The game plays the `decisions` again without asking (a few
hundred ms for 40 answers, seconds for a late game), reporting `progress`; no seat decides one of
them again. The first `ask` then has `resume_check` (`ok` is whether the decisions are the saved
ones: false means the engine changed) and an `initial_snapshot` with the full event log, times of
day included. The page adds the hash of the wasm file to what it stores.

### What differs from the server

One `state_update` per question of the person (and one at the end), not per engine step; no
separate `pending_choice` message; undo is the engine checkpoint (32 deep), and its track has the
local rules (only a switch of the seats cuts, no barriers); redo and
rewind are by replay; free-form answers (transaction deals) are refused; the splice preview, turn
redo (`ti4_host::history`, offered only by the server) and bluff holds do not exist; the page is one connection, so the staged work of a seat is
told to it only while it is shown as that seat; a secondary preview is for the seat the page is
shown as (the server: the seat of the connection), and it has no rate limit. The opening of a
scenario is played as on the server (`ti4_host::view::scenario::OpeningRun`, asked before the
page is asked); it answers only in a new game, and only when the page plays the seat of the
opening.

### Tests

`cargo test -p ti4-wasm --test it` (names with `slow_` are the slow tier, `cargo t-web-slow`).
`it/server_parity.rs` plays the same table through `ti4-server`'s own session and compares the
decisions and the whole event log; `it/host_session.rs` checks every message against the
`ServerMessage` types and the order a client relies on; `it/host_local.rs` the local play of
web2 (several seats of the page, a change of them, stepping, a save with its decisions);
`it/host_history.rs` the history, and that a save plays again from itself alone with no seat
deciding; `it/host_staged.rs` the staged work (`put_staged`, `seat_staged`, the save) and the ready
secondary that the session sends; `it/host_preview.rs` that each secondary preview equals the
server's answer at the same position; `it/host_scenario.rs` that each scenario of three seats
opens at the same question, after the same opening, as the server's launch of it. `node js/host.mjs <wasm>` runs the same
flow in a real WebAssembly instance (CI does). `node scripts/wasm-size-gate.mjs` checks the size
against `size-budget.json`.

## Commands

```sh
rustup target add wasm32-unknown-unknown

# Build (about 2 minutes; the result is target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm)
cargo build -p ti4-wasm --target wasm32-unknown-unknown --profile wasm-release
# The same build, copied to the file of an app (web2/scripts/build-wasm.sh, web's npm run build:wasm)
node web-shared/scripts/build-wasm.mjs web2/src/session/ti4.wasm

# The same seed and step count must print the same line in wasm and natively
# (arguments: seed, steps, players)
node crates/ti4-wasm/js/run.mjs target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm 3 2000 8
cargo run -p ti4-wasm --example run -- 3 2000 8

# The hosted game (see "The hosted game"): smoke test in Node 24 and the size gate
node crates/ti4-wasm/js/host.mjs target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm
node scripts/wasm-size-gate.mjs target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm

# Where the bytes are
cargo install twiggy
cargo build -p ti4-wasm --target wasm32-unknown-unknown --profile wasm-release \
  --config 'profile.wasm-release.strip=false' --target-dir /tmp/ti4-wasm-names
twiggy top -n 40 /tmp/ti4-wasm-names/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm
```
