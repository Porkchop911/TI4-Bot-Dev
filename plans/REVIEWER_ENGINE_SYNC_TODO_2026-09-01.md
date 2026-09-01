# Reviewer TODO after engine completion and Phase 9 changes

**Created:** 2026-09-01

**Purpose:** Bring `ti4-review` back into semantic and visual parity with the engine after the large engine-completion series and the still-running Phase 9 rules audit.

**Final synchronization baseline:** `f892552` (`A step limit that names the loop it died in`), after `f7645a5` completed Phase 9 with all 109 rule topics verified. Reviewer synchronization began from that clean engine baseline; only the two pre-existing untracked sample artifacts were present.

**Synchronization pass:** implemented immediately after the engine freeze. The checkboxes below distinguish completed integration work from useful follow-up enhancements; unchecked items are not required to understand the final-engine state already represented by the reviewer.

## Current result

`cargo test -p ti4-review` passes all 11 tests at the inspected tree. This proves that the reviewer still compiles and its existing session tests pass. It does **not** prove that new engine state, timing paths, or card effects are represented correctly.

The most important current mismatches are:

- Board metadata is captured once from the starting galaxy, but planets can now be placed during play or destroyed permanently.
- Neutral units are rendered through the seat-color fallback and can therefore look like seat 0's units.
- Several dynamic board tokens and states are not drawn.
- New player-sheet state, including exhausted relics and faceup exploration cards, is absent.
- Action summaries infer movement, production, and destruction from net state differences; the new card and combat effects make that inference increasingly ambiguous.
- New turn-retention, skipped-turn, cancelled-action, and forced-action paths are not covered by reviewer action-boundary tests.
- The GUI and HTML export do not yet expose the same useful information.

## P0 — establish a stable integration point

- [x] Wait for or select a named clean Phase 9 checkpoint; record its commit in this file.
- [ ] Re-run `git diff b1e63ef..<checkpoint>` for engine, model, content, policy, simulation, and checkpoint-bundle changes.
- [ ] Re-run the state-field, event-name, choice-kind, and decision-head inventories after all pending engine edits are committed.
- [x] Keep the engine's known open rules issues separate from reviewer defects. The reviewer must show the state the engine produced; it must not silently compensate for rules behavior.
- [x] Verify a current MLP bundle and map-pool format before changing session data.

## P0 — replay and schema compatibility

- [x] Load real pre-sync review files in addition to synthetic compatibility fixtures.
- [ ] Confirm every new serialized `Player`, `SystemState`, and `GameState` field has a safe old-file default. In particular cover purged planets, exhausted relics, exploration cards, discarded action cards, ion-storm and placed-planet state, agenda state, transaction history, and reroll staging.
- [ ] Decide whether the richer board/event representation requires review-session schema v3. If it does, provide a v2-to-v3 loader instead of simply rejecting all existing reviews.
- [ ] Record engine/content version or a content digest in the manifest so a replay cannot silently render against different card or planet data.
- [x] Keep the sampling temperature, profile-table selection, checkpoint digest, map-pool digest, seed, rotation, and faction seating reproducible.
- [ ] Test loading the previously reported real autosaves and checkpoint bundles, including selection of a bundle directory, `manifest.json`, and `slots.json` beside its manifest.

## P0 — preserve the user's definition of one action

An action in the reviewer is the complete active-player period: it begins when a player becomes active and ends when that player ceases to be active. Nested decisions and transactions remain inside it.

- [x] Replace the current `turn_seq`-only boundary with active-player identity and phase exit.
- [ ] Add scenario tests for a normal tactical action, strategic action, component action, and pass.
- [x] Pin retained-turn behavior: a `turn_seq` change with the same active player remains one active-player period.
- [ ] Add Master Plan and `TURN_RETAINED`: an additional action by the same active player must not accidentally become another player's action.
- [ ] Add Puppets on a String or any forced post-pass action and document whether it is one new active-player period.
- [ ] Add Crisis/`TURN_SKIPPED`, Coup d'Etat/`STRATEGIC_ACTION_CANCELLED`, Minister of Peace/`TURN_ENDED_BY_MINISTER_OF_PEACE`, and ordinary `TURN_PASSED`.
- [x] Make `Next action`, `Run N actions`, action counts, reconstructed old summaries, and autosave boundaries use the same tested definition.
- [x] Preserve stopping at every engine-step boundary.

