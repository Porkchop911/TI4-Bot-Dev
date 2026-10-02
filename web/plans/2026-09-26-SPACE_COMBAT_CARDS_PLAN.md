# Space combat cards in the battle overlay

## Goal / why?

Make a space battle feel like one continuous, understandable workflow. When a player can play a card during a space battle, the choice should appear in `SpaceCombatOverlay` alongside the fleets and dice, with the card's name, effect, timing, and a clear **Pass** action. After a card is played, everyone should see a public, readable history entry such as **“Sol played Shields Holding.”** An unplayed card or a private hand must not be revealed.

The current `reaction:sol:HITS_TO_ASSIGN:when` choice illustrates the problem: it is a legitimate chance to play Shields Holding before assigning hits, but the generic dialog displays an internal ability ID. Direct Hit already has a dedicated button in the battle overlay. Bring the other space-combat card opportunities into the same experience, including cards played just before combat and immediately after the result. **Invasion and ground combat are outside this work**; they should have their own overlay.

The end-to-end acceptance test should play **one complete scripted space battle**, not an entire game through victory, in four simultaneous views: the card holder, the opposing combatant, a third player, and a spectator.

## Implementation progress

**Status: implemented and verified.** The starting-state notes below describe the behavior before these changes; the design and acceptance criteria remain the rationale for the implementation.

- **Battle boundary (engine + model):** `GameState.active_space_combat` records the system, attacker, and defender when the combat driver opens a real space battle. It remains set through the `SPACE_COMBAT_WON` reaction window and is cleared before the hand-off to invasion. `projection.rs` now uses this marker rather than treating two fleets in one system as evidence of combat. The public boundary is also refreshed from the engine's current observation when a nested human choice pauses a step, so live views and reconnect snapshots agree at the transition.
- **Authoritative offers (engine):** Outer timing choices now carry a typed reaction context (event, relation, optionality, and space-battle association). The engine labels a sole playable card by its printed name; if several copies are playable it says that card selection follows. The inner `play_reaction_*` choice retains that battle association and the engine's existing rule of grouping same-name copies. Other abilities are not identified as cards. Battle-related choices asked during card resolution inherit the association.
- **Public plays (engine + model + server):** `GameState.action_card_plays` records only cards the engine actually announces, including a card whose effect is later cancelled by Sabotage. The shared live/batch decision-fact path checks that record and the selected, engine-supplied card identity against the content catalog. It attaches one readable public detail to the appropriate outer or inner decision; a pass or an unannounced offer produces none. Existing event projection and participant-name presentation carry those details into logs, snapshots, and history without exposing unplayed cards.
- **Battle UI (web):** `GameShell` routes battle-associated reactions, nested card selection, and card-effect follow-ups into `SpaceCombatOverlay`. The overlay renders timing, printed effects, offered controls, and Pass through `WorkflowShell`; Direct Hit uses the shared path. Non-actors receive a read-only battle view. When a nested choice changes actors, the client clears the previous actor's stale actionable choice. Ground combat and invasion remain outside this route.
- **Acceptance coverage:** The dev-only `ongoing_combat_four_views` scenario returns three human-seat credentials only in its launch response. `web/e2e/space_combat_four_views.spec.ts` completes a seeded battle in four isolated browser contexts, exercising grouped Shields Holding copies, Sabotage, and post-battle Salvage; it checks private-hand redaction, public play history, and reconnect cursors/logs. The existing `space_combat_overlay.spec.ts` separately covers Direct Hit after sustain and its nested-choice seeds. Focused server and web tests cover battle projection, selected-versus-announced play facts, routing, and stale-choice cleanup.

**Verification run:** `cargo test -p ti4-server`, `npm test`, `npm run build`, both space-combat Playwright specs, repeated four-view runs, and three repeats of each Direct Hit nested-choice seed passed. `cargo fmt --all --check` and `git diff --check` passed.

## Starting state (before implementation)

