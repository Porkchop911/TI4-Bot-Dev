# Reaction UI plan: say what happened, why you may react, and what you can do now

Status: plan only (no code changed). Branch `smoke-playthrough`. All file:line references are to the checkout at the time of writing and will drift; the symbol names are the stable handle.

## 1. Goal and user stories

Goal: whenever a player is offered a reaction, the dialog states (a) what just happened and who did it, (b) why that makes a reaction possible, and (c) exactly what each available reaction will do, in the card's own words, with a clear Pass.

User stories:

1. As a player holding Sabotage, when Anna plays Mining Initiative I see: "Anna (Red, The Federation of Sol) played the action card Mining Initiative: <full printed text>. Now you can play Sabotage: <its text>."
2. As a player holding Decoy Operation (or any other timing card), when Bob activates a system I see: "Bob activated system 27 (Lodor, Mecatol neighbour); now you can play Decoy Operation: <window text>: <effect text>".
3. As a player I never see engine identifiers (`ACTION_CARD_PLAYED`, `play_reaction_when_...`, `sabo1`) or doubled verbs ("Play play Sabotage").
4. As a player I can tick "never offer me this card" from the dialog itself (today the per-card mode toggle lives only in the player sheet and is not sent to the server, see 6.6).
5. As a spectator or a waiting player I see who is deciding and what they are deciding about, without any private information.
6. As the smoke harness (`web/e2e/smokePlaythrough.ts`) I can still find and click Play and Pass.

## 2. What exists today (findings)

Engine
* Reactions are a standing slot per seat per (event, relation) registered by `reactions::arm` (`crates/ti4-engine/src/reactions.rs:~880`, fn `arm`; slot built in fn `slot` ~`:850`). The slot is optional, unlimited, repeatable in the window, with a stateful condition `playable_now(...)` non-empty.
* The slot's effect is a decision in up to two steps:
  1. Outer decision raised by `Resolver::pick_with_context` (`crates/ti4-engine/src/timing.rs:759-830`): subtype `reaction_{when|after}_{EVENT}`, source `DecisionSource::Reaction(event_type)`, options of kind `ability` (id `reaction:<faction>:<EVENT>:<when|after>`; label `Play <Card>` and payload `card`, `card_name` if every playable card has the same name, else `Choose an action card...`) plus `decline`. The context carries only actor, source, subtype, phase, round, optional and, in a battle, the system target (`about_battle`). It does not carry the triggering event.
  2. Inner decision only when the player holds several differently named cards for the window (`reactions.rs` slot closure, ~`:858-885`): subtype `play_reaction_{when|after}_{EVENT}`, source `Rule("22.1")`, options kind `action_card` (`reaction_card_options` `:~398-415`, label `play <name>`, payload `card`, `card_name`). This inner decision has no decline option, no `optional`, and also no event data.
