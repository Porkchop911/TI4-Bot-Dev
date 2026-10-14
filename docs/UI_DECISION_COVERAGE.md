# UI decision coverage

Audit of which decisions the engine can put to a player and how the web UI presents each. Written 2026-10-07, read-only (code was not run, no browser was used). **Status updated 2026-10-07 after the decision-UI work (HEAD `bfb88b5` on `nightly-fixes-2026-10-06`); rows marked "Built" were implemented that day.** Each item has a short handle; refer to items as `/slug`.

## How this was made and limits

- Decisions were enumerated from every `DecisionContext::new(...)` call in `crates/*/src` (130 call sites, production code only) plus the dynamic ones: `<card>_pick_<kind>` in `crates/ti4-engine/src/action_cards.rs:4321`, `{card}_choose_reward` in `exploration.rs:192`, `{relic}_choose_technology` in `relics.rs:221`, reaction subtypes in `timing.rs:825` and `reactions.rs:875`, the promissory note alias in `game.rs:3933`, the diplomacy deal builder in `diplomacy/window.rs`, and the reroll/casualty helpers in `combat.rs` (`choose_reroll_dice`, `choose_casualty_owing`). Some choices are built without any context and have no subtype (strategy secondaries, over-supply removal, Sling Relay, Integrated Economy); they appear as `prompt:<text>`.
- Frequency comes from `nightly-reports/*/runs/*/trace/trace.jsonl` across 2026-10-04, -05, -06: **53,508 decisions in 74 runs**. Every subtype seen in those traces is mapped to a row below (no unmapped subtype).
- UI routing was read from `web/src/presentation/choiceModel.ts` (`deriveChoiceRendererModel`), `web/src/components/GameShell.tsx` (`workflowRenderers`, turn bar branch), `PendingChoiceModal.tsx` (embedded panels) and the presentation modules. Option payloads are not stored in traces, so "Shared specialised" for planet and system picks is inferred from engine `with_planet` payloads plus the routing rules (`isPlanetSelectionChoice`, `SYSTEM_PICK_SUBTYPE` in `systemFacts.ts:112`). Rows marked *unverified* were not confirmed in a browser or by a test.
- `docs/DECISION_UI_REVIEW.md` (2026-10-06 morning) is partly out of date: since then command tokens, secondaries, trade replenish, politics, hand limits, system picks, remove-unit, unit abilities, place_structure and reactions with more than 4 cards got dedicated UI (commits 2f1b15d ... 5eb9f72). This file reflects the code at HEAD `2b49782`.

Classes: **Dedicated** = purpose-built component for that decision; **Shared specialised** = a reusable bar/panel for a family (planet pick, system pick, card tiles, unit ability notes); **Generic fallback** = `PendingChoiceModal` radio/checkbox list with only the header (source, topic, phase chips from `decisionSource.ts`) and the engine's labels. There is no "not reachable" class: where something never appeared in nightly runs it is flagged *never seen*, because it may still be reachable in rarer setups.

## Summary

Status 2026-10-07: counts below are after the 11 decision UIs of that day moved 17 rows out of Generic fallback (before: Dedicated 41, Shared 38, Generic 65; generic share of nightly decisions was 1,242 or 2.3%). Handles still open from the priority list: `/disgrace-pick`, `/mecatol-lift`, `/law-discard`, `/focus-research`, `/plagiarize-pick`; the never-seen generic rows and the gaps in existing UIs are unchanged.

| Class | Rows (handles) | Nightly decisions (share of 53,508) |
|---|---:|---:|
| Dedicated | 46 | 48,193 (90.1%) |
| Shared specialised | 50 | 5,226 (9.8%) |
| Generic fallback | 48 | 89 (0.2%) |
| Total | 144 | 53,508 |

- 44 of the 144 rows never appeared in the nightly runs (shown with 0 and flagged in the table). Almost all rows that are still generic fallback are among them.
- Rows group several related subtypes where the UI treats them identically (all reaction windows, all `<card>_pick_planet`, all `_choose_reward`). The count is of handles, not of distinct subtype strings (about 190 including dynamic ones).
- By volume the UI is in good shape: 99% of nightly decisions already have a dedicated or shared UI. What is left in the generic list is a long tail, dominated by one decision (`/predictive-restack`, 647 of 1,242).
- Weakest spots among the *dedicated* ones are listed under "Gaps in existing UIs".

## Table of decisions

Columns: handle, slug (use as `/slug`), subtype, who answers, what is asked, UI class, component, nightly count (runs), notes. `0` means never seen in the 74 nightly runs.

### Strategy phase

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Card Draft | `/card-draft` | `draft_strategy_card` | each seat in order | pick a strategy card | Dedicated | PendingChoiceModal strategy grid (choiceModel `strategy_card_draft`, renderGeneric) | 3,263 (74) | Grid shows initiative, name, primary and secondary text from the catalog (PendingChoiceModal.tsx:354-362). Does not show which cards others took or who holds speaker. |
| Investments Pick | `/investments-pick` | `investments_pick_strategy_card` | card player (Manipulate Investments) | which strategy card gets a trade good | Shared specialised | strategy card grid with goods | 85 (17) | Plain list; same grid would fit. Source/topic header only. **Built 2026-10-07 (`6f0b6db`).** |
| Disgrace Pick | `/disgrace-pick` | `public_disgrace_choose_card` | target of Public Disgrace | choose a different strategy card | Generic fallback | PendingChoiceModal list | 16 (16) | Card ids may show as raw labels; unverified. |
| Rider Give Away | `/rider-give-away` | `prompt:give away which of your strategy cards` | card holder | which strategy card to give away | Generic fallback | PendingChoiceModal list | 1 (1) | Context-less (no subtype). 1 occurrence. Origin card not determined. |

