# Decision UI review

Review of how the web client presents decisions, written 2026-10-06. Review only: no app code
was changed. The goal is that a human can make each decision quickly, with the information the
decision depends on visible without hunting for it.

## How this was done

- **Inventory:** every decision subtype that occurred in the 36 nightly runs under
  `nightly-reports/*/runs/*/trace/trace.jsonl` (23,744 decisions), plus the engine's decision
  sources in `crates/ti4-engine/src/decision_context.rs`.
- **Routing:** how the client picks a renderer: `deriveChoiceRendererModel` in
  `web/src/presentation/choiceModel.ts`, then the `workflowRenderers` map in
  `web/src/components/GameShell.tsx:192-353`. A decision that matches no dedicated rule falls back
  to `generic_selection` (`choiceModel.ts:611`), which `PendingChoiceModal.tsx` renders as a plain
  radio or checkbox list.
- **Evidence for "what the UI shows today":** the component source, and the option and context
  data in the traces. I did not run the app, so layout statements come from the code.
- **Caveat on counts:** the traces store option ids, kinds and labels but not option payloads.
  Where I say a decision "falls back to the generic list", that is inferred from its subtype and
  the routing rules, not observed in a browser. The planet-selection rule
  (`choiceModel.ts:196-208`) depends on `payload.planet`, so a few decisions I list as generic may
  already route to the planet bar. These are marked "verify".

## What decisions look like today

| Renderer | Used for |
|---|---|
| Dedicated | payment, tactical movement and cargo, production, system activation, invasion landing, technology research, objective scoring, planet selection, agenda votes, trade, combat (sustain, casualty, retreat), reaction windows with at most 4 options |
| Generic list | everything else |

The generic list (`PendingChoiceModal.tsx`) shows:

- a title, which is the engine prompt string (`:146`, `:215-221`);
- the actor's label (`DecisionHeader.tsx:33-40`);
- one row per option, with `opt.label` and an optional `opt.description` (`:325-379`);
- a text filter (`:252`);
- a confirm button whose caption comes from a short subtype list (`:398-411`).

It does not show: where the decision comes from (card, ability, rule), what the player currently
holds, what each option costs or changes, or any rules text, with one exception: the strategy-card
draft has a special card grid with primary and secondary text (`:124`, `:354-362`).

By the subtype names, roughly **15.9k of 23.7k decisions (about two thirds)** use this generic
list. They are the bulk of what a player actually clicks. The decision gallery
(`web/src/dev/decisionGalleryCases.ts`) and the invariant matrix
(`DecisionInvariantMatrix.test.tsx`) cover the dedicated UIs. Of the generic decisions below only
`place_structure`, `bio_stims_ready` and a synthetic bounded selection have a gallery case; the
turn menu, command-token, secondary and hand-limit decisions have none.

### Data the server already sends but the generic UI ignores

`DecisionContextDto` (`protocol/types.ts:75-89`) carries `source`, `target`, `phase`, `round`,
`optional`, `outstanding` and `details`. The engine's `DecisionSource`
(`decision_context.rs:59-74`) distinguishes `StrategyCard {card, secondary}`, `ActionCard`,
`FactionAbility`, `Content`, `Agenda`, `Reaction` and `Rule`. Only the planet bar uses it
(`planetSelection.ts:78-103`, `describePlanetDecision`). Every other generic decision shows the
bare prompt.

About **2,594 decisions (10.9%) have no context at all**: their subtype in the trace is
`prompt:<text>`. They are the strategy-card secondary offers (1,975), the "spend 3 influence for a
command token" offer in its context-less form (347), the over-fleet-supply and over-capacity
removals (199) and a few ability prompts (73). The secondaries are built in
`crates/ti4-engine/src/strategy.rs:80-86` without a source, so the client cannot even tell the
player which strategy card they are reacting to.

---

## 1. Ranking: worst offenders

Rank is frequency in the traces times how little the player is shown. "Gap" is what a player needs
to decide well and does not get.

