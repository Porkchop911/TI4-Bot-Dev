# M05-004, M05-012…015 — Fleet limits and invasion

Batched: five plan packages, one verify, one commit.

| Plan ID | Covered by |
|---|---|
| M05-004 Fleet composition | `fleet.rs` — supply (37) and capacity (16) |
| M05-012 Bombardment | `invasion.rs::bombardment`, `bombardable` |
| M05-013 Landing choices | `invasion.rs::commit_ground_forces` |
| M05-014 Ground combat | `invasion.rs::ground_combat` |
| M05-015 Planet control | `invasion.rs::establish_control` |

Oracle `37061c51…`, guard verified clean before and after. Ported from `engine/fleet.py` and
`engine/invasion.py`.

**445 → 463 tests** (121 content, 273 engine, 68 model, 1 doc). Zero warnings, clippy clean.

## Rules that change play and are easy to lose

* **A captured planet is taken exhausted.** Its resources and influence belong to the round
  *after* the one you spent conquering it. Without this a planet can be spent the same turn it
  was invaded — the oracle notes the tell was the card arriving face up on the table.
* **49.5d** — if every committed force dies, the previous holder keeps the planet. Control does
  not fall to the invader by default.
* **49.5c** — recapturing your own planet changes nothing, and in particular does not exhaust it.
* **15.1f** — Planetary Shield blocks bombardment entirely, and a war sun ignores the shield.
  Omitting the exception would make a war sun strictly worse than the rules give.
* **37.1/37.1a** — fighters and anything being carried do not count against fleet supply.
* **16.2** — a space dock's fighter support is a *fighter-only* exemption, so it is not simply
  added to ship capacity. Read out of the printed ability text ("Up to 3 fighters"), because a
  Dimensional Tear supports six or twelve — treating every dock as generic would silently
  under-count that faction's whole point.
* Ship capacity carries fighters **and** ground forces from one pool, so troops in the hold
  squeeze fighters out. Pinned by `ground_forces_and_fighters_share_one_hold`.

Supply is enforced before capacity, because removing a carrier can strand the fighters it was
holding.

## Added to `ti4-content`

`UnitType::fighter_support()` — parses the dock allowance out of ability text.
`planetary_shield()` already existed.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No Fighter II half-ship supply arithmetic. | Upgraded fighters that may legally remain beyond capacity are a technology; technology is unimplemented. The oracle's `math.ceil` doubled-load accounting exists only to represent them. |
| No space cannon defence during invasion, no custodians step, no Dunlain Reaper. | Each is its own package or an unimplemented subsystem. |
| No laws, leaders, or faction hooks on control transfer. | `_establish_control` fires five registries the oracle has and this does not. |
| No structure destruction on capture (49.5a). | Structures are modelled but the sweep is not. |
| Choices asked inline through a `Table`. | Matches `combat.rs`; the step driver runs neither yet. |

## Open findings

1. **Nothing calls either module.** The tactical action still emits `TACTICAL_STEPS_UNRESOLVED`.
   Combat, invasion and fleet enforcement now all exist and all wait on the same wiring.
2. **Production is the remaining M05 gap** (M05-016…019), plus retreat, combat modifiers, and
   the differential/fuzz/benchmark packages.
3. **No independent review.** Waived by the project owner.
