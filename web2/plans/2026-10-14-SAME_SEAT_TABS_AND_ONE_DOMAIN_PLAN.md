# One seat in several tabs (web2), and web + web2 on one domain

## Status / changes since 2026-10-14 07:08 UTC

Updated 2026-10-14 16:45 Berlin, against `dev` at `6b5a36da`.

- **Phase 1 is done** and merged: `6b5a36da Merge origin/seat-staged-sync: staged work for each
  seat (phase 1)`, one commit `86406f6b server: staged work for each seat, the same in every tab`
  on branch `seat-staged-sync`. What was built, and where it differs from the plan, is in Phase 1
  below.
- **Phase 2 is done** (2026-10-14, on `dev` after `7bef1db4`): web2 keeps the staged work of an
  online seat on the server. What was built and the deviations are in Phase 2 below.
- **Phase 3 is done** (2026-10-14, on `dev` after `86cfd5cc`): the server sends a ready draft of a
  secondary, and a second answer of a seat is quiet. What was built and the deviations are in
  Phase 3 below. Phase 4 is not started.
- **Felix's work was merged** (`056ae71a Merge remote-tracking branch 'felix/main' into dev`),
  with the offline single player of web (`5381af51 Merge offline-final ...`). What matters here:
  - **The protocol moved to a new crate.** `crates/ti4-server/src/protocol/*` is now
    `crates/ti4-host/src/protocol/*`; `ti4-server` re-exports it as `crate::protocol`. Also moved
    into `ti4-host`: `session/{batch,replay,rng_force,step_snapshot}.rs`, the setup
    (`ti4_view::setup` → `ti4_host::setup`), the map catalogue. `RejectionReason` is in
    `crates/ti4-view/src/status.rs` (re-exported by `ti4_host::protocol::status`).
  - **The wasm host speaks the server protocol.** `crates/ti4-wasm/src/host_session.rs`
    (`HostSession`) takes `ClientMessage` JSON and sends `ServerMessage` JSON for web's offline
    game. Every new client message must be handled there too (phase 1 ignores `PutStaged` and
    `staged`).
  - **Deploy changed** (`deploy/public/`): `deploy.sh` builds a ref (`--ref`, default
    `origin/main`) in a temporary worktree, builds web's wasm engine (`web/scripts/build-wasm.mjs`,
    `TI4_REQUIRE_WASM`), writes `.wasm.br`/`.wasm.gz` next to each wasm of `web/dist`, runs a
    persistence pre-flight on a copy of the data (`recovery-check.sh`), and warns when the live
    Caddy CSP differs from the repo's. New `check-headers.sh` checks CSP, wasm, cache and
    compression headers. `rust-toolchain.toml` lists the `wasm32-unknown-unknown` target.
  - **The Caddyfile and the server changed for wasm.** The CSP has `script-src 'self'
    'wasm-unsafe-eval'; worker-src 'self'`; `spa_fallback` serves precompressed `.br`/`.gz` and
    sends `Vary: Accept-Encoding`.
  - **web keeps more in the browser.** `ti4.my-games` (localStorage: game id, token, player id,
    nickname), `ti4.layout.v1`, `ti4.map-picker-seen:<id>`, and an IndexedDB database
    `ti4-offline` for offline saves. web keeps its player session in **sessionStorage**
    (`ti4.player-session:<id>`), not localStorage; that was so before, and the plan had it wrong.
  - New server pieces: `POST /api/games/{id}/terminate`, socket close code 4002 "game terminated",
    `QueryReach`/`ReachInto`, per-IP `RateLimits::from_env`.