| # | Decision (subtype) | Count | Gap | Priority | Effort |
|---|---|---:|---|---|---|
| 1 | `prompt:action phase`, the turn menu | 4,250 | strategy-card text, which cards are usable, component effects, card text for "play X"; transaction offers (32% of all menu options) bury the real choices | P0 | M |
| 2 | `end_turn` | 3,290 | a modal asked after every action, usually with nothing else to pick | P0 | S |
| 3 | `gain_command_token` | 2,518 | current pool sizes, and what the tokens are for | P0 | S |
| 4 | strategy-token secondaries (`prompt:spend a strategy token to ...`) | 1,975 | no source card, no remaining tokens, "decline / draw" labels | P0 | M |
| 5 | `status_redistribute_tokens`, `warfare_redistribute_tokens`, `predictive_intelligence_redistribute` | 681 + 82 + 189 | up to 120 radio rows of "tactic 0 / fleet 2 / strategy 14" | P0 | M |
| 6 | `place_structure` | 582 | verify: where on the map, cost, what is already there | P1 | S |
| 7 | `discard_over_hand_limit` | 540 | card text, how many to discard, what is left | P1 | S |
| 8 | `buy_token_with_influence` and its context-less twin | 578 | influence available, which pool the token goes to | P1 | S |
| 9 | `politics_place_agenda`, `politics_choose_speaker` | 432 + 216 | agenda name and text; VP, order and what the speaker role gives | P1 | S |
| 10 | system picks: `diplomacy_choose_system`, `warfare_recall_token`, `jamming_pick_system` (and similar) | 220 + 82 + 15 | options are raw system ids ("14"); no map, no planets, no ships | P1 | M |
| 11 | `return_over_secret_hand_limit` | 161 | options are aliases ("dp", "pem"); no name or condition | P1 | S |
| 12 | over fleet supply or capacity: "remove a unit" | 199 | raw unit ids, no supply count, no location, no cargo effect | P1 | M |
| 13 | `trade_choose_replenish` | 170 | no commodity counts per player | P2 | S |
| 14 | reaction windows with more than 4 options | n/a | threshold at `choiceModel.ts:584` drops them into the generic list | P2 | S |
| 15 | `orbital_drop_deploy_mech`, Sling Relay, Transit Diodes | 88 + 64 + 44 | raw ids in labels, no board view | P2 | M |
| 16 | `leader_hacanagent_branch` | 93 | lists raw player ids; no reason to choose | P2 | S |
| 17 | `activate_system` (dedicated) | 767 | tactic pool, contested status, planets | P2 | M |

**Five highest-priority improvements:** the turn menu as an action bar (1, 2), command-token
decisions with pool counts and steppers (3, 5), strategy-card secondaries with a source and
remaining tokens (4, 8), a common decision header and context strip for all generic decisions
(cross-cutting section), and card text and names for the hand-limit decisions (7, 11).

---

## 2. Cross-cutting recommendations

1. **One header for every decision:** `Source - question - whose decision`, for example
   "Politics (secondary) - Place the agenda on top or bottom - your decision". Build it from
   `context.source` and the prompt. The helper already exists (`formatDecisionSource`,
   `planetSelection.ts:78`); move it to a shared module and use it in `DecisionHeader`.
2. **A context strip showing what the player holds that is relevant:** command pools, trade
   goods, commodities, ready influence and resources, hand size, strategy tokens left. Which
   fields appear depends on the subtype, so keep one table of "subtype -> fields".
3. **Show cost and net effect on each option row**, not just a label: "-1 strategy token, +2
   action cards". Derive it from the option payload where it exists and add it where it does not.
4. **No raw ids in the UI.** Labels today include `sol_mech`, `sol_carrier2`, `dp`, `pem`,
   `remove|0` and bare system numbers. A single presentation function should map unit,
   objective, card, system and planet ids to names (the content catalog already has most of them).
5. **Use the map for every system or planet choice**, with the same highlight and inspector the
   activation and planet bars already use. This was already noted as M5 and M7 in the
   playthrough todos.
