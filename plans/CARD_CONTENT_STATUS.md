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
| action cards  | 34/142 | 89/142 |
| agendas       | 51/63  | 63/63  |

`cargo run --release -p ti4-engine --example coverage_report` (final run): action cards 89 of 142
(62.7%), agendas 63 of 63, reaction windows 0 unsupported, plus the unchanged rows for
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
  keeps per-round bookkeeping, not live dice or retreats): `f_prototype`, `rout`, `scramble`,
  `sh1-4`, `dh1-4`, `fire_team`, `intercept`.
- **Invasion flow** (`invasion.rs`): `blitz`, `bunker`, `disable`, `ghost_squad`, `parley`.
- **Movement rules / system activation** (`movement.rs`): `lost_star`, `solar_flare`.
- **Vote weighting / ballot** (`vote.rs`): `bribery`, `distinguished`, `hack`.
- **Agenda outcome redirection / agenda queue** (`game.rs`): `confounding`, `confusing`,
  `deadly_plot`, `veto`, `veto3`, `veto4`.
- **Turn / phase driver hooks** (`game.rs`): `coup`, `crisis`, `master_plan`.
- **Production hook**: `war_machine1-4`.
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

- `cargo test -p ti4-engine --lib`: 986 passed, 0 failed.
- `cargo test --release --workspace` (LIBTORCH at `out/libtorch-2.9.1-cpu`): **all crates
  green**, including the ti4-policy campaign test (ledger fixed in `873178e`) and ti4-sim's
  `the_suite_reproduces_and_stays_within_the_recorded_bounds` — the v5 rebaseline in
  `plans/evidence/M08-021.md` (owner-approved 2026-08-29) covers the behavioural change, so the
  handoff's "baseline is red" warning is resolved.
- `cargo clippy -p ti4-engine --all-targets`: zero warnings in `action_cards.rs` and
  `agenda_effects.rs`.
- Every gate of the seven hook cards probed (break the effect → test fails → revert).
- The 53 remaining unimplemented action cards, grouped by the handoff's blockers plus the cards
  it does not group:
  - **Scoped roll modifiers** (6): `f_prototype`, `bunker`, `war_machine1`–`4` — need a
    `(scope_seq, filter, delta)` field on `Player` (`ti4-model`).
  - **Reroll at the roll site** (2 cards + Jol-Nar's commander + coexistence 7/7.1): `fire_team`,
    `scramble` — `bombardment_at`/`roll_ground` need `&mut Table` threaded in (`combat.rs`).
  - **Invasion flow** (5): `blitz`, `disable`, `parley`, `ghost_squad`, `bunker` — need commit
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