- **web2 since then:** the odds of a space combat (`Transport.odds`, `createWsTransport` takes the
  engine's `module`) and "a click on a system shows what is in it". No change to the staged work,
  the keys or the transport's subscription.
- Line references below are updated to `dev` at `6b5a36da`.

## Context

**Goal 1.** A player opens the same seat of an online game in two tabs or on two devices
(desktop and phone). Both tabs show the live game. Both tabs also show the same staged work,
live. Today `web2/AGENTS.md` ("Online play") says: "A duplicated tab is the same player twice.
The server allows it ..., both tabs write the same staged work, and the page does not guard
against it."

**Goal 2.** web and web2 run against one server process, on one domain, behind the Caddy on
this host. Today they are two subdomains, `ti4.*` → :3000 and `web2.*` → :3100 (vite dev),
and both proxy `/api` and `/ws` to `:8080`.

**What exists already.** The server accepts several sockets per seat:
- `connect_player` (`crates/ti4-server/src/session/registry.rs:2256`) keeps a map of
  connections for each seat.
- `GameSession::subscribe` (`session/mod.rs:465`) and the planning runner `subscribe`
  (`planning/runner.rs:257`) have one subscriber for each connection.
- Updates, reaction modes and turn status already reach every tab of a seat.
- Since phase 1: the server keeps the staged work of each seat and tells every opted-in
  connection of the seat (Phase 1 below), and every connection of the seat gets the undone steps.

**What breaks with two tabs of one seat:**
1. **Staged work is per tab.** This is the staged movement, landings and production; the
   private tactical draft with its undo history; and the secondary draft. `useLiveSession`
   reads it from `localStorage` once, at mount (`readPlanOf`, `readFollowOf`,
   `useLiveSession.ts:131-146`, `366-384`), and writes it on every change
   (`useLiveSession.ts:576-624`). The keys come from `onlineKeys(gameId, playerId)`
   (`OnlineApp.tsx:118`). So two tabs overwrite each other's work and never see the other's
   changes. Another device has none of it. *The server side is fixed by phase 1; the page still
   does this until phase 2.*
2. **The undone steps are told once.** *Fixed by phase 1:* `GameSession::undone_for` keeps them,
   and every first snapshot of a connection of the seat has them until the seat decides again.
3. **A ready secondary is sent twice.** The page sends it when the game asks the seat
   (`useLiveSession.ts:703-722`). Both tabs send it, and one of them gets a "stale nonce" toast.
   The same happens when an answer is clicked in both tabs.

## Decisions (owner, 2026-10-14)

- **Staged work is synced through the server:** the draft, the staged
  movement/landings/production and the secondary draft. UI state stays per tab: the open
  sheet, the pane, the open step of the draft (`plan.open`, `stage`).
- **Layout of the domain:** web at `/`, web2 at `/web2/`, and `/api` and `/ws` go to the one
  server.

## Phase 1 — Server: staged work for each seat — DONE

Built in `86406f6b` (branch `seat-staged-sync`), merged to `dev` in `6b5a36da` on 2026-10-14.
The merge moved the wire types to `ti4-host`, where Felix's merge had put the protocol.

**As built.**
- **Protocol** (`crates/ti4-host/src/protocol/`): additive, opt-in, `PROTOCOL_VERSION` stays 3.
  - `ClientMessage::Subscribe { staged: bool }` (`client.rs`; `#[serde(default)]`, left out
    when false).
  - `ClientMessage::PutStaged { protocol_version, game_id, slot, base_revision: u64, value:
    Option<Value> }`. `slot` is `StagedSlot`: `"movement" | "plan" | "secondary"`
    (`staged.rs`). A missing `value` clears the slot. `validate_bounds` refuses a value over
    `MAX_STAGED_VALUE_BYTES` (48 KiB as JSON) with the field `"value"`.
  - `ServerMessage::SeatStaged(SeatStagedMsg { protocol_version, game_id, revision, slots:
    {slot: {revision, value?}}, writer?: u64, connection?: u64 })` (`staged.rs`, `server.rs`).
  - Revisions count up for the whole seat: the newest message is the one with the highest
    `revision`. The message has no game version on purpose, so a stale-version filter never
    drops it.
- **State** (`crates/ti4-server/src/session/staged.rs`): `SeatStaged { revision, slots }`, and
  `StagedBook(Arc<Mutex<BTreeMap<PlayerId, SeatStaged>>>)` in `SessionConfig.staged`. A session
  that replaces another (undo, redo, batch) gets a clone of `initial_config`, so the sessions
  **share** one book. `SeatStaged::put` is the compare-and-set: `Stale` (base is not the slot's
  revision), `Unchanged` (same value: nothing written, nothing sent), `Written`.
- **Persistence** (`storage.rs`): `StagedRecord { seats }` in `staged.json`
  (`save_staged`/`load_staged`, bounded by `MAX_SNAPSHOT_BYTES`), loaded on recovery. Each
  write saves the whole book of the game. A failed save rolls the write back and is an
  `EngineError` to the writer.
- **Session** (`session/mod.rs`): `GameSession::seat_staged(seat)` and `put_staged(seat, slot,
  base, value, writer)`. A written put is sent as `SeatStaged` with `writer` through the
  existing subscriber channel of the session, to every subscriber whose viewer is that seat,
  under the lock of the book (so the order of writes holds).
- **ws handler** (`crates/ti4-server/src/ws/mod.rs`):
  - After the `InitialSnapshot` of a player connection with `staged: true`, it sends
    `SeatStaged` with `connection` set to the connection's own id (`ws/mod.rs:626-631`).
  - The update bridge drops `SeatStaged` for a connection that did not opt in.
  - `PutStaged` only for `ViewerRole::Player`; the seat is the connection's. Others get
    `Error { kind: Unauthorized }`.
  - Rate limit: 20 puts in a sliding window of 1 s per connection (`MAX_STAGED_PUTS`,
    `STAGED_WINDOW`).
  - A stale put is answered with `SeatStaged` as it is (no `writer`, no `connection`). A refused
    put (rate, save failure, game id) is an `Error` followed by `SeatStaged` as it is.
- **Undone steps:** `take_undone` is now `GameSession::undone_for` (it does not remove).
  `SessionShared.undone_from` is the length of the log that the undo left;
  `forget_undone_of_deciders` (`session/worker.rs`) drops the steps of a seat that has a
  decision after it.
- **Wasm host:** `HostSession` ignores `PutStaged` and `Subscribe::staged`; an offline game never
  gets `SeatStaged`.
- **Tests:** `crates/ti4-server/tests/it/staged.rs`
  (`two_tabs_of_a_seat_share_what_is_staged_and_nobody_else_sees_it`,
  `what_is_staged_and_what_was_undone_outlive_an_undo`), `protocol_roundtrip.rs`
  (`staged_work_is_asked_for_and_written_in_the_form_that_web2_sends`), `undo.rs` (every
  subscription after an undo gets `undone`), the unit test in `session/staged.rs`. Covered: put
  on A reaches B with the writer; stale base refused and answered; a tab that did not opt in,
  a spectator and another seat get nothing; a spectator cannot write; oversized value refused;
  a late tab gets everything; restart (recovery) keeps the slots; undo keeps the slots and a
  write to the replaced session is not lost.

**Deviations from the plan.**
- The wire types are in `ti4-host` (`protocol/staged.rs`), not `ti4-server/src/protocol`.
- `SeatStagedMsg.connection` is new: the message after the snapshot tells the connection its
  own id, which is the `writer` of its writes. `writer` is absent after the snapshot and in the
  answer to a stale or refused put.
- The book is shared through an `Arc`, not copied, across session replacements.
- The undone steps are not stored as `(session generation, steps)`: they stay in `undone` and
  are dropped when the seat appears in the decision log after `undone_from`.
- A put of an unchanged value is taken and **sends nothing, not even the echo**.
- Refused puts reuse `ErrorKind::MalformedMessage` ("too many staged writes; wait a moment",
  oversized `"value"`), not a dedicated kind.

**Gaps left.**
- No test of the rate limit, and none of the carry-over across a redo or a batch (only undo).
- "Both sockets get `undone`" is tested as two subscriptions one after the other, not two open
  sockets at once.
- The `secondary` value in `protocol_roundtrip.rs` (`{"answers": ["follow"], "ready": true}`)
  is not web2's `SecondaryDraft` shape (`answers: [{id, label}]`); fix it when phase 3 makes the
  server read the slot.
- The server never clears a slot (by design: the page drops a value of an old nonce or an old
  action). Nothing removes `staged.json` but the removal of the game's directory.
- web2's `wire.ts` has no types for any of it yet (phase 2).

## Phase 2 — web2: a seat store in place of `localStorage` for online games — DONE

**As built** (2026-10-14).
- `web2/src/session/seatStore.ts`: `SeatStore { get, set, subscribe }`; `localSeatStore(storage,
  keys)` for local games and single player; `seatSync(put, legacy)`, the online store, which
  `createWsTransport` holds and exposes as `Transport.staged` for a player (not for a
  spectator). `subscribe` tells only a change of another connection, as `(slot, value)`.
- Wire types in `wire.ts`: `StagedSlot`, `StagedValue`, `SeatStagedMsg` (`seat_staged`),
  `put_staged`, `subscribe.staged`. The transport subscribes with `staged: true` as a player.
- `useLiveSession(transport, store?: SeatStore)`; `DraftStore` is gone. `OnlineApp` passes
  `transport.staged`; `LocalApp` a `localSeatStore` over `draftKey`/`planKey`/`secondaryKey`.
  `draft.ts` and `secondaryDraft.ts` have `parseDraft(value)` in place of `readDraft`/`storeDraft`;
  `draft.ts` has `trimDraft` (oldest undo states first, then redo states).
- Migration: `legacyStaged(storage, gameId, playerId)` in `onlineKeys.ts`, given to the transport
  as `OnlineGame.legacyStaged`.
- Tests: `wsTransport.test.ts` (the echo is the base, one write on its way, key order, clear;
  another tab over a stale write; a refused write; a lost connection; migration),
  `transportContract.test.ts` (server only: a write reaches the second and a later connection
  of the seat), `liveSecondary.test.tsx` (a remote secondary draft shows in the row and is not
  written back), `draft.test.ts` (`trimDraft`), e2e `online.spec.ts` "the same seat in two tabs".

**Deviations and details.**
- Values are compared with sorted keys (`sameStaged`): the server keeps a value as a
  `serde_json::Value` without `preserve_order`, so its echo has sorted keys. Without it the store
  would put an "equal" value, get no answer, and wait.
- The hook writes a slot only when this tab's own value changed since its last look (`shown` in
  `useLiveSession`), not whenever it differs from the store: an effect of a render before a remote
  change would write the old value back.
