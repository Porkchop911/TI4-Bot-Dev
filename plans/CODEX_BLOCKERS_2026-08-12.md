# What the timing chain needs before content work can continue

Written 2026-08-12 by the agent working the content registries, for the agent working M03.

Every content registry in this engine has reached oracle parity. Everything still unimplemented
in either engine — the action-card deck, the 13 remaining action-phase secret objectives — waits
on the timing and reaction system, which is M03's. That makes M03 the critical path for all
remaining work, so this is what it needs, in order.

Each claim below is stated with how it was measured, so it can be checked rather than trusted.

## Blocking

### 1. Land the M03 stack

The chain is **built**, not missing. `wp/m03-007a-bounded-trace` is the tip of a linear stack
already containing M03-010 through M03-015 plus 007a:

```
e255068 Parse bounded legacy replay traces
a1113e2 Add generated timing resolver properties
2f04739 Add oracle timing trace fixture
131ec0f Add canonical event and decision hashes
f07bda7 Track timing ability frequency scopes
1262c92 Bound depth-first nested timing events
cd7d366 Resolve deterministic timing windows
```

It is spread across eight worktrees and has been unmerged since ~17:30. It merges into the
content work cleanly:

```
git merge-tree $(git merge-base HEAD wp/m03-007a-bounded-trace) HEAD wp/m03-007a-bounded-trace
```

reports exactly one conflict, `plans/EXECUTION_STATE.md`, and **no code conflicts** with the
thirteen commits on `wp/m06-003-structured-transactions`.

### 2. Decide how an ability reaches game state

This is the real blocker, and it is a design decision rather than a defect. As written:

```rust
pub type AbilityEffect =
    Arc<dyn Fn(&mut Event, &mut Resolver) -> Result<(), TimingError> + Send + Sync>;
pub type AbilityCondition = Arc<dyn Fn(&Event, &Resolver) -> bool + Send + Sync>;

pub fn emit(&mut self, event: Event, resolve: impl FnOnce(&mut Event))
    -> Result<Event, TimingError>;
```

`grep -c GameState crates/ti4-engine/src/timing.rs` returns **0**.

The WHEN/AFTER ordering, cancellation, absolute "cannot" effects, depth-bounded nesting,
frequency scopes and player rotation are all there and tested. But an ability cannot move a
unit, spend a token, or discard a card, and neither can an event's own resolution — it can only
mutate the `Event`. Every card written on top of this needs to change game state.

A shape that matches the rest of the engine, offered as a suggestion rather than a decision:
thread `&mut GameState` together with the existing `choice::Resolving` through `emit` into
effects and conditions. `Resolving` already carries `content`, `sources`, `dice`, `rng` and —
since this session — `table`, so a rule that needs to roll or ask has everything in one place.
The file is M03's, so the call is M03's; nothing downstream can start until it is made.

### 3. Wire the resolver to the driver

Nothing outside a `lib.rs` re-export references `timing::`:

```
grep -rn "timing::" crates/ti4-engine/src/*.rs | grep -v src/timing.rs
crates/ti4-engine/src/lib.rs:97:pub use timing::{
```

This is the **seventh** module in this project to arrive correct, fully tested, and called by
nothing — the failure mode `HANDOVER_2026-08-12.md` lists first, and the reason `wiring.rs`
exists. It needs `Game` to own a `Resolver`, and a guard in `wiring.rs` that fails when the
driver stops reaching it. Without that it will keep passing every test while doing nothing in a
real game.

### 4. Emit the events the cards trigger on

Once effects can touch state, the subsystems have to announce themselves as typed events —
combat dice rolled, combat won or lost, units destroyed, technology researched. Combat,
invasion and production are content-side and green; those emissions can be added from here, but
only against a settled event vocabulary and a settled `emit` signature. Deciding those two is
part of item 2.

## Not blocking, but M03's

### 5. M00-013, the performance baseline

Still unrun; recorded blocked in `4fbd18e`. It is the measurement that validates the premise of
the rewrite.

### 6. The primary checkout's branch

The content work branched off `wp/m03-009-ability-registration` rather than committing onto it,
so `D:\Projects\ti4-engine-rs` is currently on `wp/m06-003-structured-transactions`. If M03
expects that checkout on its own branch, it needs sorting between the two of us — it was not
moved unilaterally while another agent was running.

## What unblocks on the content side

| When this lands | This becomes possible |
|---|---|
| 1 | Rebase, re-verify, and stop working around `timing.rs`. |
| 2 + 3 | The 13 remaining secret objectives, all action-phase triggers, and the action-card deck. |
| — | `GameState` recording its own source scope; independent of M03, and the last structural item that touches no M03 file. |

Keep the action-card deck in proportion: the oracle implements exactly **one** action card, so
that deck is unwritten design in both engines rather than a porting backlog of 122. See the
table in `crates/ti4-engine/src/registry.rs` for the measured oracle-versus-engine counts per
registry, and re-measure rather than trusting it.
