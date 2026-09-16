# Request for review and advice — Astra, 2026-09-16

We would like your opinion on three things: whether the diplomacy system is the right shape, where
PPO training time can realistically be won, and how to get more out of training (victory points, and
bots that actually use the Fracture).

Everything below is measured on this machine today unless marked otherwise. Numbers we distrust are
marked as such — please tell us if we are measuring the wrong thing.

## Where the code is

- Worktree `C:/Users/Niko/Documents/ChatGPT/ti4-engine-rs/diplomacy-worktree`, branch
  `codex/diplomacy-v1`, HEAD `85d0151`.
- **Everything discussed here is uncommitted.** Do not reset or clean the tree.
- Background: `plans/DIPLOMACY_HANDOVER_2026-09-15.md`, `plans/DIPLOMACY_V2_PLAN_2026-09-15.md`,
  `plans/CLAUDE_HANDOVER_2026-09-16.md`, and `plans/TODO_2026-09-16.md` (today's changes, with
  before/after numbers).
- Build env: `LIBTORCH=D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cu128`,
  `LIBTORCH_BYPASS_VERSION_CHECK=1`, that `lib/` on `PATH`.

## 1. Functionality of the diplomacy system

### What exists

A contact is a bounded negotiation between two seats, offered as a free action to any other seat,
once per ordered pair per turn, and once per pair per agenda in the talks before each vote. Inside
it: whole-bundle offers, up to two counters, accept or decline, and one signal statement. Promises
are typed terms judged by engine events (`DoNotAttack`, `Vote`, `Attack`, `DoNotActivate`,
`FuturePayment`, `UseLeaderFor`, `ReplenishFor`), and a directional relationship matrix (trust,
cooperation, threat, hostility) moves on what actually happened. With diplomacy on, the legacy
transaction window is replaced: trades happen inside contacts.

Today the user pruned it hard, after watching games:

- removed: the free non-aggression swap, pay-to-skip-secondary, promissory-note loans, and every
  signal except a warning about a named system and (agenda phase only) a vote assurance;
- "nothing for nothing": a bundle must carry something on both sides; a one-sided legacy shape now
  carries a commitment (a commodity owed next round); `FuturePayment` became credit (goods now
  against more goods next round); trade-goods-for-trade-goods swaps are refused outright, including
  on counters;
- the Trade primary's refresh became sellable: the holder of an unplayed Trade card promises to
  refresh a partner, against a commodity later;
- commodities are now priced by direction in the policy features (0.2 to the giver, full value to
  the receiver, per 21.5) instead of being counted like trade goods on both sides.

### What the measurements say

One round, `checkpoint-212544`, 6 seats, held-out pool, before → after the prune (temperature 0.5):

| part | before | after |
|---|---|---|
| Trade take rate | 22% | 45% |
| FuturePayment (now credit) | 27% | 50% |
| NoteForNonAggression | 0% of 82 offers | 11% of 62 |
| promises settled in play | none | 7 (5 kept, 2 broken) |
| promise payments | none | 12 offered, 3 paid |
| contacts ending with no offer | 4% | **86%** |

Never chosen at either temperature, before or after: refusing a deal (0 of 57), and — before it was
removed — every conditional signal. The parts that survive with real uptake are trading, credit,
and accepting.

### What we would like your opinion on

1. Is a per-pair contact window the right primitive at all, or should diplomacy be an extension of
   the transaction (options added to a trade) rather than its own multi-step machinery?
2. **Empty contacts.** 86% of contacts open and then offer nothing. Should a contact be offered only
   when a bundle survives filtering for that pair? That is cheap to implement and is the single
   biggest cost saving available (see §2), but it makes "open a contact" a weaker signal for the
   policy — it can no longer probe.
3. Is the promise set the right one? `UseLeaderFor` covers only the Hacan and L1Z1X agents.
   `ReplenishFor` is new and unexercised in play. Are there obvious tradeable commitments in TI4 we
   are missing that an engine can actually judge?
4. Does the relationship matrix earn its keep? It feeds policy features, but nothing yet shows it
   changes behaviour. What evidence would you want before keeping or cutting it?
5. Is "a policy never refuses a deal" a modelling failure, a reward artefact, or expected?

## 2. Speed-up potential for PPO training

### Measured today

Same build, same bundle, same seeds, one update, `--no-checkpoint`, 96 games (16 seeds × 6
rotations) of 4 rounds at temperature 2.5, optimiser on CUDA:

| | decisions | rollout | optimise | total |
|---|---|---|---|---|
| `--diplomacy` | 237,220 | 25.3s | 8.5s | **33.8s** |
| without it | 119,832 | 12.5s | 4.9s | **17.4s** |

So diplomacy doubles the cost per update, and rollout is three quarters of it. The overnight run
(1000 updates) took 9h44m, matching 33.8s/update. `ppo_update` prints "Rollouts are CPU-bound and
sequential here; the optimiser honoured --device" — §7.1 requires CPU inference during rollouts.

