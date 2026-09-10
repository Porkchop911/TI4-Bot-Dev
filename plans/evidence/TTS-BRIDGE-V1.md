# TTS bridge v1 evidence

## 2026-09-09 — rescue package opened

Specification: `plans/TTS_BRIDGE_V1_BOT_GAME.md`.

### Changes

- Added an offline decision/state cycle tracer to
  `crates/ti4-mlp/examples/mlp_simulate.rs`.
- The tracer wraps the real current MLP deciders, records the exact current Rust `Choice` and
  selected `ChoiceOption`, compares serialized authoritative `GameState` before and after each
  step, and stops with a compact dump after 100 identical no-progress decisions.
- It does not alter choice construction, MLP inference, or choice application.

### Checks

```text
cargo fmt -- crates/ti4-mlp/examples/mlp_simulate.rs
cargo check -p ti4-mlp --example mlp_simulate
```

Result: pass.

### Reproduction blocker discovered

The capture path named in the handover no longer exists. Current captures were inventoried; nine
have all six strategy cards dealt and six are pre-deal captures.

The rebuilt `mlp_simulate` and the independent `mlp_decide` example both terminate natively after
loading their bundle/capture and before returning an inference result. Direct execution previously
reported Windows exit `-1073741515` (`0xC0000135`, dependency load failure). The shared
`target/release` contains CUDA DLLs (`c10_cuda.dll`, `torch_cuda.dll`, CUDA runtime DLLs) while
`.cargo/config.toml` selects `out/libtorch-2.9.1-cpu`. Prepending the CPU library directory did not
yet produce a completed inference run.

This is an environment/artifact-loading blocker, distinct from the handover's claimed action-phase
livelock. Do not attribute a native loader termination to game progression.

### Next exact action

Build the CPU diagnostic into an isolated target directory with the pinned CPU `LIBTORCH`, verify
the runtime DLL inventory against the committed manifest, then run the tracer against
`out/bridge-captures/1788964950857_postkey.json`. Once the repeated choice is captured, add the
smallest regression fixture before changing engine behavior.

## 2026-09-10 — offline progress gate and first protocol issue

### Livelock root cause and correction

The semantic-cycle tracer reproduced the action-phase livelock in 76 steps. The repeated choice
was:

```text
player: letnev
prompt: action phase
selected: component|trade|generic (open a transaction with generic)
```

All imported `Player::faction` values were the `start_game` placeholder `generic`. Transaction
partner resolution and faction rules read `Player::faction`, not `Player::id`; the importer had
therefore collapsed six distinct factions into one rules identity.

Added `every_imported_seat_keeps_its_resolved_faction_identity`, observed the expected failure
(`left: "generic", right: "xxcha"`), then set each imported faction from its already-resolved
player alias. Complete importer golden suite: **12 passed, 0 failed**.

The formerly stuck capture then completed one round in **217 steps**, with 175,467 assigned and 48
OOV features (99.97% known). Two repeats produced byte-identical output SHA-256:
`0D44CF6A149E8E11BFF66733F723BAFD4991B60CE7A3FB01C04E576971F2A045`.

The same real-board capture completed a 50-round-bounded game in **1,894 steps** with 99.14%
feature coverage and no run error.

### Exact current-Rust decision surface

Added `crates/ti4-mlp/examples/tts_bot_game.rs`. It constructs a fully seated Rust game and seats
the current MLP directly in the normal `ti4_engine::game::Game` decision table. No Python engine or
bridge-specific choice reconstruction participates.

Seed 20260910 completed in round 9:

```text
decisions  2554
events     2883
decision_fingerprint 3cc34599c311de5bfa2bbc6b0dcfd616551a0e71c4f3ea1e7065f82e5adc2f89
event_fingerprint    ef885d883df5251881a45ca20cc380459848b3e03c529ad9669f9a48925e3518
state_fingerprint    78a3d04c944777cee370baf1c891bbc0389967d058a50d478822c85f12d8462d
```

An immediate second complete run reproduced all three fingerprints exactly.

### First architectural issue

The existing executor protocol cannot represent the complete current Rust state. Its verbs cover
movement, placement/removal, cards, economy tokens, command tokens, scoring, speaker, card flips,
deals and turn advancement, but there is no operation for persistent unit damage. This is required
for sustained ships that remain damaged between decisions and for later repair.

