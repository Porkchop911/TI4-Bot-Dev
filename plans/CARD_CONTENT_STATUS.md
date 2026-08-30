# Card content — completion status (second implementer, Phase 7)

Status: **continuing the engine-completion plan** on `wp/engine-completion` (merged back to
`wp/r01-review-viewer-contract` too), updated 2026-08-30 after the handoff in
`plans/HANDOFF_ENGINE_COMPLETION.md` expanded this work past the two owned files.

Mission per `plans/PI_BRIEF_CARD_CONTENT.md`: action-card effects (baseline 34/142) and agenda
effects (baseline 51/63) in the two owned files only:

- `crates/ti4-engine/src/action_cards.rs`
- `crates/ti4-engine/src/agenda_effects.rs`

## Coverage achieved

| area          | before | after  |
|---------------|--------|--------|
| action cards  | 34/142 | 117/142 |
| agendas       | 51/63  | 63/63  |

`cargo run --release -p ti4-engine --example coverage_report` (final run): action cards 117 of
142 (82.4%), agendas 63 of 63, reaction windows 0 unsupported, plus the unchanged rows for
exploration/relics/objectives/leaders/abilities.

## Commits (oldest first)

- `ecdb26c` — agendas to 63/63: the 12 missing availability arms, each named after the standing
  rule it relies on (e.g. availability "1 or more laws" vs. standing "the law must be in play");
  registry extended, tests added.
- `09061fc` — agenda rider family: `predicted_outcome` encoding in `state.agenda_predictions`
  (`"outcome|card_alias"`; bare outcome = legacy imperial +1 VP, which keeps the vote-order
  key-based logic in `vote.rs:251` green); `resolve_predictions` rewritten to dispatch per-card
  payoffs; `assassinate_representative` (sentinel `"none|assassin"` denies the vote without a
  payout), `insider_information` (honest no-op: a hidden peek has no state effect),
  `ancient_burial_sites`, `diplomatic_pressure`.
- `30ba998` — tactical-action reaction cards: `rally`, `forward_supply_base`, `counterstroke`,
  `decoy_operation`, `emergency_repairs`, `upgrade_ship`, `experimental_battlestation`,
  `reveal_prototype` (4 resources; named `baseUpgrade` matches the subject's `base_type()`,
  unnamed II-line matches the normalized unit name).
- `9a6fe0b` — all 25 remaining `window=Action` component-action cards: `harness_energy`,
  `economic_initiative`, `industrial_initiative`, `fighter_conscription`, `impersonation`,
  `plagiarize`, `archaeological_expedition`, `divert_funding`, `exploration_probe`,
  `refit_troops`, `scuttle`, `seize_artifact`, `exchange_program` (+`refuse_exchange`),
  `mercenary_contract`, `pirate_fleet`, `pirate_contract`, `brilliance`, `overrule`,
  `strategize`. The old "unmodelled" spend test was split into a spend test plus an
  `announce()` test for genuinely unimplemented cards.
