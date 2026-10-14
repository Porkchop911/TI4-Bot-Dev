# Player Identity and Lobby Admission Refactor

## Status

PIL-01 model/persistence implementation was committed as `e0f234d` on
2026-09-23. PIL-02 was committed as `cf4a01f`; PIL-03 as `37c8823`.
PIL-04 was committed as `83957c3`; PIL-05 as `bd8f2c1`; the PIL-06 browser
follow-up was committed as `749bf44`, pending independent review. Package
progress and verification results are tracked here, without separate evidence
artifacts.
PIL-07 bot-agent changes were committed as `14a88cb`; the optional
Linux libtorch-backed advisor E2E gate passes, pending independent review.
PIL-08 integration changes were committed as `4276024`. The operator accepted
PIL-08 on 2026-09-23 with the explicitly recorded exceptions below; its full
workspace and independent-review gates have **not** passed. PIL-09–PIL-16 may
proceed under this operator exception; none is marked implemented by its
presence in this plan. PIL-09 server implementation was committed as `98e9fba`;
independent review is outstanding. PIL-10 and PIL-11 were committed as
`fe7eb8d`. PIL-12 and PIL-13 browser changes were committed as `85f9706`;
PIL-14 display-boundary changes are in the current worktree. Independent review
and real-browser integration remain outstanding. PIL-15 lobby controls are
implemented in the worktree; visual/browser inspection remains for PIL-16.

Proposed breaking-change plan. The current game is not live, so no compatibility
adapter, legacy endpoint, or persisted-data migration is required. Existing saved
unstarted lobbies and active sessions may be rejected by the new server version.
This is a hobby project for a small, trusted group (probably four simultaneous
players). Prefer straightforward recovery and understandable behavior over account
systems or elaborate lease/transaction machinery. Breaking API and storage changes
are acceptable before launch. A person must be able to resume their player from a
different computer after a client crash, even if the original credential is lost.

## Objective

Make humans and remote bot agents enter a game through the same automatic
admission flow. A client supplies a game ID, the server assigns the next open
lobby position for a new participant, and the server issues a private resumable
player-session credential. Clients must not choose a position or receive a session
credential as a command line or URL parameter. Someone without a credential may
also take over an _existing disconnected player_ from a different computer;
that operation does not create a new participant or change their position.

The host may reorder players while the game is a lobby. Starting the game
atomically freezes the roster and its order. Reordering a player never changes
that player's identity or session credential.

## Decisions

### Identity, session, and position are distinct

Use three separate concepts:

| Concept                    | Meaning                                                                        | Lifetime                                       | Authority                                         |
| -------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------- | ------------------------------------------------- |
| `PlayerId`                 | Stable game participant identity. It is created on first player admission.     | Lobby through game completion                  | Server generated                                  |
| Player-session credential  | Opaque, high-entropy bearer credential that resumes one `PlayerId`.            | Until replacement on takeover or game deletion | Server generated and private to the client        |
| Lobby slot / seat position | A physical ordered position in the lobby, optionally occupied by a `PlayerId`. | Mutable only during lobby                      | Host reorders; server assigns first open position |

`PlayerId` is not a position such as `p1`. It is retained when its holder moves
from one position to another. The engine's seating order is an ordered
`Vec<PlayerId>` derived once from the occupied lobby slots when the game starts.

Connection presence is a fourth, **ephemeral** concept. A heartbeat tracks whether
a player is currently connected; it never expires a credential, frees a slot, or
changes a `PlayerId`. After a server restart, presence starts as disconnected,
while persisted credentials remain valid.

Keep the word "seat" for game-rule and UI concepts that depend on physical
position. Do not use it in authentication names or credential storage.

### Admission and spectator behavior

`POST /api/games/{game_id}/lobby/join` is the one player entry endpoint.
It has no requested-position input. A new client omits the optional
`x-ti4-player-session` header and requests a new participant; a reconnect
supplies its existing credential. A client that lost its credential requests
takeover of an explicitly selected, disconnected `PlayerId` instead. Make
new admission and takeover distinct typed request variants so an empty slot
cannot accidentally be interpreted as an existing player.

- A new player admission is permitted only during `Lobby` phase.
- Under the registry lock, the server selects the first empty slot in current
  lobby order, creates a `PlayerId` and player session credential, fills that
  slot, sets readiness false, persists the whole updated lobby, and returns the
  result. It returns a conflict when all slots are occupied.
- A reconnect supplies the private player-session credential, receives the same
  `PlayerId`, and never consumes another slot. A valid credential works even if
  the player is currently shown as disconnected.
- A client without a credential may explicitly take over a disconnected player
  in either `Lobby` or `Running`. The server atomically replaces that player's
  credential, preserves their `PlayerId`, position, readiness and host status,
  and invalidates the previous credential. Takeover never uses an empty slot.
  If the player is connected, reject the request; if two takeovers race, only
  one succeeds. Reserve the player as present for a short connection grace
  period when takeover succeeds, so a second request cannot rotate the new
  credential before its recipient can connect. A lost client with the old
  credential must not retain access to private updates or choices after
  replacement.
- A spectator makes an explicit spectator request or uses an unauthenticated
  read-only route. Spectators never invoke admission and never occupy a slot.
- After `Running`, new admission and all seating mutations are rejected. An
  existing authenticated player may reconnect; an unauthenticated client may
  spectate or explicitly take over a disconnected existing player.

Anyone with the game link can take over a disconnected player. This is an
intentional trust tradeoff for the small group, not proof of the person's
identity. Do not add accounts, recovery codes, or host approval. Show a
"Rejoin as Player X" option only after presence has been absent for a short,
documented grace period (for example, heartbeat every 10 seconds and takeover
after 30 seconds without one). A lost connection does not itself rotate the
credential; normal reconnect with the old credential should work immediately.
Track presence per authenticated player and connection, and prevent an old
connection from continuing to receive private data after takeover. Presence
may be reconstructed as disconnected after a server restart; apply the same
grace period before offering takeover.

The game ID is an invitation locator, not an action credential. The private
player-session credential is still required after assignment for reconnects,
readiness, host actions, and choice submission. The explicit disconnected-player
takeover is the one exception that issues a replacement credential to someone
who has only the game link. Credentials must never appear in a URL, lobby
roster, logs, public snapshot, or error response.

### Host and seating controls

Creating a game creates its slot count, assigns the creator to the first slot,
and records that player's `PlayerId` as `host_player_id`. The create response
delivers its private session credential directly to the creating client, which
stores it just as it stores a later join credential.

Add one host-authorized lobby-only reorder operation. It accepts a complete
permutation of the existing slot IDs or occupied player IDs, validates it
exactly, updates the ordered slots and lobby version atomically, then persists.
It cannot add, remove, duplicate, or replace participants. The host remains the
host even if moved to another position.

The UI should show a stable player display label and position number, not imply
that `PlayerId` is a position. A host moves players and open slots; a joining
player is always placed in the first currently open slot.

Use a complete permutation of **slot IDs** for reorder. Each slot moves with
its current occupant (or emptiness); this represents moves of open positions
without guessing which player ID stands for an empty slot. Host authority is
limited to lobby reorder and start. After start, the host has no special
capability. An abandoned unstarted lobby may simply be discarded;
there is no automatic host succession or credential-expiry cleanup.

### Bot-agent lifecycle

`ti4-bot-agent` accepts `--game` and its advisor configuration, but no `--seat`
or `--token`.

1. It calls lobby join before the game has started.
2. It retains the returned private credential in process memory and marks
   itself ready, just like a human participant.
3. It sends authenticated presence heartbeats while waiting to start. These
   report presence; they do not renew or extend a credential.
4. It connects after the game begins, authenticates using that retained
   credential, and obtains its `PlayerId` from the authenticated server reply.
5. It sends application-level protocol `Ping` messages at a bounded interval
   matching the presence interval, processes `Pong`, and resumes with the same
   credential after a transport disconnect. If its process crashes and loses
   the credential, an operator may start a new bot process and explicitly
   choose its disconnected player from the public rejoin list. The agent must
   not automatically take over a disconnected person, even when only one is
   listed; selection is interactive, not a `--seat` argument.

The bot must pass the server-authenticated `PlayerId` to the advisor and reject
a pending choice for any other player. It must never infer identity from seating
position or accept a player identity supplied by command line configuration.

## Target Data Model

Replace the current lobby model in `crates/ti4-server/src/session/registry.rs`:

```rust
struct LobbyState {
    game_id: String,
    phase: LobbyPhase,
    host_player_id: PlayerId,
    slots: Vec<LobbySlot>,
    players: BTreeMap<PlayerId, LobbyPlayer>,
    seed: u64,
    lobby_version: u64,
}

struct LobbySlot {
    slot_id: LobbySlotId,
    occupant: Option<PlayerId>,
}

struct LobbyPlayer {
    ready: bool,
    session: PlayerSession,
}

struct PlayerSession {
    credential: String,
}
```