## P0 — stop deriving ambiguous history from net state alone

The current summary pairs unit departures and arrivals by owner and unit class. A destroyed carrier plus a newly produced carrier can therefore be reported as a move. New action cards can remove, replace, return, produce, or relocate units inside reaction windows, making this worse.

- [ ] Inventory which engine events carry only a name and which expose enough context to explain what happened.
- [ ] Prefer a structured review event record with actor, target, system, planet, card/effect id, units, amount, and outcome. If this requires an engine hook, specify that dependency explicitly before changing reviewer heuristics.
- [x] Evaluate state changes frame-by-frame instead of only comparing the start and end of the full action.
- [ ] Distinguish movement, transport, commitment, production, destruction, removal, replacement, retreat, and return-to-space.
- [x] Preserve the raw engine event stream alongside the human summary for diagnosis.
- [x] Add a regression in which the same unit class disappears in one location and is independently created elsewhere; it is not described as movement.

## P0 — dynamic board and ownership rendering

- [x] Render `placed_planets` from a replay-carried planet catalog in the correct system at each frame.
- [x] Render `purged_planets` as destroyed rather than ordinary neutral planets.
- [x] Render `purged_systems` distinctly while retaining their historical location.
- [x] Give neutral units a dedicated neutral color and legend; never route an unknown owner through seat 0's color.
- [x] Represent space stations distinctly from ordinary planets.
- [x] Show coexistence markers separately from the planet controller.
- [x] Draw frontier, Creuss wormhole, ion-storm face, ingress, breach, Thunder's Edge, and command tokens.
- [x] Mark planet attachments and retain printed planet metadata.
- [ ] Decide whether displayed resource/influence values are printed values, current effective values, or both; label them unambiguously.
- [x] Preserve system-space control as the thick outer edge and planet control as the planet background.
- [ ] Test systems with no planets, destroyed planets, dynamically placed planets, neutral fleets, multiple ground-force owners, and more unit groups than currently fit in a tile.

## P1 — complete the player and table sheets

- [x] Mark exhausted relics instead of presenting all relics identically.
- [x] Show faceup exploration cards.
- [x] Show promissory notes by current holder, including Support for the Throne's faceup scoring state and ordinary notes in hand. The view is omniscient by user choice.
- [ ] Resolve card, relic, leader, breakthrough, technology, objective, plot, and exploration ids to readable names; retain ids in details/tooltips.
- [ ] Show leader state, exhausted technologies, exhausted planets, used strategy cards, commodity capacity, and any meaningful once-per-round or once-per-action readiness state.
- [ ] Show faction unit variants and unit-upgrade stats where that changes how a human reads the board.
- [x] Add table state for speaker, unclaimed strategy cards and accumulated card trade goods, custodians status, laws, and discarded action cards.
- [ ] Show active agenda, vote order, submitted votes/outcomes, veto/replacement/redirection, and elected player or planet where public.
- [ ] Keep hidden information visible only because the session is explicitly omniscient; label it as such in GUI and HTML.

## P1 — human action and phase summaries

- [ ] Name action cards, relics, leaders, breakthroughs, exploration cards, faction abilities, laws, and strategy cards used during the period.
- [ ] For tactical actions, summarize activation, movement path, transported units, space cannon, barrage, combat rounds, sustain/cancelled hits, casualties, retreat, bombardment, commitments, control changes, exploration, and production.
- [ ] For production, show all units selected, total capacity used, the combined bill, discounts, and payment sources. Do not regress to one line per incremental payment item.
- [ ] For strategic actions, name the strategy card, primary result, each secondary participant/decline, cancellations, and token redistribution.
- [ ] For component actions, name the component and distinguish resolved, cancelled, failed, or no-effect outcomes.
- [ ] For transactions, show the exact terms on both sides: trade goods, commodities, action cards, promissory notes, relic fragments, secrets where Black Market permits them, and binding promises. Preserve legality/refusal/abandonment reasons separately.
- [ ] Distinguish `ACTION_CARD_PLAYED`, `ACTION_CARD_DISCARDED`, `ACTION_CARD_UNRESOLVED`, cancelled timing windows, and resolved effects.
- [ ] Add non-action-phase summaries for strategy-card selection, status scoring/readiness, agenda reveal/vote/outcome, and round transitions.
- [x] Ensure a cancelled strategic action is never summarized merely as “took a strategic action.”
- [ ] Ensure summaries remain useful when the user stops in the middle of an action: show “in progress” facts without pretending the action completed.

