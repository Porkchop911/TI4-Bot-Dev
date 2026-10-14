# BF-SSRUU-L1Z1X activation copy timing scope

## Permission and bounds

- Permission class: P1.
- Writable paths for this bounded task: `crates/ti4-engine/src/leaders.rs` and this file only.
- Read-only paths: repository instructions, timing/reaction/activation implementation, content corpus.
- Network/processes: none beyond local read and focused cargo validation.
- Artifacts: this timing/evidence note; no generated files intended.
- Destructive actions/external changes: none; no commits or pushes.

## Specification

Printed PoK `l1z1xagent` I48S says: “After a player activates a system: You may exhaust this card to allow that player to replace 1 of their infantry in the active system with 1 mech from their reinforcements.” The timing listener is AFTER `SYSTEM_ACTIVATED`; event `player` and `system` identify the activating seat and active system. For copied use, Ssruu's owner chooses whether to use the copied text, the activating player remains the beneficiary and owns the infantry/mech supply, and only the borrowing `yssarilagent` (Ssruu) exhausts. A source player's `l1z1xagent` may be readied or exhausted and is never mutated.

This bounded copy only dispatches the supported L1Z1X activation text. Exact source eligibility is derived at resolution from other seated players who actually hold `l1z1xagent` in `Readied` or `Exhausted` status. Borrower roster is static at registration; Ssruu readiness and source availability are dynamic. The active player's own mech faction and reinforcement supply govern whether the offer exists. Any accepted transition replaces one of the active player's infantry in the active system with one mech; no target infantry or no available mech means no offer and no exhaustion. Invalid/refused decisions leave units and leaders unchanged.

## Existing repository facts

- `reactions::arm` registers one static action-card reaction slot per seated player and timing window. It is the shared registry hook and is outside this package's writable paths.
- `leaders.rs` currently has native effects for the six agents `hacanagent`, `jolnaragent`, `l1z1xagent`, `letnevagent`, `solagent`, and `xxchaagent`; no existing copy dispatcher/helper exists.
- The current native I48S branch targets `state.active` and uses that seat's mech/supply. It exhausts the named `l1z1xagent`; copy use must therefore use a separate helper to avoid spending or mutating the source agent.

## Validation record

Implementation and focused resolver results will be appended here before handoff. The root integration seam is to call the exported listener factory from the shared registration path in `reactions.rs::arm`, after the static seated-player roster is known. No change to `reactions.rs` is authorized in this task.