- The movement slot is cleared only when this tab takes back what it staged for the open choice.
  A value of another choice stays in the slot and is not read (it carries its nonce). The hook no
  longer clears the slot on every update without staged steps: a tab that is behind would clear
  what another tab staged for the next choice.
- A remote plan of `null` closes the Draft workspace of the tab; a draft with no system resets
  the step in view. `draftView` already falls back when the step is no longer reached.
- What waits for the debounce is written at once on `pagehide` and when the tab is hidden: the
  e2e of a reload staged and reloaded inside 250 ms and lost the draft.
- A write with no answer in 5 s is given up. An `error` while a write is on its way is told as
  "The server did not keep what you staged. It stays in this tab." and the value is not tried
  again until the page changes it (no loop on a failing save).
- What a page of before kept is dropped on the first `seat_staged` also when the server has
  something for the seat (it is put only when the server has nothing).

**Gaps left.**
- No client test of the rate limit (the debounce keeps a slot under 4 writes a second) and no unit
  test of the flush on `pagehide` (the e2e reload covers it).
- Two tabs can still both send the same answer ("Move fleet", a choice): phase 3.
- `e2e/online.spec.ts` "a lost connection …" timed out once on its first load in one full run;
  it passed 38 times after it, and 5 times on the code before.

### The plan of phase 2