Use dedicated typed IDs for `LobbySlotId` if they cross a public or persistence
boundary. Preserve deterministic order with `Vec` for slots and `BTreeMap` for
player-keyed data. Generate `PlayerId` and credentials with cryptographically
strong random values, validate their bounds at all protocol boundaries, and
ensure generated player IDs cannot collide with an existing lobby participant.

At `start_lobby`, require every slot to be occupied and every player ready.
Derive exactly one ordered `Vec<PlayerId>` from `slots`; persist it in the game
initialization record and pass it to `create_game_with_map`. The running
`GameSession` owns that order and never exposes any mutation path.

For an active game, session credentials map to `PlayerId`, not a slot. Existing
`seat_tokens: BTreeMap<PlayerId, String>` becomes a player-session credential
map or a dedicated session-authentication service keyed by `PlayerId`. Naming
must consistently use `player_session` or `resume_token`, never `seat_token`.
Store credentials durably so an ordinary server restart does not lock out
players. Do not serialize credentials into public views or include them in
`Debug` output. A takeover must persist the replacement before returning it,
and revoke the old credential in both lobby and running-session authentication;
recovery must load the replacement rather than an older game-init credential.
For running games, keep the current credentials in a small atomically replaced
per-game record separate from the immutable game-init record, or use an equally
simple single authoritative record. Do not write a replacement only to memory
or only to the original init file while another recovery path loads stale data.
No credential expiry timestamp or expiry sweep is needed.

## HTTP and WebSocket Contract

Replace the following lobby API behavior:

| Current                                                        | Replacement                                                                                             |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `players: ["p1", "p2", ...]` at creation                       | A bounded `player_count` that creates ordered empty slots                                               |
| `POST /lobby/claim` with `{ "seat": "p2" }`                    | `POST /lobby/join` with no chosen position; server assigns the next open slot                           |
| `creator_token`                                                | Private `player_session` for the host player                                                            |
| `credential` from claim                                        | Private `player_session` from join                                                                      |
| `x-ti4-seat-token`                                             | `x-ti4-player-session` for authenticated player operations                                              |
| Public roster's `seat`, `available`, and controller assignment | Ordered slots with position, occupancy, public player label, and coarse connected/disconnected presence |
| `bot_seats` selecting server-side `BotFirstOption`             | Remove from the public create flow; remote bots join exactly like humans                                |

Define typed request and response DTOs, use strict Serde decoding, and document
the credential fields as private. Never put a session credential inside a
broadcast `LobbyResponse` or serializable `LobbyState`.
For no-token join, distinguish `{ "kind": "new" }` from
`{ "kind": "takeover", "player_id": "player_..." }`; the latter is allowed
only for an existing disconnected participant and never accepts a position.
The public roster lists eligible disconnected players so the UI can offer an
explicit rejoin modal. The server still validates eligibility at submission.

Suggested responses:

```json
// POST /api/games/{game_id}/lobby/join
{
  "player_session": "opaque-private-bearer-credential",
  "player": { "id": "player_..." },
  "lobby": { "game_id": "...", "phase": "lobby", "slots": [] }
}

// explicit spectator lobby read
{
  "game_id": "...",
  "phase": "lobby",
  "slots": []
}
```

For the running WebSocket protocol, replace `Subscribe.seat_token` with an
optional `player_session` credential. With a valid credential the server derives
`ViewerRole::Player(PlayerId)`; without one it derives `ViewerRole::Spectator`.
The server sends the authenticated viewer identity in `InitialSnapshotMsg` and
`StateUpdateMsg`, as it already does. The credential is never echoed by a server
message.

Application `Ping` updates authenticated presence. Raw WebSocket control pings
remain transport-only and do not affect presence. Invalid or revoked
credentials must fail closed without changing lobby/game state. Browser and bot
clients send authenticated presence heartbeats in the lobby and application
`Ping` during the game. A takeover revokes the old credential and terminates
or downgrades its old subscription before it can receive more private updates;
every choice still authenticates against the current credential.

## Persistence and Failure Rules

- Version the lobby and game-init schemas rather than attempting implicit
  deserialization of the current seat-token records. Bump the WebSocket
  protocol version with the renamed subscription field; old clients need not
  remain compatible.
- This breaking change may reject old persisted records with a clear schema
  error. Do not reinterpret a prior `p1` identity as a movable player without a
  reviewed migration.
- Persist creation, join, takeover, reorder, readiness, and the game-init
  record before returning credentials or start success. Individual file writes
  should remain atomic; there is no requirement for a multi-file transaction
  to protect an unstarted lobby. Treat a valid game-init record as the start
  commit point on recovery; if it is absent, recover as an unstarted lobby
  or discard it. Do not report a started game as recoverable without a valid
  game-init record. Once running, preserve the existing careful decision-log
  and replay recovery behavior.
- On storage failure, do not leak a newly generated credential and leave the
  in-memory lobby or running authentication unchanged. A failed takeover
  leaves the old credential usable. No presence observation alone changes
  roster, readiness, seating order, or credentials.
- If an occupied lobby slot is explicitly vacated before start, retire that
  `PlayerId` and revoke its credential; a later new join gets a new identity.
  Provide an authenticated leave operation for non-host players in the lobby;
  a host who abandons their lobby can discard that unstarted game. Never
  vacate a slot or replace a player automatically after start.
- Every authorization check resolves credential -> `PlayerId`; it must not
  depend on B-tree iteration order, slot iteration outside explicit first-open
  selection, wall-clock values other than the documented presence decision, or
  client-provided player IDs.

## Implementation Packages and Tests

Execute these in order. Each row is one bounded package with focused tests;
split a row further if necessary. The named files are primary edit scopes, not
permission to change unrelated code. Record additional necessary paths in this
plan before editing. The acceptance-test numbers refer to the list below. Run
formatting, focused tests, and affected-crate tests for every code package.
The server may be temporarily unusable between breaking-change commits on this pre-launch branch;
do not claim an intermediate package is a deployable release.

### PIL-01 — Identities and versioned records

- **Depends:** None. **Primary scope:** `ti4-model` ID definitions (only if a
  new typed slot ID is needed), `ti4-server/src/session/registry.rs`,
  `ti4-server/src/storage.rs`, and their focused tests.
- **Contract:** Define ordered slot IDs, stable generated player IDs, private
  player-session credentials, and bounded versioned lobby/game-init records.
  Loading an old record gives a clear schema error. Public serialization and
  `Debug` do not reveal credentials. Define where current running credentials
  live so recovery cannot reload an obsolete value after takeover; do not add
  an account system or expiry service.
- **Gate:** Serialization/size/corruption tests, unique-ID collision tests,
  private-output tests, and an old-format rejection test pass (acceptance 1,
  11, 12 for the model).
- **Progress (2026-09-23):** Added typed `LobbySlotId`, 256-bit generated
  `player_` IDs with collision retry, redacted `PlayerSession`, bounded v2
  private lobby, immutable game-init and authoritative current-session records,
  and a credential-free public lobby projection. Focused tests cover round
  trips, forced ID collision, checksum/size/reference failures, old-record
  rejection, and restart loading of a rotated current credential.
- **Integration boundary:** PIL-02 connected v2 lobby writers/readers and
  running-session recovery through the current-session record. Legacy direct
  session APIs still support v1 records; PIL-04 must ensure rotated credentials
  replace both the current-session record and the active authenticated view.
  Live takeover remains unimplemented. The v2 game-init record is the start
  commit point; a running lobby record without init recovers as unstarted.
- **Security boundary:** v2 persistence DTOs are intentionally
  serializable for disk only; client handlers must serialize the explicit
  `PlayerLobbyView` projection and never a private record. Debug output for
  existing token-bearing lobby/init DTOs is redacted during the transition;
  their private on-disk serialization remains for existing server behavior.

### PIL-02 — New admission and lobby authorization

- **Depends:** PIL-01 model/persistence code. **Primary scope:** registry lobby
  transitions, `ti4-server/src/http/games.rs`, routes, and lobby tests.
- **Contract:** Create takes `player_count` and returns the host credential;
  a new `join` selects the first open slot, while a credential-bearing `join`
  resumes the same player. Authenticated non-host leave retires that identity;
  readiness/start authorization resolves credential to `PlayerId`. Public
  spectator reads do not join. Remove public `bot_seats` and `p1` creator
  assumptions here. Reserve the typed `takeover` join variant for PIL-04;
  until then reject it explicitly, without mutating state.
- **Gate:** Concurrent first-open joins, full lobby, reconnect, leave/new ID,
  spectator read, ready/start authorization, and persistence-failure tests pass
  (acceptance 1–5, 11 as applicable).
- **Additional edit paths:** `crates/ti4-server/src/http/mod.rs`,
  `crates/ti4-server/src/storage.rs`, and focused server tests for routing,
  durable v2 start/recovery, and verification.