- `web/src/components/GameShell.tsx` sends sustain, casualty, and retreat decisions to `SpaceCombatOverlay`. It has a special check for Direct Hit's `SUSTAIN_DAMAGE_USED:after` ability. Other reaction windows may go through the generic choice dialog or `ReactionStatusBar` instead.
- `web/src/components/SpaceCombatOverlay.tsx` shows each viewer's permitted card information and has Play/Pass controls for Direct Hit. It does not render arbitrary offered combat reactions or a follow-up choice between multiple eligible cards.
- The engine opens reaction windows in `crates/ti4-engine/src/timing.rs` and `reactions.rs`. An outer choice can offer an opaque `reaction:<faction>:<event>:<when|after>` ability plus `decline`. If just one matching card copy is playable, choosing the ability plays it; with multiple playable copies, the engine asks a second, card-selection choice that groups copies of the same name. Sabotage can open another nested reaction before the original card finishes resolving.
- The server's `DecisionResolved` history entries already support public `detail` and actor-only `private_detail`. Both ordinary decisions and batches use the offered-option fact builder in `crates/ti4-server/src/protocol/server.rs`. It describes several combat decisions but does not identify a played action card. `web/src/components/EventLog.tsx` displays public details and folds batches.
- `crates/ti4-server/src/projection.rs` currently infers `board.combat` largely from an active system containing units from multiple owners, or a small set of combat choice subtypes. That is not an authoritative start/end marker and cannot safely route every reaction, especially around the first and last combat windows.
- `ongoing_combat` in `crates/ti4-server/src/dev/scenarios.rs` seeds Sol with Direct Hit, Shields Holding, Courageous to the End, and Salvage, but the other seats are bots. The existing `web/e2e/space_combat_overlay.spec.ts` tests Sol's browser and the nested-choice regression; it does not exercise four independent live viewers or play a complete human-controlled battle.

## Target architecture / design

### One battle surface, authoritative choices

The engine remains the authority on _when_ a card is playable and _which_ options exist. The browser never creates an action from cards in hand or from a window-name lookup. The server sends the offered choice with presentation-safe, typed semantics; the UI submits the original option ID and nonce. The battle overlay presents all space-battle decisions, including outer reaction opportunities, card selection when needed, Sabotage of a battle card, and the existing sustain/casualty/retreat choices.

Give the UI a reliable **space-battle association** for these choices, including the windows before the first roll and after the winner is decided. Prefer an explicit engine/server battle marker or choice context tied to the combat system and participants over checking whether two fleets happen to share a system. A generic reaction to a card played elsewhere, a space-cannon decision outside a battle, and an invasion/ground-combat decision must not be pulled into the space overlay. If a card opens a payment, target, or other follow-up choice, retain the battle context where appropriate and render an appropriate control there rather than losing the decision behind the overlay.

### Private opportunity, public play

Before a play, only the eligible player receives actionable options and their own hand information. Other viewers may see the public battle and a neutral waiting indication, but not eligible card names, aliases, option payloads, or private prompts.

When a card **actually gets played**, persist one public, meaningful fact attached to the correct decision cursor, for example `Sol played Shields Holding`. It must appear in live events, reconnect snapshots, the game log, and replay/history projections for all viewers. Prefer a typed card-play fact (actor and card identity, optionally battle/system) rendered using the public card catalog; a validated public detail is acceptable if it uses the same fact-building path. Use engine-authoritative evidence of the actual play, not a browser-provided label or merely an offered ability. A one-card outer reaction and a multi-card inner selection must each produce **one** play entry, not zero or two. Passing produces no played-card entry. A played card that is subsequently Sabotaged was still played; Sabotage itself is another public play. Avoid a premature “played” entry if resolution never reaches the play. Do not expose other cards from the hand while deriving these facts.

### Four-viewer acceptance contract

Use a deterministic dev-only scenario with **human-controlled combatants** and a noncombatant third seat. Launch four separate browser contexts: the card holder, the other combatant, the third player, and a spectator. Observe each relevant state from all four contexts before and after each scripted submission. Use real backend snapshots and browser interactions, not mocked WebSocket messages, for the acceptance flow. Confirm that history, state, and visibility agree after reconnect. Smaller targeted scenarios/tests can cover windows or branches that cannot all occur in a single battle.

## Implementation details

### 1. Establish the battle boundary and describe reaction offers

1. Trace the combat driver through the pre-combat fire, opening round, hits, retreat, win, and hand-off to invasion. Make its battle identity/system/participants available to pending choices and the public combat view across the relevant windows. Do not treat a merely contested system as proof that combat is running.
2. Give the outer engine reaction choice a typed context identifying its event, timing, and battle association. It currently has no context and labels abilities with their IDs. Supply actor-visible, engine-derived card presentation information: if exactly one card copy is playable, name it; if more than one copy is playable, say that a card choice follows (even if the copies share a name). Preserve original IDs, eligibility, and replay behavior. The follow-up `play_reaction_*` choice already has card-labelled options; make its battle association explicit too. Separate copies of the same named card must remain valid under the engine's current option rules.
3. Handle other abilities offered in the same window without mislabelling them as cards. The UI should only show a named Play control when the offer actually identifies that card. Keep card effects, Sabotage chains, and prompts with further required choices actionable until the battle advances.

### 2. Route and render decisions in the overlay

