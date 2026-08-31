# Handover to PI: finishing the engine

Written 2026-08-31. Supersedes `plans/HANDOFF_ENGINE_COMPLETION.md`, whose numbers are stale.
Companions: `plans/ENGINE_COMPLETION_PLAN.md`, `engine-rules-audit.md`.

Branch `wp/engine-completion`, worktree `D:/Projects/ti4-engine-work`, head `4422407`.
**It has not been merged back into `wp/r01-review-viewer-contract`.** Do that first or you will
redo the card content.

## Start here

```
cargo run -p ti4-engine --example remaining         # every gap, by name, from the code
cargo run -p ti4-engine --example coverage_report   # the counts
```

Both read the modules' own `unimplemented` helpers, so neither can drift the way this document
can. If they disagree with anything below, they are right.

## What is left

| area | left | items |
|---|---|---|
| relics | 6 of 24 | `titanprototype` `neuraloop` `dominusorb` `emphidia` `thalnos` `heartofixth` |
| laws | 13 | listed below |
| exploration | 7 of 80 | `frln1` `frln2` `frln3` `ed1` `ed2` `ion` `mirage` |
| leaders | 2 | `xxchahero` `jolnaragent` |
| breakthroughs | 1 of 6 | `jolnarbt` |
| action cards | **0** | done |
| agendas / objectives / secrets / faction abilities / reaction windows | **0** | done |

Everything else in the six-faction scope is implemented.

## The order I would do it in

### 1. The dice cluster — three items, one piece of shared work

`thalnos`, `crown_of_thalnos` and `heartofixth` all modify dice that have been rolled and not yet
applied. **The infrastructure already exists and was built for exactly this.**

- `GameState::reroll_staging` holds a `RerollSet` whose faces are still mutable.
- `combat::open_reroll_windows` (combat.rs:283) opens the window; `combat::staged_hits` recomputes
  hits from whatever faces survive it.
- `dice::Roll::rerolled` records *which* positions were replaced. That field exists specifically
  because Crown of Thalnos says "destroy each of their units that did not produce a hit **with its
  reroll**", which is unanswerable from the faces alone. Its doc comment says so.

So Thalnos and Crown of Thalnos are: bind into the existing window, reroll the chosen dice, then
destroy units whose *rerolled* dice missed. Thalnos also applies +1 to the results.

`heartofixth` is the one with real scope in it. "After **any** die is rolled" — and only space
combat and space cannon stage their rolls today. Bombardment, anti-fighter barrage and the gravity
rift roll and apply immediately. Either extend staging to those sites, or implement the card for
the staged sites and record the limitation in `engine-rules-audit.md`. **Do not** implement it
silently for two sites and let the ledger imply four.

### 2. The payment-decider cluster — three items, one shape

`jolnarbt`, `jolnaragent` and `xxchahero` all interrupt a payment to ask a question.

- `jolnarbt`: exhaust a technology-specialty planet instead of spending resources; you must then
  research a technology of that colour.
- `jolnaragent`: remove any number of infantry, each reducing the resources spent by 1.
- `xxchahero`: when exhausting planets, combine resources and influence and treat the total as both.

All three land on `production::payment_faces`, which is the single place a planet's spendable value
is computed and is already substituted through `planet_value_now`. Change it once and every
spending path sees it. `production::pay` already takes a `Table`, so the decider is in reach.

Note the overlap with `plans/BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT.md` — read it before
touching `payment_faces`; you may fix both at once.

`grant_chosen_technology` in relics.rs is the "ask which technology, gain it, no prerequisites"
helper. `jolnarbt` wants the colour-filtered form, which its `colour: Option<&str>` parameter
already provides.

### 3. Exploration — five of the seven are already written elsewhere

- `ed1` and `ed2` are **Enigmatic Device**, the same card as the relic implemented in `70ca02b`.
  Same text, same six resources, same `grant_chosen_technology`. Lift the arm.
- `frln1` `frln2` `frln3` are one card three times: produce 1 unit here, and influence may be spent
  as if it were resources. `production` already models a `Spend` kind; this is a substitution at the
  same `payment_faces` site as cluster 2. Do it after that one.
- `mirage` places a planet token and grants its card. `Planet::is_placed_during_play` already exists
  for exactly this class.
- `ion` is a wormhole that flips sides. `Galaxy` already carries `wormholes_off` and
  `wormholes_all_linked` — check them before deciding anything is missing.

### 4. Relics — the remaining four

- `emphidia`: two halves. Explore a planet after a tactical action (easy — `exploration` is
  complete), and a status-phase victory point for controlling the Tomb. The Tomb arrives from the
  exploration deck, so do this after `mirage`; they share the placed-planet path.
- `dominusorb`: move and transport units from systems containing your command token. A movement
  legality change, in `transit`.
- `neuraloop`: replace a revealed public objective with a random one from any deck. Note the sting —
  a *secret* objective drawn this way becomes a public one. Check `objectives::scoreable` reads it
  as public.
- `titanprototype`: choose a player; they may spend 3 resources to place a structure, or else gain a
  trade good. It is the only relic that asks a decider other than its owner. `Table::ask` addresses
  a seat, so this works, but the offer must go to the chosen player.

### 5. Laws — 13 enacted but not enforced

They vote and enact correctly today; nothing reads them once in play.