- `cc40c70` — the seven cards the hooks from `873178e` (on `wp/engine-completion`) unblock:
  `distinguished`, `bribery` (via `vote::add_votes`, scoped by `agenda_seq` and read in
  `vote::record`), `sh1`–`sh4` (via `combat::grant_hit_cancellation`, two cancellable hits per
  copy, stacked across the round), `intercept` (via `combat::bar_retreat` on the declarant,
  inferred as the other ship-bearing combatant of the active system). Documented gaps: a bonus
  whose vote is already banked when `VOTES_CAST` fires counts for nothing (zero-planet voter /
  abstainer), and the unguarded `VOTES_CAST` row also offers `bribery` after a non-speaker's
  vote (a `Guard` sees the event and the holder but not the seating, so "the voter is the
  speaker" is inexpressible in `reactions.rs`).

After the handoff, the engine-completion line (hook `873178e` + handoff `3f2d27b`) was fast-
forwarded into this branch and continued here; `cc40c70` was merged back into
`wp/engine-completion` (clean fast-forward) so both branches agree.

- Scoped roll modifiers + production window timing: `f_prototype` (marker `fighter_bonus_round`,
  consumed in `combat::effective_hits_on` and the anti-fighter barrage, fighter units only,
  2 per copy), `bunker` (marker `bunker_invasion`, +4 per copy to the planet controller's
  bombardment threshold for that invasion, `invasion::bombardment_at`), `war_machine1`–`4`
  (marker `war_machine_use`, +4 value / −1 cost = 5 faces, folded into
  `production::capacity`/`available` for the activation it was played in). The production window
  now opens **before** the step spends (`AftermathWindow::enter_production`, event
  `PRODUCTION_USED` at step entry with player+system payload, `After` relation), so a War Machine
  played there buys into the step it answers; the window refreshes the pending choice with the
  grown budget. `PRODUCTION_RESOLVED` still fires after the step. Model fields added on `Player`
  in `ti4-model` (`fighter_bonus_round`, `bunker_invasion`, `war_machine_use`), each `Vec<u32>`
  keyed by combat-round / activation seq so copies played in different rounds don't stack. Tests
  drive the engine's own paths (`roll_fleet`, `anti_fighter_barrage`, `bombardment_at`,
  `ProductionWindow` refresh, and a full `Game::step` driver for the reaction-to-window chain);
  each effect probed (break → test fails → revert). The WILD WILD Galaxy variant ("reduce the
  combined cost by 5") is not modelled; base text only.

Every batch: effect + dispatch + a test driving the engine's own path (via
`resolve_card[_loaded]` / `run`), rule text quoted in doc comments, and the gate probes
(exchange A-side infantry, pirate-fleet crew placement, scuttle goods payment — each break makes
the test fail, then reverted).

- Coexistence 7/7.1 bombardment target choice (engine plumbing, no new card): the invasion
  window now pauses on `Stage::ChoosingBombardment` and asks the table, per bombarding unit,
  whose units on a coexisting planet take that unit's hits (7, 7.1), capped per target with no
  spill (7.2). Roll-then-apply split (`roll_bombard_plan` consumes the dice; application is
  the pause-and-answer path in the window or the inline ask in the synchronous `bombardment`
  wrapper, which now takes `&mut Table`); `InvasionWindow::drive` settles before its first
  question and stops when a scoring occurrence is queued. This unblocks `fire_team` and
  `scramble` (next batch) and closed the `debug_assert!(false)` landmine `exchange_program`
  had made reachable; it also moved the ti4-sim behavioural baseline v5 → v6
  (`plans/evidence/M08-021.md`).

- Invasion-flow cards (the handoff's next group): `blitz` (at invasion start: each of the
  invader's non-fighter ships in the active system without BOMBARDMENT gains BOMBARDMENT 6 until
  the end of the invasion — `roll_bombard_plan` grants `(6, 1)` to such a ship; the Bunker −4
  penalty still applies to the blitzed roll), `disable` (at invasion start in a system holding
  ≥1 opponent PDS unit: those PDS units lose PLANETARY SHIELD and SPACE CANNON during this
  invasion — `bombardable` skips their shields and `space_cannon_offense` gates their cannons;
  the effect re-verifies the window text before marking), `parley` (after another player commits
  units to land on a planet you control: the committed units return to the space area — the
  Committing stage records `GameState.last_committed_unit` before the `UNITS_COMMITTED` emit and
  the effect hands the unit back to space before any combat), and `ghost_squad` (same window:
  move any number of your ground forces from any planet you control in the active system to any
  other planet you control — whole (planet, type) groups, re-asked with an explicit decline).
  The window rows already existed in `window_table()`; this group added the effects, the
  per-seat activation-scoped markers `Player.blitz_invasion` / `Player.disable_invasion` (the
  `bunker_invasion` / `war_machine_use` precedent — the marker lapses when the next tactical
  action begins, so no end-of-invasion cleanup), the `last_committed_unit` hand-off, and the
  `commit_on_your_planet` guard narrowing the shared UNITS_COMMITTED row to landings on a planet
  the card holder controls. Tests drive the engine's own paths (the `Game::step` driver for
  blitz/disable, the `InvasionWindow` committing-stage driver for parley/ghost squad, both with
  a cardless control arm), and each effect was probed (break → the new test fails → revert). The
  group moved the ti4-sim behavioural baseline v7 → v8 (`plans/evidence/M08-021.md`).

