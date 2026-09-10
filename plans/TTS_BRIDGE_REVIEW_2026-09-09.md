# The TTS bridge, for review — 2026-09-09

**Author:** Claude Opus 5 — not eligible to review this.
**Branch:** `wp/tier-c-review-remediation-obs008c2b-003e1` at `bd54868` (all of this is uncommitted).
**Status:** the trained MLP answered **11 decisions with zero fallbacks** on a live six-player TTS
table and produced real commands. Everything below was verified against that table today, except
where it says otherwise — and the last section says otherwise about several things.

**Please attack §7 first.** It lists what is *not* tested, and one item there is the difference
between "this works" and "this appears to work".

---

## 1. What this is, and the decision that shaped it

The goal: play against the trained policy on a real Tabletop Simulator table.

The project already had a working Python stack — `tools/play_tts.py` in the pinned historical
repository reads the board, seats a game from it, plays bots forward and queues the physical result.
It works today; I ran it against the live table and it produced a legal command. **I did not find
that until after building most of a second implementation in Rust**, which is the single biggest
process error here and is why §6 exists.

The shipped architecture, after that correction:

```
TTS mod  --POST-->  bridge/server.py  <--HTTP--  tools/play_tts.py  --stdin/stdout-->  mlp_serve
   |                (vendored, unported)          (pinned, unmodified)                  (Rust)
   +-- executor Lua reads /poll, runs commands, reports outcomes
```

**Python owns the table, the rules, legality, command building and refusal reporting. Rust owns
only the judgement.** The seam is one method: `ScoredBot.choose(choice) -> Option`.

`bridge/server.py` is vendored **verbatim** (byte-identical body, provenance header added) from the
pinned repository at `37061c5`. It is standard-library only, binds `127.0.0.1`, and imports nothing
from the old engine. Keeping it satisfies what M11-002 through M11-005 asked for; those four
packages should be **struck from the Rust plan**, not left looking merely unstarted.

---

## 2. What is proven, and by what

Every claim here has a command behind it.