**Four are ownable cards with a discard trigger** — `committee` (choose the elected player, no
vote), `arbiter` (swap a strategy card at end of strategy phase), `minister_peace` (end the active
player's turn), `minister_war` (remove a command token, then take an additional action). All four
follow `holds_ministry`, which exists and already has four callers to copy.

**Two are victory points that move** — `shard_of_the_throne` (on winning a combat against the
owner) and `crown_of_emphidia` (on taking a planet in the owner's home system). Both give a point
to the taker *and* take one from the previous owner. Get the losing half right; a version that only
grants is worse than not implementing it at all.

**The rest**: `revolution` (destroy a non-fighter ship after researching / exhaust planets per
technology), `classified` (a scored secret becomes public), `minister_industry` (PRODUCTION when
placing a space dock), `checks` (give away your chosen strategy card / ready only 3 planets),
`nexus` (nexus wormholes off, or a gamma token on Mecatol), `warrant` (play with secrets revealed),
`crown_of_thalnos` (see cluster 1).

`laws::active(state, alias)` is the predicate entry point. `laws::enforced_aliases()` is the ledger
— **add to it in the same commit as the predicate and its caller, never before.**

## The one defect class to watch for

Seven times in this work the bug was the same shape: **a registry that had drifted from the code
that dispatches from it, guarded by a test asserting something weaker than the invariant.**

- `action_cards::unimplemented` returned every card and never consulted `effect_for`; its guard
  asserted `all.len() > 50`, which held either way.
- Public objectives were counted from `registered_aliases` while the scorer also reads `cost_of`.
- The relic ledger counted `registered_aliases().len() + 1` — a bonus for a card already in the
  list, and no source filter, so it reported more relics than the corpus it was counting.
- The breakthrough count was the literal `2`, with a comment explaining that only two were read
  anywhere in the engine. True when written; it was 5 by the time anyone looked. (Fixed here.)
- Five passive relics were nearly offered as component actions, because `available_actions` offered
  from `registered_aliases` rather than from the arms of `use_relic`.
- Four laws sat in `enforced_aliases` with predicates and no callers.

**Before trusting any count, grep the helper for a caller.** A number in a report is not evidence
that the code path runs.

The mirror of this: **grep the model before declaring anything blocked.** `Galaxy::wormholes_off`,
`System::is_scar`, `GameState::strategy_card_goods` and the whole reroll-staging system were all
built and uncalled. At least one of the items above is less work than it looks for this reason.

## Verification

```
RUSTFLAGS="-D warnings" cargo clippy -p ti4-engine --all-targets   # CI runs this
cargo test -p ti4-engine                                           # 1,058 green at 4422407
cargo test -p ti4-sim                                              # 52 green; includes the gate
```

`-D warnings` is not optional. The branch was failing it on seven pre-existing clippy errors that a
laxer local invocation hid. They are fixed at `4422407` — keep it that way.

**`cargo test -p ti4-sim` needs `out/pools/full_np8_12_holdout.json`,** which is untracked and lives
only in the main checkout. Copy it into your worktree, or `fixture_capture_is_deterministic` fails
for a reason that has nothing to do with your change.

### The behavioural gate

`crates/ti4-sim/src/behavior.rs` runs two checks: a *value* gate (metrics within recorded bounds)
and a stricter *protocol-integrity* gate (the recorded bounds must equal what the tree recomputes).
The second is why the baseline moves even when every value is still in range.

Re-baseline with `cargo run -p ti4-sim --example rebaseline_behavior`, which prints old against new
and changes nothing by itself. Record every move in `plans/evidence/M08-021.md` with its cause.
Currently **v17**.

Adding a new event type lengthens the event stream and dilutes all six `share_*` metrics by one
uniform factor, while leaving `vp_pace`, `score_spread`, `faction_differentiation` and `completion`
untouched. A real change in play moves the latter. When you move the baseline, say which of the two
you are looking at — that distinction is the only thing keeping the gate meaningful.

## Things that will bite

- **Strategy cards key on `id`, not `alias`.** The only content category that does. Cost a
  debugging round on `investments`.
- **Expired fixtures.** Five tests died during this work because their premise was "the engine does
  not implement X" and then it did. When a test fails right after you implement something, check
  whether it was asserting the gap before you assume you broke it.
- **`fleet::enforce_everywhere` exists and is deliberately uncalled.** Wiring it into `Game::step`
  broke 8 fixtures. It needs its own reviewed change, not a drive-by.
- **A duplicate `AGENDA_PHASE_BEGAN` emission is known and deliberately unfixed** (`emit` plus
  `mirror_timing_log`), because fixing it moves the baseline. Fold it in when you are moving the
  baseline anyway.

## Scope

Six players. Base + PoK + Codices + Thunder's Edge. No variants, no galactic events. Faction content
outside the six trained factions is out of scope. Engine and policy features only — reward shaping
is explicitly not in scope. Training is paused by the user's decision; do not resume it.

## Two open bugs

- `plans/BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT.md` — overlaps cluster 2.
- `plans/BUG_2026-08-29_PROMISSORY_NOTE_TRANSACTION_OFFERS.md` — independent.

## On ownership

`crates/ti4-engine/src/action_cards.rs` and `crates/ti4-engine/src/agenda_effects.rs` were yours
during the split. I touched `action_cards.rs` once, in `4422407`, for two lines:
`frontline_deployment_puts_three_on_one_planet` had lost its `#[test]` attribute to a stray
duplicate introduced in `34b7b0d`, and had not been running since. Restored, and it passes — the
card was fine, only the test was switched off. Nothing else in that file changed.