- **Progress (2026-09-23):** Implemented `player_count` creation with generated
  host identity and private `player_session`; first-open `/lobby/join`,
  credential reconnect, authenticated non-host leave, readiness/start, public
  spectator lobby read, and explicit rejection of the reserved takeover request.
  The new HTTP paths use only bounded v2 lobby records. Starts persist current
  credentials and a credential-free init; restart recovery loads the current
  credential record. WebSocket messages still use the v1 `seat_token` field
  until PIL-03; the registry recognizes v2 credentials there without leases.
  Pre-launch browser/bot clients still require their later packages.
- **Verification:** `cargo test -p ti4-server` passed (including concurrent
  first-open admission, storage-failure rollback, HTTP authentication and
  spectator reads, v2 restart/replay, and existing WS/protocol coverage);
  `cargo fmt --package ti4-server` applied; `cargo clippy -p ti4-server
--all-targets` passed with existing dependency warnings. Strict `-D warnings`
  remains blocked by unrelated warnings in `ti4-model` and `ti4-engine`.
- **Next:** PIL-03 replaces the WebSocket subscription field, adds authenticated
  presence/heartbeat behavior and revocation-aware subscriptions; PIL-04
  implements explicit takeover. PIL-05 adds host slot reorder and strengthens
  start/recovery crash-boundary testing. PIL-08 removes legacy direct-registry
  seat/lease paths and updates remaining clients and docs.

### PIL-03 — Authenticated presence and WebSocket identity

- **Depends:** PIL-02. **Primary scope:**
  `ti4-server/src/protocol/`, `ti4-server/src/ws/`, registry presence, and
  their focused tests. Do not implement takeover in this package.
- **Contract:** Bump the protocol version; `Subscribe.player_session` resolves
  the actor, missing credentials mean spectator, and choice submission checks
  current authentication. HTTP lobby heartbeats and application `Ping` update
  ephemeral presence, never credential validity. A documented short grace
  period controls the disconnected display; restart begins with no presence.
  Raw WebSocket control pings do not mark a player present. Provide a way for
  subscriptions to stop private delivery if their credential is later revoked.
- **Gate:** Spectator privacy, invalid credentials, ping/timeout boundary,
  brief credential reconnect, per-choice authentication, and restart-presence
  tests pass (acceptance 4, 5, 11, 12).
- **Additional edit paths:** `crates/ti4-server/src/http/games.rs` (heartbeat),
  and `crates/ti4-server/tests/{player_lobby_admission,protocol_roundtrip,size_bounds,ws_lifecycle}.rs`.
- **Progress (2026-09-23):** WebSocket protocol v3 uses optional
  `Subscribe.player_session`; the old field and protocol version are rejected.
  The authenticated server-reported viewer is derived from the current registry
  credential, and choice submissions reauthenticate before dispatch. Subscribe
  registers an ephemeral per-connection presence record; application `Ping`
  refreshes it, HTTP `/lobby/heartbeat` records an authenticated heartbeat,
  and control pings and spectators do neither. Public slots expose `connected`
  and `can_take_over` (a display hint, not an authorization decision). The
  documented interval is 10-second heartbeats and a 30-second absence grace;
  presence is not persisted, never expires credentials, and restart begins
  disconnected with a fresh grace window. Connection close begins its grace
  window. New admissions have their own grace window even in an old registry.
  Subscriptions and outbound delivery check current authentication and stop
  private updates after revocation; the idle socket also checks at most every
  250 ms. The update forwarder is async and cancels when the outbound channel
  closes. Debug formatting of subscribe messages redacts credentials.
- **Verification:** `cargo fmt --package ti4-server` and `cargo test -p
ti4-server` passed, including protocol-v3 rejection, spectator/invalid-token
  subscriptions, application-vs-control ping, grace expiry, credential
  continuity, and restart presence. The first full test attempt hung in two
  WebSocket lifecycle tests because a blocking subscription forwarder survived
  runtime shutdown; replacing it with a cancellable async forwarder made the
  isolated WebSocket suite and the full affected-crate suite pass. Full Clippy
  was not run (blocked by `ti4-model`). No independent review was performed.
- **Next/PIL-04:** Implement durable credential rotation and a short takeover
  reservation under the same registry lock as eligibility checks; clear stale
  presence for the replaced credential so its old connection cannot refresh
  or reintroduce presence on disconnect. Update both lobby authentication and
  the authoritative running-session credential record before returning the
  replacement. Serialize choice authorization with credential rotation so a
  previously authorized choice cannot race a takeover, and verify an already
  connected old subscription cannot receive a private message after commit.
  The `can_take_over` hint must be rechecked atomically on submission. PIL-06
  and PIL-07 must send application pings every 10 seconds; PIL-08 updates
  remaining older client/fixture protocol fields.

### PIL-04 — Disconnected-player takeover

- **Depends:** PIL-03. **Primary scope:** registry credential
  rotation, current-credential storage, `ti4-server/src/http/games.rs`,
  `ti4-server/src/ws/`, and focused server/recovery tests.
- **Contract:** The explicit no-token `join` takeover variant selects an
  existing disconnected `PlayerId` in lobby or running game, rotates and
  durably saves its credential before returning it, preserves identity and
  seating, and prevents old live subscriptions from seeing more private data.
  The recipient gets a brief connection grace reservation; a connected player
  cannot be taken over. No host approval, code, or expiry is added.
- **Gate:** Connected-player refusal, concurrent takeover, old-token refusal
  for HTTP/WS/choices, old-subscription closure, storage-failure rollback,
  lobby/running takeover, and restart-loading-new-token tests pass (acceptance
  5, 9, 11, 12).