- Cancel API (the handoff's next group): `sabo1`–`4` (Sabotage). "When another player plays an
  action card other than 'Sabotage': cancel that action card." The machinery was already in
  place — `reactions::announce` emits `ACTION_CARD_PLAYED` through the resolver and skips the
  card's effect when the event comes back cancelled (the card is still spent: 1.15 cancels the
  event, not the spend), and the Sabotage window row already existed in the window table. This
  group added the two missing pieces: the cancellation itself (the reaction slot that owns the
  triggering `ACTION_CARD_PLAYED` event is the only code that still holds that event — a card
  effect's signature carries no event — so the slot cancels the event after a successfully
  played Sabotage; the effect-table entry exists so a played Sabotage reports as resolved
  rather than `ACTION_CARD_UNRESOLVED`), and the "other than 'Sabotage'" guard (the window row's
  guard reads the played card's alias off the event payload, so the four copies cancel other
  cards being played, not each other). Tests: `sabotage_cancels_the_card_being_played` (a
  `Game::step` driver — A plays Flank Speed in his activation's after window, B's Sabotage
  cancels the announcement, and the marker never lands; cardless control arm) and
  `sabotage_reacts_only_to_a_card_that_is_not_sabotage` (the guard at function level, via
  `playable_now`). All three halves probed (break the cancel / break the guard / remove the
  dispatch entry → the exact test fails → revert). The group moved the ti4-sim behavioural
  baseline v8 → v9 (`plans/evidence/M08-021.md`).

- Movement (the handoff's next group): `solar_flare` (all copies) and `lost_star` (Lost Star
  Chart). Both are played in the "After you activate a system" window of the owner's own tactical
  action and set an activation-scoped marker on the seat (the `blitz_invasion` / `war_machine_use`
  shape — the marker lapses when the next tactical action begins, so no cleanup). **Solar Flare**
  — "During the 'Movement' step of this tactical action, other players cannot use SPACE CANNON
  against your ships": the engine's cannon step is the one that belongs to the named action, so
  `combat::space_cannon_offense` reads the marker and suppresses the whole step (no roll, no hit,
  no `SPACE_CANNON_HITS`); every gun in that step belongs to another player and fires at the
  active player's ships, which is exactly what the card forbids. **Lost Star Chart** — "During
  this tactical action, systems that contain alpha and beta wormholes are adjacent to each
  other": a new switch `Galaxy.wormhole_star_links`, re-derived at the top of every `Game::step`
  by `laws::apply_to_galaxy` from the active player's marker (no movement path can consult a map
  that forgot the card), and `Galaxy::wormhole_partners` treats a both-wormhole system as linked
  to every other both-wormhole system while the switch is on. **On this map the effect is empty
  by the data**: 82b Mallice - Nexus is the only system carrying both an alpha and a beta
  wormhole, so a single such system has no partner; the rule is implemented as printed and pinned
  by the galaxy's own test, and the historical oracle never implemented the card either. Tests:
  `solar_flare_keeps_the_opponents_space_cannon_dark_for_the_action` (a `Game::step` driver —
  A's cruiser and B's PDS in the activated system: the control arm rolls the gun and announces
  `SPACE_CANNON_HITS`, the card arm does neither and the cruiser is still in the system when the
  action ends), `lost_star_points_the_map_at_the_chart_for_the_players_action` (the game's map
  points at the chart during the owner's action and not otherwise; the card is a resolved
  card, not `ACTION_CARD_UNRESOLVED`), `the_star_chart_reaches_the_map_through_the_active_
  players_marker` (the laws wiring: on for the active player's matching activation, off for a
  different `activation_seq`, off when another player is active), and
  `the_star_chart_rule_links_the_both_wormhole_systems` (galaxy level). Four probes (cannon
  suppression removed / the laws' flag derivation pinned off / both effect markers removed / the
  dispatch entries deleted → the exact test fails → revert); the galaxy's both-link branch is
  behaviorally indistinguishable from ordinary same-letter matching, so its pin guards against
  an over-broad implementation rather than detecting its absence. The group moved the ti4-sim
  behavioural baseline v9 → v10 (`plans/evidence/M08-021.md`) — the smallest shift of any
  re-baseline so far: the point estimates do not move and the bootstrap bounds move only in
  their last digits (the chart is inert on the base map and the flare bites only in a corner the
  bots rarely reach), with the protocol-integrity check, not the value gate, forcing the move.