A 2-round, 6-seat cost probe with random deciders reports diplomacy at only +4% decisions. **We do
not trust that number** as a training-load figure: the trainer plays 4 rounds at temperature 2.5,
where seats open contacts constantly (534 contact options offered in a single round, about a third
taken, each opening a multi-step window).

### Where we think the time goes, and what we want advice on

1. Decision count is the driver. Cutting empty contacts should remove a large share of the
   diplomacy surcharge. Do you agree that is the first move, and what else would you cut?
2. Rollouts are sequential CPU. Is batching inference across the 96 games (or across seats within a
   game) feasible given the engine's step model, and is that where the real win is?
3. Per-decision feature extraction and option payloads: every offer option carries a serialized
   bundle. We measured option-payload bytes at +7% after the prune (was ×11), so this is no longer
   a memory problem — but is it still a CPU problem in extraction and scoring?
4. Is 4 epochs × minibatch 4096 sensible for this decision volume, or is the optimiser side worth
   tuning before the rollout side?
5. Anything structurally wrong with 96 games per update as the unit of work?

## 3. Quality of training: victory points and the Fracture

### What the last run produced

1000 updates from `checkpoint-240`, temperature 2.5, learning rate 1e-4, on the train pool. Held-out
paired greedy evaluation afterwards (`crossplay_eval`, 720 games per row, candidate in one seat
against five frozen copies of the pre-diplomacy champion, **diplomacy off** because that harness has
no diplomacy wiring). The null is benchmark-against-itself, not zero:

| run | margin | VP | win | waste |
|---|---|---|---|---|
| null | −1.476 | 3.225 | 10.4% | 18.9% |
| start, checkpoint-240 | −1.386 | 3.307 | 13.2% | 22.4% |
| mid, checkpoint-97716 | −1.807 | 2.917 | 8.1% | 6.3% |
| mid, checkpoint-164756 | −1.340 | 3.353 | 14.3% | 5.4% |
| final, checkpoint-212544 | −1.494 | 3.164 | 13.2% | 9.9% |

1000 updates bought no base-game strength: the final checkpoint sits at the null and slightly below
where it started, wandering rather than improving. The one clear gain is waste, 22% → 10%. Mean VP
per seat is ~3.35 over 4 rounds.

The run's reward settings cannot be authenticated: the manifest does not record reward flags and the
run predates our launcher. Shell history shows the intent (vp 1.1, objective 0.35, r1 bonus 3, r1
shaping 0.1, clearance 0.5, fleet 0.03, tech 0.1, strategy diversity 0.5, fleet-hoard 1, zero-fleet
10, trade-goods hoard 0.05, **styx 16**, fracture entry 0.3), but the trainer's own header for our
1-update reproduction shows several of those at 0, so we believe the reward lines did not reach the
process. Treat the run as "defaults plus what the first command line carried".

### Questions

1. **Maximising VP.** With a shaped reward (clearance, waste, fleet, tech, diversity) the policy
   optimises the shaping and not the points. What would you use as the primary objective for a
   stage-2 run that must actually raise VP and win rate against a frozen benchmark? Is paired
   crossplay margin the right target metric, or should we train against a frozen opponent directly?
2. **Styx.** `--styx-bonus 16` next to `--vp-weight 1.1` is about 14.5 VP of reward for holding one
   planet. If that ever reaches the trainer it will dominate everything. What is a defensible value,
   and should terminal-control bonuses exist at all versus letting the VP the planet already carries
   do the work?
3. **The Fracture.** Bots do not use it. Three causes need different fixes and
   `crates/ti4-mlp/examples/fracture_census.rs` separates them: never in play (nobody rolled it in),
   in play but never offered (engine bug — one such bug was fixed in `d151134`), or offered and
   never chosen (valuation). Reward levers exist: `--fracture-entry-bonus` (once per seat-game on
   first entry) and `--fracture-planet-bonus` (per Fracture planet held at the horizon). We have not
   re-run the census since the recent engine fixes.
   - Is a one-off entry bonus the right shape, or does it just buy a single pointless trip?
   - Should Fracture planets be rewarded terminally, or should the policy learn their value from the
     points and resources they already provide?
   - How would you distinguish "the policy correctly judges the Fracture not worth it in a 4-round
     horizon" from "the policy never learned it exists"? Our horizon may simply be too short for a
     detour that pays off later.
4. Is a 4-round horizon defensible for stage-2 at all, given custodians, the agenda phase and the
   Fracture all live later in a real game?

## What would help most

Ranked opinions rather than exhaustive analysis: what you would do first, what you would drop, and
anything where you think our measurements are answering the wrong question. If you want a different
measurement, say which — we can produce reviewer sessions, cost probes, crossplay evaluations and
single-update timings quickly.