- **Additional edit paths:** `crates/ti4-server/src/session/mod.rs` (keep the
  active session's internal credential map current),
  `crates/ti4-server/tests/player_lobby_admission.rs`, and
  `crates/ti4-server/tests/ws_lifecycle.rs`.
- **Progress (2026-09-23):** The no-credential typed takeover request now
  rotates only an existing disconnected player, under the registry lock after
  rechecking the 30-second absence/grace rule. A new credential is persisted
  before delivery; lobby takeovers atomically replace the lobby record, while
  running takeovers atomically replace the authoritative current-session record.
  In-memory lobby and active session authentication then switch to the new
  credential; old presence connection IDs are discarded and the recipient gets
  a fresh connection grace reservation. Recovery overlays the running lobby's
  possibly stale credential copies with the authoritative session record.
  HTTP snapshots and WS choice submissions serialize authentication with
  takeover; old WebSocket connections stop on revoked-credential checks.
  Concurrent requests produce one winner, without changing player identity,
  position, readiness or host. No old credential appears in public responses.
- **Verification:** `cargo test -p ti4-server --test player_lobby_admission
--test ws_lifecycle` passed (8 + 6 tests); `cargo fmt --package ti4-server`
  applied and `cargo test -p ti4-server` passed (all unit, integration and doc
  tests). Full Clippy was not run, as requested (blocked by `ti4-model`).
  No independent review was performed; do not treat PIL-04 as independently
  reviewed or deployable on its own.
- **Next/PIL-05:** Implement the complete slot-ID reorder with exact
  permutation validation and host authorization under the same registry lock;
  verify first-open admission after reorder, immutable started order, and
  crash-before/after-init recovery with running credential rotations retained.
  PIL-06/PIL-07 should use the new explicit takeover request only when the
  public `can_take_over` hint is true and handle conflict if presence changes.

### PIL-05 — Reorder and committed start

- **Depends:** PIL-04. **Primary scope:** registry reorder and
  start, game-init storage/recovery, HTTP reorder route, and focused tests.
- **Additional edit paths:** `crates/ti4-server/src/http/mod.rs` for the
  reorder route and `crates/ti4-server/tests/player_lobby_admission.rs` for
  HTTP, concurrency, and recovery-boundary coverage.
- **Access:** P1; writable paths are the primary and additional paths above
  plus this plan. No external reference, network download, persistent worker
  process, or external-state change; only bounded local test storage.
- **Contract:** Only the lobby host may submit a complete slot-ID permutation;
  join and reorder serialize. Start requires full occupancy/readiness and
  derives exactly one ordered player vector for the engine. A valid init record
  is the start recovery commit point; a partial pre-start transition never
  presents an active game. Host has no post-start privilege.
- **Gate:** Reorder/empty-slot/join race, bad permutations, host preservation,
  exact engine order, rejected post-start changes, and crash-before/after-init
  tests pass (acceptance 6–9, 12).
- **Progress (2026-09-23):** Added host-authenticated `POST
/api/games/{game_id}/lobby/reorder` accepting strict `{ "slot_ids": [...] }`.
  The registry validates an exact permutation, including empty slots, under
  the same lock as join/start. A changed order increments the lobby version
  and is saved before publication; storage failures leave memory and disk
  unchanged. The host's identity, credential and readiness stay with the
  occupant. Start already derives its only engine order from the final slot
  vector, and writes current sessions and the running lobby before the
  credential-free init commit. Recovery treats a Running lobby lacking init as
  unstarted and treats valid init as started, even if the lobby phase is stale;
  rotated running credentials still come from the authoritative sessions file.
- **Verification:** `cargo fmt --package ti4-server`, focused
  `cargo test -p ti4-server --test player_lobby_admission` (13 passed),
  `cargo test -p ti4-server` (all unit/integration/doc tests passed), and
  `git diff --check` passed. Focused cases cover concurrent join/reorder,
  exact slot validation and HTTP authorization, persistence rollback, ordered
  initialized engine players, and both sides of the init crash boundary with
  post-start credential rotation. Full Clippy was not run as requested
  (blocked by `ti4-model`). No independent review was performed.
- **Next/PIL-06 and PIL-07:** Browser host controls should submit the entire
  current `slot_id` order, including open slots, to `/lobby/reorder`; show
  the returned positions and handle authorization/conflict responses. Bot
  clients should use the reordered public positions for display only; their
  authenticated `PlayerId` remains their sole acting identity. PIL-08 must
  verify the actual browser/bot start path and full workspace gate.

### PIL-06 — Browser flows

- **Depends:** PIL-04 and PIL-05. **Primary scope:** `web/src/`
  UI, protocol decoding and storage, plus focused browser tests.
- **Contract:** Show Join, Watch, and eligible "Rejoin as Player X" actions,
  the stable public label and position, lobby-only reorder controls, and
  heartbeat/ping presence. Store credentials in tab-scoped storage, never
  navigation state or URLs; use the authenticated `PlayerId` after reconnect.
- **Gate:** Browser tests cover new join, watch without admission, remote-
  computer takeover with no stored credential, old-token invalidation, host
  reorder, readiness, and protocol-version handling (acceptance 4–9, 11).
- **Progress (2026-09-23):** Browser creation now sends `player_count` and
  stores the returned private `player_session` only in per-game tab-scoped
  `sessionStorage`. A public lobby GET does not join; an authenticated
  `/lobby/join` reconnect obtains the server-authenticated `PlayerId` (the
  public lobby deliberately has no viewer field). A credential-free client
  can Join the next open slot, Watch as spectator, or explicitly select a
  public `can_take_over` player; a 409 race is shown as an error. Revoked
  credentials are removed after failed reconnect/heartbeat. The lobby shows
  stable player labels, position numbers, readiness and presence; host-only
  controls submit a complete slot-ID permutation including empty slots.
  Game HTTP snapshots and WS subscribe use `x-ti4-player-session` and v3
  `player_session`; player sockets send application pings every 10 seconds
  and reconnect on transport loss with the same in-memory credential. A
  snapshot with a different server-authenticated viewer is refused. HTTP
  lobby heartbeats continue at 10 seconds, including while running.
- **Verification:** `npm run build` (content check, TypeScript and Vite)
  and `npm test` (25 files, 159 tests) pass; browser unit tests exercise
  create, admission vs spectator reads, fresh-computer takeover, old-token
  refusal, host reorder payload, readiness, v3 subscribe/pings/reconnect,
  viewer identity mismatch and v2 rejection. `git diff --check` passes.
  Full Clippy was not run as requested (blocked by `ti4-model`). No
  independent review was performed. The existing v2 golden fixtures are
  identified as legacy and rejected at protocol ingress; regenerating the
  server fixtures and the remaining old Playwright manual-claim flows remain
  PIL-08 integration work. At this point no v3 browser/server E2E had run;
  the focused lifecycle run below was added in the UI follow-up.
- **Create-response correction (2026-09-23):** A real create response was
  rejected by the browser because its old shared string validator capped
  _all_ identifiers and credentials at 64 characters. The server-generated
  `player_` ID is 71 characters and the `session_` credential is 72. The
  decoder now uses separate bounds for game/slot IDs (64) and player IDs/
  player sessions (128, matching the WebSocket session bound). A regression
  test decodes the full-length generated shapes on both create and join and
  rejects an oversized credential. Build and all 159 web tests pass after
  this fix; the host's already-created lobby can be reached with its private
  credential only if the original response was stored (the previously
  rejected response was not stored by the browser).
- **Lobby UI and leave follow-up (2026-09-23):** Removed the visible game ID
  and raw `PlayerId` strings from the lobby and rejoin controls. `Player N`
  is derived from the fixed `slot_N` label, which moves with its occupant;
  the current position is shown separately. Copy Game URL copies the current
  browser origin and path, without a credential or query string. Non-host
  lobby participants now see **Leave lobby**, which calls the authenticated
  `/lobby/leave` transition and forgets the tab credential only after a
  successful response; server storage failures leave the credential and
  player in place. The server forbids host leave (no host succession), so
  the misleading Forget button is not shown to the host. Running-game Exit
  Game remains a local exit, not a mid-game roster change.
- **Actual browser/server gate:** Replaced the old manual-claim
  `lobby_lifecycle.spec.ts` with a real Chromium create → spectator watch →
  join → leave → verify slot opens → rejoin → ready → start → reload flow.
  `cargo build -p ti4-server --bin server` passed; the focused Playwright
  test passed (1/1, 5.5 s) against isolated local ports 38080/33000 because
  8080/3000 were already in use (those processes were not touched). Default
  ports remain unchanged; the test runner accepts optional port overrides.
  `npm run build`, `npm test` (25 files, 161 tests), and `git diff --check`
  passed. This is a focused E2E flow, not a claim that the remaining legacy
  Playwright suites or the full PIL-08 gate pass. No independent review or
  full Clippy run was performed.
- **Next/PIL-07 and PIL-08:** Bot agents should retain the join-issued
  credential and server-authenticated identity; do not infer it from lobby
  position. PIL-08 should replace legacy v2 browser fixtures and manual-seat
  Playwright workflows with real create/join/watch/takeover/reorder/start
  flows against the current server, including a running spectator snapshot,
  credential revocation and idle-period heartbeat coverage. Review the
  browser/server boundary before treating this as deployable.

### PIL-07 — Bot-agent flows

- **Depends:** PIL-04 and PIL-05. **Primary scope:**
  `crates/ti4-bot-agent/` and focused bot integration tests.
- **Contract:** Remove `--seat`/`--token`; join from game/advisor configuration,
  mark ready, heartbeat while waiting, send application pings when running,
  and reconnect with the existing credential. A fresh process without that
  credential can offer an interactive selection of eligible disconnected
  players for explicit takeover; it never selects one automatically. Only an
  authenticated server-reported `PlayerId` goes to the advisor; reject choices
  for any other player.
- **Gate:** Bot E2E covers ready/start, idle longer than the presence grace
  period, disconnect/reconnect, fresh-process takeover, and advisor identity
  (acceptance 9–11).
- **Additional edit path:** `crates/ti4-bot-agent/README.md` for the new CLI
  and crash/rejoin instructions. **Access:** P1; bot source, tests, README and
  this plan only; no external reference, network download, external-state
  change, or destructive action. Tests bind bounded loopback HTTP/WS ports.
- **Progress (2026-09-23):** Removed the seat and token CLI/config inputs. A
  fresh agent reads the public lobby and requests the next open player position;
  when none is open, it lists eligible disconnected positions and requires an
  explicit interactive choice before sending a typed takeover request. It
  retains the returned private session in process memory, marks ready in a
  lobby and sends authenticated HTTP heartbeats every 10 seconds until start;
  a takeover of a running player skips lobby readiness. The running WebSocket
  subscribes with protocol-v3 `player_session`, verifies the server-reported
  `PlayerId` on every snapshot/update before advising, rejects choices for
  another player or a choice before authentication, sends application `Ping`
  every 10 seconds, processes `Pong`, and reconnects with the same session.
  Updated the opt-in real-advisor E2E fixture to create/join/start without
  manual seat claims; its identity assertions use actual lobby occupants.
- **Verification:** `cargo fmt --package ti4-bot-agent` and `cargo test -p
ti4-bot-agent` pass (9 unit/integration tests; real-server join/ready/heartbeat
  stays connected past the 30-second grace, starts, then exercises a
  fresh-process explicit running takeover after absence and old-token refusal;
  mock WS covers ping/pong, reconnection and advisor submission). The opt-in
  `cargo check -p ti4-bot-agent --features real-e2e --tests` passed with the
  README's Linux `LIBTORCH=out/libtorch-2.9.1-cpu-linux`, version bypass and
  `LD_LIBRARY_PATH` settings. `cargo test -p ti4-bot-agent --features real-e2e
--test real_e2e -- --ignored` with the same settings passed (1/1, 11.51 s):
  a real server, pinned advisor, two newly admitted bots and a scripted host
  advanced a bounded game prefix, with decisions by both generated bot IDs.
  The initial check without the README's environment had mistakenly used the
  Windows pin `out/libtorch-2.9.1-cpu` and failed on its absent `XNNPACK.lib`;
  this was an environment-selection error, not a missing Linux installation.
  Full Clippy was not run, as requested (blocked by `ti4-model`). No independent
  review was performed.
- **Next/PIL-08:** Include the bot CLI and private-credential boundary in the
  integration review; extend real-server bot coverage to an idle running-game
  ping interval and a transport reconnect if required for the full gate (the
  current real-server idle test covers lobby heartbeats and running takeover,
  while mock WS tests cover pings and reconnect). Complete workspace/protocol
  fixture cleanup and the full acceptance gate.

### PIL-08 — Contract cleanup and end-to-end gate

- **Depends:** PIL-06 and PIL-07. **Primary scope:** obsolete
  lobby/claim/seat-token paths, README, protocol fixtures, server/web E2E,
  storage recovery tests, and integration docs. List exact paths in the task
  spec; preserve unrelated tests and utilities.
- **Contract:** Remove obsolete public claim, manual-seat, and `seat_token`
  contracts across server and clients; test the actual new HTTP/WS flow. Old
  persisted formats may fail clearly. Do not weaken running-game recovery.
- **Gate:** All acceptance tests 1–12 pass through appropriate unit and E2E
  layers; format, affected crates, workspace suite, protocol round-trips,
  concurrency, recovery, and a real bot E2E with idle time past the presence
  grace period pass.
- **Execution scope (2026-09-23):** P1, repository-only edits. Exact initial
  writable paths: `web/plans/2026-09-23-PLAYER_IDENTITY_LOBBY_REFACTOR_PLAN.md`, `README.md`,
  `web/README.md`, `web/playwright.config.ts`, `web/e2e/*.spec.ts`,
  `web/e2e/lobbyHelpers.ts`,
  `web/src/protocol/fixtures.test.ts`, `crates/ti4-server/fixtures/*.json`,
  `crates/ti4-server/examples/generate_protocol_fixtures.rs`,
  `crates/ti4-bot-agent/tests/real_e2e.rs`,
  `crates/ti4-server/src/bin/server.rs`,
  `crates/ti4-server/tests/{fixtures_verification,lobby_lifecycle,player_lobby_admission,ws_lifecycle}.rs`,
  and, if removal of internal legacy APIs is feasible without weakening replay,
  `crates/ti4-server/src/{session/registry,session/mod,session/worker,storage}.rs`
  and their affected tests. No historical reference, download, external-state
  change or destructive action. Local loopback E2E servers and bounded temporary
  game data only. Existing protocol-v2 fixtures require semantic inspection before
  regenerating with the versioned generator. Full Clippy is excluded by operator
  instruction (`ti4-model` blocker). Record gate results and any remaining
  legacy internal API as an explicit follow-up here rather than claiming closure.
- **Progress (2026-09-23, uncommitted):** Removed the standalone server's
  automatic `demo` game with fixed `p1`–`p3` seats and `BotFirstOption` and its
  obsolete lease environment setting. Browser E2E setup now creates, joins,
  readies and starts through the actual HTTP contract using private tab-scoped
  player sessions; the old manual-claim/seat-token flows are gone. A new real
  browser/server case reorders an open position, joins into the first open
  position, verifies a running spectator has no private cards or credential,
  keeps the host present past the 30-second absence grace, takes over the
  disconnected guest from a fresh context, refuses the old credential and
  reconnects with the rotated credential. The opt-in real-advisor/two-bot test
  now keeps both bots connected through 31 idle running seconds, asserting
  that neither can be taken over. Protocol-v3 golden fixtures were regenerated
  from `ti4-server::fixtures` using the checked-in
  `generate_protocol_fixtures --write` example; the semantic diff includes the
  current nested choice, redacted private-state payload, and galaxy layout,
  not merely a changed version number. The server test now checks checked-in
  fixture JSON against its producer on every run, and the browser parses the
  same checked-in fixtures. Current README files describe the admission flow.
- **Checks:** `cargo fmt --package ti4-server --package ti4-bot-agent --check`,
  `cargo run -p ti4-server --example generate_protocol_fixtures` (read-only
  verification mode), `cargo test -p ti4-server` (all unit, integration and
  doc tests), `cargo test -p ti4-bot-agent` (9 passed),
  `cargo test -p ti4-bot-agent --features real-e2e --test real_e2e -- --ignored`
  with the pinned Linux libtorch runtime (1 passed, 42.13 s including idle),
  `npm test` (25 files, 161 passed), `npm run build`, and full Playwright
  Chromium E2E (5 passed, 43.4 s on isolated 38080/33000 ports) passed.
  `git diff --check` passed. The first full E2E attempt exposed a race in an
  old speculative multi-click loop; that loop was replaced by an assertion
  at the actual three-human decision boundary before the five-test pass.
  `cargo test --workspace` was attempted twice: without `LIBTORCH` it failed
  at the Windows-pin `XNNPACK.lib`; with the pinned Linux runtime it progressed
  but stopped compiling `rfd 0.17.2` because neither Linux `gtk3` nor
  `xdg-portal` backend feature is enabled. A narrower workspace run with the
  Linux runtime and `--exclude ti4-review` compiled, then stopped at the
  pre-existing missing `ti4-bridge/tests/golden/hexsummary_captures.json`
  (6 failures in `hexsummary_golden`; no fixture was fabricated). No full
  Clippy was run. No independent review was performed.
- **Operator acceptance with exceptions (2026-09-23):** PIL-08 is **accepted
  for progression to PIL-09** at the operator's explicit request, without
  resolving the exceptions below. This records an acceptance decision, not a
  claim that the original full workspace/review gate passed or that the
  uncommitted implementation is independently reviewed. Retain the recorded
  failures and remaining internal legacy API as open findings.
- **Carried-forward follow-up (nonblocking for PIL-09):** Resolve the existing Linux
  workspace `rfd` feature/build configuration or run the full suite on the
  supported Windows host with its pinned runtime; separately restore the
  approved bridge golden captures and report exact workspace results. Review
  the internal legacy direct-registry `create_lobby`/
  `claim_seat`/lease and v1 `seat_token` storage paths in
  `ti4-server/src/{session/registry,session/mod,session/worker,storage}.rs`:
  they remain reachable by in-repository direct-session utilities/tests, but
  are not exposed through current HTTP/WS admission or the server binary.
  Remove or isolate them only after preserving the direct-session replay and
  recovery test coverage; do not misrepresent them as a live player endpoint.
  Obtain independent security/schema review of the server/browser/bot boundary
  and resolve any actionable findings before claiming the original PIL-08 gate
  is fully verified. Track these exceptions while PIL-09–PIL-16 proceed; do
  not silently turn operator acceptance into a passing workspace or review result.

### PIL-09 — Persisted participant nicknames

- **Depends:** PIL-08. **Primary scope:** `crates/ti4-server/src/storage.rs`,
  `crates/ti4-server/src/session/registry.rs`, `crates/ti4-server/src/http/games.rs`,
  and focused server/recovery tests. Record any additional edit paths before use.
- **Execution scope (2026-09-23):** P1; writable paths are the three primary
  server files, `crates/ti4-server/tests/{player_lobby_admission,lobby_lifecycle,ws_lifecycle,session_recovery_replay}.rs`, and this
  plan. No external references/downloads or external state; bounded local test
  storage and loopback HTTP tests only. No destructive actions. Full Clippy is
  excluded by operator instruction (`ti4-model` blocker).
- **Contract:** Require a nickname when creating a host, admitting a new player,
  or explicitly taking over a disconnected player. A credential reconnect uses
  the already stored nickname and cannot rename its player. Nicknames belong to
  `PlayerId`, not to slot IDs or positions; reorder and start preserve them.
  Takeover may choose a new nickname, including during a running game, without
  changing identity, position, readiness, or host status. Duplicate nicknames
  are allowed: a name does not authenticate or uniquely identify anyone.
- **Validation:** Define a single bounded, nonempty, trimmed Unicode nickname
  contract at the server boundary; reject control/format characters and
  overlong inputs rather than silently truncating or rewriting names. Browser
  validation mirrors this contract; React renders names as text, never HTML.
  Keep nickname fields out of authentication names and credentials out of
  nickname/public fields. The public roster includes each occupied player's
  nickname and position; running-game clients must be able to obtain the
  current nickname roster after restart and takeover.
- **Persistence:** Version changed records and reject incompatible records
  clearly as appropriate for this pre-launch branch. In particular, running
  takeover already writes an authoritative current-session record while its
  lobby record may contain stale data: define and test one authoritative
  recovery path for the _current nickname_ as well. Persist the new nickname
  before returning a successful admission/takeover; a failed write leaves the
  previous nickname and credential intact. Never use a display name as an
  authorization or game-rule key.
- **Gate:** Focused storage, HTTP, concurrency, and recovery tests for creation,
  new join, reconnect without rename, duplicated names, reordered slots,
  lobby/running takeover rename, restart and failed-write rollback pass
  (acceptance 13–14). Run formatting and affected-crate tests. This package
  changes the nickname contract, not the engine's `PlayerId`-based protocol.
- **Progress (2026-09-23, uncommitted):** Create requires `nickname`; new join
  requires a nickname, credential reconnect omits it and rejects attempts to
  rename, and explicit takeover requires a new nickname. The same validation
  runs at the registry and storage boundaries: exact, nonempty, trimmed Unicode
  text, at most 64 UTF-8 bytes, with control and formatting characters rejected.
  No truncation, uniqueness check, or name-based authentication. The public
  slot projection carries the current occupant's nickname alongside position;
  `GET /lobby` remains available after start to running-game clients and
  spectators. Reorder moves occupants without rebinding their names. The
  player records now use schema v3 (v2 is explicitly refused). Running-session
  records store both current credentials and nicknames; successful running
  takeover writes that one authoritative file before changing live state, and
  restart overlays both fields from it rather than trusting the stale lobby.
  Failed lobby/running writes leave both fields unchanged. Direct registry
  creation, joining and takeover require explicit nickname arguments too.
- **Verification:** `cargo fmt --package ti4-server` and `cargo test -p
ti4-server` passed (all unit, integration and doc tests); `git diff --check`
  passed. Focused tests exercise Unicode/bounds/format validation, duplicate
  names, HTTP admission/reconnect refusal, name continuity across reorder,
  running takeover with a stale lobby on disk, lobby takeover restart, and
  rollback on obstructed lobby/current-session writes. Full Clippy was not run
  per operator instruction. No independent review was performed. This is a
  breaking server request/storage contract; current browser and bot creation
  and new-join requests need their PIL-11 and PIL-10 changes before end-to-end
  admission works again.
- **Next:** PIL-10 must send `nickname` for bot new join/takeover and omit it on
  reconnect; PIL-11 must mirror the 64-byte exact Unicode server validation,
  decode the optional public slot nickname and use the server's stored name on
  reconnect. PIL-13 should refresh its running-game roster from `GET /lobby`
  after takeover (including spectator sessions); PIL-16 must verify this path
  with the actual browser/bot and server. PIL-08's workspace/review exceptions
  above remain open; they are not resolved by PIL-09 tests.

### PIL-10 — Bot nickname on admission

- **Depends:** PIL-09. **Primary scope:** `crates/ti4-bot-agent/` and focused
  bot tests; list additional necessary paths before editing.
- **Execution scope (2026-09-23):** P1; writable paths:
  `crates/ti4-bot-agent/src/{lib,main}.rs`,
  `crates/ti4-bot-agent/tests/real_e2e.rs`,
  `crates/ti4-bot-agent/README.md`, and this plan. No external references,
  downloads, destructive actions, or external-state changes; bounded loopback
  server tests only. Full Clippy excluded as requested (`ti4-model` blocker).
- **Contract:** A bot supplies an explicit configurable nickname for new join
  and explicit takeover, using the same bounded server contract as a human.
  Reconnect keeps the existing nickname. A bot still acts only as the
  server-authenticated `PlayerId`; a nickname is display data, not an advisor
  identity or a way to select a takeover target.
- **Gate:** Bot join/reconnect/takeover tests assert sent nickname, displayed
  roster name and unchanged acting identity (acceptance 13–14). Run formatting
  and affected-crate tests.
- **Progress (2026-09-23, uncommitted):** `--nickname` is required for bot
  startup; `BotConfig` validates it with the server's exact nickname validator
  before network access. New join and explicitly selected takeover send that
  nickname. The interactive rejoin list shows stored nickname and position,
  including for duplicate names. A credential reconnect omits nickname and
  keeps the server-stored value; advisor identity still uses only the
  authenticated `PlayerId`. The real-server bot test verifies the joined
  public name, reconnect without rename, running takeover rename, unchanged
  identity/position and revocation; the opt-in two-bot/advisor E2E now supplies
  names for the host and both bots.
- **Verification:** `cargo fmt --package ti4-bot-agent`,
  `cargo test -p ti4-bot-agent` (10 passed), and the opt-in
  `cargo test -p ti4-bot-agent --features real-e2e --test real_e2e -- --ignored`
  with the pinned Linux libtorch runtime (1 passed, 42.10 s) passed. No full
  Clippy or independent review was performed.

### PIL-11 — Browser nickname entry and retention

- **Depends:** PIL-09. **Primary scope:** `web/src/components/Lobby.tsx`,
  `web/src/App.tsx`, `web/src/hooks/useLobbySession.ts`, protocol DTO/decoder
  files and focused browser tests. Record additional necessary paths first.
- **Execution scope (2026-09-23):** P1; writable paths:
  `web/src/{App,App.test}.tsx`, `web/src/components/{Lobby,Lobby.test}.tsx`,
  `web/src/hooks/useLobbySession.ts`, `web/src/protocol/{types,decode,client.test}.ts`,
  `web/src/protocol/nickname.ts`, and this plan. No external references,
  downloads, destructive actions or external-state changes; browser tests use
  mocked HTTP and tab/local storage. Full Clippy excluded as requested.
- **Contract:** Offer a nickname field before creating a lobby, joining a new
  position, or explicitly taking over a disconnected player. Prefill it from
  `localStorage` and save the chosen local preference; keep credentials in the
  existing per-game `sessionStorage`. A successful reconnect shows the server's
  stored nickname, even if the local preference differs. There is no in-lobby
  edit or rename action; takeover can send a different nickname. Display
  nickname together with position on rejoin choices, since names may repeat.
  No nickname or credential goes into a URL.
- **Gate:** Browser tests cover create, new join, watch without nickname-based
  admission, local preference reuse, duplicate names, reconnect without
  mutation, fresh-computer takeover rename and invalid input (acceptance
  13–15). Run build and affected browser tests.
- **Progress (2026-09-23, uncommitted):** Creation, new join and explicit
  takeover require an entered nickname; successful admission stores it as a
  cross-game `localStorage` preference, while the per-game bearer credential
  remains in tab-scoped `sessionStorage`. Browser validation mirrors the
  server's 64-UTF-8-byte, exact trimmed Unicode/control/format contract; the
  decoder validates public nickname fields on occupied slots and null on open
  slots. The lobby shows the server's current nickname with physical position
  (including duplicate names) and offers no edit action after joining.
  Reconnect sends only the credential and displays the current server name,
  regardless of the local preference. Watch remains a spectator read with no
  admission, and neither name nor credential is put in the URL.
- **Verification:** `npm run build` and `npm test` passed (25 files, 178
  tests). Focused `npx vitest run src/App.test.tsx
src/components/Lobby.test.tsx src/protocol/client.test.ts` passed (26 tests).
  No full Clippy or independent review was performed.
- **Further plan/PIL-16:** The existing real-browser Playwright admission
  helpers still send pre-nickname create/join requests; update them to supply
  nicknames and assert takeover rename, duplicate-name presentation and
  restart recovery in PIL-16 before claiming the complete browser/server E2E
  gate. PIL-13 still owns nickname display in running-game components; PIL-08's
  workspace and independent-review exceptions remain open.

### PIL-12 — Colorblind-safe physical seat identity

- **Depends:** PIL-11. **Primary scope:** `web/src/presentation/boardPresentation.ts`,
  lobby/board/player UI components, seat-identity presentation helpers,
  `web/src/index.css` and focused tests. Split this row if the diff exceeds
  the atomic package limits; record additional paths before editing.
- **Execution scope (2026-09-23):** P1; writable paths:
  `web/src/presentation/{boardPresentation,boardPresentation.test,playerDisplay,playerDisplay.test}.ts`,
  `web/src/presentation/PlayerIdentity.tsx`,
  `web/src/presentation/PlayerIdentity.test.tsx`,
  `web/src/components/{Lobby,Lobby.test,Board,Board.test,PlayerSheet,PlayerSheet.test,SystemInspector,SystemInspector.test}.tsx`,
  `web/src/index.css`, and this plan. No external references/downloads or
  external-state changes; bounded local browser tests only. Full Clippy excluded.
- **Contract:** Map the _current physical position_ (1–8), not `PlayerId`,
  nickname, faction, or creation slot ID, to the eight Okabe–Ito colors:
  orange `#E69F00`, sky blue `#56B4E9`, bluish green `#009E73`, yellow
  `#F0E442`, blue `#0072B2`, vermilion `#D55E00`, reddish purple `#CC79A7`,
  black `#000000`. Give each position a distinct minimal inline SVG symbol
  with a text/accessible seat label. Ensure contrast on the dark UI (especially
  black) through outlines or badge backgrounds. An occupant moving in the
  lobby takes on the destination position's color/symbol; the started seating
  order freezes that mapping. Keep ownership colors consistent across lobby,
  player sheet, board, map details and legends without relying on color alone.
- **Gate:** Tests cover all eight position mappings, distinct icons, reorder,
  started-game consistency, empty positions, and visible/accessible non-color
  labels (acceptance 16). Run build and affected browser tests.
- **Progress (2026-09-23, uncommitted):** Pinned the eight Okabe–Ito colors
  and distinct SVG symbols to physical position (never ID, nickname, faction,
  or slot ID). Added outlined, numbered accessible badges including a
  white-outlined black position 8, used on occupied and open lobby slots,
  board legend, player sheet and system inspector. Board planet controllers
  and command tokens have position symbols alongside the ownership colors;
  board/inspector/player-sheet colors all use the same seat order. Lobby
  reorder immediately changes an occupant's position styling, while the
  started board uses the frozen `seating_order`.
- **Verification:** `npm run build`, `npm test -- --silent` (27 files,
  183 tests), and `git diff --check` passed. Focused tests pin all eight
  colors/symbols, black-badge contrast, open-slot badge, reorder color,
  board-owner mapping, and started-game board/legend/sheet consistency.
  Browser visual inspection and independent review remain outstanding;
  PIL-16 owns real-browser visual/a11y integration. No full Clippy run.

### PIL-13 — ID-free running-game presentation

- **Depends:** PIL-11 and PIL-12. **Primary scope:** `web/src/App.tsx`,
  `web/src/components/`, `web/src/presentation/` and focused tests. Split into
  smaller component clusters if needed and keep one shared display resolver;
  name exact edit paths in each child package before implementation.
- **Execution scope (2026-09-23):** P1; writable paths:
  `web/src/{App,App.test}.tsx`, `web/src/presentation/{playerDisplay,playerDisplay.test}.ts`,
  `web/src/presentation/PlayerIdentity.tsx`,
  `web/src/presentation/PlayerIdentity.test.tsx`,
  `web/src/components/{GameShell,EventLog,TurnStatusBar,Board,SystemInspector,PlayerSheet,PendingChoiceModal,PaymentDrawer,TacticalMovementOverlay,CombatResolutionModal,TradeDeskModal,AgendaBallotModal,ReactionStatusBar,ProductionBuilderDrawer}.tsx`
  and their focused tests, plus this plan. No external references/downloads or
  external-state changes; bounded browser tests only. Full Clippy excluded.
  Dynamic free-form server prompt/option/error conversion remains PIL-14;
  explicit typed participant fields are resolved here.
- **Contract:** Resolve each `PlayerId` to the _current server-provided_
  nickname plus seat context at the presentation boundary. Replace raw IDs in
  visible and accessible lobby, player-sheet, board/inspector ownership,
  turn/speaker/winner status, event log, spectator notices, trade and choice
  workflows, tooltips and errors. For duplicate nicknames, add the physical
  position and seat symbol wherever the referent would otherwise be ambiguous.
  Internal identity comparisons, React keys, protocol DTOs, choice IDs and
  submissions remain ID-based; no raw player ID is rendered in UI text,
  accessible names, titles or player-identity DOM attributes. Refresh displayed
  names after takeover, including in a running game. Handle a missing mapping
  with a neutral position/participant label, never a raw ID.
- **Gate:** Rendered-text and accessibility tests cover player/spectator views,
  duplicate names, reorder, takeover rename and all named surfaces. Audit
  player-identity interpolation sites and test that no raw player ID appears
  in the rendered UI (acceptance 17). Run build and affected browser tests.
- **Progress (2026-09-23, uncommitted):** A single `playerDisplay` resolver
  combines the current server lobby roster with the started seating order;
  duplicate nicknames gain both position and symbol, missing identities get
  a neutral label. `GameRoute` passes its periodically refreshed public
  lobby (including during running games and spectator views) into the running
  presentation context. Typed participant fields now resolve in player sheet,
  board hover/inspector, legend, turn/speaker/winner status, event log, trade
  partner and decision actor headers/notices. Player-identity test IDs and
  private-card owner attributes no longer expose raw IDs. Identity comparisons,
  React keys, target IDs and choice submissions remain stable ID-based.
- **Verification:** `npm run build`, `npm test -- --silent` (27 files,
  183 tests), and `git diff --check` passed. Render tests cover duplicate
  names, spectator-facing board/status/events, rename propagation without
  changing the underlying game event, unknown-ID fallback, trade target
  display with unchanged submitted option ID, and absence of raw IDs in the
  composed running-view markup. No independent review or real-browser E2E
  was performed. No full Clippy run.
- **Further plan:** PIL-14 must handle _free-form server-supplied_ prompts,
  option labels/descriptions and error messages (including the existing trade
  answer prompt that can contain a raw player ID). They are still passed
  through unchanged here: a typed reference can be safely resolved, while
  blindly replacing text risks changing game content or machine IDs. PIL-16
  must test the live polling/refresh path for player and spectator during a
  running takeover, all workflow labels and accessibility, and visually check
  badge/board contrast at narrow widths. PIL-08's workspace/review exceptions
  remain open.
- **PIL-13 follow-up (2026-09-23):** Reviewed running text outlets against
  the PIL-14 boundary: game choices, server errors, event/status descriptions,
  and board inspector actions now resolve participant prose at render time.
  The roster continues polling during running games for both players and
  spectators; the same original choice/event displays a renamed participant
  after takeover. Stable option IDs and source protocol records remain intact.
  Browser build and 187 tests pass; live-browser refresh remains PIL-16.

### PIL-14 — Dynamic prompt and error presentation boundary

- **Depends:** PIL-13. **Primary scope:** server-to-web choice/status/error
  presentation contracts and `web/src/protocol/`, `web/src/presentation/`,
  affected choice components, and focused server/browser tests. Name exact
  files before implementation; split server and browser changes if necessary.
- **Execution scope (2026-09-23):** P1; additional writable paths:
  `web/src/presentation/{participantText,participantText.test}.ts`,
  `web/src/presentation/PlayerIdentity.tsx`,
  `web/src/components/{GameShell,PendingChoiceModal,WorkflowShell,TurnStatusBar,EventLog,SystemInspector}.tsx`,
  `web/src/{App,App.test}.tsx` and focused component tests. Existing choice
  components receive display-only choice data through `GameShell`; protocol
  records and engine decisions are not edited. No network or external state;
  bounded browser tests only. Full Clippy excluded by operator instruction.
- **Contract:** Audit server-supplied choice prompts, option labels and
  descriptions, status/error text and dynamically constructed UI copy for
  embedded player IDs. Provide a typed participant reference or a reviewed,
  boundary-aware display conversion for references within text; never blindly
  replace arbitrary substrings, game content IDs, stable option IDs or private
  data. Unknown player references must display a neutral label rather than
  leaking an ID. Keep the original machine values for submissions and
  deterministic replay. After a running takeover, displayed references use
  the current nickname without rewriting historical protocol/game records.
- **Gate:** Tests with realistic prompts, option labels, errors, duplicate
  nicknames, ID substrings and takeover show zero raw player IDs in rendered
  or accessible text and unchanged submitted IDs (acceptance 17–18). Run
  formatting, build and affected crate/browser tests.
- **Progress (2026-09-23, uncommitted):** Added a shared render-only prose
  boundary for standalone `player_` references. It resolves known roster
  participants through the PIL-13 display resolver, and recognizes the
  server's 64-hex-digit generated identity shape to mask unknown references.
  It requires token boundaries, leaving embedded substrings, composite
  content/machine IDs and option IDs unchanged. The dispatcher renders a
  shallow display copy of choice prompts, labels and descriptions in all
  workflows; the underlying choice and submitted option IDs remain unchanged.
  Minimized choice banners, board inspector actions, event/status text, lobby
  and workflow errors use the same resolver. Duplicate names carry seat
  context; a running takeover refreshes presentation from the current public
  roster without rewriting historical decisions.
- **Verification:** `npm run build`, `npm test -- --silent` (28 files, 187
  passed), and `git diff --check` passed. New tests exercise a realistic
  offer prompt, option label/description, error, duplicate names, unknown
  generated ID, punctuation and substring boundaries, takeover rename, and
  unchanged submitted machine ID. No Rust server files changed; no Rust
  crate checks or full Clippy ran. Independent review and PIL-16 live-browser
  acceptance remain outstanding.
- **Further plan/PIL-16:** Exercise dynamic text across the live browser/server
  trade answer and error paths, including a running takeover and spectator
  refresh. Inspect actual accessible text for ID leaks and verify composite
  content IDs remain literal without being misidentified as participants.

### PIL-15 — Lobby interaction and accessibility polish

- **Depends:** PIL-11 and PIL-12. **Primary scope:** `web/src/components/Lobby.tsx`,
  `web/src/hooks/useLobbySession.ts`, `web/src/index.css`, and focused tests.
- **Execution scope (2026-09-23):** P1; additional writable paths:
  `web/src/{App,components/Lobby.test}.tsx`,
  `web/src/hooks/useLobbySession.test.tsx`, and this plan. Browser build and
  local unit tests only; no external state, downloads or destructive actions.
  Full Clippy excluded by operator instruction.
- **Contract:** Use the same styled button system for create, copy, join,
  watch, rejoin, reorder, ready, start and leave. Give actions coherent focus,
  hover, disabled and in-flight states; block duplicate submissions and
  conflicting controls while a request is pending, and surface an actionable
  response on failure. Never show readiness controls before authenticated
  join. Show Start only to the host, disabled with a readable reason until all
  positions are filled and ready. Clearly distinguish Ready and Not ready
  using text and different shape/weight/treatment, not identical color or
  color alone. Distinguish Connected and Disconnected using text plus symbols
  and contrasting colorblind-safe styling; presence remains advisory and
  takeover eligibility is still validated by the server. Preserve keyboard
  operation, readable small-screen layout, and explicit labels for reorder
  controls.
- **Gate:** State-matrix tests cover unjoined watcher, joined player, host,
  running game, pending actions, failed actions, disabled controls and keyboard
  labels. Visual and accessibility inspection checks readiness/presence without
  color (acceptance 19). Run build and affected browser tests.
- **Progress (2026-09-23, worktree):** Create and all lobby actions now use
  styled buttons with explicit pending/disabled states, shared hover/focus
  behavior and readable mobile reorder targets. The mutation hook serializes
  joins, takeovers, reorder, readiness, start and leave, ignoring conflicting
  requests even within one render; stale polling responses cannot overwrite a
  mutation result. Clipboard errors offer a manual-copy fallback, and mutation
  errors remain visible for retry rather than disappearing on the next poll.
  The host alone sees Start, with a specific open-position/unready-player
  explanation. Readiness and presence use distinct text, symbols, weight and
  border treatments in addition to color. Fixed an existing null-equality bug
  where an unauthenticated spectator matched an empty slot and saw Ready.
- **Verification:** `npm run build` and `npm test -- --silent` passed (29
  files, 193 tests); `git diff --check` passed. Added tests for the
  spectator/host/running state matrix, described disabled Start, keyboard
  reorder labels, pending controls, failed clipboard, duplicate create
  suppression, mutation serialization and retry. CSS includes 44px reorder
  targets and wrapping at narrow widths. Real-browser visual/accessibility
  inspection and independent review have not occurred; PIL-16 must verify
  these controls in the running UI at desktop and narrow viewport sizes.

### PIL-16 — Nickname and lobby UI end-to-end gate

- **Depends:** PIL-09–PIL-15. **Primary scope:** server/web E2E, browser tests,
  protocol fixtures and integration documentation. List exact paths first.
- **Contract:** Verify the complete create → watch → join → reorder → ready →
  start → reconnect → takeover sequence, including duplicate nicknames, a
  different nickname on takeover, an idle presence interval and server restart.
  Audit actual rendered and accessible UI for raw player IDs, seat symbol/color
  consistency and clear enabled/disabled states in lobby and running views.
  Inspect focus traversal, 44px mobile reorder targets, and the readiness and
  presence badges without color; verify that failed actions can be retried
  after an intervening lobby poll and that pending actions cannot be submitted
  twice from a real browser.
  Do not rebaseline fixtures just to conceal a failure.
- **Gate:** Acceptance 13–19 pass at appropriate unit, browser and real
  browser/server E2E layers. Run formatting, affected-crate/browser suites,
  workspace tests and the relevant PIL-08 recovery/privacy gates. Record
  exact results, source versions and independent review findings before
  claiming completion.

## Required Acceptance Tests

1. Creating a three-slot game assigns the creator exactly one stable player
   identity and host session; the returned public lobby contains no credential.
2. Two concurrent new joins receive distinct players and the first two open
   slots in server order; exactly one join wins each slot.
3. A full lobby rejects new player admission without state change.
4. A spectator read or spectator WebSocket subscription never creates a player,
   reports authenticated presence, or changes lobby version.
5. A reconnect using a valid session returns the same `PlayerId` and does not
   consume an open slot; an invalid session is rejected without mutation.
   A brief disconnect does not invalidate that credential. When a non-host
   explicitly leaves before start, their old credential cannot reconnect and
   the next new join receives a different `PlayerId`.
6. Host reorder preserves each player's credential, readiness, and host status;
   the next join uses the first open slot in the new order.
7. Only the host can reorder; malformed, duplicated, missing, or unknown slot
   permutations are atomic rejections.
8. Start rejects incomplete or unready lobbies and, once successful, freezes
   the exact final player order used by the engine.
9. After start, new admissions and reorders fail; valid existing sessions
   reconnect and can act as their original player identity. An unauthenticated
   client may take over a disconnected player after the grace period and gets
   the **same** `PlayerId`; a connected player cannot be taken over. Exactly
   one concurrent takeover succeeds. Old credentials and old live subscriptions
   lose access immediately, including after a server restart.
10. A bot started with only game/advisor configuration joins, survives an idle
    interval exceeding the presence grace period through authenticated
    heartbeats/pings, marks itself ready, reconnects as the same player, and
    sends advisor requests only for its authenticated player. A new process
    without the lost credential can take over its disconnected player.
11. Credentials never occur in public lobby responses, spectator snapshots,
    broadcast messages, URLs, structured logs, or debug output covered by tests.
12. A server restart preserves player credentials and running-game recovery;
    a crash before a complete game-init record does not misreport an active
    game. Presence reconstructs as disconnected without expiring credentials.
13. Host creation, new join and explicit takeover supply validated nicknames;
    duplicate names are accepted. Reconnect cannot rename a player. The
    publicly displayed name belongs to the stable player identity through
    reorder and start, never to the creation slot or its occupant's position.
14. A running or lobby takeover may replace the nickname while retaining the
    player identity, seat and authorization. Restart loads the new name and
    credential; a failed write changes neither. A bot sends a nickname but
    still acts only as its server-authenticated `PlayerId`.
15. The browser retains a nickname preference in `localStorage`, sends it
    on creation/new join/takeover, never sends it to rename on reconnect, and
    keeps the private session in `sessionStorage`. A stored preference does
    not override a server-bound nickname; no name or credential is in a URL.
16. Positions 1–8 use the pinned Okabe–Ito colors and eight distinct SVG
    symbols, with readable labels and contrast on the dark theme. Reorder
    changes the occupant's position styling; started games keep fixed seating
    order. Neither nickname nor player ID determines the seat styling.
17. No raw `PlayerId` appears in visible or accessible Web UI text or
    player-identity attributes in the lobby or running game, including board,
    inspector, status, events, spectator notices, choices and errors. Duplicate
    names remain distinguishable by seat context. Internal protocol and choice
    submissions still use stable IDs.
18. Dynamic server-supplied prompts, option descriptions, labels and errors
    that refer to participants display current nicknames/seat context without
    changing stable machine identifiers, rewriting historical game records,
    replacing unrelated substrings or leaking unknown IDs.
19. Unjoined viewers have no Ready control; only joined players can toggle
    readiness and only the host can start or reorder. Controls consistently
    show disabled/in-flight/error states and usable keyboard labels. Ready vs
    Not ready and Connected vs Disconnected remain distinguishable without
    reliance on color, at desktop and narrow widths.

## Non-Goals

- User accounts, passwords, third-party identity providers, or cross-game
  identity persistence. A browser's local nickname preference is convenience
  text, not cross-game identity or proof of ownership.
- Allowing a client to pick a position.
- Mid-game roster or seating-order changes.
- Automatic replacement of disconnected players after game start.
- Account-backed identity verification: anyone with the game link can explicitly
  take over a disconnected player's identity. No recovery codes, timed session
  expiry, automatic slot release, or host approval for takeover.
- Compatibility with the existing manual claim API, `p1`/`p2` identity scheme,
  old saved lobby records, CLI parameters, or WebSocket field names.

## Risks to Review

- This is an authorization and persistence-schema redesign; test every public
  boundary, restart path, and credential revocation path before release.
- Confirm game views, replay records, and advisor payloads treat `PlayerId` as
  an opaque stable identity and do not assume `p1` through `p8`.
- Preserve deterministic seating semantics: automatic admission and host
  reorder must be serialized and the final ordered player vector must be the
  only ordering passed to engine setup.
- Presence is a convenience signal, not proof of identity. Test grace-period
  edges, server-restart presence, concurrent takeovers, and revocation of old
  subscriptions; the game link must be treated as an invitation to a trusted
  group.
- Ensure private credentials cannot be copied into debug derives, error text,
  test snapshots intended for public views, or browser navigation state.