Additionally, `Game::emit_typed` passes a payload-bearing `Event` through the timing resolver but
does not retain that event publicly. `Game::events` receives only string labels, with payloads
discarded. Event-to-command translation therefore cannot reliably recover the player, system,
planet, unit, amount, or card involved from the observable event stream.

This is not an MLP/decision-surface failure. It is the next bridge contract decision: either add a
public, deterministic applied-effect/typed-event journal and extend the Lua protocol for missing
physical states (recommended), or attempt state-delta inference after every step. The latter is
ambiguous for moves versus remove/place pairs and cannot express semantic effects that leave the
same aggregate state.

## 2026-09-10 — resolved typed-event journal implemented

`timing::Resolver` now retains every successfully resolved payload-bearing `Event`, including
cancelled events, in deterministic emission order and exposes it through `applied_events()`.
The existing string event log is unchanged.

The journal lives in `Resolver`, not only in `Game::emit_typed`: subsystem and nested timing
emissions call the resolver directly. A first attempt at a game-driver-only journal missed the
cancelled `STRATEGY_CARD_CHOSEN` event, and its regression test failed 1 event against 2. Moving the
journal to the actual emission boundary made the test pass and preserves the cancellation flag and
original payload.

Focused check:

```text
cargo test -p ti4-engine timing_cancellation_keeps_a_strategy_card_on_the_mat --lib
1 passed, 0 failed
```

An existing unrelated duplicated-`#[test]` warning at `game.rs:6708` remains present; this package
did not create or alter it.

### Lua damage/repair blocker

The pinned executor and bridge sources contain no damage command and no code documenting how this
TTS mod physically marks sustained units. The uploaded hex summary also contains no damaged-state
field, so a guessed implementation (rotation, tint, or damage token placement) could neither be
validated nor reconciled.

The exact representation appears to exist only in the installed/live TTS mod or save, outside the
approved repository paths. Inspecting that private user artifact is P3 under
`plans/SCOPED_PERMISSIONS.md`. The next safe action is read-only inspection of the active save/mod's
unit-damage mechanism, then a golden Lua command/executor test before any save is patched.

## 2026-09-10 — acknowledgement-gated Rust coordinator and live dry run

The Rust bridge now has a `Coordinator` which queues one already-translated command at a time,
waits for the executor result carrying that exact command id, and stops on refusal, executor error,
malformed result, transport failure, or timeout. A complete engine transition is translated before
the first command is queued.

`crates/ti4-mlp/examples/tts_live.rs` reads the latest loopback upload, imports the actual faction /
colour seating, seats the existing CPU MLP checkpoint directly in the current Rust `Game`, and is
dry-run by default. Its first run against the current round-one six-player table produced:

```text
mode       dry-run
round      1
seats      6
step       0: 1 command(s)
  {"action":"card","color":"White","name":"Technology","to":"area"}
```

No TTS command was queued by this run. Live mode is intentionally limited to one engine step until
fresh post-command telemetry reconciliation exists; acknowledgements prove executor completion,
not that the next decision sees an identical table.

The complete deterministic bot game still finishes at 2,554 decisions with unchanged decision,
event, and state fingerprints. Exact named objective scoring now owns the score update, removing a
second overlapping `score_total`; the command census reports 36 `score` commands for the same 36 VP
changes. Exact card destinations are processed before discards, so player-to-player transfers do
not depend on seat iteration order.

The historical Lua now supports exact named cards to hands, reinforcement command-token placement,
and named exploration attachments to exact planets. Save 55 was repatched from those sources; all
four injected scripts are current and compile. The focused historical executor/conformance/patcher
suite passes 70 tests.

## 2026-09-10 — damage representation resolved offline

User-authorized read-only inspection of `TS_Save_55.json` found the mod's `Burninator` object and
its complete damage convention. A damageable Generic/Figurine is damaged exactly when its Z
rotation is between 90 and 270 degrees. `Burninator` observes that flip and attaches the
`sustained` fire decal; returning the unit upright removes the decal. Damage is therefore a stable
physical property that can be acted on and reconciled without inventing metadata.

Added Rust command builders for `damage`, `repair`, and damage-selective `remove`. The latter is
necessary when fresh and damaged copies coexist: removing an arbitrary copy preserves aggregate
unit count but can leave the wrong survivor damaged. Focused bridge command tests: **12 passed,
0 failed**.