* Two non-card abilities share the windows: Instinct Training (`instinct_training`, `reactions.rs:~1057`, subtype `instinct_training_cancel`, source `Content("it")`, window ACTION_CARD_PLAYED when) and the L1Z1X agent (`l1z1x_agent`, `:~992`, subtype `l1z1x_agent_swap`, window SYSTEM_ACTIVATED after). Their prompts already embed some trigger text ("cancel <card alias>", "let <seat> replace...") but as raw ids in the prompt string.
* The trigger is a typed `Event { id, event_type, payload: BTreeMap<String, Value>, cancelled }` (`crates/ti4-engine/src/event.rs:14`). The event is in hand at both decision sites (`pick_with_context(.., event, ..)` and the slot closure's `event`). So the data to build a trigger exists exactly where the decision is created; it is just thrown away.
* Window text: each action card record carries `window` (printed timing) and `text` (effect). `window_for` (`reactions.rs:623`) reads `window` and maps it through `window_table()` (`:315-612`) to (event, relation, guard). The web catalog (`web/scripts/generate-content-manifest.mjs:101,117,141,159`) copies only `text` into `description`; `window` is not in `generatedContentManifest.ts`. Note: the engine `ChoiceOption` has no `description` field (`crates/ti4-engine/src/choice.rs:48-68`); the `description: "When another player plays an action card: cancel that action card."` in the E-capture mock (`web/e2e/screenshots/E-reaction-offer/capture-1-*.ts`) is invented by the mock and does not exist on the real wire. The real option has only `label`, `kind`, `payload.card`, `payload.card_name`.
* `playable_now` (`reactions.rs:640-665`) silently drops cards whose seat mode is `Never` (commit 5a7f407; `state.rs` `reaction_card_modes`). Such windows never open, so no toast and no decision. If every card is dropped the slot condition is false and nothing is asked.

Event payloads at the moment of the trigger (what is recorded today; see inventory):
* Always `player`. Sometimes `system` (SYSTEM_ACTIVATED `game.rs:2269`, INVASION_BEGAN `:358`, PRODUCTION_USED `:447`, SHIP_DESTROYED combat.rs:2105, SPACE_CANNON_HITS `:250`, HITS_TO_ASSIGN combat.rs:3335, ANTI_FIGHTER_BARRAGE_STARTED combat.rs:2938), `planet` + `system` (PLANET_CONTROL_GAINED invasion.rs:2345), `card` (ACTION_CARD_PLAYED `reactions.rs:713-716`, ACTION_CARD_DISCARDED `game.rs:1945`), `agenda` (AGENDA_REVEALED `game.rs:3724`), `elected_player` (AGENDA_RESOLVED `:4317`), `partner` (TRANSACTION_OPENED `:1750`), `unit`/`last` (SHIP_DESTROYED), `hits`/`round`/`gunner`.
* Gaps: SHIP_MOVED has only `player` (`game.rs:2471`, no system, no units); STRATEGIC_ACTION_BEGAN has only `player` (the card is a local `card` variable at `:1795`); TURN_BEGAN, PLAYER_PASSED, TURN_PASSED, ACTION_COMPLETED, STRATEGY_CARD_CHOSEN carry `player` only; INVASION_BEGAN / COMBAT_ROUND_STARTED need the attacker and system checked.
* The event id is the engine-local `EventSequence` id (`event.rs`, one-based, per trace).

Server
* The pending choice is delivered as `PendingChoiceEnvelope { nonce, choice: EngineChoice{player, prompt, options, context, details} }` (`crates/ti4-server/src/protocol/server.rs:18`; TS twin `web/src/protocol/types.ts:99-118`). `details` is a free `Record<string, unknown>` display bag ("who played a card" already mentioned in its doc comment but unused for reactions). `DecisionContext` is serialized as-is, `DecisionContext::visible_to` (`decision_context.rs:~235`) strips `outstanding` for other seats, and `canonical()` (`:~258`) feeds the replay fingerprint (OBS-003b) and the sim behaviour baseline; any new field must stay out of `canonical()` or follow the versioned process in `crates/ti4-sim/src/behavior.rs`.
* The public event log already records the play of a card: `played_card_detail` (`server.rs:~590`) yields "`<actor> played <Card name>`" for reaction selections, and `worker.rs:~675-740` (`on_observed_offer`) publishes the settled decision to the log before the next nested offer is shown, deliberately waiting until `state.action_card_plays` contains the card (so "X played Sabotage" appears before the window that reacts to it). The log record has `actor`, `detail` (string), `movement` (`MovementFact`), `round`, `phase`, `stage`, but no card id, system, or structured trigger. Reaction decision records themselves are suppressed from the log by `public_decision_facts` (`server.rs:~417-423`) except for the card play.
* Public state already shows `state.action_card_plays: Vec<(PlayerId, ActionCardId)>` (`ti4-model/src/state.rs:1347`), `active_system`, `active`. `projection.rs:110,161` already special-cases `reaction_*HITS_TO_ASSIGN` subtypes.
* Grouping label: `server.rs:~268` classes reaction subtypes as "reactions" (`stage_for` of the decision stage).

Web
* `GameShell.tsx:249-270` routes workflow `action_card_reaction` to `ReactionStatusBar`. `choiceModel.ts:~584` classifies a decision as that workflow if the subtype starts with `play_reaction_`, or if it is optional, has at most 4 options and `source` has a `Reaction` key. Note the outer decision `reaction_when_X` normally qualifies via the second clause; but a reaction with more than 4 options falls to a generic list.
* `ReactionStatusBar.tsx` (300 lines): `DecisionHeader` with a hard-coded title "Respond to the action card" (also `decisionGalleryCases.ts:324`), the raw `choice.prompt` shown twice (header instruction and `reaction-bar-prompt`), a title "Reaction Opportunity", and `extractTriggerContext` (`:17-60`) which prints `context.actor` (this is the *viewer*, the seat being asked, not the trigger actor, so "Triggered by: <me>" is wrong), a title-cased subtype ("Play Reaction When Action Card Played"), and a target system only if the context target is a system (only in battles). Buttons render `Play {opt.label}`, producing "Play play Sabotage" (outer option label is already "Play X", inner is "play X"). Pass is "Pass (Spacebar)", Enter plays the first option, Pin stops the countdown. Spectators see "Waiting for <label>...".
* Card text: `contentCatalog.ts` `findActionCardMeta(id)`, `findActionCardByName(name)`, `getActionCardMeta` (`:91-118`) give name and effect text. `presentation/cardOptions.ts:38,53` already formats `meta.description`. `presentation/cardDatabase.ts` and `technologyData.ts` hold tech data. There is no `window` text in the catalog.
* Shared header from commit 247777e: `DecisionHeader` + `presentation/decisionSource.ts` `describeDecisionHeader` produce an eyebrow and chips (topic, source, phase/round). For `Reaction(...)` source it formats the raw event type; it has a topic table keyed by subtype that has no reaction entry.
* Auto-resolve toast: `AutoResolveToast.tsx`, driven by `option.auto_resolved` (single non-optional option auto-picked). Reactions are optional so never auto-resolved; the `Never` mode drops them engine-side with no feedback.
* Mode toggle (5a7f407): `PlayerSheet.tsx:16-17,204-241` keeps `reactionModes` in React state, per viewer, never sent. The engine field `reaction_card_modes` is therefore never set in a live game (grep finds no server/protocol write).
* Event log: `EventLog.tsx` renders `GameLogEntry` with `detail`/`private_detail`; builds a tree by round/phase.
* Mocks: `web/src/dev/decisionGalleryCases.ts:250` (one reaction case, `play_reaction_after_ACTION_CARD_PLAYED`, no trigger), `ReactionStatusBar.test.tsx` (one test), E-reaction-offer captures 1-3 (note the gaps already listed in its `manifest.json`).

Smoke harness
* `smokePlaythrough.ts:83` lists `reaction-status-bar` as a decision container to scan; `:99` excludes `pin-reaction`; `:104` classifies `play-reaction` and `pass`/`decline` as commit controls; `reaction-error-badge` is an error banner. Candidates are identified by test id and visible text, so test ids `reaction-status-bar`, `play-reaction-btn-<optionId>`, `pass-reaction-btn`, `pin-reaction-toggle`, `reaction-error-badge` are an implicit contract and must be preserved (see 10).

## 3. INVENTORY: every reaction window the engine can open

Columns: subtype is `reaction_<rel>_<EVENT>` (outer), plus `play_reaction_<rel>_<EVENT>` (inner, several different cards). "Trigger actor" is the seat that caused the event (payload `player` unless noted). "Data" lists what the event payload holds today (with "ADD" for what the plan adds at the emit site). "UI today" is the same for every row: `ReactionStatusBar.tsx` (`:17-60` extract, `:197-212` details, `:215-247` trigger context, `:268-278` buttons) showing the raw prompt `"<rel> <EVENT>"` or `"play an action card (<rel> <EVENT>)"`, a viewer-name "Triggered by", and `Play <label>`; none says who acted or what happened, except in battle where the context target is the system. The table therefore lists only what differs.

Emit sites: `emit_typed` in game.rs (G), `Resolving::emit` in combat.rs (C) / invasion.rs (I) / game.rs (G).

| # | Event, relation | Emit site | Trigger actor | Data today | Add | Who may react (window text via `window_table`, `reactions.rs:315-612`) |
|---|---|---|---|---|---|---|
| 1 | ACTION_CARD_PLAYED, when | `reactions::announce` `reactions.rs:707-716` (also component actions) | `player` | player, card | card name resolved in projection | Another player's non-Sabotage card (Sabotage, guard `another_players_card_is_not_sabotage`); Instinct Training (any card incl. Sabotage). Reacting to a reaction: yes (Sabotage on Sabotage is excluded by the guard; Instinct Training cancels it) |
| 2 | ACTION_CARD_DISCARDED, after | G `:1945`, announce_discard | player | player, card | none | Reverse Engineer-style "after an action card is discarded" |
| 3 | SYSTEM_ACTIVATED, after | G `:2269` (`:1871` free tactical) | `player` (active) | player, system | system name, others' units there (derive) | 7 printed windows: "after you activate a system [that contains your ships / another player's ships]", "after you activate an anomaly", "after another player activates a system that contains your units / structures / 1 of your command tokens"; L1Z1X agent |
| 4 | SHIP_MOVED, after | G `:2471` `note_arrival` | `player` | player only | system, ship list (from `MoveOutcome::Arrived`) | "after the active player moves ships into the active system during a tactical action"; "after a player moves ships into a system that contains your ships" |
| 5 | STRATEGIC_ACTION_BEGAN, when | G `:1808` | `player` | player | strategy card id (local `card` at `:1795`) | "when another player would perform a strategic action" (Coup d'Etat style) |
| 6 | STRATEGY_CARD_CHOSEN, after | strategy phase | `player` | player (+ card, verify) | card id | "when another player chooses a strategy card during the strategy phase" |
| 7 | STRATEGY_PHASE_BEGAN, after | G `:1210,4401` | none (phase) | none | none | "at the start of the strategy phase" |
| 8 | TURN_BEGAN, after | G `:1194` | `player` (whose turn) | player | none | "at the start of another player's turn, if they have a readied strategy card" |
| 9 | TURN_PASSED / PLAYER_PASSED, after | G `:4594`, `:1538` | `player` | player | none | Crisis ("end of any player's turn, if at least 2 not passed"), "end of a player's turn, if you have passed" |
| 10 | ACTION_COMPLETED, after | G `:4421` | `player` | player | action kind (tactical/strategic/component) | "after you perform an action" |
| 11 | STRATEGY_CARDS_WOULD_RETURN, when | G `:3512` | `player` | player | none | "when you would return your strategy card(s)..." |
| 12 | AGENDA_PHASE_BEGAN, after | G `:4381` | none | none | none | "at the start of the agenda phase" |
| 13 | AGENDA_REVEALED, when/after | G `:3724` | none (speaker flips) | agenda | agenda name/text (resolve in web) | 19 cards "when/after an agenda is revealed" |
| 14 | VOTES_CAST, after | G `:3891` | `player` (voter) | player, ... | outcome is secret/ballot: only "voted" publicly | "after you cast votes", "after the speaker votes" |
| 15 | AGENDA_RESOLVED, when | G `:4317` | none (outcome) | player = outcome, `elected_player`, agenda | agenda name, outcome label | 3 windows: elected you / another player elected / outcome would be resolved (Deadly Plot, private ballot guard) |
| 16 | TRANSACTION_OPENED / RESOLVED, when/after | G `:1750,2951,3056,3105` | `player` (proposer) | player, partner | none | "when a transaction..." windows |
| 17 | PLANET_CONTROL_GAINED, when/after | invasion.rs `:2345` | `player` (invader) | player, planet, system, previous_owner | none | "when you gain control of a planet", "after another player gains control of a planet you control" |
| 18 | INVASION_BEGAN, after | G `:358` | `player` | player, system | none | "at the start of an invasion", "...in a system that contains your opponents' PDS" |
| 19 | UNITS_COMMITTED, after | invasion.rs `:2815` | `player` (invader) | player, controller | system/planet, counts | ground-force commit windows |
| 20 | GROUND_ROLLS_MADE, after | invasion.rs `:1897` | none | player, ... | planet, hits | ground combat windows |
| 21 | SPACE_COMBAT_STARTED / COMBAT_ROUND_STARTED, after | combat.rs `:2860,2862` | attacker (`player`) | player (attacker), system | defender, round | "at the start of a combat [round]", "first round of a space combat" (guard `combatant`) |
| 22 | ANTI_FIGHTER_BARRAGE_STARTED, when | combat.rs `:2938` | `player` (side) | system, player, round | none | AFB-related cards |
| 23 | SPACE_CANNON_HITS, when | G `:250` | `gunner` | system, player = victim, gunner, hits | none | Space Cannon cancel cards |
| 24 | HITS_TO_ASSIGN, when | combat.rs `:3335` | opponent | system, player, hits, round | none | Shields Holding, Direct Hit etc. (special-cased in `projection.rs:105-110,161`) |
| 25 | SUSTAIN_DAMAGE_USED, when/after | combat.rs `:2187` | `player` | per emit | none | sustain-related |
| 26 | SHIP_DESTROYED, when/after | combat.rs `:2105` | none (owner is victim) | system, player, unit, last | none | Crash Landing ("your last ship...") |
| 27 | RETREAT_STEP_STARTED / RETREAT_DECLARED, after | combat.rs `:3399,2120` | `player` | per emit | destination system | retreat cards |
| 28 | SPACE_COMBAT_WON, after | G `:564` | `player` (winner) | per emit | system | "after you win a space combat" (Salvage) |
| 29 | PRODUCTION_USED, after | G `:447` | `player` | player, system | units produced | production reaction |
| 30 | UNIT_ABILITY_ROLLED, after | combat.rs `:488` | `player` | per emit | unit, hits | unit-ability cards |

Row notes: (a) the exact set is the `window_table()` entries, enumerated in section 3 of the engine (about 40 distinct event/relation pairs; roughly 150 mapped action cards); the table above groups the pairs by trigger family. Each implementer must run the existing `reactions::windows_without_an_event` and `unmapped_windows` test helpers (`reactions.rs:1131-1152`) to refresh it. (b) Cells marked "verify" were not read to the payload line and must be confirmed during Phase 1. (c) Instinct Training and the L1Z1X agent are the only non-card reaction abilities; any future leader/tech reaction should go through the same trigger builder.

Common property of all rows: the trigger exists as one `Event` with a type, an id and a payload, and the decision is created in one of exactly three places: `pick_with_context` (outer, `timing.rs:759`), the slot closure inner choice (`reactions.rs` slot), and the two ability closures. A single `Trigger::from_event` helper in the engine covers all of them.

## 4. Target design

### 4.1 Layout

The dialog has two blocks and the existing actions row.

1. "What happened" (the trigger): actor chip (colour dot, faction name, player name; "You" when the viewer is the actor), one plain-language sentence, the card name with its full printed text when a card was played, and the involved system/planet/units with a map link ("Show on map" pans/highlights the system and flashes it for 3 s) or a small inline mini-highlight (hex tile thumbnail built from `boardPresentation`).
2. "You can now" (the reactions): one row per reactable card or ability: name, window text in a muted label ("Window: When another player plays an action card other than Sabotage"), full effect text, and the button "Play <name>". Under the rows: Pass, and per row a small "Don't offer this card again" toggle (the per-card mode, see 4.7).

The header shows a short eyebrow "Reaction" (from `describeDecisionHeader`) and the title is derived from the trigger kind, not hard-coded: "Anna played an action card" / "Bob activated a system" / "Agenda revealed". The raw prompt is not shown (kept only as a `title` attribute/`aria-description` for debugging).

### 4.2 Wording templates (per trigger kind)

Template variables: `{actor}` = "{Faction} ({colour})" or "You"; `{card}` = card name; `{text}` = full printed effect text; `{window}` = printed window; `{system}` = "System {n} ({name})" with planets.

| Kind | What happened | You can now |
|---|---|---|
| action_card_played | "{actor} played the action card {card}: {text}." | "Now you can play {reactCard}." with `{reactCard}` text under it. User example 1 verbatim: "Player X played action card Y (full card text Z). Now you can play Sabotage." |
| system_activated | "{actor} activated {system}." (+ "It contains your N ships / structures / command token" when the guard was such a clause) | User example 2 verbatim: "Player X activated system Y; now you can do Z (your card's text)." Z = "play {reactCard}: {window}: {text}" |
| ship_moved | "{actor} moved ships into {system}: 2 Cruisers, 1 Dreadnought." | "Now you can play {reactCard}: ..." |
| strategic_action_began | "{actor} is about to use the {strategyCard} strategy card." | "Before it happens you can play {reactCard}: ..." |
| strategy_card_chosen | "{actor} chose the {strategyCard} strategy card." | same |
| turn_began / turn_passed / player_passed | "It is {actor}'s turn." / "{actor} passed." / "{actor} ended their turn." | same |
| agenda_revealed | "The agenda {agenda} was revealed: {text}." (no actor) | "Before/After voting you can play ..." (when/after from relation) |
| agenda_resolved | "The outcome {outcome} is about to be resolved for {agenda}." / "{elected} was elected." | same |
| votes_cast | "{actor} cast their votes." (never the outcome until revealed) | same |
| planet_control_gained | "{actor} gained control of {planet} in {system}." | "After this you can ..." |
| invasion_began / units_committed | "{actor} began an invasion of {system}." | same |
| combat_started / combat_round | "A space combat began in {system}: {attacker} vs {defender}. Round {n}." | same |
| space_cannon_hits / hits_to_assign / sustain / ship_destroyed | "{opponent} scored N hits on you in {system}." etc. (reuse `combatSummary.ts`) | same |
| transaction_opened | "{actor} opened a transaction with {partner}." | same |
| fallback | "{subtype-human-name}" (e.g. "A reaction window opened: after a ship was destroyed.") | "You can play {reactCard}: {window}." |

Relation wording: when -> "Before this resolves, you can ..."; after -> "Now you can ...". The "Play play" bug disappears because the label is never prefixed by the client: the client renders "Play {cardName}" from `payload.card_name` and the engine option label is ignored for known `action_card`/`ability` options (fallback to the label with a leading `play ` stripped).

### 4.3 ASCII wireframes

Desktop (centre/side dialog, about 520 px):

```
+--------------------------------------------------------------+
| REACTION  .  Action phase, round 3                       [-] |
| Anna played an action card                    (14 s)  [Pin]  |
+--------------------------------------------------------------+
| WHAT HAPPENED                                                |
| [red dot] Anna - The Federation of Sol                       |
| played the action card                                       |
|   MINING INITIATIVE  (Action)                                |
|   "Gain trade goods equal to the number of planets you ..."  |
+--------------------------------------------------------------+
| YOU CAN NOW                                                  |
| +----------------------------------------------------------+ |
| | SABOTAGE            window: when another player plays an | |
| |                     action card other than Sabotage      | |
| | "Cancel that action card."                               | |
| | [x] Don't offer Sabotage again        [ Play Sabotage ]  | |
| +----------------------------------------------------------+ |
|                                      [ Pass (Space) ]        |
+--------------------------------------------------------------+
```

System-activation variant of the first block:

```
| WHAT HAPPENED                                                |
| [blue dot] Bob - Arborec activated  System 27 (Lodor)  [map] |
|            [hex thumbnail: your 2 ships highlighted]         |
```

Phone (full-width bottom sheet, scrollable body, sticky action bar):

```
+--------------------------+
| Reaction - Action, R3  [-]|
| Anna played an action card|
+--------------------------+
| [red] Anna (Sol)         |
| played MINING INITIATIVE |
| "Gain trade goods ..."   |
|  (tap to expand text)    |
|--------------------------|
| YOU CAN NOW              |
| SABOTAGE                 |
| when another player ...  |
| "Cancel that action card"|
|  [x] never offer again   |
+==========================+
| [ Play Sabotage ]        |
| [ Pass ]        14 s     |
+--------------------------+
```

Several reactable cards: one "You can now" row per card, each with its own Play button; the sticky bar keeps only Pass (and the countdown).

### 4.4 Data model: the typed `trigger`

Engine (new, `crates/ti4-engine/src/decision_context.rs`): add to `DecisionContext`

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub trigger: Option<DecisionTrigger>,

pub struct DecisionTrigger {
    pub kind: TriggerKind,          // snake_case enum, see below
    pub event_type: String,         // raw engine event, for fallback and debugging
    pub event_id: u64,              // Event::id, ties the dialog to a log entry
    pub relation: String,           // "when" | "after"
    pub actor: Option<PlayerId>,    // who caused it; None for phase/agenda events
    pub subject: Option<PlayerId>,  // second seat: partner, victim, elected player, defender
    pub card: Option<String>,       // action card alias (public once played), or strategy card id
    pub agenda: Option<String>,
    pub system: Option<SystemId>,
    pub planet: Option<PlanetId>,
    pub units: Vec<TriggerUnits>,   // {owner, unit_type, count}, public board facts only
    pub hits: Option<u32>,
    pub chain: Vec<u64>,            // event ids of the enclosing emission stack (reaction to a reaction)
    pub window: Option<String>,     // printed window of the reacting card, if one
}
```

`TriggerKind` values: `action_card_played, action_card_discarded, system_activated, ship_moved, strategic_action_began, strategy_card_chosen, strategy_phase_began, turn_began, turn_passed, player_passed, action_completed, strategy_cards_would_return, agenda_phase_began, agenda_revealed, votes_cast, agenda_resolved, transaction, planet_control_gained, invasion_began, units_committed, ground_rolls, combat_started, anti_fighter_barrage, space_cannon_hits, hits_to_assign, sustain_damage, ship_destroyed, retreat, space_combat_won, production_used, unit_ability_rolled, other`. `other` plus `event_type` is the open-ended fallback, so a new window never breaks the UI.

Where built: one function `Trigger::from_event(event, state, content, resolver_stack)` in a new `crates/ti4-engine/src/trigger.rs` (or in `reactions.rs`), called at the three decision sites: `timing.rs` `pick_with_context` (`.contextualized(...)` just before `context.ask_seeing`, using `.with_trigger(..)`), `reactions.rs` slot closure inner choice, and the ability closures (`instinct_training`, `l1z1x_agent`). It maps `event_type` to `kind` through one `match` and reads payload keys. `units` and `system` for SHIP_MOVED need the emit site change (see below). `chain` comes from `Resolver::emission_stack` (already tracked in `timing.rs:491`, expose a getter).

Emit-site additions (small, behaviour neutral):
* SHIP_MOVED (`game.rs:2471`): add `system` (destination) and `units` (from `MoveOutcome::Arrived`).
* STRATEGIC_ACTION_BEGAN (`game.rs:1808`): add `card`.
* ACTION_COMPLETED: add `action` kind. STRATEGY_CARD_CHOSEN: add `card` if missing. COMBAT_ROUND_STARTED: add `defender`, `round`. AGENDA_REVEALED already carries `agenda`.
Adding payload keys changes `Event.payload`; check that nothing hashes the payload (sim baseline, `ti4-sim/src/behavior.rs`, replay fingerprint) before landing, or add the keys under a documented baseline bump.

Fingerprint safety: `DecisionContext::canonical()` must not include `trigger` (it is display metadata, deterministic from the event so replay is unaffected). Leave `CONTEXT_VERSION` alone because the field is additive and optional; if the owner prefers a version bump, readers "refuse unknown version" so that is a breaking change for old replays: do not bump. Add `trigger` to `DecisionContext::visibility()` as Public after redaction (4.5).

### 4.5 Redaction and projection

Rule: a trigger contains only public information. Everything it names has already happened in public:
* A played action card is public once played (the log already says so, `played_card_detail`), so `card` is included. A card being merely held or an unplayed alternative is not part of the trigger.
* Decisions that are hidden until revealed must not leak: VOTES_CAST must not carry the outcome or vote count, AGENDA_RESOLVED guard `voted_or_predicted_another_outcome` must not tell the viewer anything beyond "the outcome would be resolved" (it reads the viewer's own ballot, which is private to the viewer and is shown only in "You can now"), secret objectives and private promissory content never appear. Strategy card chosen is public.
* The viewer is always the context actor, so the "You can now" block is private to the viewer and may name the viewer's hand cards. For any other seat, `visible_to` drops `outstanding`; the same function keeps `trigger` (public) and drops the reacting-card data (`window`, reacting card ids in options are not in context anyway).
* Server work: none beyond serialization if the engine builds the trigger; add a projection-side assertion test. In `projection.rs` the pending choice is built for the actor; spectators and other seats get `PublicTurnStatus` (waiting for X) in `protocol/status.rs`; extend it with the same public trigger (kind, actor, card, system, event_id) (superseded: reaction windows must not name the waiting seat; every viewer but the asked seat gets `waiting_for_reactions`, so a trigger sentence here would have to stay seat-free) so the "Waiting for X" notice can say "Waiting for Bob to respond to Anna's Mining Initiative". Add `trigger` to the status struct as optional.
* Option enrichment (server or engine): add `payload.window` (printed window) and `payload.text`? Decision: do not send card text on the wire; the web catalog already has `text`. Instead add `window` text to the web content manifest (generator change at `web/scripts/generate-content-manifest.mjs`, key `window`). This keeps the protocol small and the text in one place (content), and the web can render any card id. A card id is already in `payload.card`.

### 4.6 Protocol and decoder, backward compatibility

* Rust: `trigger` is `Option` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, so old saved decisions and replays deserialize unchanged, and old clients ignore the unknown field (check that `DecisionContext` is not `deny_unknown_fields` anywhere in the client path; `GameEvent` and `SeatDecisionDetail` are, so do not add `trigger` there without a coordinated TS update).
* TS: add `DecisionTriggerDto` and `trigger?: DecisionTriggerDto | null` to `DecisionContextDto` in `web/src/protocol/types.ts:75`; extend `web/src/protocol/decode.ts` if it validates contexts (it currently does not touch `context`, grep found nothing), plus a `protocol/fixtures.test.ts` fixture with and without trigger.
* Client-side fallback ladder for reactions without trigger data: (1) `context.trigger`; (2) derive from the last public event log entry whose `event_id`/version precedes the decision (card play via `detail` regex "played X"; with `movement` for moves) and from public state (`state.action_card_plays.at(-1)`, `state.active`, `state.active_system`); (3) only the decoded subtype: "A reaction window opened: after a ship was destroyed" built from `event_type` by a small humanizer table; (4) the raw prompt. The UI never shows an empty "What happened" block; it degrades to the line from step 3. Tracks `source: "trigger" | "log" | "subtype"` in the model for tests.

### 4.7 Per-card mode toggle (finish it)

Because the toggle is not wired (section 2), plan a small protocol addition: client message `set_reaction_mode { card, mode }` handled by the server worker, written to `Player.reaction_card_modes` (it is part of `GameState`, so snapshots/replays carry it; confirm it is serialized and part of history so undo works), returned in the player view so `PlayerSheet` and the dialog read the same value. When a card is set to Never mid-dialog, the dialog submits Pass for it and sets the mode. Auto-declined windows (a `Never` card was the only candidate) produce a quiet toast "Skipped: you chose not to be offered Sabotage" through the existing `AutoResolveToast` channel, using a new engine event or a counter in public status (`reaction_skipped: { card, event_id }`) so the user can see why no dialog appeared and click "Offer again". Without server wiring, only a local React-state toggle exists, which does nothing in a live game; this is the largest hidden gap and is called out in the rollout.

### 4.8 Component structure (web)

* New `web/src/presentation/reactionModel.ts`: `describeReaction(choice, ctx: {state, events, catalog, viewerSeat, display}) -> { heading, trigger: {actor, sentence, card?, system?, units?, source}, reactions: [{optionId, name, window, text, kind}], passOptionId }`. Pure, testable, uses `findActionCardMeta`, `getActionCardMeta`, `findActionCardByName` and a `windowTextFor(cardId)` accessor from the catalog.
* `ReactionStatusBar.tsx` becomes thin: header, `<ReactionTrigger/>`, `<ReactionOptions/>`, actions row. The raw `extractTriggerContext` and the duplicated prompt are removed. Keep the test ids listed in section 10.
* `choiceModel.ts:584`: keep routing, but also route any decision with `context.trigger` and kind of reaction subtypes (`reaction_*`, `play_reaction_*`, `instinct_training_cancel`, `l1z1x_agent_swap`) to `action_card_reaction` regardless of option count; fix the "at most 4 options" fall-through.
* `decisionSource.ts`: add a reaction topic ("Reaction") and format `Reaction(EVENT)` with the humanizer; add a title function `reactionTitle(trigger)` used by `GameShell.tsx:324`-style title tables and `decisionGalleryCases.ts`.
* Map link: reuse `mapOverlays.ts`/`boardPresentation.ts` highlight helper used by invasion/planet selection to pulse `trigger.system`; clicking "Show on map" while the dialog is minimized should not block Pass.
* Event log: add the same trigger sentence to log entries by recording the reaction offer? No: do not log offers (private info about who holds what). Instead make the log entry for the triggering event the source of truth for the fallback ladder (4.6) and add `event_id`/`card` to `GameEvent` only if Phase 3 is accepted (see 6, open question 3).

## 5. Edge cases

* Several players react in turn: each gets their own decision (player order from `Resolver::player_order`); each sees the same "What happened" and their own "You can now". Waiting players see the public waiting notice with the trigger. After an earlier player plays a card, the later player's trigger is unchanged but the new card play is in `chain`; show "(after Bob responded with Sabotage)" using the `chain`/`action_card_plays` tail.
* Reacting to a reaction (Sabotage after a reaction-played card; Instinct Training on a Sabotage): trigger.kind = action_card_played with `chain` non-empty; wording "Bob played Sabotage in response to Anna's Mining Initiative." Use `chain` to resolve the parent via the log.
* Simultaneous windows (several slots on one event for the same player, e.g. a card plus the L1Z1X agent): the outer decision lists each ability; the sentence is shown once and rows are grouped by source (card / technology / leader). The inner `play_reaction_*` decision must repeat the same trigger and add a Pass option (currently has none, `reactions.rs` inner `Choice`); today declining is only possible at the outer step, so the inner dialog should show Back or Pass semantics (engine change: add `decline` and `optional(true)` to the inner choice; this alters decider behaviour and the sim baseline, so keep behind the versioned process or leave the inner step non-passable and say so in the UI).
* The trigger actor is the viewer (e.g. "after you activate a system"): "You activated System 27"; the actor chip reads "You". No confusion with the viewer in `triggerActor` as today.
* Spectators / non-actors: never receive the dialog. They get the public status line (4.5). Observers should see no hand data.
* Auto-declined windows: when `Never` filters out all cards nothing is asked (engine) and a toast is shown (4.7). When the single candidate is a mandatory (non-optional) ability, `auto_resolved` keeps the existing toast; extend the toast text with the trigger sentence, replacing `decisionType` raw text.
* Cancelled triggers: after Sabotage cancels the card, the "after" windows do not open (`event.cancelled`), so no stale dialog; make sure a dialog already on screen disappears when the nonce changes (existing reset effect).
* Countdown/auto-pass timer: keep behaviour; it is paused by Pin; the trigger block adds no timers.
* Long text: full effect texts up to roughly 300 characters; collapse after 4 lines on phones with "Show more".
* Missing catalog entry: `getActionCardMeta` returns `humanizeId(id)` and text "Action Card": show the name and omit the text rather than the placeholder.
* Event id: the engine event id is per-trace and restarts on replay/rebuild; use it only as an in-session key tying dialog and log, never as a durable id.
* Undo/history: a trigger is derived from the event, so it recomputes identically on replay.

## 6. Dependency on the event log

* Minimum viable path needs no log change: the trigger comes from the engine context.
* The fallback ladder (and spectators) read the log. The existing entries (`detail` "X played Card", `movement`) are enough for card plays and moves. Entries for activation, combat start, agenda reveal etc. depend on `public_decision_facts` (`server.rs:417-423` returns `None` for reaction subtypes by design) and the non-reaction decision records.
* Ordering guarantee to preserve: `worker.rs:~675-740` publishes the card play entry before the nested offer. A test must pin this (X played Y appears in the log before the reaction decision for it is delivered).
* Optional Phase 3: give `GameEvent` an optional structured `trigger`-like `fact` (kind, card, system) next to `detail`, so the log and the dialog share one source. Requires touching `deny_unknown_fields` on `GameEvent` (`server.rs:150`) and the TS `GameLogEntry`.

## 7. Tests

Engine (`crates/ti4-engine/src/reactions.rs` test module, `timing.rs`, `decision_context.rs`)
* Per trigger kind: a scripted game opens the window, a recording decider captures the `Choice`, asserts `context.trigger.kind`, `actor`, `card`/`system`/`units`, `relation`, `event_id`. One test per row group in section 3 (30 rows -> about 20 tests; a table-driven test over `window_table()` events that asserts every mapped event type produces a non-`other` kind, so a new window cannot silently fall back).
* `canonical()` unchanged with and without trigger (fingerprint test); sim baseline test still green (`ti4-sim` behaviour check), since payload additions are neutral.
* Inner `play_reaction_*` carries the same trigger as the outer.
* Never-mode drops cards (existing) and produces the skipped record when added.
* Redaction: `visible_to(other_seat)` keeps trigger, never includes `outstanding`; VOTES_CAST trigger has no outcome.

Server
* Projection test: pending choice JSON to the actor contains `context.trigger` (golden JSON for Sabotage and for system activation); other seats' status has only the public part; a seat that does not hold the card learns nothing about the reacting card.
* `session` test: card-play log entry precedes the nested reaction offer (ordering guarantee).
* Protocol roundtrip: JSON without `trigger` decodes; with it re-encodes identically; `set_reaction_mode` accepted only for the owner and legal cards.

Web
* `reactionModel.test.ts`: every `TriggerKind` template (verbatim examples 1 and 2 as snapshot strings), "You" actor, fallback ladder 1/2/3/4, missing catalog card, two cards, relation wording, `Play play` regression.
* `ReactionStatusBar.test.tsx`: renders both blocks, test ids retained, Enter/Space, Pin, spectator notice, error badge, toggle posts a mode message.
* `choiceModel.test.ts`: routing for outer, inner, Instinct Training, L1Z1X, more than 4 options.
* `contentCatalog.test.ts`: `window` present for every action card with a window; manifest staleness check (`generate-content-manifest.mjs:241`).
* `decisionSource.test.ts`: reaction topic, no raw `ACTION_CARD_PLAYED`.
* DecisionGallery (`web/src/dev/decisionGalleryCases.ts`): replace the single case with 8: Sabotage after card played, Instinct Training, system activated (with and without your ships), ship moved, agenda revealed (no actor), combat start, two cards at once (inner), fallback (no trigger). `DecisionGallery.test.tsx` iterates cases and must pass.
* Screenshot artifact E: update the three captures to supply a real-shaped `context.trigger` and remove the invented `description`; add captures 4 (system activated), 5 (phone viewport 390 px), 6 (fallback). Rerun `cd web && npm run screenshots -- E` (not in this planning session), update `manifest.json` notes (the three listed gaps become "fixed", keep a list of remaining ones).
* Smoke harness: add a unit test for `collectCandidates`/policy that the new markup still yields the commit candidates (`play-reaction-btn-*`, `pass-reaction-btn`) and does not yield the new "never offer again" checkbox as a clickable decision (add `reaction-mode` to the `EXCLUDED` regex at `smokePlaythrough.ts:99`, or give the toggle `role="switch"` and make `collectCandidates` ignore it as a pre-existing checkbox exclusion).

## 8. Smoke harness contract

The clicker finds controls by test id/label inside containers listed in `smokePlaythrough.ts:83`. Preserve: container `data-testid="reaction-status-bar"`; Play buttons `play-reaction-btn-<optionId>` with text starting "Play"; Pass `pass-reaction-btn` text "Pass"; `pin-reaction-toggle`; `reaction-error-badge`; one Play button per offered option, so the policy still has the same candidate set; no new required click before Play (the "never offer again" toggle is optional and excluded from the clicker); no modal-in-modal (the "Show on map" control must not be a commit-like label and should match `EXCLUDED` e.g. test id `reaction-show-on-map`, text avoiding "confirm|done|pass"). Expanding card text ("Show more") likewise excluded (`inspect`-style id). The steer policy (`smokePolicy.ts` weights) matches descriptions: keep the string "Play <card>" in the button description, and give the mode toggle a description that includes `inspect` or `pin-reaction` so weights do not boost it. Add a smoke assertion: no visible text matches `/\b[A-Z]{3,}_[A-Z_]{3,}\b/` (raw engine ids) or `/Play play/i` inside `reaction-status-bar`, so a regression fails the smoke run.

## 9. Phased rollout

| Phase | Content | Size | One agent session? |
|---|---|---|---|
| 0 | Quick wins, web only: fix "Play play" (strip duplicate verb, build button text from `payload.card_name`); replace raw prompt and title; humanize subtype; remove wrong "Triggered by: viewer"; fallback ladder levels 2-3 using the log/state (card play + active system); show the card effect text for each option from the catalog; update tests, gallery, E captures | S-M | yes |
| 1 | Engine: `DecisionTrigger`/`TriggerKind`, `from_event`, call at the 4 decision sites, emit-site payload additions (SHIP_MOVED, STRATEGIC_ACTION_BEGAN, others), engine tests per kind, fingerprint/baseline check | M-L | yes, but alone (engine builds are heavy, so schedule when memory is free) |
| 2 | Server/protocol/web decode: pass-through, TS types, `window` text in the content manifest, `reactionModel.ts`, new `ReactionStatusBar` blocks, templates, wireframes, map link, tests, gallery 8 cases, E re-capture, smoke contract test | M-L | one session after Phase 1 |
| 3 | Mode toggle wired end to end: `set_reaction_mode` message, owner checks, view/state sync, in-dialog toggle, skipped-window toast | M | one session (touches protocol, worker, PlayerSheet) |
| 4 | Public waiting status with trigger for other seats; optional structured log fact; inner `play_reaction_*` pass option (needs baseline process) | M | separate session; baseline-gated item may be deferred |

Phase 0 gives an immediate, safe improvement; Phase 1+2 deliver the user's examples fully; Phase 3 fixes the dead toggle.

## 10. Risks

* Payload additions or new context fields could move the sim behaviour baseline or replay fingerprint: keep `canonical()` untouched and verify before committing (Phase 1 gate).
* Card text longer than the viewport on phones: collapse/expand.
* The inventory contains rows marked "verify"; the exhaustive enumeration must be regenerated from `window_table()` when implementing.
* Another agent is editing `web/src/components/*` and `choiceModel.ts` concurrently; implement Phase 0 on a quiet checkout and rebase before touching `GameShell.tsx`.

## 11. Open questions for the user

1. Should the full printed text always show (can be 5-6 lines), or collapse by default after the first sentence on desktop too?
2. Do you want the actor shown as faction + colour + player nickname ("Anna, Sol, red"), or only faction/colour as elsewhere in the UI (`PlayerIdentity`)?
3. Is it acceptable to extend the public event log schema (`GameEvent`, `deny_unknown_fields`) with a structured fact, or should the log stay string-based and only the decision context gain the trigger?
4. For "when" windows (before the event resolves) should the dialog say what will happen if you pass (e.g. "Mining Initiative will resolve"), or only what you can do?
5. Should the inner multi-card decision get a real Pass/back option (engine behaviour change, sim baseline bump)?
6. Should "Never offer" be per card name (all four Sabotage copies) or per copy (today per alias id)? And should it persist across games (profile) or only per game?
7. Is revealing the trigger actor's faction colour in the spectator "waiting" notice desired, and may the notice also show the card name (it is public once played)?
8. Timer: keep the auto-pass countdown default as is (currently only if `autoPassTimeoutSeconds` is passed; GameShell passes none), or introduce one?
9. Which non-card reactions (leaders, technologies, promissory notes) should get the same treatment first beyond Instinct Training and the L1Z1X agent?

## 12. Decisions (2026-10-06)

- **Card text (open question 1): full text is the default.** A reaction dialog always opens with the
  full printed text of every card shown (what happened, and each card you can play). Each card
  block has a "Shrink" control for players who know the card; shrinking collapses that block to
  its first sentence with a "Show full text" control. The choice is remembered per card in
  localStorage (a player who shrinks Sabotage sees it compact next time), plus one "Compact card
  text" switch that applies to all cards. Default state for a new player is expanded everywhere.
  Reference: the side-by-side page with both versions (collapsed saves only about 60-100 px per
  dialog at phone width).
- **Defaults taken for the other open questions until the user says otherwise:** the actor is
  shown as faction and colour, as elsewhere in the UI (question 2); the public event log stays
  string-based and only the decision context gains the trigger (question 3); "Never offer" is per
  card name and per game (question 6).