1. Replace the Direct Hit-only routing branch in `GameShell.tsx` with a battle-associated choice route. Keep actor and spectator behavior working even when the current actor is the defender or when the card is played at the end of combat. Avoid duplicate dialogs or duplicate minimized pills.
2. Add a reaction area in `SpaceCombatOverlay.tsx`: short timing text (for example, “Before assigning incoming hits”), offered **Play [name]** controls with the printed effect, and **Pass** when offered. If several cards are possible, show the subsequent engine card-selection choice in the same overlay. Direct Hit should use the shared path while retaining its useful sustain warning.
3. Reuse `WorkflowShell`'s pending-submission, nonce reset, and error presentation. Submit one offered choice at a time, and allow a newly opened nested choice to become actionable promptly. Keep the battle view informational for non-actors; do not infer the opponent's hand from hidden card data.
4. Preserve the chosen overlay across battle-related follow-ups, including Sabotage and any effect-specific targeting. Keep invasion and ground-combat follow-ups outside this route.

### 3. Record public card plays at the right boundary

1. Extend the shared server decision-fact path (`protocol/server.rs`, used by `session/worker.rs` and batch commit) with a validated card-play fact. Associate the engine's actual played card with the decision that caused it: the outer ability if it immediately selects the sole playable card, or the inner card choice if there are multiple. Account for nested card plays and their order. Do not derive a public card name by parsing opaque ability IDs or by reading a player's entire hand into an event.
2. Use the existing public event projection for that fact, while retaining private choices and hand redaction. Keep decision counts, cursors, undo/redo, recovery, and grouping correct; do not add a second misleading “decision resolved” just to display the play. Show the faction/player's friendly name via the existing participant presentation in the UI.
3. Check the pass, single-card, multiple-card, repeated copy, Sabotage, and failed/interrupted-play paths. Verify actor, opponent, third player, and spectator see the same public play and no hidden alternatives in both live events and snapshots.

### 4. Build the scripted E2E battle

1. Add a deterministic, dev-only scenario or fixture with known fleets, seed, cards, and controllable seats. Expose test credentials to the E2E harness through the dev scenario response only; never expose another player's credential or hand in a normal player/spectator snapshot. Include card opportunities at representative battle phases (at least before hits, after sustain, a counter-card/Sabotage response, and an immediate post-battle window); split into additional seeded scripts if a single battle cannot naturally reach every branch.
2. Open four isolated Playwright browser contexts. At every checkpoint assert the active player's overlay and offered controls; all other viewers' read-only status and absence of hidden names/options; legal submission and advancement; and the correct public play in each live log. Assert action-card counts and actor-only hand changes where applicable. Advance through casualties/retreat/round transitions until the battle actually finishes, then verify the result and that any invasion is no longer shown as space combat.
3. Reconnect/reload all four views, compare their public card-play history and decision cursors with server snapshots, and recheck private-hand redaction. Use bounded, condition-based waits rather than fixed sleeps. Keep the nested-choice seed regression and add coverage for multiple card options, as well as unit/server fact tests for paths the full script does not encounter.

## Verification and completion criteria

- Engine/server tests: battle association and projection boundaries; selected-card fact for one/multiple cards, pass, nested Sabotage, and history replay/reconnect; public-versus-private projections for all roles. An unplayed card must never be logged as played.
- Web tests: dispatcher routing for each battle phase and non-battle counterexamples; overlay controls, actor gating, multiple-card follow-up, pending/error behavior, and minimized/reopened UI.
- E2E: one real, complete, deterministic battle observed concurrently by both combatants, a noncombatant, and a spectator, including public log checks before and after reconnect. Run the seed-specific nested-choice regression repeatedly. Run the relevant `ti4-server` tests, web tests, and web build after implementation.
- Done means every offered **space-battle card choice** is usable within the overlay; no internal `reaction:…` label is the only explanation of a choice; a card play is publicly understandable and durable; private hands stay private; and invasion/ground combat never masquerades as space combat.

### Scope notes and risks

- “All space combat cards” means all **engine-offered reaction decisions associated with the space-battle flow**, including generic cards such as Sabotage when they answer a card played in that battle. Cards used during earlier tactical movement or a separate space-cannon exchange are not automatically battle choices. Keep the routing based on the actual battle association, not a hard-coded list of aliases.
- An outer ability selection is not always the final card choice; after a card play, another player may immediately react before the original engine step returns. The play fact and acknowledgement must survive this nesting without double-counting or stranding the next decision.
- A full battle can verify the integrated four-view experience, but one outcome cannot exercise every possible card and branch. Use focused engine/UI tests and additional small deterministic E2E cases for the remaining windows rather than forcing impossible sequences into one script.