6. **Replace bare yes/no with named buttons** that state the effect, plus an obvious skip.
7. **Defaults and keyboard:** pre-select the sensible option, make Enter confirm, number keys pick
   rows, Space skip or end turn (reaction windows already use Space to pass).
8. **Confirmation by stakes:** one click for reversible or low-cost choices; for permanent ones
   (discard, destroy, burn a token) show a one-line summary of what will change before confirming.
9. **Use bars instead of modals for frequent decisions.** A modal is right for rare, weighty
   choices. The turn menu and "end turn" happen thousands of times; they belong on a persistent
   bar at the edge of the board.
10. **Mobile:** modals at 960px width with a scrolling option list are poor on a phone. Use a
    bottom sheet, 44px touch targets, and grouped, collapsible long lists.
11. **Say why an option is unavailable** (disabled with a tooltip) instead of leaving it out, where
    the engine can supply a reason.
12. **Cover them in the gallery.** Add a gallery case per decision below, so each redesign has a
    fixture and a screenshot.

### Server data needed (summary)

- Give the context-less decisions a context: `strategy.rs:80-86` (secondaries), the
  over-supply and over-capacity removals, Sling Relay and the other `prompt:` ones. At minimum a
  `source` and a `subtype`.
- A stable `details` object per subtype (for example `details.pools = {tactic, fleet, strategy}`
  for token decisions, `details.strategy_tokens_left`, `details.influence_ready`).
- Option payloads that name the thing: `payload.entity = {kind, id}` for cards, units, systems and
  planets, and `payload.cost` and `payload.effect` where there are any.
- `payload.system` on the system picks (`strategy_cards.rs:837` diplomacy, `:1229` warfare recall
  use `ChoiceOption::labelled(id, kind, id)` with no payload).

---

## 3. Findings by phase

Format: **UI today / player needs / proposal / data / effort**.

### A. Action phase: the turn menu

#### A1. `prompt:action phase` (4,250) and A2. `end_turn` (3,290)

- **UI today:** generic radio list titled "action phase". Options, from the traces:
  "take the strategic action of 2. Diplomacy", "take a tactical action", "open a transaction with
  sol" (one per other player), "Carth of Golden Sands" (a leader), "play Reactor Meltdown",
  "use Sling Relay", "Orbital Drop: spend a strategy token to land 2 infantry". Average 4.3
  options, maximum 43. Of 18,325 options seen, 5,782 (32%) are "open a transaction".
- **Player needs:** what each strategy card does (primary text), which of their cards are
  unused, whether a tactical action is possible (tactic tokens left), what each component or
  action card does, and the cheap way to pass or end the turn.
- **Proposal:** a persistent action bar next to the board, not a modal:

```
 [Tactical (3)]  [Strategic v]  [Components v]  [Trade]            [End turn]
   Strategic menu:  2 Diplomacy   -- text: "Choose 1 system other than Mecatol..."
                    8 Imperial    -- text: ...           (used cards greyed)
   Components menu: Carth of Golden Sands (leader)  -- text
                    Play Reactor Meltdown (action)   -- text on hover/expand
```

  One Trade button opens a player picker (instead of one row per player), which removes about a
  third of the menu. `end_turn` becomes the always-visible button on the bar; when there is
  nothing else to do, a single click or Enter ends the turn.
- **Data:** strategy-card text is in the content catalog already (`findStrategyCardMeta`);
  action-card and leader text likewise. Needs `details.tactic_tokens_left` for the Tactical
  button state, or the client can read the player's pools.
- **Effort:** M. **Priority:** P0.

### B. Strategy phase

#### B1. `draft_strategy_card` (1,602), partly customized

- **UI today:** a card grid with initiative, name and printed primary and secondary text
  (`PendingChoiceModal.tsx:124`, `:354-362`). This is the best of the generic renderers.
- **Missing:** which cards the other players already took (so the player sees what is left and
  who is ahead in initiative), and your own previous picks in a two-card game. A one-line summary
  "cards left: 3; speaker: X" in the header would help.