The event journal now reserves a parent event's slot before opening timing windows and replaces it
with the final resolved/cancelled form afterward. This fixes a subtle but consequential ordering
case: a nested `SHIP_DESTROYED` (for Direct Hit) must follow its parent `SUSTAIN_DAMAGE_USED` in
emission order even though the nested event completes first. Both the nested-order regression and
the cancelled-strategy-card regression pass.

The remaining boundary is a write outside this repository: the authoritative executor and save
patcher are in the pinned historical repository, and installing the extension rewrites the private
TTS save. Neither has been mutated during this read-only inspection.

### Executor installation

After explicit authorization, the pinned historical executor was extended with `damage`, `repair`,
and damage-selective `remove`. Planet filtering is supported so two mechs in different planet
regions of one system cannot be confused. The implementation uses `object.is_face_down` and
`object.flip()`, exactly matching Burninator's convention.

Historical executor/patcher tests: **48 passed, 0 failed**. Rust command-builder tests after the
planet-aware payload update: **12 passed, 0 failed**.

The backup-producing patcher upgraded `TS_Save_55.json`; its subsequent `--check` reports all four
objects `patched, and current`, and the patcher's embedded Lua compiler reports all four scripts
compile. The pre-patch save remains at `TS_Save_55.json.pre-bridge-backup`.

Live execution is now gated only by TTS loading the rewritten save. Once loaded,
`!gamedata localhost 30` starts telemetry and polling so an actual damage/repair round trip can be
observed without editing the running table through any other channel.

## 2026-09-10 — live damage round trip and return-channel repair

The first live attempt exposed two independent transport defects before any board mutation:

1. The stock telemetry helper cached a CRC before knowing whether its HTTP request succeeded, so a
   bridge started after the first attempt could not be reached again without changing the board.
2. Command polling was bootstrapped only from the successful telemetry-response callback. TTS sent
   the POST and the server consumed its reply, but TTS classified the response as an error and never
   called `onBridgeResponse`; command delivery and its return channel therefore stayed split.

The installed executor now wraps an explicit `startPeriodicUpdates` call to clear the failed CRC
cache and start `bridgeStartPolling()` directly. Telemetry no longer has to succeed before the
independent two-second command channel exists. Historical focused tests: **63 passed, 0 failed**;
the repatched save compiled and reported all four scripts current.

Live proof against reloaded `TS_Save_55`:

```text
result 1 ok: ping
damaged 1 units in 6
result 2 ok: damage
repaired 1 units in 6
result 3 ok: repair
```

The tested target was one Red dreadnought in tile 6. The repair reversed the mutation and the queue
returned to zero.

## 2026-09-10 — first strict event translations and next issue

`TtsCommands` is no longer a stub. It translates final resolver events strictly:

- `SUSTAIN_DAMAGE_USED` -> `damage`, retaining player, system, optional planet, and unit;
- `SHIP_DESTROYED` -> damage-selective `remove`;
- cancelled and explicitly no-physical-effect events -> no commands;
- every unclassified event -> an error, never silent success.

`SHIP_DESTROYED` now carries the destroyed unit's `damaged` flag; Direct Hit's staged destruction
records `true`. This prevents a table with fresh and damaged copies of the same hull from removing
the wrong physical survivor. Four focused translation tests pass.

The next translation boundary cannot be solved by typed events alone: Emergency Repairs directly
clears unit damage flags without emitting a repair event, and other physical state changes likewise
do not all have complete payload-bearing events. Meeting the translation gate therefore requires a
hybrid effect collector: use typed events for semantic identity/order and authoritative before/after
state differences for otherwise unannounced physical mutations. Any delta that admits more than one
physical explanation must stop as ambiguous rather than queue guessed commands.

## 2026-09-10 — hybrid unit-effect collector and neutral-unit blocker

The hybrid collector now diffs the complete authoritative unit multiset after every Rust decision.
It preserves damage-state flips as one operation and classifies add/remove pairs by owner, physical
unit type, damage state and exact source/destination region. One-to-many and many-to-one transport
are deterministic; a many-source/many-destination transition is refused because snapshots do not
identify the route pairing.

The fixed-seed complete game contained:

```text
unit_delta {"add": 261, "damage": 5, "remove": 225, "repair": 3}
pairing    {"ambiguous_move_steps": 2, "ambiguous_move_units": 5,
            "destroy_units": 106, "move_units": 172, "place_units": 193}
```