### Strategy cards

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Token Gain | `/token-gain` | `gain_command_token` | active seat | which pool receives a token | Dedicated | CommandTokenPanel (describeCommandTokens, details.kind command_tokens) | 3,775 (72) | Pools, purpose, reinforcements, one batch. Needs onSubmitBatch, otherwise falls back to plain list (commandTokens.ts:93). |
| Token Buy | `/token-buy` | `buy_token_with_influence` | seat | spend 3 influence for a token? | Dedicated | CommandTokenPanel buy mode / StrategySecondaryPanel | 687 (66) | Leadership gain+buy panel with change-payment. Context-less twin ("spend 3 influence...") uses the secondary yes/no panel. |
| Token Restack | `/token-restack` | `status_redistribute_tokens` | every seat in status phase | arrange tokens across tactic/fleet/strategy | Dedicated | CommandTokenPanel redistribute mode | 1,538 (66) | Steppers map to the matching `a\|b\|c` id. |
| Warfare Restack | `/warfare-restack` | `warfare_redistribute_tokens` | Warfare player | redistribute command tokens | Dedicated | CommandTokenPanel redistribute mode | 215 (56) |  |
| Predictive Restack | `/predictive-restack` | `predictive_intelligence_redistribute` | Predictive Intelligence owner | move one token between pools, repeat, finish | Dedicated | CommandTokenPanel restack mode | 647 (35) | Not built via redistribute_tokens, no `details.kind`, so no panel (technology.rs:395-412). Options are "move 1 token from X to Y". **Built 2026-10-07 (`a44d0e8`).** |
| Secondary Offer | `/secondary-offer` | `prompt:spend a strategy token ... (strategy secondary)` | each follower | follow the played card (draw, build, produce, research, ready, replenish) | Dedicated | StrategySecondaryPanel in PendingChoiceModal (describeStrategySecondary; details.kind strategy_secondary) | 4,148 (71) | Names card, who played, card text, tokens left, cost. Still context-less on the wire (no subtype/source) per strategy.rs:55-90; header cannot use source. |
| Research Pick | `/research-pick` | `research_technology` | Technology player / followers | which technology(ies) to research | Dedicated | TechnologyModal research mode | 745 (70) | Primary allows 2 picks, secondary 1. |
| Ready Planet | `/ready-planet` | `ready_planet` | Construction/Leadership/Xxcha effects | which planet to ready | Shared specialised | PlanetSelectionBar | 366 (64) | Planet payload; map highlight. |
| Diplomacy Pick | `/diplomacy-pick` | `diplomacy_choose_system` | Diplomacy player | system whose other players ready planets / get tokens | Shared specialised | SystemPickParts (list facts, "Choose on the map", confirm bar); boardPresentation highlight | 467 (71) | Options show planets, ships, tokens (c5f8a7e). Header names source. |
| Speaker Pick | `/speaker-pick` | `politics_choose_speaker` | Politics player | who becomes speaker | Dedicated | PoliticsDecisionParts (PoliticsContextPanel, option notes) | 446 (71) | Standing per seat. |
| Agenda Place | `/agenda-place` | `politics_place_agenda` | Politics player | put top agenda on top/bottom | Dedicated | PoliticsDecisionParts | 892 (71) | Agenda name/text from details.agenda. |
| Replenish Pick | `/replenish-pick` | `trade_choose_replenish` | Trade player | whom to replenish | Dedicated | TradeReplenishPanel | 355 (67) | Table with commodities now/after, button per seat and Done. |
| Warfare Recall | `/warfare-recall` | `warfare_recall_token` | Warfare player | which command token to recall from map | Shared specialised | SystemPickParts | 215 (56) | Highlights systems on map; shows tokens. |
| Warfare Tactical | `/warfare-tactical` | `warfare_free_tactical` | Warfare (Thunder's Edge) player | free tactical action? | Generic fallback | PendingChoiceModal list | 0 (never seen) | Never seen in nightly runs. Rendering path unverified. |
| Imperial Score | `/imperial-score` | `imperial_score_objective` | Imperial player | score a public objective | Dedicated | ObjectivesModal scoring mode + Imperial outcome card (imperialOutcome.ts) | 60 (39) | Shows +1 VP with Mecatol or secret draw. |
| Construction Pick | `/construction-pick` | `place_structure` | Construction player/follower | planet for PDS/space dock (1 of 2, 2 of 2) | Shared specialised | PlanetSelectionBar with structure facts (planetSelection.ts:223) | 1,187 (69) | Cost, step and what is on the planet (2f1b15d, 415b860). |
| Construction Ability | `/construction-ability` | `construction_choose_ability` | Construction (Thunder's Edge) player | choose which construction ability | Generic fallback | PendingChoiceModal list | 0 (never seen) | Never reached. Unverified. |

### Action phase

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Turn Menu | `/turn-menu` | `prompt:action phase` | active seat | pick an action (tactical, strategic, component, trade, pass) | Dedicated | TurnActionBar (turnBar.ts isTurnMenuChoice) | 9,999 (74) | Persistent bar, grouped components. Options the bar cannot place fall back to the list (GameShell.tsx ~712). |
| End Turn | `/end-turn` | `end_turn` | active seat | end turn or use closing abilities | Dedicated | TurnActionBar | 7,639 (73) |  |
| Mid Pause | `/mid-pause` | `mid_action_pause` | active seat | continue after paused tactical action | Generic fallback | PendingChoiceModal list | 0 (never seen) | Never seen in nightly. Possibly resolved by server batch-resume banner (PausedPlanBanner); unverified. |
| Rule Prompt Misc | `/rule-prompt-misc` | `prompt:none / prompt:and take which / prompt:which command token comes back` | varies | context-less prompts of rare effects | Generic fallback | PendingChoiceModal list | 8 (5) | Subtype missing; cannot be identified exactly (one-off prompts in traces). |

### Tactical

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Activate System | `/activate-system` | `activate_system` | active seat | which system to activate | Dedicated | SystemActivationBar + map | 2,047 (74) | Missing (per earlier review): tactic tokens left, enemy presence summary. |
| Move Ships | `/move-ships` | `movement_step` | active seat | move ships to active system | Dedicated | TacticalMovementOverlay (batch submit) | 2,058 (74) | Known user complaint in web/TODOS.md ("ship movement max values seem broken"): not re-verified. |
| Load Cargo | `/load-cargo` | `load_cargo` | active seat | load ground forces/fighters onto ships | Dedicated | CargoLoadingTray / TacticalMovementOverlay | 0 (never seen) |  |
| Bombard Pick | `/bombard-pick` | `bombardment_target` | attacker | bombardment targets | Generic fallback | excluded from planet selection; no dedicated renderer found (choiceModel.ts:142) | 0 (never seen) | Never reached in nightly. InvasionOverlay may show; unverified. |
| Dunlain Deploy | `/dunlain-deploy` | `deploy_mech` | Dunlain Reaper owner | deploy mech after invasion | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. Unverified. |

### Invasion and ground combat

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Land Forces | `/land-forces` | `commit_ground_forces` | attacker | commit ground forces to planets | Dedicated | InvasionLandingTray (+InvasionOverlay) | 185 (45) |  |
| Ground Hits | `/ground-hits` | `assign_ground_casualty` | owner of hit units | which ground unit takes hit | Dedicated | InvasionOverlay (line ~533) | 0 (never seen) | Never reached in nightly. |
| Ground Round | `/ground-round` | `fight_ground_combat_round` | attacker | fight next ground round | Dedicated | InvasionOverlay | 6 (2) |  |
| Next Ground | `/next-ground` | `start_next_ground_combat` | attacker | begin next ground combat (Coexistence) | Dedicated | InvasionOverlay (assumed; boardView.invasion path) | 0 (never seen) | Never reached; unverified. |
| Mecatol Lift | `/mecatol-lift` | `remove_custodians` | seat on Mecatol | spend 6 influence to remove custodians | Generic fallback | PendingChoiceModal list (header topic "Mecatol Rex") | 13 (13) | Payment follows via pay_influence. |

### Space combat

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Hit Sustain | `/hit-sustain` | `sustain_damage` | defender/owner | cancel hit with sustain damage | Dedicated | SpaceCombatOverlay + HitAssignmentPanel | 79 (19) | Windowed hits, non-fighter binding fixed in 5a75e68. |
| Hit Assign | `/hit-assign` | `assign_casualty` | owner | which units are destroyed | Dedicated | SpaceCombatOverlay + HitAssignmentPanel | 59 (17) |  |
| Retreat Call | `/retreat-call` | `announce_retreat` | combat participants | announce retreat? | Dedicated | SpaceCombatOverlay (retreat stage) | 186 (29) | Does not preview destination; earlier review. |
| Retreat Route | `/retreat-route` | `retreat_to` | retreating seat | retreat to which system | Dedicated | SpaceCombatOverlay | 22 (14) |  |
| Combat Reroll | `/combat-reroll` | `(card)_reroll / jolnar_commander_reroll / crown_of_thalnos_*_reroll / fire_team_reroll` | roller | reroll which dice | Generic fallback | PendingChoiceModal list | 0 (never seen) | Never reached for Crown (per nightly). Option label shows unit/die text only. |
| Munitions Reroll | `/munitions-reroll` | `munitions_reserves_reroll` | Letnev | spend 2 trade goods to reroll | Dedicated | munitions offer card (before the roll; no dice to show) | 79 (27) | Yes/no during combat round; costs shown only in label. **Built 2026-10-07 (`821d8b7`).** |
| Ixth Adjust | `/ixth-adjust` | `heart_ixth_die_adjust` | Ixthian | adjust die | Generic fallback | PendingChoiceModal list | 0 (never seen) |  |
| Kenara Bump | `/kenara-bump` | `wrath_of_kenara_bump` | Hacan flagship owner | bump die | Generic fallback | PendingChoiceModal list | 0 (never seen) |  |
| War Funding | `/war-funding` | `war_funding` | roller | war funding reroll | Generic fallback | PendingChoiceModal list | 0 (never seen) |  |
| Courageous Hits | `/courageous-hits` | `courageous_to_the_end_assign_casualty` | owner | casualties for Courageous | Dedicated | routes by option kind `casualty` to SpaceCombatOverlay (choiceModel.ts:483) | 0 (never seen) | Inferred from kind routing; unverified. |
| Assault Cannon | `/assault-cannon` | `assault_cannon_destroy` | target | destroy a ship | Dedicated | routes by option kind `casualty` if kind set; otherwise generic | 0 (never seen) | Unverified. |

### Reactions and timing

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Reaction Window | `/reaction-window` | `reaction_(when\|after)_EVENT` | any seat with a reactable card | play action card / ability or pass | Dedicated | ReactionStatusBar (limit of 4 lifted, 9db337c/c1f390d) | 1,286 (64) | Trigger sentence from context.trigger; whats-happened block. |
| Reaction Pick | `/reaction-pick` | `play_reaction_(when\|after)_EVENT` | reacting seat | which card to play | Dedicated | ReactionStatusBar | 46 (20) |  |
| Reaction Ability | `/reaction-ability` | `instinct_training_cancel / l1z1x_agent_swap` | owner | use ability | Dedicated | ReactionStatusBar (ABILITY_SUBTYPES) | 8 (1) |  |
| Note Use | `/note-use` | `(promissory alias) e.g. support_for_the_throne` | note holder | use the promissory note now? | Generic fallback | PendingChoiceModal list | 0 (never seen) | Subtype is the note alias (game.rs:3933). Frequency by alias not tallied; not found in nightly top subtypes. |

### Production and payment

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Produce Build | `/produce-build` | `produce_unit` | producing seat | which units to build | Dedicated | ProductionBuilderDrawer | 649 (63) | Sling Relay "produce one unit" has no context but is shown through unit-ability notes only. |
| Place Build | `/place-build` | `place_unit` | producing seat | where to place unit | Dedicated | ProductionBuilderDrawer | 0 (never seen) |  |
| Pay Resources | `/pay-resources` | `pay_resources` | payer | exhaust planets/TG for resources | Dedicated | PaymentDrawer/PaymentBar + map | 336 (62) |  |
| Pay Influence | `/pay-influence` | `pay_influence` | payer | exhaust for influence | Dedicated | PaymentDrawer/PaymentBar | 227 (48) |  |
| Aida Discount | `/aida-discount` | `exhaust_for_production_discount` | AIDA owner | exhaust tech for discount | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |
| Sling Relay | `/sling-relay` | `prompt:Sling Relay: produce one ship` | owner | produce one ship | Shared specialised | UnitAbilityParts (names, cost) | 132 (34) | Context-less (602e6c2). |
| Remove Unit | `/remove-unit` | `prompt:remove a unit: over fleet supply/capacity in N` | owner over limit | which unit to remove | Shared specialised | RemoveUnitParts / RemoveUnitPanel (details: fleet.rs:544) | 433 (27) | Parsed from prompt text; names, supply count, cargo effect. Context-less. |
| Integrated Economy | `/integrated-economy` | `prompt:Integrated Economy on (planet) (N cost left)` | owner | spend toward free production | Generic fallback | PendingChoiceModal list | 2 (1) | Context-less, 2 occurrences. |

### Status phase and limits

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Hand Limit | `/hand-limit` | `discard_over_hand_limit` | seat over limit | which action card to discard | Shared specialised | PendingChoiceModal card tiles (cardOptions.ts) | 908 (54) | Card text and counter; shared with expedition_discard_action_card. |
| Secret Limit | `/secret-limit` | `return_over_secret_hand_limit` | seat over limit | which secret objective to return | Shared specialised | PendingChoiceModal card tiles (cardOptions.ts) | 297 (53) |  |
| Score Public | `/score-public` | `score_objective` | scoring seat | score public/secret objective | Dedicated | ObjectivesModal scoring mode | 380 (62) |  |
| Score Secret | `/score-secret` | `score_secret_objective` | scoring seat | score secret objective | Dedicated | ObjectivesModal scoring mode | 36 (25) |  |
| Entropic Tech | `/entropic-tech` | `entropic_scar_gain_faction_technology` | seat | choose technology from scar | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |

### Agenda

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Vote Outcome | `/vote-outcome` | `cast_vote` | each voter | vote outcome | Dedicated | AgendaBallotModal | 360 (11) |  |
| Vote Planets | `/vote-planets` | `vote_exhaust_planet` | voter | exhaust planets for votes | Dedicated | AgendaBallotModal + map | 255 (11) |  |
| Vote Tiebreak | `/vote-tiebreak` | `vote_tiebreak` | speaker | break tie | Dedicated | AgendaBallotModal | 64 (11) |  |
| Elect Tiebreak | `/elect-tiebreak` | `agenda_elect_tiebreak` | speaker | which tied player the agenda names | Generic fallback | PendingChoiceModal list (planet bar if planet payload) | 1 (1) | Player picks: raw seat ids mapped via useParticipantText. |
| Redistribution Pick | `/redistribution-pick` | `redistribution_choose_settler` | trailing player | who may settle planet | Generic fallback | PendingChoiceModal list | 1 (1) |  |
| Defense Act Pick | `/defense-act-pick` | `defense_act_choose_pds` | owner | which PDS | Shared specialised | PlanetSelectionBar (planet payload) | 0 (never seen) | Not seen; inferred from payload. |
| Law Discard | `/law-discard` | `offer_discard_law` | Imperial Arbiter holder | discard to swap strategy card | Generic fallback | PendingChoiceModal list | 12 (4) |  |
| Predict Outcome | `/predict-outcome` | `predict_agenda_outcome` | Technology Rider holder | predict outcome | Dedicated | rider panel (agenda card itself not shown: engine does not record it) | 31 (10) |  **Built 2026-10-07 (`096e3aa`).** |
| Agenda Talks | `/agenda-talks` | `diplomacy_agenda_talks` | voters | agenda talks (diplomacy phase) | Generic fallback | unknown | 0 (never seen) | Not seen; unverified. |

### Trade and diplomacy

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Trade Propose | `/trade-propose` | `propose_transaction` | active seat | offer to another seat | Dedicated | TradeDeskModal | 3,898 (70) | Offered via turn menu one row per partner. |
| Trade Answer | `/trade-answer` | `answer_transaction` | partner | accept/decline offer | Dedicated | TradeDeskModal | 1,139 (46) |  |
| Deal Draft | `/deal-draft` | `diplomacy_ask_item / offer_item / amount / review / response` | both seats | multi-step deal builder | Generic fallback | unknown (not in choiceModel) | 0 (never seen) | Not seen in nightly. Possibly handled elsewhere; could not determine. |

### Action cards

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Plague Pick | `/plague-pick` | `plague_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 22 (21) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Cripple Pick | `/cripple-pick` | `cripple_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 21 (20) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Mining Pick | `/mining-pick` | `mining_initiative_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 16 (16) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Uprising Pick | `/uprising-pick` | `uprising_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 2 (2) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Deploy Pick | `/deploy-pick` | `f_deployment_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 22 (22) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Unstable Pick | `/unstable-pick` | `unstable_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 1 (1) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Expedition Deck | `/expedition-deck` | `arch_expedition_pick_planet` | card player | which planet | Shared specialised | PlanetSelectionBar | 1 (1) | Generated by action_cards.rs:4321 `<card>_pick_planet`. Gallery has Mining Initiative only. |
| Reparations Pick | `/reparations-pick` | `reparations_exhaust / reparations_ready` | card player | which planet | Shared specialised | PlanetSelectionBar (planet payload) | 0 (never seen) | Not seen. |
| Crash Pick | `/crash-pick` | `crashlanding_choose_ground / _planet` | card player | where ground forces land | Shared specialised | PlanetSelectionBar for _planet; _ground generic | 0 (never seen) | Not seen. |
| Spy Pick | `/spy-pick` | `spy_pick_player` | card player | rob which player | Shared specialised | player picker (faction, VP, trade goods, commodities) | 21 (20) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Assassin Pick | `/assassin-pick` | `assassin_pick_player` | card player | target player | Shared specialised | player picker (faction, VP, trade goods, commodities) | 3 (3) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Insub Pick | `/insub-pick` | `insub_pick_player` | card player | whose tactic pool | Shared specialised | player picker (faction, VP, trade goods, commodities) | 7 (7) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Forward Base Pick | `/forward-base-pick` | `fsb_pick_player` | card player | another player | Shared specialised | player picker (faction, VP, trade goods, commodities) | 5 (5) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Burial Pick | `/burial-pick` | `abs_pick_player` | card player | which player | Shared specialised | player picker (faction, VP, trade goods, commodities) | 4 (4) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Pressure Pick | `/pressure-pick` | `dp[0-9]_pick_player` | card player | choose a player | Shared specialised | player picker (faction, VP, trade goods, commodities) | 6 (3) | Player-id options become names via useParticipantText; no standing shown. **Built 2026-10-07 (`2bf5da7`).** |
| Pressure Note | `/pressure-note` | `dp2_pick_promissory note` | card player | which note to give | Generic fallback | PendingChoiceModal list | 1 (1) |  |
| Jam System | `/jam-system` | `jamming_pick_system` | card player | which system to jam | Shared specialised | SystemPickParts | 28 (28) |  |
| Jam Player | `/jam-player` | `jamming_pick_player` | card player | whose token | Shared specialised | player picker (faction, VP, trade goods, commodities) | 28 (28) |  **Built 2026-10-07 (`2bf5da7`).** |
| Effort System | `/effort-system` | `war_effort_pick_system` | card player | which system | Shared specialised | SystemPickParts | 12 (12) |  |
| Probe Pick | `/probe-pick` | `probe_pick_system` | card player | which frontier | Shared specialised | SystemPickParts | 8 (8) |  |
| Silence Pick | `/silence-pick` | `silence_choose_system` | card player | system for ignore-blockade | Shared specialised | SystemPickParts | 5 (5) |  |
| Skilled Retreat | `/skilled-retreat` | `skilled_retreat_choose_system` | card player | withdraw to which system | Shared specialised | SystemPickParts | 12 (10) |  |
| Unexpected Recall | `/unexpected-recall` | `unexpected_pick_recall` | card player | recall token from where | Shared specialised | system picker (`_pick_recall`) | 6 (6) | Suffix `_pick_recall` does not match system regex (systemFacts.ts:112). **Built 2026-10-07 (`d01eda8`).** |
| Refit Pick | `/refit-pick` | `refit_pick_infantry` | card player | which infantry to replace | Shared specialised | unit pick with names and places | 48 (24) |  **Built 2026-10-07 (`2df0dd4`).** |
| Scuttle Pick | `/scuttle-pick` | `scuttle_pick_ship` | card player | which ship | Shared specialised | unit pick with names and places | 32 (15) |  **Built 2026-10-07 (`2df0dd4`).** |
| Divert Pick | `/divert-pick` | `divert_funding_pick_technology` | card player | which technology to return | Shared specialised | technology pick tiles | 36 (18) |  **Built 2026-10-07 (`0d98525`).** |
| Plagiarize Pick | `/plagiarize-pick` | `plagiarize_pick_technology` | card player | tech to steal | Generic fallback | PendingChoiceModal list | 8 (8) |  |
| Focus Research | `/focus-research` | `f_researched_pick_technology` | card player | research | Generic fallback | PendingChoiceModal list (maybe TechnologyModal via kind research; unverified) | 9 (9) |  |
| Repeal Pick | `/repeal-pick` | `repeal_pick_repeal` | card player | which law to repeal | Generic fallback | PendingChoiceModal list | 4 (4) | No law text in rows; unverified. |
| Bribery Count | `/bribery-count` | `bribery_pick_count` | card player | how many TG | Generic fallback | PendingChoiceModal list | 2 (2) |  |
| Seize Pick | `/seize-pick` | `seize_pick_fragment` | card player | which fragment | Generic fallback | PendingChoiceModal list | 1 (1) |  |
| Confusing Elect | `/confusing-elect` | `confusing_legal_text_elect` | card player | elect via legal text | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |
| Exchange Answer | `/exchange-answer` | `exchange_program_answer` | target | accept exchange | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |
| Ghost Move | `/ghost-move` | `ghost_squad_move` | card player | move | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |

### Exploration and relics

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Explore Reward | `/explore-reward` | `(card)_choose_reward (local_fabricators, vfs1, exp1, cm3 ...)` | explorer | choose reward of exploration card | Dedicated | exploration card as printed | 29 (6) | Planet cards appear in invasion overlay only when board.invasion set. **Built 2026-10-07 (`1a79345`).** |
| Relic Tech | `/relic-tech` | `(relic)_choose_technology` | relic owner | tech from relic | Generic fallback | PendingChoiceModal list | 0 (never seen) | Not seen. |
| Codex Pick | `/codex-pick` | `codex_take_action_card` | owner | take action card | Generic fallback | PendingChoiceModal list | 1 (1) |  |
| Titan Builder | `/titan-builder` | `titan_prototype_choose_builder` | owner | who builds | Generic fallback | list | 0 (never seen) | Not seen. |
| Converter Pick | `/converter-pick` | `stellar_converter_choose_target` | owner | planet | Shared specialised | PlanetSelectionBar | 0 (never seen) | Not seen. |
| Emphidia Pick | `/emphidia-pick` | `crown_of_emphidia_choose_planet` | owner | planet | Shared specialised | PlanetSelectionBar | 0 (never seen) | Not seen. |
| Dominus Purge | `/dominus-purge` | `dominus_orb_purge_to_move` | owner | purge relic to move | Generic fallback | list | 1 (1) |  |
| Neuraloop Purge | `/neuraloop-purge` | `neuraloop_choose_relic_to_purge` | owner | which relic to purge | Generic fallback | list | 0 (never seen) | Not seen. |
| Scanlink Pick | `/scanlink-pick` | `scanlink_explore` | Scanlink owner | explore planet | Shared specialised | PlanetSelectionBar | 7 (3) |  |
| Expedition Discard | `/expedition-discard` | `expedition_discard_action_card / expedition_discard_secret` | owner | pay with cards | Shared specialised | card tiles (cardOptions.ts) | 0 (never seen) | Thunder's Edge; not seen. |
| Expedition Place | `/expedition-place` | `expedition_place_system / expedition_placement_tiebreak` | owner | where to place | Generic fallback | list | 0 (never seen) | Not seen. system suffix does not match system regex. |

### Technologies, factions, leaders

| Handle | Slug | Subtype | Who | Asks | UI class | Component | Seen (runs) | Notes |
|---|---|---|---|---|---|---|---:|---|
| Bio-Stims | `/bio-stims` | `bio_stims_ready` | owner | ready planet/tech | Shared specialised | PlanetSelectionBar (extra options supported) | 47 (13) | Gallery case. |
| Psycho Pick | `/psycho-pick` | `psychoarchaeology_exhaust_specialty` | owner | exhaust specialty for TG | Shared specialised | PlanetSelectionBar (planet payload) | 76 (5) | Verify; label gain. |
| Transit Diodes | `/transit-diodes` | `transit_diodes_redeploy` | owner | move ground force | Shared specialised | UnitAbilityParts | 158 (9) |  |
| Chaos Mapping | `/chaos-mapping` | `chaos_mapping_choose_system` | owner | produce where | Shared specialised | SystemPickParts / UnitAbilityParts | 0 (never seen) | Never reached. |
| Quantum Swap | `/quantum-swap` | `quantum_datahub_swap` | owner | swap | Generic fallback | list | 0 (never seen) | Not seen. |
| Conduit Link | `/conduit-link` | `spatial_conduit_link` | owner | link | Generic fallback | list | 0 (never seen) | Not seen. |
| Nullification | `/nullification` | `nullification_field_end_turn` | owner | exhaust to end turn | Generic fallback | list | 2 (2) |  |
| Orbital Pick | `/orbital-pick` | `orbital_drop_choose_planet` | Sol | planet to drop on | Shared specialised | PlanetSelectionBar | 62 (18) |  |
| Orbital Mech | `/orbital-mech` | `orbital_drop_deploy_mech` | Sol | deploy mech | Shared specialised | UnitAbilityParts | 196 (61) |  |
| Peace Annex | `/peace-annex` | `peace_accords_annex` | Xxcha | annex planet | Shared specialised | PlanetSelectionBar | 71 (27) |  |
| Biomes Pick | `/biomes-pick` | `production_biomes_choose_player` | Hacan | player | Generic fallback | list | 0 (never seen) | Not seen. |
| Hacan Agent | `/hacan-agent` | `leader_hacanagent_branch` | Hacan | gain 2 commodities or replenish another | Dedicated | PoliticsDecisionParts (optionNote) | 233 (63) |  |
| Hacan Hero | `/hacan-hero` | `leader_hacanhero_free_production` | Hacan | free production | Generic fallback | list | 1 (1) |  |
| Xxcha Ready | `/xxcha-ready` | `leader_xxchaagent_ready_planet` | Xxcha | ready planet | Shared specialised | PlanetSelectionBar | 103 (27) |  |
| Xxcha Strip | `/xxcha-strip` | `leader_xxchaagent_remove_infantry` | Xxcha | remove infantry? | Generic fallback | list | 5 (5) |  |
| Xxcha Hero | `/xxcha-hero` | `leader_xxchahero_te_place / _planet` | Xxcha | place | Shared specialised | PlanetSelectionBar (planet payload) | 0 (never seen) | Not seen. |
| L1Z1X Hero | `/l1z1x-hero` | `leader_l1z1xhero_destination` | L1Z1X | destination | Generic fallback | list | 0 (never seen) | Not seen. |
| Jol-Nar Hero | `/jol-nar-hero` | `leader_jolnarhero_swap` | Jol-Nar | swap | Generic fallback | list | 0 (never seen) | Not seen. |
| Sucaban Pick | `/sucaban-pick` | `doctor_sucaban_exhaust / _remove_infantry` | Jol-Nar agent | planet | Generic fallback | list | 0 (never seen) | Not seen. |
| Specialist Planet | `/specialist-planet` | `specialist_compounds_choose_planet / _technology` | owner |  | Shared specialised | PlanetSelectionBar / list | 0 (never seen) | Not seen. |
| Fracture Ingress | `/fracture-ingress` | `fracture_choose_ingress_system` | Cabal/Creuss |  | Generic fallback | list | 0 (never seen) | Not seen. |
| Legendary Menu | `/legendary-menu` | `legendary_end_of_turn / legendary_pass` | owner | use legendary planet ability | Dedicated | legendary ability panel | 86 (12) |  **Built 2026-10-07 (`c435dd9`).** |
| Legendary Arms | `/legendary-arms` | `legendary_arms_vault / legendary_place / maxis / salvage / aurex / exterrix / acropolis / galactic_council` | owner | legendary effects | Shared specialised | PlanetSelectionBar for planet-payload ones; others list | 70 (11) | Mixed; per-subtype not verified. |
| Dynamic Pick | `/dynamic-pick` | `(card)_pick_(player\|planet\|system\|...) fallback` | card player | fallback naming for any card with a single pick | Generic fallback | generic or shared depending on payload | 0 (never seen) | action_cards.rs:4321. |

## Added by the Porkchop911 merge (2026-10-07)

Found by comparing every `DecisionContext::new` subtype before (`2b49782`) and after the merge (`bfb88b5`): 13 new subtypes, none known to the web client and none seen in a nightly run (the traces predate the merge). All fall back to the generic list unless noted; routing was read from the engine option shapes, not run in a browser. The counts at the top were taken before these were added; with the 13 built (12 shared, 1 dedicated, none generic) the totals are 157 handles: Dedicated 47, Shared specialised 62, Generic fallback 48.

| Handle | Slug | Subtype | Engine file | Asks | UI now | Status |
|---|---|---|---|---|---|---:|
| Ground Sustain | `/ground-sustain` | `ground_effect_sustain` | invasion.rs | use SUSTAIN DAMAGE in ground combat (option kind `ground_effect_sustain`) | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`359a290`) |
| Crimson Pay | `/crimson-pay` | `crimson_payment` | strategy_cards.rs | Crimson commander: gain or convert (kind `economy`) | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`a153733`) |
| Deepwrought Pay | `/deepwrought-pay` | `deepwrought_payment` | strategy_cards.rs | Deepwrought commander: gain or convert | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`a153733`) |
| Deepwrought Reduce | `/deepwrought-reduce` | `deepwrought_reduce_research` | strategy_cards.rs | reduce a research cost by 1 | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`0cab4dc`) |
| Research Waiver | `/research-waiver` | `research_waiver` | technology.rs | research waiver offer | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`febd08d`) |
| Waiver Pay | `/waiver-pay` | `research_waiver_payment` | strategy_cards.rs | payment for a waived research | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`febd08d`) |
| Vote TG | `/vote-tg` | `vote_spend_trade_goods` | vote.rs | Hacan commander: spend trade goods while voting | Dedicated (VoteGoodsPanel) | Built 2026-10-07 (`40d01d0`) |
| PDS Alternative | `/pds-alternative` | `place_structure_pds_alternative` | strategy_cards.rs | place a PDS or an alternative | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`b21efdb`) |
| Reinforce Place | `/reinforce-place` | `place_units_from_reinforcements` | action_cards.rs | place units (kind `place_unit`, system and count) | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`b46ceff`) |
| Take Revealed | `/take-revealed` | `take_revealed_action_card` | action_cards.rs | take a revealed action card | Shared specialised (card tiles) | Built 2026-10-07 (`c80e99a`) |
| Coexist | `/coexist` | `coalescence_coexist` | invasion.rs | Titans: fight or coexist | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`2220fbe`) |
| L1Z1X Copy | `/l1z1x-copy` | `leader_l1z1xagent_copy_planet` | leaders.rs | L1Z1X agent: choose a planet | Shared specialised (planet bar) | Built 2026-10-07 (`31a5bd0`) |
| Ssruu Round | `/ssruu-round` | `leader_ssruu_round_agent_unit` | engine | Ssruu round agent: choose a unit | Shared specialised (OfferCardPanel) | Built 2026-10-07 (`31a5bd0`) |