- **Effort:** S. **Priority:** P2.

#### B2. `investments_pick_strategy_card` (25)

Same grid treatment as B1; currently falls to the plain list. **Effort:** S. **Priority:** P3.

### C. Strategy card follow-ups (primary and secondary)

#### C1. Strategy-token secondaries (1,975) and `buy_token_with_influence` (578)

- **UI today:** a modal titled "spend a strategy token to draw two action cards" with two radio
  rows, "decline" and "draw" (for others: "build", "produce", "research", "ready", "replenish",
  "spend"). No source: these are created without a context (`strategy.rs:80-86`).
- **Player needs:** which strategy card was played and by whom, what the secondary does in full,
  how many strategy tokens the player has left, and the cost (4 resources for research, 3
  influence for a token).
- **Proposal:**

```
 Politics (played by Hacan) - secondary
 Spend 1 strategy token to draw 2 action cards.
 You have 3 strategy tokens.  Hand: 4 action cards.
 [Spend token and draw]   [Skip]
```

  Named primary button with the effect, and the secondary text from the catalog. For influence
  purchases show "Influence available: 5 ready (Jord 4, ...)".
- **Data:** add `source = StrategyCard{card, secondary: true}` and `details.tokens_left` in
  `strategy.rs`. For influence purchases the planet readiness is already in the player view.
- **Effort:** M (server context plus one shared yes/no component). **Priority:** P0.

#### C2. `gain_command_token` (2,518)

- **UI today:** generic list: "tactic pool", "fleet pool", "strategy pool".
- **Player needs:** current size of each pool and what that pool does, and how many tokens remain
  to place when several are gained.
- **Proposal:** three large buttons with the current count and effect, for example
  "Tactic 3 -> 4: needed to activate systems", "Fleet 4 -> 5: fleet supply", "Strategy 2 -> 3:
  secondaries and abilities". Remember last choice as a default; a key 1/2/3 picks.
- **Data:** the pools are in the player view; add `details.pools` to remove a second lookup.
- **Effort:** S. **Priority:** P0.

#### C3. Redistribution: `status_redistribute_tokens` (681), `warfare_redistribute_tokens` (82), `predictive_intelligence_redistribute` (189)

- **UI today:** for status and warfare, up to 120 radio rows such as "tactic 0 / fleet 2 /
  strategy 14", in a scroll area with a text filter. Predictive Intelligence lists moves like
  "move 1 token from tactic to fleet".
- **Player needs:** the current pools, the total to distribute, and an immediate view of the
  result.
- **Proposal:** three bars with +/- steppers and a "remaining" counter, as already sketched in the
  playthrough todo M9: invalid distributions are impossible, Reset returns to the current
  state. The submitted option is the matching `a|b|c` id.
- **Data:** none new; the options enumerate all valid triples, so the stepper can map to an id.
- **Effort:** M. **Priority:** P0.

#### C4. Politics: `politics_place_agenda` (432), `politics_choose_speaker` (216)

- **UI today:** "place redistribution where" with "on top of the deck" / "on the bottom";
  "who becomes speaker" with "hacan becomes speaker". The agenda's name appears only inside the
  prompt; its text and outcomes do not.
- **Proposal:** show the agenda card (name, type, outcomes, text) above the top/bottom choice. For
  the speaker, show each player's faction, VP, current seat order and whether they hold the
  speaker token now.
- **Data:** the agenda card text is available (`AgendaBallotModal` already reads
  `context.details.agenda_card`); add it to this decision's details. Speaker data is in the view.
- **Effort:** S. **Priority:** P1.

#### C5. System picks: `diplomacy_choose_system` (220), `warfare_recall_token` (82), `jamming_pick_system` (15) and similar (`skilled_retreat_choose_system`, `war_effort_pick_system`, `probe_pick_system`, `silence_choose_system`)

