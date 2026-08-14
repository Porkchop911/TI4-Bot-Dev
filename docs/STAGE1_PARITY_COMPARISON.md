# Stage-1 Python/Rust parity and comparison

This document defines the comparison that must pass before Rust and Python learning curves or
training throughput are called comparable.

## Reference experiment

The behavioral reference is the deployed table in
`D:\Projects\ti4-engine\out\stage1_pg_headsplit_20260810.json`:

| Setting | Reference value |
|---|---:|
| factions | Letnev, Jol-Nar, Hacan |
| starting policy | blank, no teacher or imitation |
| profile representation | schema 4, collision-free named features, 14 heads |
| learning rate | 0.03 |
| entropy | 0.01 |
| gradient clip | 1.0 |
| temperature | 1.0, fixed |
| train seeds | 16 per update |
| rotations | 3 per seed |
| games per update | 48 |
| maps | varied |
| clear bonus | 22 |
| expansion / unit / conjunctive weights | 2 / 1 / 0 |

The Python deployed clearance was Hacan 0.979, Jol-Nar 0.979, and Letnev 0.938 on 96-game
panels. `planets_gained` moved on the first evaluation and is the early diagnostic; clearance may
remain zero for hundreds of updates.

## What was wrong in the Rust comparison

The old Rust curve was a different experiment:

- schema 2 with 512 signed hash buckets rather than schema 4 named weights;
- no structured system, planet, route, or unit facts;
- six factions in fixed seats rather than three factions rotated through every seat;
- 8 games per update rather than 16 seeds times 3 rotations;
- learning rate 0.05 rather than 0.03;
- a Rust-specific full-content map and choice environment rather than proven Save-54 parity.

The resulting curve learned production but not expansion. That result was useful evidence for the
missing representation; it was not evidence that the Python curriculum failed.

## Implemented repair

The old hashed path remains compatibility-frozen. New Stage-1 parity runs use:

- sparse schema-4 profiles that grow named weights on observation;
- the explicit extractor's generalising text features (bare numeric identities removed);
- `target`, `origin`, `destination`, `invasion`, `landing`, route, and unit facts;
- faction-keyed profiles that follow the faction through physical-seat rotations;
- one map seed shared across every rotation;
- a reference `FactionPlan` with the learning and reward settings above;
- checkpoint loading from the Python profile table;
- per-faction clearance, planets gained, systems, units gained, and shortfall.

Run a blank smoke curve:

```powershell
cargo run -p ti4-training --example stage1_parity --release -- `
  --updates 50 --every 25 `
  --map-pool D:\Projects\ti4-engine\data\map_pools\save54_e2000_n8192.json.gz
```

Test the solved Python weights first:

```powershell
cargo run -p ti4-training --example stage1_parity --release -- `
  --updates 0 `
  --checkpoint D:\Projects\ti4-engine\out\stage1_pg_headsplit_20260810.json `
  --map-pool D:\Projects\ti4-engine\data\map_pools\save54_e2000_n8192.json.gz