- Agenda (the handoff's "agenda / turn flow" group, first sub-batch): `veto`/`veto3`/`veto4`
  (Veto) and `confusing`/`confounding` (Confusing / Confounding Legal Text). All five reuse the
  existing `AGENDA_REVEALED` / `AGENDA_RESOLVED` window rows — no new events. **Veto** — "Discard
  that agenda and reveal 1 agenda from the top of the deck; players vote on this agenda
  instead": the effect (played into the `AGENDA_REVEALED` window) draws the replacement from the
  top of the agenda deck and hands it to the driver via `GameState.agenda_veto_replacement`;
  `Game::reveal_agenda` (a new helper called from `open_next_vote`) discards the vetoed agenda and
  follows the replacement chain to the agenda that actually goes to a vote — a Veto on a Veto is
  legal and the chain is bounded by the finite deck. **Confusing** — "When you are elected as the
  outcome of an agenda: choose 1 player; that player is the elected player instead": the elected
  seat redirects the election to a chosen seat (with two players the single other seat is taken
  outright). **Confounding** — "When another player is elected: you are the elected player
  instead": the holder takes the election for itself. Both record `GameState.agenda_elected
  _override`, which `close_vote` reads after the `AGENDA_RESOLVED` window: the vote's own result
  still settles predictions and any law, but the agenda's elected-player effect and the "elected
  by an agenda" feat follow the redirect (`AGENDA_OUTCOME_REDIRECTED`). The `AGENDA_RESOLVED`
  payload gains an additive `elected_player` field, set only for a real seat, so the Confounding
  window can tell "a player was elected" from a law, a planet, or a For/Against outcome (a plain
  "outcome is not me" guard would fire on those). Tests: `veto_reveals_the_next_agenda_instead_of
  _the_vetoed_one` (driven over all three copies — the vetoed agenda is discarded, the
  replacement from the deck is voted on, the vetoed agenda is never voted, the election's outcome
  is untouched, and the card is spent), `confusing_redirects_the_election_to_a_chosen_seat`,
  `confounding_makes_the_holder_the_elected_player` (the ballots elect a; the card redirects the
  election to b, the seat recorded as elected), and `confounding_is_silent_on_an_agenda_that
  _elects_no_player` (an Elect-Planet agenda names a planet, not a seat, so the window stays
  silent and the card stays in hand). Five probes confirmed each. The group is behaviorally inert
  for the recorded ti4-sim suite — the v10 bounds still reproduce exactly — so it needed no
  re-baseline.

- Vote order (the handoff's next group): `hack` (Hack Election) — "After an agenda is
  revealed: During this agenda, you vote last." The marker `Player.hack_votes_last_agenda` (an
  `Option<u32>` on the seat in `ti4-model`, `#[serde(default)]`, in the manual `PartialEq`,
  `None` on a fresh seat) records the `agenda_seq` the card was played into: `reveal_agenda`
  bumps `agenda_seq` before its window opens, so the marker binds to the vote that reveal
  produces — including a Veto replacement voted on in the same cycle — and expires at the next
  reveal with no cleanup, the `extra_votes_agenda` precedent. `VoteWindow::new` (vote.rs) reads
  it: the order is the non-speaker seats in clockwise order, the speaker last if still voting,
  then the hackers (several keep their relative clockwise order) at the very end. The same
  rewrite fixed a latent ordering bug: the old code rotated the seated list left by one, popped
  the old first seat and re-pushed the speaker — re-seating a speaker who had been barred from
  voting (the Imperial Rider's prediction cost), which re-admitted the barred seat and dropped
  the player on its left. A barred speaker is now simply gone from the order. Tests: four
  `VoteWindow::new` unit tests (the holder last behind the speaker, several hackers keep their
  clockwise order, the marker expires with the agenda, the barred speaker is gone) plus two
  full-game drivers in `game.rs` (a three-seat agenda phase with a `RecordingDecider` that
  logs the (player, prompt) sequence: with the card b is asked in the reveal window and the
  outcome questions go to c, then the speaker a, then b; the cardless control goes b, c, a —
  same two-to-one tally, different order). Both halves probed: the hack partition disabled →
  both unit tests and the full-game test fail on the exact order; the old speaker re-seating
  restored → the barred-speaker test fails. The group moved no behavioural bound: the release
  re-baseline run reproduced all ten v11 values to the last digit, 0 metrics outside, so no
  v12 was needed (`plans/evidence/M08-021.md`).

## Partial implementations (exact gaps)

- **Choice-dependent agenda riders** (const/diplo/war family): the payoff auto-fires only when
  the chosen option is unique; multiple options → skip + comment. The tech rider stays partial.
- **Sanction**: the vote-denial half works (sentinel); the token-return half needs the ballot,
  which `resolve_predictions(state, outcome)` does not carry.
- **Mercenary contract**: the planet-card half is unmodelled (the engine does not track planet
  cards in hand).
- **Divert funding**: the deck half is unmodelled (no `technology_deck` field; a returned
  technology leaves the seat and is not restored).
- **Brilliance**: only the breakthrough-gain half is offered (the corpus has no
  technology-specialty planet marker for the ready-planet half).
- **Overrule / Strategize**: a `FreeTactical` outcome records `state.active` +
  `state.active_system`; the move and its windows belong to the driver.

## The 25 unimplemented action cards, grouped by blocking root cause

Each window below is mapped to an engine event (Phase 8: 0 unsupported windows); the block is the
state or flow the effect needs, which lives in files outside the ownership scope. The invasion
flow group (`blitz`, `disable`, `parley`, `ghost_squad`), the Cancel API group (`sabo1`–`4`)
and the Movement group (`lost_star`, `solar_flare`), the Agenda group (`veto`/`veto3`/`veto4`,
`confusing`, `confounding`), the Turn-flow group (`deadly_plot`, `coup`, `crisis`,
`master_plan`) and the Vote-order group (`bribery`/`distinguished` via `vote::add_votes` in
`cc40c70`, `hack` in the batch above) closed with the batches above; only `rout`, `dh1-4` and
the live-dice half of `intercept` remain in the combat-dice group.

- **Combat dice / hit-assignment / retreat flow** (state local to `combat.rs`; the model only
  keeps per-round bookkeeping, not live dice or retreats): `rout`, `dh1-4`,
  `intercept` (now played via `combat::grant_hit_cancellation` / `combat::bar_retreat`
  bookkeeping where the rule allows; the live-dice half stays unmodelled).
  `fire_team` and `scramble` left this group with the reroll group below.
- **Vote weighting / ballot** (`vote.rs`): done — `bribery`/`distinguished` (the `cc40c70`
  batch, via `vote::add_votes` scoped by `agenda_seq`) and `hack` (the vote-order batch above).
  The only documented gap left: a bonus whose vote is already banked when `VOTES_CAST` fires
  counts for nothing (zero-planet voter / abstainer), and the unguarded `VOTES_CAST` row also
  offers `bribery` after a non-speaker's vote (a `Guard` sees the event and the holder but not
  the seating, so "the voter is the speaker" is inexpressible in `reactions.rs`).