`doctor_sucaban_exhaust` became conditional (`doctor_sucaban_borrowed_exhaust` when the source is copied); it is a variant of the existing row. Status 2026-10-07: all 13 built the same day on branch porkchop-decision-uis-2026-10-07 (commit ids in the Status column); each has a gallery case and a screenshot in web/e2e/screenshots/M-porkchop-decision-uis. Most share the engine-built offer card (details.kind "offer", OfferCardPanel); /vote-tg has its own panel, /take-revealed reuses the card tiles and /l1z1x-copy the planet bar. Deepwrought reduce was also being misrouted to the technology picker (its option kind is research); fixed.

## Gaps in existing UIs (dedicated or shared, with weaknesses)

Handles refer to the table. Sources: `docs/DECISION_UI_REVIEW.md`, `TODO.md`, `web/TODOS.md`, code reading.

| Handle | Gap |
|---|---|
| `/secondary-offer` | Still context-less on the wire (no subtype or source, `crates/ti4-engine/src/strategy.rs:55-90`); the panel works from `details` only, so the header chips and source line are missing, and it cannot be matched in the gallery by subtype. |
| `/turn-menu` | Options the bar cannot place fall back to the old list (`GameShell.tsx` turn menu branch). Which ones is not determined. |
| `/activate-system` | No tactic-token count, enemy presence summary or reachable fleets (earlier review). |
| `/move-ships` | `web/TODOS.md` still says "ship movement max values seem broken" and "invasion is completely broken"; not re-verified at HEAD. |
| `/retreat-call` | No preview of the destination or what is left behind. |
| `/card-draft` | Does not show what the other seats already took or the speaker. |
| `/reaction-window` | Trigger text depends on `context.trigger` (display-only field) or the log fallback; with neither it shows a one-line "Reaction window" sentence (`reactionModel.ts` describeReaction level 3). |
| `/hand-limit`, `/secret-limit` | Shared tiles, but no per-card preview of what remains after the discard; multi-discard is submitted one at a time. |
| `/trade-propose` | The turn menu still lists one row per partner; the earlier proposal for a single Trade button is not confirmed done. |
| Event log | `web/TODOS.md`: log lines say "Decision resolved" with no text; not re-verified. |
| Map highlighting | Only planet picks, system picks, activation, payment and vote planets highlight the map (`boardPresentation.ts:305-340`). Player, card, ship, tech and unit picks have no map link. |