- **UI today:** radio rows labelled with the system number, "14", "69". The engine builds them
  as `ChoiceOption::labelled(id, kind, id)` with no payload (`strategy_cards.rs:837`,
  `:1229`). (Not verified for the action-card ones; the same pattern is likely.)
- **Player needs:** the system name, its planets and their value, what is in it (own or enemy
  ships, command tokens), and where it is.
- **Proposal:** choose on the map, as for activation: highlight the legal systems, show the
  system inspector on hover, confirm bar "Diplomacy: choose a system - [Confirm]". For Warfare
  recall, show the command tokens on the map and which systems they sit in.
- **Data:** add `payload.system` to each option so the existing system-pick machinery can match
  it; add `context.target` where a single system applies.
- **Effort:** M. **Priority:** P1.

#### C6. `trade_choose_replenish` (170), `peace_accords_annex` (31)

Show each candidate's current commodities and faction; for Peace Accords show the adjacent
planets with resources and influence. **Effort:** S. **Priority:** P2.

### D. Status phase and limits

#### D1. `discard_over_hand_limit` (540) and `return_over_secret_hand_limit` (161)

- **UI today:** "over the hand limit - discard one of 11" with names such as "Direct Hit", and
  "return a secret objective to the deck" with options "return dp", "return pem". The secret
  objective rows show aliases and no condition.
- **Player needs:** how many to discard and how many remain after, each card's text, and for
  secrets the objective's condition and points, plus whether it can still be scored.
- **Proposal:** a hand view with the cards as tiles, text visible, a counter "Discard 2 of 11
  (limit 7)", multi-select when more than one must go, and a confirm that says what leaves.
- **Data:** names and text from the content catalog; add `details.limit` and `details.count`.
- **Effort:** S. **Priority:** P1.

### E. Tactical and logistics

#### E1. `activate_system` (767), dedicated

- **UI today:** a bar saying "Activate <name> (#id)" with Confirm and Cancel
  (`SystemActivationBar.tsx:98-139`); for blocked systems, "already contains your command token".
  The prompt line is the engine text.
- **Missing:** tactic tokens left, whether the system holds enemy ships or structures (a combat
  is coming), which of the player's fleets can reach it, and a short summary of its planets.
- **Proposal:** a single line under the name: "Tactic tokens 3 | enemy: 2 ships | 2 planets (Res
  4 / Inf 2) | reachable by: Fleet A, Fleet B". Server data is not needed.
- **Effort:** M. **Priority:** P2.

#### E2. `place_structure` (582), verify

- **UI today:** options "place pds on arretze", "place spacedock on kamdorn". The options
  carry a `unit` key (`strategy_cards.rs:975`), so this may already go to the planet bar; if so,
  check that choosing between a PDS and a space dock on the same planet is explicit.
- **Proposal:** planet pick on the map plus a structure toggle, with each structure's cost and
  what the planet already has. **Effort:** S. **Priority:** P1.

#### E3. Over fleet supply or capacity (199): "remove a unit: over fleet supply in 01"

- **UI today:** context-less; options "remove sol_carrier", "remove destroyer",
  "remove dreadnought".
- **Player needs:** fleet supply used and allowed ("9 of 8"), the system, each ship's value and
  cargo, and for capacity problems what is carried.
- **Proposal:** the system's fleet as tiles with unit icons, cost and cargo, a header "Remove 1
  ship: fleet supply 9/8 in Jord", and a cargo-effect preview.
- **Data:** add a context with `details.supply = {used, limit}` and `target = System`.
- **Effort:** M. **Priority:** P1.

#### E4. Abilities with unit moves: `orbital_drop_deploy_mech` (88), Sling Relay produce (64),
`transit_diodes_redeploy` (44), `bio_stims_ready` (18)

Labels carry raw ids ("deploy 1 sol_mech for 3 resources", "produce 1x sol_carrier2 for 3",
"move hacan_mech from arretze to hercant"). Show unit names and icons, cost against resources
available, and for Transit Diodes the source and target planets on the map. The Sling Relay
production prompt has no context, so it misses the production drawer; give it the
`produce_unit` subtype to reuse that UI. **Effort:** M. **Priority:** P2.