Inspection showed both nominally ambiguous steps were valid many-to-one transport: infantry moved
from two exact planet/system regions into one destination. The command protocol therefore now uses
one-based planet indexes and `0` for the space area on placement, removal, damage and relocation.
The Lua executor filters exact source regions and can place the destination directly in space or on
one planet. Legacy commands that omit location fields retain their old whole-hex behaviour.
Faction-specific Rust ids such as `sol_carrier2` and `l1z1x_dreadnought` now normalize to the shared
TTS Carrier and Dreadnought models.

Verification after these changes:

```text
ti4-bridge: 73 unit + 6 hex golden + 12 import golden + 3 wire golden passed
historical executor/save patcher: 51 passed
TS_Save_55: all four patches current and all four embedded Lua scripts compile
```

The first complete-game dry-run translation stopped, correctly and without queueing a guessed
command, at step 880 (round 5):

```text
Add neutral Destroyer x1, fresh, system 19 space
```

This is Pirate Contract, not a seated-player mutation. Save 55 contains a physical object named
`Neutral Destroyer`, but the executor's ordinary unit lookup is color/seat based and the Rust
translator has no color for the deliberately unseated `neutral` owner. The next protocol decision
is therefore an explicit neutral-unit inventory/lookup path; mapping `neutral` to a player color
would be incorrect.

### Neutral inventory decision and complete unit dry run

The operator confirmed that the table's brown plastic is the intended neutral inventory. The
translator now maps only the unseated `neutral` unit owner to Brown for physical commands; it does
not add a Brown player to Rust state or the MLP decision surface.

The same complete fixed-seed game then passed unit-effect translation through all 2,554 decisions:

```text
round      9
finished   true
commands   {"damage": 5, "move": 142, "place": 123, "remove": 84, "repair": 3}
decision_fingerprint 3cc34599c311de5bfa2bbc6b0dcfd616551a0e71c4f3ea1e7065f82e5adc2f89
event_fingerprint    ef885d883df5251881a45ca20cc380459848b3e03c529ad9669f9a48925e3518
state_fingerprint    78a3d04c944777cee370baf1c891bbc0389967d058a50d478822c85f12d8462d
```

Rust verification increased to **74 unit tests**, with all 6 hex-summary, 12 import and 3 wire
golden tests also passing. This establishes complete-game dry-run coverage for physical unit
movement, production, destruction, damage and repair. It does not yet claim live-table execution
or coverage of every non-unit physical effect.

### Live exact-region movement proof

After reloading patched Save 55, localhost health reported five telemetry uploads, an empty queue,
and active independent polling. One Red dreadnought was moved from tile 6 space to empty tile 35
space using explicit `from_planet: 0` / `to_planet: 0`, then restored to tile 6 only after the first
result was acknowledged:

```text
moved 1 units from 6 to 35
result 4 ok: move
moved 1 units from 35 to 6
result 5 ok: move
```

The table therefore executed the new exact-region movement path and was returned to its starting
unit position. This is a reversible single-unit live proof, not yet a complete live bot game.

## 2026-09-10 — live Rust MLP draft and atomic-action blocker

After reloading Save 55, the acknowledgement-gated CPU runner physically completed the strategy
draft. Fresh telemetry confirmed White/Technology, Blue/Leadership, Purple/Warfare,
Yellow/Construction, Red/Trade, Green/Diplomacy, with the action turn on Blue.

This live exercise found and fixed two importer defects: a partial draft now remains in Strategy
until every seat has its proper allotment, and imported holdings are removed from
`unclaimed_strategy_cards` so the same physical card cannot be drafted twice. The bridge suite is
now 80 unit + 6 hex + 13 import + 3 wire tests, all passing.

It also proved that executor acknowledgements precede settled telemetry. A three-second read was
stale; the next periodic upload confirmed both card placement and exact turn. Reconciliation must
wait for a genuinely newer upload.

The first action-phase dry trace exposed the next blocker without changing TTS. A tactical action
spans several `Game::step()` calls. Early activation/movement steps are translatable, but a later
continuation had multiple possible infantry sources for one landing destination; snapshot pairing
selected tile 14 even though tile-27 space was also possible. Streaming per step could therefore
partially mutate TTS before the ambiguity is discovered.

The coordinator must collect and validate a complete logical action before queueing its first
command, and movement/landing attribution must use typed semantic events rather than arbitrary
structural source pairing. Action-phase live execution remains disabled until that boundary exists.