- **Wire types** in `web2/src/session/wire.ts`: `staged?: boolean` on the `subscribe` request,
  a `put_staged` request, a `seat_staged` server message (`StagedSlot`, `StagedValue`,
  `writer?`, `connection?`). The doc comments name `ti4_host::protocol::staged`.
- **New seam** in `web2/src/session/`: `SeatStore`, with
  `get(slot)`, `set(slot, value)` and `subscribe(listener)`.
  - `localSeatStore(storage, keys)` wraps the `localStorage` code of today (`readPlan`,
    `readFollow`, `readDraft`, `storeDraft` of `draft.ts`, `secondaryDraft.ts` and the movement
    effect). Local games (`LocalApp.tsx`, `SingleApp.tsx`, keys from `savedGame.ts`:
    `draftKey`, `planKey`, `secondaryKey`) keep using it unchanged.
  - The online store is part of `createWsTransport` (`wsTransport.ts`). It sends
    `staged: true` with `subscribe` (`wsTransport.ts:320-326`), keeps the slots and revisions,
    and debounces puts (about 250 ms).
  - **Its own writes, as phase 1 built it:**
    - It learns its id from `connection` of the first `seat_staged` after the snapshot. Until
      then it does not put.
    - The echo of its own put (`writer` = its id) carries the new revision of the slot: take the
      revision, keep the value.
    - One put in flight for each slot: the next value waits for the echo (or the stale answer),
      then goes on the new base. Otherwise the second put is stale.
    - Do not put a value equal to the known value: the server answers an unchanged put with
      nothing, and the queue would wait for an echo that never comes.
    - A `seat_staged` with a higher `revision` and another or no `writer` is the server's value:
      it replaces the slot, also a value not yet sent (last write wins only between equal
      bases).
  - **Errors.** A refused put arrives as a generic `error` (`kind` `malformed_message`) and is
    shown today as a toast (`wsTransport.ts:270-279`). The store must keep its puts under the
    bounds (size, rate) so that this does not happen, and the toast must not name "value".
  - It is exposed as an optional `Transport.staged`, so `game/`, `ui/` and `useLiveSession`
    still do not ask which transport they have. `wasmTransport.ts` has none.
- **Replace `DraftStore`.** `DraftStore` (`useLiveSession.ts:106`: storage + `key` +
  `planKey` + `secondaryKey`) becomes a `SeatStore`. `OnlineApp.tsx:113-125` passes
  `transport.staged` in place of `onlineKeys(...)` + `localStorage`.
- **React to the other tab** (the main client change). `useLiveSession` subscribes to the
  store. A change written by another tab replaces:
  - `plan.draft`: keep this tab's `open` and `stage`, and clamp `stage` when the draft
    shrank;
  - `follow` (the secondary);
  - the staged `local.movement`, `local.landing` or `local.production`, only when the saved
    nonce equals the open choice's nonce.

  Compare with `same()` (`useLiveSession.ts:129`) before setting, so an applied remote value
  does not write back (no echo loop).
- **Migration.** On the first `seat_staged`, if the server's slots are empty and the old
  `onlineKeys` entries exist, put them once and then remove the local keys. This sits beside
  `moveLegacy` in `onlineKeys.ts:75`.
- **Trim the plan history** when the serialized draft is over the size bound (48 KiB). Drop the
  oldest undo states first (`Draft.history`/`cursor` in `draft.ts`).
- **Tests:**
  - A unit test of the online store with a fake socket, in the style of `wsTransport.test.ts`:
    the echo sets the revision; one put in flight; a stale answer replaces the value.
  - A server case for `SeatStore`, added to `transportContract.test.ts`: a value set on one
    connection reaches a second connection of the seat. (Needs `cargo b-web` first, see
    Verification.)
  - A dom test: a remote change of the secondary draft shows in the seat row
    (`liveSecondary.test.tsx` style).
  - e2e in `e2e/online.spec.ts`: two tabs with the same `?online=<id>&seat=<p>`. Tab A
    stages a movement, and tab B shows the same tokens and check marks. Tab A edits the
    draft, and tab B's Draft tab follows.