## Priority list: generic-fallback decisions that deserve a dedicated UI

Ranked by nightly frequency (count, number of runs of 74) and how confusing the raw option list is. All items are rows in the table above.

| # | Handle | Seen (runs) | Why it is confusing | Suggestion |
|---:|---|---:|---|---|
| 1 | `/predictive-restack` | 647 (35) | A list of "move 1 token from tactic to fleet" steps ending in "finish"; no pool counts. 52% of all generic-fallback volume. | Emit `details.kind=command_tokens`, `mode=redistribute` and arrangement options like `/token-restack`, or add a "move" mode to CommandTokenPanel (`technology.rs:395-412`, `commandTokens.ts:72`). **Done 2026-10-07 (`a44d0e8`).** |
| 2 | `/legendary-menu` | 86 (12) | Offers "use a legendary planet ability" with raw ability labels and no effect text. | Show planet, ability text and cost per option, or add it to the turn bar as a component. **Done 2026-10-07 (`c435dd9`).** |
| 3 | `/investments-pick` | 85 (17) | Strategy cards listed as plain text. | Reuse the strategy card grid: widen `strategyDraft` in `PendingChoiceModal.tsx:147` to this subtype and show trade goods already on each card. **Done 2026-10-07 (`6f0b6db`).** |
| 4 | `/munitions-reroll` | 79 (27) | Mid-combat yes/no costing 2 trade goods, easy to click through; no dice or hit count shown. | Combat-overlay prompt with TG balance and the current dice/hits. **Done 2026-10-07 (`821d8b7`).** |
| 5 | `/refit-pick` | 48 (24) | Which infantry to replace by a mech; raw planet or unit ids. | Planet/unit picker with names and mech reinforcements left. **Done 2026-10-07 (`2df0dd4`).** |
| 6 | `/divert-pick` | 36 (18) | Returns a technology to get another; names without text. | Technology tiles with tier, colour and text (reuse TechnologyModal cards). **Done 2026-10-07 (`0d98525`).** |
| 7 | `/scuttle-pick` | 32 (15) | Raw ship ids; destructive. | Ship tiles with system and refund value; confirm summary. **Done 2026-10-07 (`2df0dd4`).** |
| 8 | `/predict-outcome` | 31 (10) | Outcome labels without the agenda being voted. | Reuse AgendaBallotModal card block. **Done 2026-10-07 (`096e3aa`).** |
| 9 | `/explore-reward` | 29 (6) | Exploration card with several effects; only labels. | Show card name, text and effect per option (`findExplorationCardMeta` already exists; used only in InvasionOverlay). **Done 2026-10-07 (`1a79345`).** |
| 10 | `/player-picks` (cluster: `/jam-player` 28/28 runs, `/spy-pick` 21, `/insub-pick` 7, `/pressure-pick` 6, `/forward-base-pick` 5, `/burial-pick` 4, `/assassin-pick` 3) | ~74 (all 28 jam runs) | Plain list of seats; no standing, VP, trade goods or what the card does to them. | One shared seat-picker (faction, VP, TG, commodities, speaker) like `PoliticsDecisionParts`. **Done 2026-10-07 (`2bf5da7`).** |
| 11 | `/disgrace-pick` | 16 (16) | Strategy card ids without who holds them. | Card grid with holder. |
| 12 | `/mecatol-lift` | 13 (13) | Costs 6 influence, then a payment step; no explanation of custodians or the VP gained. | Confirmation card with influence available and the +1 VP and Imperial effect. |
| 13 | `/law-discard` | 12 (4) | Imperial Arbiter swap offered with raw law and card labels. | Show the law's text and the card to be swapped. |
| 14 | `/unexpected-recall` | 6 (6) | Recalls a token from a system; list of raw ids. | Quick win: extend `SYSTEM_PICK_SUBTYPE` (`systemFacts.ts:112`) with `_pick_recall` so it gets the system picker. **Done 2026-10-07 (`d01eda8`).** |
| 15 | `/focus-research`, `/plagiarize-pick` | 9, 8 | Technology picks without text. | Route through TechnologyModal research mode (needs option kind `research`). |

