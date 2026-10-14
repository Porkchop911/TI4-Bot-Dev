# Auto-resolved and still-asked choices

Where a choice with one possible answer is settled without the player, who settles it, and where a
case is deliberately still asked or parked. Written 2026-10-07 for audit item H6 (decided: **UI only,
no engine change**) and checked against the code on this branch.

## Two mechanisms

1. **Server auto-resolve (exists today).** `Table::auto_resolve(&choice, reason)`
   (`crates/ti4-engine/src/choice.rs`) returns the only option, leaves an `AutoResolved` note and
   does **not journal** the decision. The session worker turns the note into a
   `StateUpdate.auto_resolved` entry for the deciding seat; the client shows a corner toast
   (`web/src/hooks/useCornerToasts.ts`, wording in `web/src/presentation/autoResolveText.ts`:
   known prompts have a sentence, others read `Only one choice: <label> (<reason>)`). The question
   never reaches the client, so it is not in the decision log.
2. **Client auto-submit (this change).** The engine still asks; the web client answers for the
   viewer's own seat through the normal `submit_choice` path (nonce plus expected version). The
   answer is journaled like any click. Histories, replay, bots and undo are unchanged, and old
   histories (which contain these answers) replay as before. Code: `web/src/presentation/loneChoice.ts`
   (what is "lone"), `web/src/hooks/useLoneAutoSubmit.ts` (when to act), mounted in
   `web/src/components/GameShell.tsx`; toast through `useCornerToasts` (`localNotes`);
   setting `web/src/hooks/useAutoSubmitSetting.ts`, button `auto-submit-btn` in `PlayerSheet.tsx`.

## Server-auto-resolved today (toast, not journaled)

| Case | Rule | Code | Toast |
|---|---|---|---|
| Last strategy card of the draft (4-player games; 3, 5, 6 players leave several) | one card on the mat | `game.rs` `Game::step`, subtype `draft_strategy_card`, reason "only one strategy card left" | "Only one strategy card was left: you took <card>" |
| Last way to pay a cost | one source left to cover what is owed | `production.rs` (`auto_resolve`, "it was the only way left to pay") | "Only one way left to pay: <source>" |
| Expedition: only action card to discard (Thunder's Edge) | one card held | `thunders_edge.rs` | "Only one action card to discard: <card>" |
| Expedition: only secret objective to discard | one held | `thunders_edge.rs` | "Only one secret objective to discard: <objective>" |

## Client-auto-submitted by this change

| Case | Derived from the pending choice (strict) | Engine source | Toast |
|---|---|---|---|
| Lone strategic action | `details.kind == "turn_menu"`, `details.closing != true`, prompt `action phase`, **exactly one option**, id `strategic` | `strategy.rs` `strategic_action_options` (bare `strategic` id only for one unused card; with `pass` only when every card is used) plus the appended tactical, trade, contact, payment, relic, exploration, technology and component options in `game.rs` `Game::turn_options`; `with_turn_menu` adds `details` | "Only one choice: take your strategic action" |
| Movement with no unit that can move | prompt `movement`, **exactly one option**, id `done_moving` (the decline entry; every ship, fighter and ground force move would be another option). The movement dialog stays closed while the client answers (`useLoneAutoSubmit` returns `answeringNonce`, `GameShell` renders nothing for it); if the setting is off or the submit is not sent, the dialog shows as before | `tactical.rs` movement choice (move options, then `done_moving`) | "Only one choice: no unit can move, finishing movement" |
| Lone system activation | `context.subtype == "activate_system"`, prompt `activate a system`, exactly one option, kind `activate` | `tactical.rs` activation choice (systems only, no decline); reached only after the seat chose the tactical action | "Only one choice: activate system <id>" |

Never matched: a menu that also offers `pass`, tactical, components, trade, contact or any action
card; two or more unused cards (`strategic|<card>` ids, a real choice); the end-of-turn menu
(`closing: true`); Warfare's free tactical action (different prompt and subtype); every other
decision.

### When the client acts (guards)

* Only the seat's own client: `choice.actor === viewerSeat`; spectators have no seat and never act.
* **Armed only by forward progress.** The history cursor must have grown, with nothing to redo
  before or after and the same history generation, since the previous decision was evaluated. So:
  a page load or a reconnect with no progress does not act (the decision is asked); an **undo**
  (cursor falls, redo available), a **redo** and a **restore** never act, so the player can undo past
  the lone decision again. Undo is host-only for everyone; the rule is state based, so it holds on every
  tab and for every seat. A decision reached after an undo and answered by hand arms the next one
  normally.
* One evaluation per nonce (a ref), a 0.4 s pause so the board paints, a re-check of the setting and
  of "busy" (pipeline runner running, history change running) when the timer fires, and a
  `localStorage` claim of the nonce so a second tab of the same seat does not send too. The server's
  nonce and version check still rejects any repeat. A rejected submit is not retried; the decision
  stays open for a click.
* Setting: "Auto" button in the player sheet header, default on, stored under
  `player_auto_submit_lone` (viewer local, `try/catch`). Off asks every time.
* Smoke harness (`web/e2e/smokePlaythrough.ts`) switches the setting off in every tab it opens, since it
  clicks the turn bar itself (`TI4_SMOKE_AUTO_LONE=1` keeps it on).

## Resolved silently by the engine (no toast, not journaled)

| Case | Code |
|---|---|
| Scheming: discard when the hand is one distinct card | `factions/yssaril.rs` (`options.len() == 1`) |
| Action-card choice effects (own hand / revealed row), one distinct card, not optional | `action_cards.rs` (`options.len() == 1 && !optional`, two places) |
| Placing units when one spot fits and it is not optional | `action_cards.rs` (`spots.len() == 1 && !optional`) |
| Timing window with one eligible, non-optional ability | `timing.rs` `pick` (two places) |
| Codex / Crown of Emphidia with only the decline left | `relics.rs` (`options.len() == 1`, two places) |
| Phases or steps with no options at all | `Game::legal_options` returns `None` |

## Deliberately still asked

| Case | Why |
|---|---|
| Pass as the only entry (every card used) | passing ends the seat's round; keep it an explicit act (`Game::turn_options`) |
| Strategic with company on the menu | a real choice |
| "Take a tactical action" | a menu entry, never alone unless the strategic rule applies |
| End of turn | explicit end of turn (`closing_options`) |
| Strategy card secondary, reaction windows with a decline | declining is a real option |
| Primary technology research with one candidate | not on the approved list; carries payment and waiver steps |
| Drafting with 3, 5 or 6 players, any pick that is not the last | several cards remain |
| Movement and commit steps with any move on offer | offer a "done" entry beside the moves; only a movement whose sole option is `done_moving` is answered for the player |

## Parked

* Primary technology research with one candidate.
* Toasts for the silent single-option picks above (would need an engine `auto_resolve`).
* A dedicated sentence in `autoResolveText.ts` for the two client cases ("You had only one option");
  today's text is the generic "Only one choice: ...".
* A decision that stays open because the page was loaded or reconnected on it (nothing arms the
  client then): the player clicks it. Could be relaxed to "after a reconnect that advanced".
* An engine-side variant (journal-free auto-resolve, branch `auto-resolve-lone-cases-2026-10-07`,
  not merged) was not adopted: it would change recorded histories (old histories stop replaying).

## Effect on recorded histories

None. The engine asks as before and the client's answer is recorded as the ordinary decision, so a
replay of any old or new history behaves as before. Bots, which never run the web client, still answer
these questions themselves.