## P1 — decision inspection and policy diagnostics

- [ ] Inventory new choice kinds and confirm each routes to the intended linear and MLP decision head rather than `other` accidentally.
- [ ] Add readable grouping for reactions, rerolls, dice modification, hit cancellation, retreat, agenda redirection, unit replacement, special payment, and multi-party choices.
- [x] Display the policy path, resolved head, actual sampling temperature, chosen probability, rank, and probability of the highest-scored option.
- [x] Add a concise “sampled below the greedy choice” indication when applicable.
- [ ] Preserve complete feature inspection, but add search/filtering so large option sets remain usable.
- [ ] Audit blind decisions. A blind trace currently has no meaningful score/probability; either provide the correct observation path or label it as unavailable rather than equivalent to a scored decision.
- [ ] Verify OOV registry and MLP-bundle versions against the final engine/policy checkpoint format.

## P1 — GUI and HTML parity

- [ ] Define one presentation model used by both native GUI and HTML export instead of maintaining two independently drifting renderers.
- [x] Bring dynamic planets, destroyed systems, tokens, neutral units, expanded player sheets, objectives, table state, and summaries to both outputs.
- [x] Preserve view-only behavior for loaded review files.
- [x] Keep the HTML self-contained and escape embedded session/content text.

## P2 — storage and performance

- [ ] Measure frames per round, serialized bytes per frame, autosave latency, GUI history latency, and final file size on a complete current game.
- [ ] Pay special attention to repeated full-state snapshots containing decks, discard piles, agenda state, reroll staging, and other new fields.
- [ ] If realistic games approach the 512 MiB session limit, design periodic full checkpoints plus deterministic deltas rather than merely raising the limit.
- [ ] Keep autosaves recoverable and atomic while long `Run N` or `Run to end` commands are active.
- [ ] Verify that stopping and autosaving during a large combat/reroll window does not corrupt replay reconstruction.

## Verification matrix before declaring synchronization complete

- [x] Current MLP bundle + current map pool: load and run one complete active-player period at an overridden temperature.
- [ ] Current linear profile checkpoint: same sequence, including temperature override.
- [x] Load three existing review files, including a 248-frame autosave; retain v2 default compatibility.
- [ ] Tactical fixture with movement, transport, combat, invasion, exploration, production, and control change.
- [ ] Component-action fixture using an action card and one using a relic/exploration card.
- [ ] Strategic-action fixture with primary, secondaries, redistribution, transaction, and cancellation.
- [ ] Agenda fixture with vote modification, veto/replacement, election, and enacted law.
- [ ] Board fixture with Mirage, a destroyed planet, neutral units, coexistence, a space station, and every dynamic token type.
- [ ] Transaction fixture containing a non-Support promissory note and Black Market terms.
- [ ] Action-boundary fixtures for Fleet Logistics, Master Plan, Puppets, skipped turn, forced end, pass, and game end.
- [ ] GUI visual inspection at normal and constrained window sizes.
- [ ] Interactive HTML visual inspection. Browser setup failed in this environment; the real smoke export rendered successfully and its JavaScript passed `node --check`.
- [x] `cargo test -p ti4-review` (15 passing tests).
- [x] Reviewer clippy with warnings denied, allowing only the package's pre-existing missing-panics-doc lint.

## Completion definition

The reviewer is synchronized only when a human can explain a current game from the board, sheets, decisions, events, and summaries without opening raw JSON; old supported reviews still load; GUI and HTML agree; action boundaries follow active-player periods; and a complete seeded game can be replayed deterministically from its recorded provenance.
