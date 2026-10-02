# Space combat overlay states

## Goal

Make `SpaceCombatOverlay` explain **what is happening now** without mixing pre-roll predictions, rolled hits, hit assignment, and surviving fleets in one view. Keep every offered combat decision usable by its actor and the battle visible to opponents and spectators.

The intended flow is:

`Before rolls → Roll results → Resolve hits → Retreat / next round → Before rolls` (repeat), then `Combat complete`.

- **Before rolls:** show both fleets, including ships damaged in earlier rounds, and current win odds.
- **Roll results:** show hits by ship type and total hits for each side. Distinguish the first-round anti-fighter barrage from normal space-combat rolls.
- **Resolve hits:** show the roll totals, incoming hits still to resolve, and where hits have gone. A sustain marks a surviving ship damaged; a casualty reduces its ship count. Keep damaged ships visible in the fleet. Hit-cancellation reactions belong here.
- **Retreat / next round:** after _both_ sides finish resolving hits, show the resulting fleets. An announced retreat selects its destination at this point; otherwise the next round starts. The stay/announce-retreat choice happens **before** that round's rolls, not after it.
- **Combat complete:** close or dock the battle view and let the next game decision proceed. Defer an automatic post-combat recap for now; it is not the between-round fleet view. A later optional recap can be designed separately.

Only show odds when they correspond to the **current pre-roll fleet and conditions**. Hide them while a new calculation is pending or has failed, from the first combat roll until the next pre-roll state, and after the battle ends. Do not show a stale percentage or substitute the current heuristic as though it were up-to-date simulated odds.

## Current state

- `web/src/components/SpaceCombatOverlay.tsx` renders fleet rows, damaged counts, per-type roll badges, round-hit totals, and the odds card together for most active choices. It already has sustain, casualty, retreat, and reaction controls, plus actor/spectator modes. Fleet rows remember destroyed types within a battle, but the view does not separate the stages above.
- The overlay requests advisor odds whenever its derived unit/damage input changes. That includes **mid-assignment**, when one side's hits may be resolved and the other's are not. An earlier advisor response can remain on screen while the replacement is loading or fails. The heuristic fallback also changes as ships are removed, regardless of phase.
- `web/src/components/GameShell.tsx` routes battle choices to the overlay and shows it to spectators while `board.combat` exists. When combat ends it retains the last combat board and combines it with current system units to show an automatic recap that can block the next decision until docked (`GameShell.test.tsx` covers this behavior).
- `crates/ti4-engine/src/combat.rs` already distinguishes opening, retreat announcements, rolling, sustaining, assigning, retreating, and done. Both sides' combat hits are computed before either side absorbs them. First-round anti-fighter barrage occurs before retreat announcements; a declared retreat happens after the round's hits are resolved.
- `crates/ti4-server/src/projection.rs` exposes a public `board.combat` while `active_space_combat` is set. Its `stage` field is currently the **pending choice's subtype**, not an authoritative engine phase. It projects round hits, remaining hits, and dice, but does not expose a reliable round-start snapshot, hit-assignment ledger, or distinct roll/result phase. Types live in `crates/ti4-server/src/protocol/view.rs` and `web/src/protocol/types.ts`.
- Rolls and automatic casualty resolution can advance between client snapshots. The browser cannot assume every internal engine stage will be separately observable just because it exists in `combat.rs`. Existing actor/non-actor and four-viewer tests are in `web/src/components/SpaceCombatOverlay.test.tsx`, `GameShell.test.tsx`, and `web/e2e/space_combat_four_views.spec.ts`.

## Target architecture

### One public, authoritative presentation phase

Expose a small, viewer-independent combat presentation phase on `board.combat`, for example `pre_roll | barrage | resolving_hits | retreating | complete` (use the actual engine boundaries when implementing). Keep decision subtype separate: it identifies the **offered action**, not the battle phase. A nested reaction or card choice must not erase the underlying phase. Continue to use the existing battle identity, system, attacker, defender, and round; do not infer combat from two fleets sharing a system.

Treat `roll results` as the entry to `resolving_hits` if the server immediately advances from rolling to the first reaction or sustain choice. Do not require an artificial acknowledgement click or pause just to show dice. The same screen can start with produced hits and then progressively show their resolution. A dedicated roll-only pause is a future UX change if real players need time to read the rolls before assignment.

First-round barrage needs its own label and hit totals, scoped independently of the combat round's normal dice. Do not reuse a previous round's dice or totals under a new round number. Provide a stable battle/round identifier (including a battle occurrence or sequence if the same seats fight again in one system) so client snapshots and odds cannot bleed into a new battle or after undo/replay.

### Round view versus live fleet

Use the public board as the source of truth for **current** ships and their damaged status. At the beginning of hit resolution, retain a public round-start fleet snapshot or equivalent server-projected deltas so each side can see the change caused by hits: e.g. `2 fighters → 1 (1 destroyed)` and `1 dreadnought → 1 (now damaged)`. Pair this with produced hits, cancelled/absorbed hits, and remaining hits where available. Do not guess an assignment from a die's rolling ship type: the rolling ship produced a hit; the opponent chooses what absorbs it. Keep already-damaged ships distinguishable from ships damaged **this round**.

Keep sensitive offers actor-only. All participants may see public fleet changes and combat rolls; a spectator must not receive another player's private card hand or option payloads. The UI submits only server-offered IDs and never generates legal choices from presentation data.

### Odds have a validity window