- **Agenda outcome redirection / agenda queue** (`game.rs`): done — `deadly_plot` (the
  `AGENDA_RESOLVED` window + the discard path in `close_vote`; see the Turn-flow group below).
- **Turn / phase driver hooks** (`game.rs`): done — `coup`, `crisis`, `master_plan` (the
  Turn-flow batch: `STRATEGIC_ACTION_BEGAN` / `TURN_PASSED` / `ACTION_COMPLETED` typed events
  and the `advance_turn` retention, skip and cancellation paths; `TransientFlags` in
  `state.rs`).
- **Production hook**: done — `war_machine1-4` (see the scoped roll modifiers batch above).
- **Event payload only** (the fact the effect needs is not in `GameState`): `lieinwait`
  (no transaction-history field to know two neighbours transacted).
- **Unmodelled attachment** (no per-card trade-good slot on `Player::action_cards`):
  `investments` (the 5-TG gain half is modelable; the "place on cards" half is not).

### Writable in a follow-up batch (state is on `GameState`, event exists)

`infiltrate` (planet-gain + unit placement), `crashlanding` (last-ship-destroyed + command
token), `courageous` (ship destroyed + reinforcement move), `stability` (status-phase card
return + command token), `summit` (strategy-phase start + 1 TG), `blackmarketdealing`
(transaction + sell a ship for 1 TG), `reparations` (planet taken + 1 TG),
`reverse_engineer` (component-card discard + research), `salvage` (space combat won +
reinforcement move), `mjets1-4` (space-cannon window + ship move), `waylay` (a roll-site
modifier on the anti-fighter barrage: hits from the roll are produced against all ships, not
just fighters — the `f_prototype` shape, at the AFB roll), `disgrace` (strategy-card choice +
leader return, if leader state is on `GameState`), `puppetsonastring` (turn end +
strategy-card discard), `extremeduress` (turn start + TG), `reflective` (produce 2 hits
against the opponent's ships in the active system, when one of the holder's ships uses SUSTAIN
DAMAGE — the `SUSTAIN_DAMAGE_USED` event and its When/After window rows already exist, so the
effect has a live binding point). These are reaction cards: each needs a full-game scripted
scenario in which the window actually fires, so this is a separate session, not a continuation
of the component-action batches.