Never-seen generic rows to cover when they appear (cannot rank by frequency): `/warfare-tactical`, `/construction-ability`, `/mid-pause`, `/entropic-tech`, `/quantum-swap`, `/conduit-link`, `/titan-builder`, `/neuraloop-purge`, `/expedition-place`, `/deal-draft` (not found in `choiceModel.ts`; how the web client displays the diplomacy deal builder was not determined), `/note-use` (promissory notes by alias).

## Cross-checks and unknowns

- Nightly digest summary (`nightly-reports/latest-summary.md`) lists never-reached subtypes `bombardment_target`, `assign_ground_casualty`, `warfare_free_tactical`, `construction_choose_ability` and relic actions; consistent with rows marked never seen. `/bombard-pick` has no dedicated renderer in `choiceModel.ts` (explicitly excluded from planet selection at line 142), so it would land in the generic list unless `InvasionOverlay` handles it; not determined.
- `/legendary-arms` and `/specialist-planet` rows mix planet-payload subtypes (planet bar) and others; per-subtype routing not verified.
- Gallery coverage: `web/src/dev/decisionGalleryCases.ts` and `removeUnitGalleryCases.ts` hold cases for most dedicated and shared rows (activate, movement, cargo, ground commit, payment, produce, sustain, casualty, retreat, votes, trade, reaction, scoring, draft, research, structure, Bio-Stims, Elect Planet, system picks, politics, hand limits, tokens, secondary, Imperial, unit abilities, remove unit). Generic rows with no gallery case include everything in the priority list above.
- Tests: `DecisionInvariantMatrix.test.tsx` and component tests per panel exist; per-row test coverage was not tallied.
- The decision-delivery registry (`crates/ti4-engine/tests/decision_delivery_inventory.rs`) lists choice construction sites by function, not subtype, so it was not used as the subtype source.