```

The command exits unsuccessfully if any solved faction is below 0.80 clearance. For investigation
only, `--allow-solved-regression` prints the measurements without failing the command.

## Gates, in order

1. **Execution:** no rollout errors and non-empty trajectories.
2. **Representation:** explicit named profiles; structured board features occur in recorded legal
   options.
3. **Rotation:** every faction occupies every physical seat for each map seed.
4. **Solved-profile transfer:** every imported solved faction reaches at least 0.80 clearance.
5. **Blank learning:** `planets_gained` rises by updates 25–50.
6. **Decision-boundary parity:** equivalent positions expose equivalent legal semantic choices and
   progress/reward measurements.
7. **Performance:** only after the applicable semantic gates pass may timings be presented as a
   Rust/Python speedup.

`tools/benchmark_training.py` now labels the existing measurement a timing diagnostic. A clean
process and non-empty decision stream no longer count as semantic parity.

## Remaining differences

The repair does not claim complete engine parity. In particular:

- Without `--map-pool`, Rust retains its legacy spiral generator. Parity runs and training now load
  the exact Python `ti4-map-pool-v1` JSON.GZ artifact and use Python's deterministic draw rule.
- Rust still raises fewer decision windows because rules, reactions, faction abilities, and card
  effects remain incomplete.
- Some Rust-local choice kinds require semantic aliases when applying Python feature weights.
- Option prompts and payload shapes are not yet covered by a complete cross-engine golden corpus.

The implemented decision surface was audited again on 2026-08-14. Stateful nested choices now
route through `ask_seeing`: action-card and agenda targets, reactions, exploration, payment,
fleet/capacity removal, space-cannon and combat casualties, and driven timing windows all expose
the current public position to the policy. The remaining plain `Table::ask` calls in engine source
are tests plus the deliberately context-free timing resolver API; the live game driver uses its
stateful counterpart.

That audit also closed concrete missing windows rather than merely exposing them:

- Integrated Economy now triggers after conquest, offers learned unit and payment choices under
  the conquered planet's resource cap, places the units, and enforces supply;
- Sol's Orbital Drop learns the destination and optional mech DEPLOY/payment;
- Xxcha's Peace Accords learns accept/decline and destination, excludes Mecatol, then resolves
  control-gain technology and exploration effects;
- Psychoarchaeology, Chaos Mapping, Predictive Intelligence, and Bio-Stims now open their learned
  start/end-turn choices; and
- standalone Munitions Reserves, fleet removal, and casualty helpers now receive observations too.

This is closure of the implemented Stage-1 decision-routing class, not a claim that every TI4 card
and timing window exists in Rust. Unsupported content and a complete cross-engine event-ordering
corpus remain separate work.

These differences are why solved-profile transfer precedes long training. If solved weights fail,
running thousands of blank updates cannot distinguish a training defect from a different game.

## First repaired measurements

On 32 held-out seeds × 3 rotations (96 seat-games per faction), importing the solved Python table
now produces:

| faction | clearance | planets gained | systems | units gained |
|---|---:|---:|---:|---:|
| Hacan | 0.000 | 1.26 | 1.68 | 0.07 |
| Jol-Nar | 0.000 | 0.11 | 1.10 | 0.00 |
| Letnev | 0.010 | 0.80 | 1.64 | 0.05 |

This correctly fails the solved-profile gate. Adding factual payloads to Rust movement, cargo,
landing, and production choices materially changed the result—Hacan rose from 0.02 to 1.26 planets
gained—but does not close environment parity.

A 50-update run from blank under the repaired configuration showed immediate learning signal:

| faction | planets at 0 | planets at 25 | planets at 50 | units at 50 |
|---|---:|---:|---:|---:|
| Hacan | 0.02 | 0.15 | 0.20 | 0.82 |
| Jol-Nar | 0.02 | 0.05 | 0.00 | 1.81 |
| Letnev | 0.05 | 0.11 | 0.16 | 0.52 |

Hacan and Letnev now show the early planet movement the old hashed run lacked. Jol-Nar still turns
its signal into production rather than expansion, so the correct conclusion is “the primary
representation defect is repaired; decision/environment parity remains open,” not “the port now
matches Python.”

## Follow-up investigation: stalled underfilled cargo holds

An ordered solved-profile trace exposed a Rust engine defect specific to openings such as
Jol-Nar's. Its carrier has capacity four but only three loadable units. After all three had been
selected, `CargoWindow` was neither full nor explicitly closed, but it had no legal pickup left.
The tactical driver interpreted the missing choice as the end of the action and never sailed the
carrier. The unchanged home cargo was then offered again on the next activation.

The repaired window is complete when either its capacity is full **or every available candidate is
loaded**, matching the Python loop's `if not free: break`. Its prompt, decline identity, and payload
now also carry the Python cargo facts (`capacity_remaining`, loaded ground/fighters, origin,
damage, galvanization, and ground still available). Cargo weighted-feature overlap rose from
roughly 69% to 98% on the solved diagnostic.

On the same 24-seat-game/faction panel, before and after this repair:

| faction | planets before | planets after | systems after | units after |
|---|---:|---:|---:|---:|
| Hacan | 1.50 | 1.92 | 2.04 | 0.00 |
| Jol-Nar | 0.04 | 1.62 | 1.92 | 0.04 |
| Letnev | 0.88 | 1.12 | 1.79 | 0.00 |

The Jol-Nar trace now sails, lands twice, and reaches `2 planets / 2 systems / 1 unit` after its
first tactical action. This rules out its learned activation/landing policy as the original cause.

## Confirmed parity blockers

1. **Closed: strategy-card execution.** The driver now applies primaries and accepted secondaries
   for all eight cards, including Leadership's no-token secondary, Hacan's Trade waiver, Jol-Nar's
   Brilliant substitution, and the two Thunder's Edge replacements. See
   [`STRATEGY_CARD_PARITY.md`](STRATEGY_CARD_PARITY.md). A fresh solved-profile comparison is still
   required to measure the effect; implementation alone is not a parity result.
2. **Closed: constrained varied Save-54 maps.** Rust now validates and loads the exact Python
   `ti4-map-pool-v1` JSON.GZ artifact, selects `tile_seed % pool_length`, replaces its three physical
   home slots in rotation order, and shares the selected outer arrangement across rotations. The
   legacy spiral remains available only when no pool is supplied.

### Strategy-card follow-up measurement (2026-08-14)

After wiring every primary and accepted secondary, the 96-seat-game solved-profile panel reports:

| faction | clearance | planets | systems | units | shortfall |
|---|---:|---:|---:|---:|---:|
| Hacan | 0.094 | 1.34 | 1.80 | 1.36 | 3.281 |
| Jol-Nar | 0.010 | 1.53 | 1.96 | 2.23 | 2.844 |
| Letnev | 0.125 | 1.28 | 1.82 | 1.55 | 3.146 |

The solved gate still fails every faction's 0.80 target, but Construction/Warfare and the other
card effects have removed the previous near-zero-units symptom and produced nonzero clearance for
all three factions. A blank 50-update smoke curve also moves planets from `0.01/0.02/0.06` at
update 0 to `0.15/0.27/0.21` at update 50 (Hacan/Jol-Nar/Letnev). This is evidence that the card
repair matters; it does not close environment parity.

### Root-cause isolation (2026-08-14)

The transfer divergence is not one optimizer defect. It is three stacked environment/observation
contract differences. All panels below use 32 seeds times three rotations (96 faction-games).

First, the published Python target and the ordinary Rust runner do not sample the same maps:

| engine / board family | Hacan | Jol-Nar | Letnev |
|---|---:|---:|---:|
| Python, captured Save-54 board | 0.781 | 0.885 | 0.115 |
| Python, constrained varied Save-54 maps | **0.990** | **0.969** | **0.906** |
| Rust, unrelated varied partial spiral after the fixes below | 0.198 | 0.115 | 0.156 |
| Rust, captured Save-54 board after the fixes below | 0.312 | 0.292 | 0.010 |

The especially low fixed-board Letnev result is therefore not a useful 0.938-versus-0.010 port
comparison: the Python policy itself scores only 0.115 there. Python's varied family keeps each
home's three adjacent planets distributed over a two-planet and a one-planet system, varies those
systems among economy-similar replacements, and balances the outer board. Rust's ordinary map
builder does none of those things. The 0.97 published target belongs to the constrained varied
family.

The completed same-pool differential uses
`save54_e2000_n8192.json.gz`, game seeds `82_000_000..82_000_031`, the Python offset
`+20_000_000`, and all three rotations:

| same pool and seed selections | Hacan | Jol-Nar | Letnev |
|---|---:|---:|---:|
| Python solved profiles | **0.969** | **0.979** | **0.865** |
| Rust solved profiles | 0.312 | 0.292 | 0.010 |

Rust's corresponding planet means are 2.43 / 2.26 / 1.35, against Python's
3.80 / 3.00 / 2.81. Map-family uncertainty is therefore removed from this comparison: the
remaining gap is in game/choice execution, and Letnev is the largest trace target.

Second, nested production decisions were blind. `ProductionWindow::drive` used `Table::ask`, so
`LearnedBot` received no `Observed` board state, deliberately fell back to the first legal option,
and recorded no production decisions. Since fighters sort first, TE Construction produced fighters;
the second carrier loaded cargo but had no ground force, and no invasion window opened. Routing
production through `ask_seeing` moved Jol-Nar on the captured board from **0.021 to 0.323** clearance
and its individual planet-bar pass rate from **0.021 to 0.323**. This is the largest isolated engine
effect. Other synchronous `Table::ask` sites remain an audit item; the bug is architectural, not
specific to fighters.

Third, several legal choices had different learned identities:

- TE Construction flattened the abstract Python `structure` branch into every concrete placement
  and changed the production option's kind and label. Restoring the Python two-stage choice made
  Jol-Nar choose `produce|home` at the expected 0.995 probability and raised units, although it did
  not by itself close the planet gate.
- Rust production used `produce|infantry`; Python trained `build|infantry|2` and
  `build|infantry|1`, with `done_producing` as the stop id. Rust also charged one production-capacity
  point for a two-unit purchase instead of two. These contracts now match.
- Every Rust strategy secondary used one generic `follow/decline` choice. Python trained
  card-specific `yes/no` prompts. Restoring those contracts raised captured-board Hacan from 0.135
  to 0.208 in the cumulative ablation and substantially raised its unit-bar pass rate.
- Rust movement omitted the Python payload facts `capacity`, `damaged`, and `gravity_drive`.
  The imported Hacan and Jol-Nar heads have material weights on those exact names. Restoring them
  raised captured-board Hacan from 0.208 to 0.312 in the cumulative ablation.

The ablations are cumulative and can interact, so their deltas must not be added. Their direction
and the trace-level failure chain are repeatable. The map comparison is now controlled; the next
parity work is auditing every synchronous decision for `ask_seeing` and aligning the remaining
decision boundaries. Training-speed claims remain invalid until the solved transfer gate passes.

### Solved-profile transfer gate closed (2026-08-14)

Ordered same-seed traces then isolated four more execution contracts:

- the game driver discarded `FactionPlan.sources`, so a FULL-content parity run silently executed
  under PoK and never granted Thunder's Edge breakthroughs;
- Gravity Drive never generated the learned `move_gd|origin|index` option or spent its one-ship
  bonus, preventing Hacan's first carrier from reaching the two-planet system;
- Transit Diodes never opened its free start-of-turn redeployment, leaving Jol-Nar's second carrier
  without the infantry that takes its third planet;
- Rust's `target:reachable` feature excluded the fleet's own system. Python's learned observation is
  the straight-line movement envelope and includes distance zero; the exclusion heavily suppressed
  Hacan's learned "activate home and produce" action.

Technology choices also now use authoritative current printings and printed names, ordinary
production resolves faction mechs/flagships and excludes structures, and production payloads carry
Python's `count` and `system` facts. Letnev's Gravleash and the six expedition actions were added
along the same trace path.

On the exact same 32 map-pool selections and all three rotations, the cumulative fixed result is:

| engine / solved profile | Hacan | Jol-Nar | Letnev |
|---|---:|---:|---:|
| Python | **0.969** | **0.979** | **0.865** |
| Rust after decision-window closure | **0.865** | **0.865** | **0.823** |

Rust's detailed panel is:

| faction | clearance | planets | systems | units | shortfall |
|---|---:|---:|---:|---:|---:|
| Hacan | 0.865 | 3.74 | 2.97 | 2.96 | 0.167 |
| Jol-Nar | 0.865 | 2.95 | 2.97 | 2.39 | 0.177 |
| Letnev | 0.823 | 2.84 | 2.97 | 2.40 | 0.281 |

This passes the deliberately conservative transfer gate of 0.80 per faction. It does **not** claim
complete decision-by-decision engine parity: Rust still has unsupported content/timing windows,
different RNG consumption, and no complete golden event corpus. Those are residual engine-scope
gaps, not reasons to reject the port's policy representation or learning loop.

### Optimized training substrate (2026-08-14)

The faction trainer now shares immutable schema-4 profiles, schedules individual seed/rotation
games on a persistent Rayon work-stealing pool, reduces trajectories to sufficient statistics on
the rollout workers, and merges statistics in parallel by faction/head without changing their
game-order accumulation. Exact worker-versus-parent statistics and one-versus-32-worker rollout
tests pass.

On the same 16-seed x 3-rotation Stage-1 update, sustained throughput improved from approximately
`0.41 s/update` in the first threaded implementation to `0.091 s/update`; the final 200-update panel
averaged 16.8 equivalent cores and peaked at 36 process threads. See
`docs/TRAINING_PIPELINE.md` for commands, Stage-2 integration, and checkpoint rules.
