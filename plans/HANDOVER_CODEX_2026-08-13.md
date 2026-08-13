# Handover to codex — 2026-08-13

Branch `wp/m06-003-structured-transactions`, head `09449ea`. Working tree clean.
Workspace green: 18 test binaries pass, 0 failures, `RUSTFLAGS="-D warnings" cargo clippy
--workspace --all-targets` clean.

---

## Where the project actually stands

The owner's standing instruction is to **prioritise a running simulator over depth-first card
porting** — get to a state where simulations and learning can run under Rust. Everything below is
ordered against that.

The engine plays complete six-player games end to end. What it does *not* do is produce games worth
learning from. Measured over 24 games at head:

| | random seats | scored seats |
|---|---|---|
| ships moved | 352 | 867 |
| invasions | 171 | 352 |
| planets taken | 14 | 98 |
| space combats | 62 | 133 |
| objectives scored | 4 | 10 |
| **top score, any seat** | **1–2** | **1–2** |
| ending | objectives exhausted, round 9 | objectives exhausted, round 9 |

Every game still ends with the objective deck empty and nobody past two points. A learner given
these trajectories has almost no signal: the reward is nearly always zero and never a win.

Reproduce either column with:

```
cargo run -p ti4-sim --example diag --release            # scored
cargo run -p ti4-sim --example diag --release -- --random
cargo run -p ti4-sim --example prompts --release         # one game, prompt by prompt
```

Both examples are committed tools, not scratch. `prompts` exists because no event count could
explain why doubling the number of invasions did not move the scoreboard.

---

## The next task, and why it is the only one that matters right now

`prompts` on a single scored game reports:

```
248  action phase        →  tactical×73, pass×54, strategic×54
 96  movement            →  done_moving×66, move|16|0×3, ...
 73  activate a system   →  23×6, 30×6, 24×5
  9  commit ground forces
```

**73 activations produced 30 ship moves.** The bot spends a command token on a system nothing of
its can reach, then ends movement because there is nothing to move. Nine ground commitments in a
nine-round game is why nobody takes planets, and nobody taking planets is why no objective
requirement is ever met.

This is not a tuning problem. `ScoredBot::raw_score` scores `"activate"` with a flat `6.0`, because
until head it had no way to tell one system from another.

### What is already in place for you

Head `09449ea` built the seam:

- `ti4_engine::choice::Observed<'a>` — public facts only: `board()`, `system()`,
  `controlled_planets()`, `systems_with_units_of()`, `systems_with_token()`, `seat()` (a
  `PublicSeat` of counts, never identities), `revealed_objectives()`, `scored_by()`, `galaxy()`.
  Private holdings are reachable only via `redacted_for(viewer)`, which **copies** — deliberately,
  so reading private information has a visible price.
- `Decider::choose_seeing(&Choice, &Observed)`, defaulted to `choose`. Scripted tests and the
  random smoke runner stay ignorant of the board.
- `Table::ask_seeing`, sharing `settle` with `ask` so validation and the decision log cannot drift.
- All eight driver decision sites in `game.rs` already pass an `Observed`.
- `ti4_policy::valuation` already takes `&Observed`: `system_value`, `planet_value`,
  `fleet_strength`, `stranded_troops`, `unit_value`, and the named clamped multipliers.

### What to write

In `crates/ti4-policy/src/bot.rs`, implement `Decider::choose_seeing` on `ScoredBot` and route the
board-dependent kinds through it. In rough priority order:

1. **`activate`** — `valuation::system_value(seen, player, system)` is written and tested and is
   waiting for exactly this call. Then port the oracle's `_worth_considering` filter for
   activations (`bots.py` ~line 3671): a system no ship can reach and nothing can be built in must
   be **removed from consideration**, not merely outscored. The oracle's comment records why —
   when every option scored zero the tie broke at random and two factions sailed for a home system
   nothing of theirs could reach. Scoring it zero was not enough.
2. **`move`** — prefer moving toward the activated system's prize; the oracle's `_score_move`.
3. **`load` / `land`** — an invasion needs troops aboard before it needs a landing. `land` is
   currently a flat `8.0` and the load step declines six times in twenty-nine.
4. **`produce`** — the current value-per-resource rule is state-free and fine; adding the board
   lets it prefer hulls where they are needed.

Keep `raw_score` as the blind fallback. The two paths should not diverge in kind coverage: a kind
scored only in `choose_seeing` is one that silently gets no judgement wherever the engine still
calls plain `ask` (windows that own a slice of the game rather than the whole of it).

### How to know it worked

Rerun `diag`. The number to watch is **top VP per game**, not activity counts — this project has
twice been fooled by activity that did not reach the scoreboard. Success is games that end with
somebody near or at ten, and `endings` showing `victory_points` rather than
`objectives_exhausted`.

---

## Conventions this repo enforces, learned the hard way

- **Mutation-check every new rule.** Break it, watch a named test fail, restore it. Several tests
  in this repo were vacuous when first written and passed with the rule deleted — including one of
  mine two commits ago (a stranded-troops test whose extra unit sat in space, so the ground-force
  filter was never reached).
- **Wiring guards must be behavioural.** `wiring.rs` once contained greps over import lines that
  `-D warnings` already enforced; they passed while the subsystem was disconnected. If you add a
  guard, delete the call it guards and confirm the guard fails.
- **A card or ability with no registered handler is unavailable, never silently free.** Gaps are
  counted in a ledger (`registry.rs`, `unscored_kinds()`, `unimplemented()`), not hidden.
- **Measure before claiming.** Two claims in this session were wrong and were caught by the owner,
  not by me: an action-card count off by 35×, and four leaders described as needing machinery the
  oracle already had. Both were assertions about the oracle made without opening it.
- The Python oracle at `D:\Projects\ti4-engine` is **read-only**. Never edit, format, stage,
  commit, clean, reset, generate caches in, or run anything that writes into it.
- Work only inside `D:\Projects\ti4-engine-rs`. Preserve unrelated changes; no destructive reset or
  checkout; do not rewrite shared branch history.
- `git add -A` will sweep the `.worktrees/` gitlinks into a commit. `.gitignore` now covers them,
  but prefer `git add crates plans`.

---

## State of the rest of the plan

**M08 (authored bots)** — 001 through 004 and 011 landed; 005 is the task above. The oracle's
`bots.py` is 6,608 lines and the Rust bot is ~450; the gap is deliberate, and the parts not yet
ported are named in `unscored_kinds()` (transactions, tiebreaks, leaders, action cards) rather than
faked.

**Engine parity** — registries are at parity. Faction abilities 13/15, promissory notes complete,
leaders 15/18 for the six in-scope factions (Firmament is out of scope by owner decision).

The three leaders still open, and why:
- `jolnaragent` (Doctor Sucaban) and `jolnarcommander` (Agnlan Oln) need the oracle's **event
  ability** path — an ability that receives the triggering event and modifies it, rather than
  reading the game after the fact. Sucaban lowers a research cost before it is paid; Oln rerolls
  dice from the roll that just happened. The effects are small; the path does not exist here yet.
- `xxchahero` is unimplemented in the oracle too.

**Also open:** 11 reaction windows blocked on combat interrupt points, Quash blocked on
agenda replace-in-window, Thunder's Edge (1,085 oracle lines, zero Rust), and M00-013, the Python
performance baseline, still unrun.

**Not blocking you.** M08-005 does not depend on any of them.