- **Update `web2/AGENTS.md`, "Online play".** Replace the "duplicated tab" bullet: staged work
  is kept by the server for each seat and is the same in every tab and device; what is open
  on the screen is per tab. Also the bullet "A seat for each tab": what is staged is no longer
  kept under `onlineKeys`.

## Phase 3 — One sender for answers of a seat — DONE

**As built** (2026-10-14).
- **The Rust type of the slot** (`session/staged.rs`): `SecondaryDraft { round, primary, card,
  answers: [DraftAnswer { id }], ready }`, read with `SecondaryDraft::of(value)`; other fields and
  the labels are ignored. `script()` makes the `DraftStep`s. Pinned by the unit test
  `a_draft_of_a_secondary_is_read_as_web2_writes_it`, and `protocol_roundtrip.rs` now puts web2's
  shape (`answers: [{id, label}]`, the gap of phase 1).
- **The sender** (`session/ready_secondary.rs`): `ready_draft` (the slot is ready, has answers, and
  is of the round, the primary and the card that the choice asks: `details.kind ==
  "strategy_secondary"`, `played_by`, `card`), `check` (`run_draft_on` with the scope; it fits when
  every answer is played and the stop is `Complete` or `Stopped`, as `isOver` of web2), `unready`
  (writes the value back with `ready: false`) and `spent` (clears the slot), both through
  `staged::put_and_tell` with no writer.
  - **When the game asks the seat**: the decider of the seat (`decider.rs`) reserves the choice
    before it broadcasts it, checks the draft with no lock held, and answers the first question
    itself; the rest is a script (`begin_script`), as `apply_draft` does. The seat is not asked
    (nothing is broadcast). A draft that does not fit is made not ready, and the seat is asked.
  - **When a tab marks the draft ready while the game asks the seat**: a written put of slot
    `secondary` runs `GameRegistry::send_ready_secondary` (`spawn_blocking`, under the game gate,
    for the current session only), which calls `GameSession::send_ready_secondary` →
    `apply_draft`.