Calculate odds for the initial pre-combat fleet before the barrage, then invalidate and recalculate if barrage losses, repairs, or other pre-roll changes alter the fleet. Recalculate again at the next round's pre-roll board after hits and retreats are settled. Invalidate the displayed odds immediately if that input changes, the phase changes to combat rolls/hit resolution, a new battle starts, or history is restored. Show a neutral `Calculating odds…` state or no odds card until the matching response arrives; on failure show a short unavailable message rather than an old percentage. On roll results, assignment, retreat destination, and combat completion, show no odds. These are _unconditional future-combat estimates_, not probabilities updated to account for already-known dice or a half-resolved hit queue.

Key/abort requests by battle identity, round, phase, and normalized odds input (unit counts, damage, factions, and any modifiers the advisor actually models). Accept a response only if its key still matches the current pre-roll view. Do not imply that the advisor includes cards, technologies, or modifiers it was not given; document/label the modeled scope if needed.

## Implementation details

1. **Define the phase contract.** Trace observable pauses from opening/barrage through retreat announcements, rolls, hit-resolution reactions, sustain/casualties, retreat destination, and win reactions. Decide which are publicly observable without pausing the engine. Project a stable phase and round/battle identity in `CombatView`; keep `stage`/choice subtype distinct during nested reactions. Explicitly reset or scope dice and produced-hit totals at the next round so old results cannot appear as new ones.
2. **Project assignment evidence.** Add the smallest public data needed to display the round-start fleets and resolved changes (or a per-round public hit ledger with `sustained`, `destroyed`, `cancelled`). Tie it to the battle and round and update it for automatic as well as player-selected casualties. Keep existing `hits_to_assign` semantics for cancellation windows; clarify whose hits remain and whether that count is for the current queue or both sides.
3. **Split overlay presentation.** Derive a typed view model from the combat phase, board, and pending choice. Render pre-roll fleet/odds, barrage or round rolls, hit resolution with assignment markers, and retreat/next-round fleet status. Render offered reactions and follow-up choices within their underlying phase. Keep current damaged badges in all fleet views; hide only per-hit assignment markings once the round has finished. Maintain stable casualty row positions while assignment is interactive; clear round-only markers at the next round.
4. **Gate odds.** Replace the current unit-change-driven display with a pre-roll-only request tied to the exact input key. Clear stale results synchronously when the key/phase changes, abort superseded requests, and ignore late responses. No odds card during rolls, hit resolution, retreat destinations, or after combat. If odds are still being computed, indicate that briefly without showing percentages.
5. **Simplify the completion hand-off.** Remove the automatic blocking recap from `GameShell` and let the next decision render immediately. Preserve the existing public event log as the record of the battle. If a recap is wanted later, design it as an explicitly opened, nonblocking view of final ships (including persistent damage), not a continuation of hit assignment.
6. **Verify at the boundaries.** Cover before first roll, barrage versus normal roll, both players' produced hits, Shields Holding cancellation, one sustain, one casualty (including an automatic casualty), the other side's queued hits, next round with damaged survivors, retreat announcement/destination, win reaction, completion, spectator visibility, slow/out-of-order advisor replies, and undo/replay. Extend the existing four-viewer battle scenario rather than constructing decisions solely in the browser.

## Crates to change

| Area                     | Files / purpose                                                                                                                                                                                                                                                                                                                          |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ti4-engine`             | `crates/ti4-engine/src/combat.rs` and, if necessary, the combat state/driver: expose public phase/round boundaries and assignment facts across pauses; preserve the existing simultaneous-hit and retreat rules.                                                                                                                         |
| `ti4-server`             | `crates/ti4-server/src/projection.rs`, `src/protocol/view.rs`, and focused projection tests: publish the phase, scoped rolls/hits, and public assignment evidence consistently for every viewer.                                                                                                                                         |
| `web` (not a Rust crate) | `web/src/protocol/types.ts`, `src/components/SpaceCombatOverlay.tsx`, `src/components/GameShell.tsx`, their tests, and `web/e2e/space_combat_four_views.spec.ts`: phase-aware rendering, odds validity, and nonblocking completion. `web/src/services/advisorService.ts` only if the request inputs or response handling need extending. |

`ti4-advisor` should not need a change for display timing; only consider it if the proposed odds contract requires new simulation inputs that the current `/battle` endpoint cannot represent.

**Done when:** all viewers see the same public battle phase and hit outcomes; only the actor sees actionable private choices; odds appear only for the matching pre-roll state; persistent ship damage survives into later rounds; old roll/assignment markings do not; and the next game decision is available immediately when combat finishes.

## Implementation progress (2026-09-27)

- [x] Engine records a battle sequence, presentation phase, round-start fleet, separate first-round barrage results, and both seats' remaining hit queues. The server projects these public facts independently of the pending choice subtype; normal dice and hits reset at the next round.
- [x] The overlay separates pre-roll odds, barrage/normal roll totals, live hit resolution (new damage and destroyed counts versus the round start), and post-hit retreat views. Damaged survivors stay visible, while round-only annotations disappear on the next round.
- [x] Advisor requests run only in the pre-roll state, keyed by battle/round/input and aborted on changes; pending/failed responses show no percentage. The automatic completion recap no longer delays the next decision.
- [x] The four-view dev scenario includes an attacking destroyer for anti-fighter barrage and stops at an opening combat reaction. The browser test checks matching public phases, barrage dice, simultaneous queues, damage, casualties, later rounds, private offers, completion, and undo/replay.
- [x] Focused UI tests cover the phase display, hand-off, unavailable/late odds, and round-scoped damage markers.

Verification: web unit suite, four-view browser spec, server suite, TypeScript typecheck, and scoped Rust checks pass. The full engine suite has an unrelated pre-existing `fingerprint::tests::the_participating_context_fields_are_pinned` failure (`space_battle` is declared but absent from the pinned V2 field set). Workspace-wide `cargo check` requires the unavailable libtorch headers for `torch-sys`; `cargo check -p ti4-server` succeeds.