## FINDING — `ti4-policy` test ledger was wrong; **resolved in `873178e`**

`ti4-policy`'s `scored_games_stay_legal_and_deterministic_across_nested_windows` failed on
`9a6fe0b` (green on `647c404`): `bot p2 was offered the secret sb it does not own
(ledger: ["faa", "pe", "syc"])`, seed 7777, rotation 0.

Diagnosis (engine verified correct):

1. The offer site (`objectives.rs::pending_choice` → `next_askable`) builds options exclusively
   from the seat's own `secret_objectives` via `secrets::scoreable_on` / `scoreable_event`.
   The engine cannot offer a player a secret it does not hold.
2. `secrets::enforce_hand_limit` (rule 45.4) returns an unscored secret **to the deck** when a
   player holds more than 3 (4 with the Obsidian). A returned secret can later be drawn by any
   player (`secrets::draw`), and the deck is also fed by the Archived Secret agenda.
3. In the failing campaign, `sb` was legally in p2's hand when the scoring window offered it;
   p2 later went over the hand limit (a 4th secret dealt by the Archived Secret agenda — an
   effect that only became reachable in the campaign because the new card effects shifted the
   trajectory) and `sb` was returned to the deck. At game end nobody holds or scored it, so the
   test's ledger — final hand ∪ `scored_by` — no longer contains it, though the offer was legal.
4. The test's premise comment, "A secret never changes hands except by scoring (61.18), so this
   is exact", is false under 45.4 + Archived Secret + any card that draws secrets (including
   `impersonation`). Disabling `impersonation`'s draw does not make the test pass — the
   trajectory shift comes from the new cards as a whole.