- **Refactors**: `GameSession::put_staged` is `staged::put_and_tell` with `Some(writer)`;
  `GameSession::run_draft` is `run_draft_on(&shared, …)` with the failed outcome around it.
  `SessionShared` has the book (`staged`, a clone of the config's) and `answered`.
- **A double answer**: `SessionShared.answered` keeps the newest 16 nonces of each seat, noted
  when a choice is reserved (`reserve_choice`, `apply_draft`, the decider's send) and forgotten when
  the reservation is released. The ws handler keeps the nonces that the connection itself sent.
  A refused `submit_choice` or `apply_draft` whose nonce the seat answered, and not from this
  connection, is `RejectionReason::AnsweredElsewhere` (`ti4-view/src/status.rs`) for a connection
  with `staged: true`, and `StaleNonce` for any other (`answered_elsewhere` in `ws/mod.rs`).
- **web2**: `answered_elsewhere` in `wire.ts`; `wsTransport` shows no toast for it and schedules
  no resync. `useLiveSession` does not send a ready draft when `transport.staged` exists
  (`serverSends`), and does not hold the live question back then. `SeatStore.subscribe` has a
  third argument `server` (the message has no `writer` and no `connection`); a server write of the
  secondary with `ready: false` sets the note "Your draft no longer fits the game. It is not
  ready." in every tab.
- **Docs**: `web2/AGENTS.md` (Online play: "One sender for the answers of a seat", "A ready draft
  of a secondary is sent by the server"; Strategy cards), the doc comments of
  `protocol/staged.rs` and `PutStaged`.
- **Tests**: `crates/ti4-server/tests/it/ready_secondary.rs`: sent when the seat is reached (the
  slot is cleared); a draft that does not fit is made not ready and the seat is asked; marked
  ready while asked (session and over a socket); two sockets mark it ready and it is sent once;
  `answered_elsewhere` for an opted-in socket, `stale_nonce` for one that did not opt in, and
  not for the socket that answered. web2: `wsTransport.test.ts` (no toast, no resync),
  `liveSecondary.test.tsx` (with `staged` the page sends nothing and shows the note of a server
  write), e2e `lobby.spec.ts` "a ready secondary of a seat in two tabs is sent once, by the
  server, with no toast" (the host's action ends with no frame `submit_choice`/`apply_draft` from
  the two tabs of Blair).

**Deviations from the plan.**
- The seat is reached in the decider, not by a check after the broadcast: the server answers
  before the seat is asked, as the scripts of the server do. So the page needs no "holding".
- **The server clears the slot after it sent the draft** (phase 1 said that the server never
  clears a slot). Every tab drops the draft at once, and a draft of an old action does not stay
  when no tab is open.
- Answers of web's planning (`ApplyPlanning`, `SubmitPlanningChoice`) are not mapped: web does not
  opt in, and its planning messages answer with `PlanningResult`, not `ActionRejected`.
- A connection that did not opt in is now told `stale_nonce` for an answered nonce also where it
  was told `unauthorized_seat` or `no_pending_choice` before (the game went on to another seat, or
  the answer was still on its way).
- Not sent: while a change of the history runs (`history_pause`), and when the game cannot be
  copied (`run_draft_on` is `None`). The seat is then asked with its draft still ready; the page
  shows the question.

**Gaps left.**
- web (`web/`) was not run: it does not opt in and its messages did not change.
- The e2e of phase 3 and the older "a secondary is drafted in one tab while a rule bot takes its
  time" fail now and then (about 2 in 10 here, 3 in 8 for the older one also without phase 3): a
  rule bot of the late-game scenario (Sardakk) sometimes reacts when the strategic action begins,
  and the action of the host ends with no secondary. The whole `npm run e2e` passed once.
- No test of `history_pause` or of a copy that cannot be made.

### The plan of phase 3

- **The server sends a ready secondary.**
  - When a put of slot `secondary` has `ready: true`, and the game then asks that seat the
    secondary, the server runs the script and applies it. This is the same check the page does
    today: `run_player_draft` (`registry.rs:2957`), then `apply_player_draft`
    (`registry.rs:2936`) if the whole script still fits.
  - The server must read the value of the slot, which phase 1 keeps opaque. The value is web2's
    `SecondaryDraft` (`web2/src/session/secondaryDraft.ts`): `{ round, primary, card, answers:
    [{ id, label }], ready }`. Give it a Rust type next to `session/staged.rs` that reads only
    `round`, `primary`, `card`, `answers[].id` and `ready`, and pin the shape with a test
    against a value that web2 writes.
  - If the script no longer fits, the server clears `ready` in the slot, through `put_staged`
    with no client writer, so every tab gets it. Every tab then shows "Your draft no longer
    fits the game. It is not ready."
  - The page no longer sends a ready draft itself (`useLiveSession.ts:703-722`).
  - This also removes the limit in AGENTS.md that "the page must be open when the seat is
    reached".
  - The wasm host (`HostSession`) keeps no slots: a local or offline game keeps the page's
    sending. Only `wsTransport` drops it, so the page asks the transport whether the server
    sends (`Transport.staged`).
- **A double answer is quiet.**
  - Case: an answer, plan or apply arrives with a nonce that this seat has already answered,
    from another connection of the seat.
  - The server then rejects it with a new reason `answered_elsewhere`, in `RejectionReason`
    (`crates/ti4-view/src/status.rs`) and in the TS mirror (`wire.ts`).
  - web2 shows no error toast for it (`wsTransport.ts` `rejection()`, line 64, and the
    `action_rejected` case, line 250, which also schedules a resync). The update that follows
    already shows the answer.
  - **web was checked:** `web/src/protocol/decode.ts` accepts any `reason` record, but
    `rejectionMessage` (`web/src/protocol/client.ts:315`) has no default case and would show
    `undefined`. So send `stale_nonce` to every connection that did not opt in (`staged: false`:
    web, the bot agent), and `answered_elsewhere` only to opted-in ones.
- **Tests:**
  - A server test: two sockets, both mark ready, the secondary is applied once.
  - A server test: a second answer to an answered nonce is `answered_elsewhere` for an
    opted-in socket and `stale_nonce` for one that did not opt in.
  - An e2e test: two tabs, a ready secondary, no toast in either tab.

## Phase 4 — web2 under `/web2/` on the same origin as web

**web2 build and dev:**
- In `web2/vite.config.ts`, set `base: "/web2/"`.
- Add the new host to `allowedHosts` (today only `web2.beckermann-zibert.com`). Keep the
  `CDN-Cache-Control` header and `cacheDir` while Cloudflare is in front.
- Keep the proxy for local development without Caddy.
- `/api` and `/ws` stay absolute root paths (`socket.ts` `pageSocket`, `fetch("/api/…")` in
  `wsTransport.ts` and `table.ts`), so they need no change.
- The engine (`LocalApp.tsx:41`, `new URL("../session/ti4.wasm", import.meta.url)`) is
  emitted under the base by vite: no change.

**Links that assume `/`.** Grep for `href: "/"`, `"/?`, `` `/${ ``, `location.assign`,
`location.href =`, and `location.origin`. Use `import.meta.env.BASE_URL`. Found on `dev`:
- The "Start" links: `OnlineApp.tsx:36` and `SingleApp.tsx:23` (`START`).
- The addresses built in `onlineStart.ts` (`onlineAddress`, `tableAddress`) and their users:
  `StartApp.tsx` (`` `/${onlineAddress()}` ``, `singlePlayer`), `defaultGame.ts`
  (`SINGLE_PLAYER`), the copy link `` `${location.origin}/${tableAddress(gameId)}` ``
  (`OnlineApp.tsx:362`).
- `LocalApp.tsx:66` (`"/?example=live-picker"`), and the `other.href` of `Toolbar.tsx` and
  `LobbyScreen.tsx`.
- `index.html`: `/manifest.webmanifest`, `/icon.svg`, `/icon-192.png`. Check that vite
  prefixes them with the base in the build.
- `public/manifest.webmanifest`: `start_url` and `scope` become `/web2/`; the icon paths
  (`/icon-*.png`, `/icon.svg`) too.
- `web2/playwright.config.ts`: `baseURL` (line 64) and the `webServer` URL.

**Server serves both clients.**
- Add an optional `TI4_WEB2_DIR` in `crates/ti4-server/src/public/mod.rs` (`PublicConfig`,
  next to `web_dir` / `TI4_WEB_DIR`, line 74).
- In `http/mod.rs`, route `/web2/*` to a second `ServeDir` with its own `index.html`
  fallback, before the `spa_fallback` of web (`http/mod.rs:164`). Factor the serving out of
  `spa_fallback` (`http/mod.rs:204`) and keep what it does now: `precompressed_br()` and
  `precompressed_gzip()`, `Vary: Accept-Encoding`, and the cache header. The cache paths become
  `/web2/assets/` immutable and the rest `no-cache`.
- `/web2` without the slash redirects to `/web2/`.

**Public mode** (only if the public server will serve web2; this was open question 1 of
`plans/2026-10-12-LOBBY_PLAN.md`):
- web2 needs the password screen. It uses `GET`/`POST /api/access`, as web does; reuse its
  flow.
- The public server must refuse `&seed=` and the scenarios. The server already refuses the
  dev routes.
- Check that the access cookie's `Path` is `/`, so both clients share the login.
- The CSP of `deploy/public/Caddyfile` already allows what web2 needs for its engine
  (`'wasm-unsafe-eval'`; web2 compiles `ti4.wasm` in the page, not in a worker). Check that the
  web2 build has no inline script and no font or image from another origin.

**Caddy on this host, dev form.** One site, for example `ti4.beckermann-zibert.com`:

```
ti4.beckermann-zibert.com {
	@server path /api/* /ws/* /health
	reverse_proxy @server localhost:8080 { transport http { read_timeout 0 } }
	handle /web2* { reverse_proxy localhost:3100 }   # vite, base /web2/ (HMR goes through too)
	handle       { reverse_proxy localhost:3000 }     # web vite
}
web2.beckermann-zibert.com { redir https://ti4.beckermann-zibert.com/web2{uri} }
```

- web's offline game needs the same CSP on this site as on the public one, if the dev site sets
  a CSP at all: `script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'`.

**Production form** (`deploy/public/`): the server serves `/`, `/web2/`, `/api` and `/ws`,
and Caddy keeps its single `reverse_proxy` as in `deploy/public/Caddyfile`.
- `deploy.sh` (section "3. Build") also builds web2 in its temporary worktree:
  `npm ci && npm run build` in `web2/`, after web.
  - web2 needs `web2/src/session/ti4.wasm`, which is not in the repository. It is the same
    crate and profile that `web/scripts/build-wasm.mjs` builds (`ti4-wasm`, `wasm-release`), so
    copy `$CARGO_TARGET_DIR/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm` and do not build
    it twice. `web2/scripts/build-wasm.sh` reads `target/...` and ignores `CARGO_TARGET_DIR`,
    which `deploy.sh` sets: fix the script or do not use it there.
  - Write `.br`/`.gz` next to each `.wasm` of `web2/dist` too (the step "Compressing wasm"
    only looks at `web/dist`).
  - Install `web2/dist` as `$RELEASE/dist2`.
- `env.example` gets `TI4_WEB2_DIR=/opt/ti4-public/current/dist2`.
- `check-headers.sh` checks `/web2/`, a `/web2/?online=…` fallback, `/web2/assets/*` and the
  web2 wasm (cache, compression), as it does for `/`.
- `recovery-check.sh` needs no change (it does not serve the clients).
- Update the README diagram and `docs/OFFLINE.md` if it names the paths of the server.

**Shared storage on one origin.** The two clients do not clash:
- web uses sessionStorage `ti4.player-session:<id>`; localStorage `ti4.my-games`,
  `ti4.layout.v1`, `ti4.map-picker-seen:<id>` and `ti4_*`; IndexedDB `ti4-offline`.
- web2 uses `ti4.online.*`, `ti4.single`, `ti4.local.*`, `ti4.players`, …
- `ti4.nickname` is shared on purpose.
- The two share the localStorage quota of the origin (about 5 MB). web2 keeps its saved games
  there (`savedGame.ts`); after phase 2 the online staged work is on the server.

**Optional: one seat in web and in web2.**
- web keeps its player session in sessionStorage, per tab: a web2 tab cannot read it.
  `ti4.my-games` (localStorage, `web/src/protocol/myGames.ts`) has `gameId`, `token` and
  `playerId` of each game of the browser. web2 can copy a player from there into
  `ti4.online.<id>.players` (`keepPlayer`) and does not move it, so web keeps working.
- A link "Open in web2" in web and "Open in web" in web2 gives a web tab and a web2 tab of
  the same seat. web opens a game at `/games/<id>` with the token in sessionStorage, so "Open in
  web" must put it there before it loads.
- web does not opt in to staged sync, so its staged work stays its own.

## Critical files

**Server:**
- `crates/ti4-server/src/ws/mod.rs`
- `crates/ti4-host/src/protocol/{client,server,staged,status}.rs`,
  `crates/ti4-view/src/status.rs` (`RejectionReason`)
- `crates/ti4-server/src/session/{mod.rs,staged.rs,worker.rs,registry.rs}` (staged state,
  undone, carry-over; phase 3: the server's sending of a ready secondary)
- `crates/ti4-server/src/storage.rs`
- `crates/ti4-server/src/http/mod.rs`
- `crates/ti4-server/src/public/mod.rs`
- `crates/ti4-wasm/src/host_session.rs` (every new client message)
- `crates/ti4-server/tests/it/{staged,protocol_roundtrip,undo}.rs`

**web2:**
- `web2/src/session/{wsTransport.ts,wire.ts,transport.ts,useLiveSession.ts,onlineKeys.ts,
  draft.ts,secondaryDraft.ts,transportContract.test.ts}`
- `web2/src/dev/{OnlineApp.tsx,LocalApp.tsx,SingleApp.tsx,onlineStart.ts,StartApp.tsx,
  defaultGame.ts}`, `web2/src/game/shell/Toolbar.tsx`, `web2/src/game/lobby/LobbyScreen.tsx`
- `web2/vite.config.ts`, `web2/index.html`, `web2/public/manifest.webmanifest`,
  `web2/playwright.config.ts`, `web2/scripts/build-wasm.sh`
- `web2/AGENTS.md`

**web** (phase 3 check, optional link of phase 4):
- `web/src/protocol/{client.ts,decode.ts,myGames.ts}`, `web/src/App.tsx`

**Deploy:**
- `deploy/public/{Caddyfile,deploy.sh,check-headers.sh,env.example,README.md}`
- The live Caddyfile: the owner edits this; nothing is installed by the repo. `deploy.sh`
  compares its CSP with the repo's (`TI4_LIVE_CADDYFILE`, default `/etc/caddy/Caddyfile`).

## Verification

- **Rust** (gates from `web2/AGENTS.md`):
  - `cargo fmt --all -- --check`
  - `cargo l-web`
  - `cargo t-web staged::`, then the whole `cargo t-web`
  - `cargo b-web`
  - `cargo t-web-slow` before the push
  - A change of `ti4-host`'s protocol also builds `ti4-wasm`: `scripts/build-wasm.sh` (web2)
    and the wasm tests of `crates/ti4-wasm` (`tests/it/server_parity.rs`).
- **web2:**
  - `npm run typecheck`, `npm run lint`
  - `npm test`, which includes the transport suite against `target/debug/server`
  - `npm run e2e -- -g "same seat"`, then the whole `npm run e2e`
- **web** (phase 3, and the optional link): `npm test` and the e2e of a game online; the
  offline e2e (`npm run test:e2e:offline`) when `HostSession` changed. A visible change in web
  ships the screenshots of `CLAUDE.md` ("UI work").
- **Manually on this host:**
  - Start `target/debug/server`, the web vite (:3000) and the web2 vite (:3100, base
    `/web2/`), and apply the Caddy snippet.
  - Open `https://ti4.beckermann-zibert.com/web2/?online&fill=random` on desktop, and open the
    same `?online=<id>&seat=<p>` on the phone.
  - Stage a movement on one device and see it on the other.
  - Mark a secondary ready on both devices and see it applied once, with no toast.
  - Undo on one device; both devices restage the undone steps.
  - Open `https://ti4.beckermann-zibert.com/` (web) in another tab and check that it still
    works against the same server, and that its "Play offline" still starts.
- **Production form:** set `TI4_WEB_DIR` and `TI4_WEB2_DIR` locally and check that `/`,
  `/web2/`, `/web2/?online=…` (SPA fallback), `/web2/assets/*` and the web2 wasm (`.br`)
  serve with the right cache headers: `deploy/public/check-headers.sh --bin <server>`. Then
  `deploy.sh --ref <branch> --preflight-only` before the real deploy.

## Open questions (2026-10-14)

- Where is the Caddy of this host configured? `/etc/caddy/Caddyfile` is not on this machine
  (checked from the worktree). The dev form above needs its real place.
- Does the public server serve web2 (open question 1 of the lobby plan)? Phase 4's production
  form and the password screen depend on it.
- web's offline single player and web2's single player both run `ti4-wasm` in the page, with
  their own saves (IndexedDB `ti4-offline` vs localStorage `ti4.single`). On one origin a
  player sees two "single player" entries; nothing in the decisions says whether that is
  wanted. Not part of this plan.
