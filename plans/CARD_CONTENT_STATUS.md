# Card content — completion status (second implementer, Phase 7)

Status: **continuing the engine-completion plan** on `wp/engine-completion` (merged back to
`wp/r01-review-viewer-contract` too), updated 2026-08-29 after the handoff in
`plans/HANDOFF_ENGINE_COMPLETION.md` expanded this work past the two owned files.

Mission per `plans/PI_BRIEF_CARD_CONTENT.md`: action-card effects (baseline 34/142) and agenda
effects (baseline 51/63) in the two owned files only:

- `crates/ti4-engine/src/action_cards.rs`
- `crates/ti4-engine/src/agenda_effects.rs`

## Coverage achieved

| area          | before | after  |
|---------------|--------|--------|
| action cards  | 34/142 | 95/142 |
| agendas       | 51/63  | 63/63  |

`cargo run --release -p ti4-engine --example coverage_report` (final run): action cards 95 of 142
(66.9%), agendas 63 of 63, reaction windows 0 unsupported, plus the unchanged rows for
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

## The 60 unimplemented action cards, grouped by blocking root cause

Each window below is mapped to an engine event (Phase 8: 0 unsupported windows); the block is the
state or flow the effect needs, which lives in files outside the ownership scope.

- **Combat dice / hit-assignment / retreat flow** (state local to `combat.rs`; the model only
  keeps per-round bookkeeping, not live dice or retreats): `rout`, `scramble`, `dh1-4`,
  `fire_team`, `intercept` (now played via `combat::grant_hit_cancellation` / `combat::bar_retreat`
  bookkeeping where the rule allows; the live-dice half stays unmodelled).
- **Invasion flow** (`invasion.rs`): `blitz`, `disable`, `ghost_squad`, `parley` (bunker landed
  with the scoped roll modifiers above).
- **Movement rules / system activation** (`movement.rs`): `lost_star`, `solar_flare`.
- **Vote weighting / ballot** (`vote.rs`): `bribery`, `distinguished`, `hack`.
- **Agenda outcome redirection / agenda queue** (`game.rs`): `confounding`, `confusing`,
  `deadly_plot`, `veto`, `veto3`, `veto4`.
- **Turn / phase driver hooks** (`game.rs`): `coup`, `crisis`, `master_plan`.
- **Production hook**: done — `war_machine1-4` (see the scoped roll modifiers batch above).
- **Cancel an effect** (no cancel API exists): `sabo1-4`.
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
reinforcement move), `mjets1-4` (space-cannon window + ship move), `waylay` (anti-fighter
window + ship move), `disgrace` (strategy-card choice + leader return, if leader state is on
`GameState`), `puppetsonastring` (turn end + strategy-card discard), `extremeduress` (turn
start + TG). These are reaction cards: each needs a full-game scripted scenario in which the
window actually fires, so this is a separate session, not a continuation of the component-action
batches.

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

- `cargo test -p ti4-engine --lib`: 990 passed, 0 failed.
- `cargo test --workspace` (LIBTORCH at `out/libtorch-2.9.1-cpu`): every engine-line crate green
  (ti4-model, ti4-content, ti4-engine, ti4-policy, ti4-sim's 50 non-suite tests); **ti4-sim's
  two suite tests are red on the pre-change baseline `c18c276` too** and are tracked, not new:
  - `the_suite_reproduces_and_stays_within_the_recorded_bounds` panics on the engine's
    coexistence-7/7.1 gap: `bombardment_at` cannot ask *whose* units on a coexisting planet take
    the hits (the debug announcement at `invasion.rs`), reached now that `exchange_program` makes
    coexisting planets real; the recorded bounds (29 vs 30) shift with it.
  - `fixture_capture_is_deterministic`: the recorded capture predates the trajectory shifts and
    replays to a different terminal step; needs re-recording through its versioned process after
    the engine gap is closed.
  Both are the handoff's remaining group: thread a decision interface into
  `bombardment_at`/`roll_ground` (coexistence 7, 7.1, fire_team, scramble), then re-record the
  sim baseline.
- `cargo clippy -p ti4-engine --all-targets`: zero warnings in the touched files.
- Every effect of the scoped roll modifiers batch probed (break the consumption site → its test
  fails → revert): fleet roll threshold, AFB guard, bombardment penalty, production budget.
- The 47 remaining unimplemented action cards, grouped by the handoff's blockers plus the cards
  it does not group:
  - **Reroll at the roll site** (2 cards + Jol-Nar's commander + coexistence 7/7.1): `fire_team`,
    `scramble` — `bombardment_at`/`roll_ground` need `&mut Table` threaded in (`combat.rs`/`invasion.rs`).
  - **Invasion flow** (4): `blitz`, `disable`, `parley`, `ghost_squad` — need commit
    undo/modify (`invasion.rs`).
  - **Cancel API** (4): `sabo1`–`4` — a card effect that *sets* the `cancelled` flag.
  - **Movement** (2): `lost_star`, `solar_flare` — `wormholes_all_linked`-style galaxy scoping
    (`movement.rs`/`laws`).
  - **Agenda and turn flow** (9): `veto`, `veto3`, `veto4`, `confusing`, `confounding`,
    `deadly_plot`, `coup`, `crisis`, `master_plan` — agenda redirection + queue (`game.rs`).
  - **Vote order** (1): `hack` — re-orderable vote sequence (`vote.rs`).
  - **Not grouped by the handoff** (25, most blocked on movement/state the handoff's groups do
    not cover): `blackmarketdealing`, `courageous`, `crashlanding`, `dh1`–`4`, `disgrace`,
    `extremeduress`, `infiltrate`, `investments`, `lieinwait`, `mjets1`–`4`,
    `puppetsonastring`, `reflective`, `reparations`, `reverse_engineer`, `rout`, `salvage`,
    `stability`, `summit`, `waylay`.