Suggested fix (implemented by the engine implementer in `873178e`): extend the per-seat ledger
with the secrets that seat **returned to the deck**, read from the
`"return a secret objective to the deck"` records. Final hand ∪ scored ∪ returned is exactly
"ever held", and the hidden-info net keeps its meaning. Verified: the campaign test passes
again, and the engine offer paths were re-checked to offer only the seat's own hand.

## Verification state at this checkpoint

- `cargo test -p ti4-engine --lib`: 1021 passed, 0 failed (plus the 5 doc/other targets); the
  engine-line crates all green.
- `cargo test --workspace` (LIBTORCH at `out/libtorch-2.9.1-cpu`): every engine-line crate
  green (ti4-model, ti4-content, ti4-engine, ti4-policy, ti4-sim's 52 non-fixture tests);
  **ti4-sim's behavioral suite stays green at v11** — the vote-order package (Hack Election +
  the barred-speaker fix) reproduced all ten v11 values to the last digit in the release
  re-baseline run, `0 metric(s) outside the recorded bounds`, so no v12 re-baseline was needed
  (`plans/evidence/M08-021.md`); the last move was v11 for the turn-flow cards. The one
  remaining red is `fixture_capture_is_deterministic`: pre-
  existing (same failure mode on the pre-change tree — seed `919_601`'s replay now finishes at
  step 781 before any production-head menu of ≥3 options); it belongs to the M09-019b profile
  module's own versioned process and is tracked, not new.
- `cargo clippy -p ti4-model -p ti4-content -p ti4-engine --all-targets`: zero warnings in the
  touched files (lib and tests); the remaining workspace warnings are pre-existing in untouched
  files (`production.rs` method chain, `strategy.rs` / `fracture.rs` casts, the
  `coverage_report` example's length).
- The vote-order package was probed break → the exact new test fails → revert: the hack
  partition in `VoteWindow::new` disabled (both the unit order tests and the full-game driver
  fail on the exact (player, prompt) sequence) and the old speaker re-seating restored (the
  barred-speaker test fails). The Agenda group's five probes, the Movement group's four
  probes, the Sabotage group's three and the Turn-flow group's eleven were recorded at their
  own checkpoints. The galaxy's both-link branch has no detecting probe — it is
  indistinguishable from same-letter matching by the map's data (documented above).
- The 25 remaining unimplemented action cards, grouped by the handoff's blockers plus the cards
  it does not group (`fire_team`, `scramble` and the Jol-Nar commander reroll closed the reroll
  group; the invasion-flow, Cancel API, Movement, Agenda, Turn-flow and Vote-order groups closed
  with the batches above):
  - **Agenda and turn flow** (0 remaining of 9): the five agenda cards
    (`veto`/`veto3`/`veto4`, `confusing`, `confounding`) closed with the Agenda batch, and the
    four turn cards (`deadly_plot`, `coup`, `crisis`, `master_plan`) closed with the Turn-flow
    batch (new window rows, the three new typed turn events, the `TransientFlags` bitfield and
    the `advance_turn` retention/skip/cancellation paths).
  - **Vote order** (0 remaining of 3): `bribery`/`distinguished` closed with the `cc40c70`
    batch (via `vote::add_votes`) and `hack` closed with the vote-order batch (the
    `hack_votes_last_agenda` marker read by `VoteWindow::new`, plus the fix for a barred
    speaker being re-seated at the end of the order).
  - **Not grouped by the handoff** (25, most blocked on movement/state the handoff's groups do
    not cover): `blackmarketdealing`, `courageous`, `crashlanding`, `dh1`–`4`, `disgrace`,
    `extremeduress`, `infiltrate`, `investments`, `lieinwait`, `mjets1`–`4`,
    `puppetsonastring`, `reflective`, `reparations`, `reverse_engineer`, `rout`, `salvage`,
    `stability`, `summit`, `waylay`.