| Claim | Evidence |
|---|---|
| The wire works both directions | `bridge_selftest` against the real vendored server: telemetry posted, board read back decoded, command queued and stamped, collected by a poll, outcome reported as a log line and matched back to its command |
| A live table imports | Operator ran `!gamedata localhost`; the capture imported with all six seats resolved, correct per-faction home planet counts (Hacan 3, Xxcha/Letnev/Jol-Nar 2, Sol/L1Z1X 1), Jol-Nar's 4 starting techs, 3/3/2 token pools |
| A real tactical action imports | Operator played a turn. Activation (command token in tile 64, tactics 3→2), fleet move, landing, **control-without-units** via owner token on Archon Tau, and a **Diplomacy** resolution (five foreign command tokens in L1Z1X's home) all read correctly from the board string alone |
| The MLP can score an imported state | `mlp_decide` on the live capture: chose `tactical`, **116 features assigned / 28 OOV (80.6% known)** |
| The policy actually ranks options | In the shim test `sol` was offered `pass` **first** and chose `tactical` — not index 0 |
| End to end through the real driver | `bridge/play_with_mlp.py --seats all --dry-run --turns 1`: **11 decisions answered, 0 fallbacks**, 5 commands built including two `score` actions the Python bots did not find |

77 Rust tests pass; `cargo clippy --all-targets` and `cargo fmt --check` are clean for `ti4-bridge`
and for the two new `ti4-mlp` examples.

---

## 3. Six format traps, all found the hard way

These are the substance of the work. Each was found by a test or a live capture failing, not by
reading, and each is the kind that produces a plausible wrong answer rather than an error.

1. **The corpus zero-pads, the mod does not.** Mod writes tile `1`; corpus says `01`. Nine tiles
   differ and they are **home systems** — a naive lookup drops every seat's starting position and
   builds a board where nobody lives anywhere. `mapping::corpus_id` normalises; a golden test
   asserts no single-digit tile is ever reported unknown.
2. **Mecatol is not always tile 18.** Every real capture puts tile **112** at the origin — an
   alternate printing (`mrte`, alias `mr2`). A test that hardcoded 18 failed against real data. The
   assertion is now on the *planet* at the origin.
3. **The wormhole nexus has two faces and the summary shows neither.** Mod writes `82`; corpus
   splits `82a` (gamma only) and `82b` (alpha, beta, gamma). Found only because the live capture
   reported tile 82 unknown. Guessing wrong does not lose a tile — **an open nexus is adjacent to
   every alpha and beta wormhole on the board**, so it silently rewires movement galaxy-wide. Placed
   on the locked face (the setup state) and **declared** as `Mapped::assumed_nexus_locked`.
4. **Lua cannot distinguish an empty dict from an empty list.** `handSummary` is `{"Actions": 2}`
   when populated and `[]` when empty. A strict map deserialiser works on every fixture *until
   somebody plays their last action card*. Accepts both; a *populated* array is still refused,
   because that is a genuine shape disagreement.
5. **The mod drops the leading article.** `Xxcha Kingdom` against the corpus's `The Xxcha Kingdom`,
   and `Jol-Nar` against alias `jolnar`. Matching normalises article, case and punctuation.
6. **Units are a sixth abbreviation scheme, and tiles convert back.** The executor matches TTS object
   names — `Space Dock`, `War Sun`, `PDS` — and upgrades collapse (`fighter2` → `Fighter`, because
   the upgrade is a card not a model). Tiles go back out **unpadded**: `01` → `1`. The importer pads
   in, the builders strip out, and a one-directional test would never catch it.

A seventh, in the integration rather than the formats: **`seat_bots(game, ...)` registers each bot
into `game.table` and *also* returns a dict.** Wrapping the dict entries wraps a handle nobody
consults — it reported six seats wrapped and answered **zero** decisions. That is the failure mode
worth dwelling on: a bridge that looks integrated and silently plays the old policy.

---

## 4. Design decisions to challenge

**Nothing hidden is invented, and gaps are data.** `Imported::gaps` is a typed list, not a log line.
`start_game` deals real hands, so imported states have theirs **cleared** — a test caught that on
first run, and without it the bot would confidently play action cards it does not hold. Current gaps
on a live table: tiles 901–907 unknown, phase inferred, exhaustion unknown, decks unknown.

**Built on a real setup, not assembled field by field.** `GameState` has 34 fields, most of them
decks the table cannot show. The importer starts from `ti4_engine::setup::start_game` and overwrites
only what telemetry observed, so the invisible two-thirds stay internally coherent. *Is this right,
or does it smuggle in setup state that contradicts a mid-game table?* Genuinely unsure.

**Phase is inferred from whether strategy cards are dealt.** Strategy before, Action after. This
cannot separate Action from Status or Agenda; it names Action and declares the inference. *A
reviewer may know a better signal — this is the weakest inference in the module.*

**No HTTP crate.** `client.rs` is ~80 lines of HTTP/1.1 over `TcpStream`: one request per
connection, `Connection: close`, loopback-only unless the caller insists. Adding a dependency to a
shared workspace mid-experiment risked breaking other sessions' builds for no benefit.

**Answers are validated twice.** `ask_private` checks the answer was on offer, and `rust_bot.py`
checks again. A trust boundary enforced on one side only is not one.

**Refusals fall back and say so, once per reason.** `fallback=None` makes them fatal, which is what
a scored evaluation wants.

---

## 5. Known limitations, stated not hidden

- **The board is stale within a turn.** `mlp_serve` re-imports the newest capture per request, so it
  is current *between* turns and lags *within* one — a bot's queued commands have not reached the
  table while it is still deciding. Option scoring is unaffected (options arrive fresh from Python);
  board context can lag. **This is the first thing to suspect if advice looks stale, and the item I
  would most like a reviewer to propose a better design for.**
- **Hyperlane paths are not modelled.** `galaxy_from_board` places hyperlane tiles at their position;
  `Galaxy` tracks them only as a flag. Movement across hyperlanes is *not* answered correctly.
- **19% of features fall to out-of-vocabulary columns** on an imported state. That is the designed
  family-OOV fallback, not a failure, but nobody has established what the healthy range is for a
  *simulated* state to compare against.
- **The policy's training horizon is four rounds.** An 8-round probe (12 maps, same panel) scored
  6.606 VP against 3.845 at four — 0.826/round versus 0.961, a taper not a cliff, with no
  truncations. Agendas *are* seen in training (`OBS-008f2`); an earlier claim otherwise in this
  session was wrong and is retracted.

---

## 6. Process failures worth recording

1. **I rebuilt working machinery.** Told "recycle the old bridge", I checked which modules imported
   the old engine, concluded the semantic half could not be reused, and started rewriting — instead
   of looking for an entry point and running it. `tools/play_tts.py` was three greps away.
2. **I asserted a capability limit that was wrong.** I said the policy had never seen an agenda
   phase, from a *stale doc comment* describing a bug that was later fixed. Read as current fact.
3. **`cargo clippy --fix` rewrote a committed shared file** (`ti4-mlp/src/bot.rs`, the training hot
   path) as a side effect of fixing an example. Behaviourally identical, reverted by hand. Worth a
   standing rule: never run `--fix` with a whole-package scope in this tree.

---

## 7. What is NOT tested — read this before trusting anything above

- **No MLP command has ever reached the table.** Every run was `--dry-run`. Nothing has been
  physically moved by this integration.
- **Only round 1 was ever imported.** No mid-game or late-game board, no board with laws in play, no
  agenda phase, no combat state, no exhausted planets, no Mecatol taken (so `custodians_removed`
  has never been observed true on live data).
- **The nexus face assumption has never been wrong-way tested.** No capture had an *open* nexus.
- **`--read-hands` has never been used with the MLP.** Hands are empty and declared as gaps; the
  interaction between the driver's hand reporting and the importer's clearing is unexercised.
- **One decision path only.** The 11 answered decisions were action-phase and scoring choices. No
  combat, no invasion, no agenda vote, no production has been answered by the MLP through this path.
- **No performance measurement.** Per-decision latency is unmeasured; 11 decisions felt fast, which
  is not data.
- **The 8-round probe is 12 map clusters** and the paired-difference sd is ~0.27 VP/map, so it
  resolves ~0.3 VP at best. Treat the taper as directional.

---

## 8. Reproducing it

```powershell
# 1. the endpoint
python bridge/run_bridge.py --capture out/bridge-captures
#    then in TTS chat:  !gamedata localhost
#    (30s is the floor; a still table sends nothing, the mod suppresses equal-CRC uploads)

# 2. check the import
cargo run -p ti4-bridge --example import_capture              # newest capture
cargo run -p ti4-bridge --example import_capture -- --tile 64 # one system in full

# 3. the policy service
cargo build --release -p ti4-mlp --example mlp_serve

# 4. play (PATH needs target\release and out\libtorch-2.9.1-cpu\lib)
python bridge/play_with_mlp.py --seats all --dry-run --turns 1
```

`bridge_selftest` needs no TTS at all and exercises the whole wire.

---

## 9. Files

| Path | Lines | What |
|---|---:|---|
| `bridge/server.py` | 452 | Vendored verbatim, unported. Do not edit to add behaviour. |
| `bridge/run_bridge.py` | 58 | Entry point the vendored server never had |
| `bridge/rust_bot.py` | 218 | `RustPolicy.seat(bot)` — the `choose` shim |
| `bridge/play_with_mlp.py` | 107 | Wraps `play_tts` from outside; pinned repo unmodified |
| `crates/ti4-bridge/src/wire.rs` | 607 | M11-001, six envelopes + outcome line parsing |
| `crates/ti4-bridge/src/hexsummary.rs` | 866 | M11-007, sticky board grammar |
| `crates/ti4-bridge/src/mapping.rs` | 333 | M11-009, doubled-height → axial, tile normalisation, nexus |
| `crates/ti4-bridge/src/import.rs` | 612 | M11-010, telemetry → `GameState` + gaps |
| `crates/ti4-bridge/src/commands.rs` | 378 | M11-014/015, command builders |
| `crates/ti4-bridge/src/client.rs` | 504 | Loopback HTTP client, no dependencies |
| `crates/ti4-mlp/examples/mlp_serve.rs` | 259 | The decision service |
| `crates/ti4-bridge/tests/` | 642 | Golden corpora: 9 board captures, 9 full telemetry uploads |

`ti4-bridge` is a **dev**-dependency of `ti4-mlp`, so the trainer does not drag TTS wire formats into
every training build. Workspace `Cargo.lock` gained one line.

---

## 10. Questions I would most like answered

1. Is starting from `start_game` and overwriting the right shape for the importer, or does it
   smuggle in setup state that contradicts a mid-game table?
2. Is there a better signal for the phase than "are the strategy cards dealt"?
3. What should the within-turn staleness design be? Options considered: Python serialises its own
   `GameState` (the two are field-for-field ports, but Rust has fields added since), or Rust tracks
   its own state forward through the decisions it answers.
4. Is 80.6% feature coverage healthy? Nobody has measured the same number on a simulated state.
5. Is the nexus locked-face default right, or should an unknown face be a refusal rather than an
   assumption?