### F. Combat

- **Assign hits:** already redesigned (staged panel).
- **`announce_retreat` (25) / `retreat_to` (4):** dedicated, but the retreat option does not show
  where the fleet would end up or what it leaves behind. Show the destination's contents and
  whether a retreat is legal (adjacent, not blocked). **Effort:** S. **Priority:** P2.
- **Reaction windows during combat** (`reaction_after_COMBAT_ROUND_STARTED` 30,
  `reaction_after_SPACE_COMBAT_STARTED` 5, `reaction_when_HITS_TO_ASSIGN` 4, and others): see H1.
- **Combat result summary:** requested separately; nothing in this review changes it.

### G. Agenda

- `cast_vote`, `vote_exhaust_planet`, `vote_tiebreak` use `AgendaBallotModal` with the card, the
  outcomes and the results. Remaining gaps worth checking: votes available (sum of influence of
  ready planets) next to the vote count, the order of voting and the speaker's tie-break.
  **Effort:** S. **Priority:** P3.
- `politics_place_agenda`: see C4.

### H. Reactions, cards, leaders

#### H1. Reaction windows (`reaction_*` and `play_reaction_*`, 434 occurrences across subtypes)

- **UI today:** `ReactionStatusBar` shows what triggered the window and the card's trigger text.
  It applies only when the subtype starts with `play_reaction_`, or the decision is optional,
  has a `Reaction` source and at most 4 options (`choiceModel.ts:584-590`). A window with more
  than 4 cards becomes the generic list.
- **Proposal:** lift the 4-option limit and let the bar scroll or wrap; show the card text on
  expand; always show the trigger first ("After Sol activated system 14 ...").
- **Effort:** S. **Priority:** P2.

#### H2. Action cards offered from the turn menu ("play Reactor Meltdown", about 120 each)

See A1: show the card's text where it is offered. **Effort:** part of A1.

#### H3. Leaders and abilities: `leader_hacanagent_branch` (93), `leader_xxchaagent_*` (36),
`psychoarchaeology_exhaust_specialty` (34), `legendary_*`

`leader_hacanagent_branch` lists "replenish player_44a9e420..." in the trace; the client maps
participant ids in prose to names, so check the rendered label, but the decision still shows
no reason to prefer one option. Show each player's commodities and what "replenish" gives.
Legendary planet abilities need the ability text next to the choice.
**Effort:** S. **Priority:** P2.

### I. Trade and diplomacy

`propose_transaction` (1,566) and `answer_transaction` (445) have the redesigned trade desk. One
open item from the turn menu: the one-row-per-player entries should become one Trade button
(see A1).

---

## 4. Suggested order of work

1. **Context and source on every decision** (server, small): give the context-less decisions a
   context; add `details` for tokens, limits and supply. Unblocks most of the rest.
2. **Common header and context strip** in `DecisionHeader` and `PendingChoiceModal` (client):
   immediate improvement for all generic decisions.
3. **Action bar** replacing the turn-menu modal and the end-turn modal (A1, A2).
4. **Command-token decisions** (C2, C3) with pools and steppers.
5. **Strategy-token secondaries** (C1) with the shared named-button pattern.
6. **Hand-limit and secret-return tiles** (D1) and the id-to-name audit.
7. **System picks on the map** (C5) and fleet-supply removal (E3).
8. Everything marked P2 or P3.

Each step should add a decision-gallery case and a screenshot, so reviews have something to look
at.

## 5. Open questions for the user

- Should the turn menu become a persistent bar at all, or stay a modal with a better layout? The
  bar changes how the board feels, and it replaces the screen most players see most often.
- Is it acceptable to add server-side `details` and `payload.entity` fields, or must the client
  derive everything from the existing payloads?
- Which of the remaining dedicated UIs (payment, production, trade) should be re-reviewed with
  real players? This review checked them only by reading the code and the gallery.
