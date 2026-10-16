# Sol round-1 plan pilot (2026-10-16)

Follow-up to [SOL-STRATEGY-PROBE](SOL-STRATEGY-PROBE-2026-10-16.md). This pilot measures the direction and variance of guide-based round-1 overrides; it produces no labels and makes no claim about strength.
The way guide knowledge would reach the MLP (labels, shaping or curriculum) is deliberately left undecided.

## Design (operator-agreed)

- **Setup:** checkpoint-16448, T=0.001, all six seats MLP, diplomacy ON, DEFAULT/FULL, Train pool.
- **Origins:** 610170001–010.
- **Dice:** 16 dice seeds per arm, the same seeds in every arm.
  - A dice seed offsets the engine's chance streams (dice, decks, exploration) by `dice << 48` words. Map, galaxy and speaker stay fixed.
  - Dice 0 reproduces the earlier probe's diplomacy-ON baseline state hashes exactly (smoke check on 001 and 002).
- **Overrides:** hard overrides on the guide's crucial round-1 decisions. Every MLP is still asked at every choice, so its own stream advances as it would unforced.

| Arm | Override | Claims |
|---|---|---|
| control | none: Sol drafts Leadership and follows secondaries itself | C18, C19 |
| tech-elsewhere | a fixed other seat per origin is forced to draft Technology | table-change control |
| tech-gd | as tech-elsewhere; Sol keeps 1 strategy token + 4 resources until it follows Technology, then researches Gravity Drive | C17, C23 |
| trade | Sol drafts Trade | C21, C22 |
| military-support | whenever Sol's strategy pool is empty, Sol offers Military Support to the richest seat for 1 trade good | C13, C15 |

**Comparisons:**
- tech-gd against tech-elsewhere: isolates Sol's own Gravity Drive choice.
- The other three arms against control.

**Shortcuts:**
- Payment choices have no decline option, so they are logged as reserve leaks rather than blocked.
- In the tech-gd arm, Sol never buys tokens with influence while the reserve is active.

## Run

- **Output:** `out/strategy-wiki/extractions/sol-round1-pilot-20261016-results-01.json` and `-summary-01.json`. Tests passed (3/3). The run took 6m33s on CPU with 16 workers.
- **Failures:** 787 of 810 games completed. All 23 failures are the known 501-ask `AGENDA_REVEALED` engine guard; none was scored.
- **Determinism:** 9/10 repeat checks match. The tenth, origin 004 control dice 0, hit the same agenda guard in both runs, as in the earlier campaign.
- **Runner exit:** 2 for that reason. The report is retained.
- **No-op arm:** at origin 002 the forced Technology seat already drafts Technology naturally, so tech-elsewhere equals control there.
- **Formatting:** `rustfmt --check` reports formatting differences. The source was left unformatted so the report's `source_sha256` still matches it.

## Results (paired by origin × dice; CI = 95% bootstrap over 10 origins)

| Arm vs reference | Pairs | ΔVP | CI | Sign + / − / 0 | ΔGD by R1 | ΔResources R2 | Pair SD | Pairs for 0.3 VP |
|---|---|---|---|---|---|---|---|---|
| tech-gd vs tech-elsewhere | 150 | +0.05 | [−0.31, +0.45] | 43/34/73 | +0.79 | +0.27 | 1.20 | ~126 |
| tech-elsewhere vs control | 147 | −0.22 | [−0.82, +0.23] | 28/54/65 | +0.20 | +1.00 | 1.51 | ~198 |
| trade vs control | 154 | −0.66 | [−1.38, +0.06] | 28/86/40 | 0 | +2.32 | 1.51 | ~199 |
| military-support vs control | 150 | −0.16 | [−0.41, +0.03] | 36/55/59 | 0 | +2.23 | 1.11 | ~108 |

**Mean Sol VP at the round-4 cutoff:**

| Arm | VP |
|---|---|
| control | 3.58 |
| tech-elsewhere | 3.36 |
| tech-gd | 3.41 |
| trade | 2.89 |
| military-support | 3.41 |

**Reading:**
- **Gravity Drive:** forcing it in round 1 works mechanically (100% by end of R1, against 0% in control). The continuing MLP does not turn it into VP; the effect is about zero with a wide CI.
- **Trade instead of Leadership:** the largest and most consistent effect, negative (86 of 154 pairs lose). Sol scores more public objectives (+0.3) but loses more VP elsewhere. This agrees with the MLP's own Leadership preference and with C18.
- **Military Support:** turns into trade goods but not VP; slightly negative, with the CI touching zero.
- **Table change:** forcing another seat onto Technology costs Sol about −0.2 VP on its own. That is why tech-gd is compared with tech-elsewhere, not with control.

## Limits

- 10 origins; the dice seeds within an origin are dependent. Only origin-level CIs carry the inference.
- Pair SD is 1.1–1.5 VP. Detecting 0.3 VP needs roughly 30–40 origins × 16 dice, not more dice per origin.
- Sol's VP stays below the 5.5 VP-per-seat contention threshold, so no claim about competition is made.
- Overrides cover a single decision family each; the MLP plays everything after them. A null result means "this MLP doesn't exploit it", not "the guide is wrong".

## 40-origin confirmation (fresh Train origins 610170011–050)

**Run setup:**
- Arms: control, trade and military-support; 16 dice each; 30 workers.
- Before this run the worker cap was raised from 16 to 32. That is the only source change since the pilot; it changes `source_sha256`.
- The first attempt (16 workers) was stopped at game 133/1960. Its `-confirm40-results-01.log` is kept and has no report.
- Run -02 took 9m50s.

**Outcomes:**
- 1877 of 1960 games completed. All 83 failures are engine no-progress guards (501 asks): 48 on `AGENDA_REVEALED` and 35 on `after SYSTEM_ACTIVATED`, a guard not seen in the pilot. None was scored.
- Determinism: 38/40 repeats match. The two flagged ones (origins 046 and 049) failed identically in both runs, with the same state hash and step count. They are guard hits, not nondeterminism.
- Output: `sol-round1-pilot-20261016-confirm40-results-02.json` and `-summary-02.json`.

| Arm vs control | Origins | Pairs | ΔVP | 95% CI (origins) | Sign + / − / 0 | ΔObjectives | ΔRank |
|---|---|---|---|---|---|---|---|
| trade | 38 | 587 | −0.87 | [−1.22, −0.53] | 114/373/100 | −0.13 | +0.91 (worse) |
| military-support | 38 | 597 | +0.01 | [−0.26, +0.27] | 180/196/221 | +0.17 | 0.00 |

**Reading:**
- **Trade:** the pilot's Trade-instead-of-Leadership signal holds on fresh origins, and the CI now excludes zero. For this MLP, Leadership (its natural pick and C18) beats the guide's Trade line (C21) by about 0.9 VP at the round-4 cutoff.
- **Military Support:** forced selling (C13/C15) is neutral.

Neither result is a training label. The Trade result says the MLP's existing draft choice is right here; it does not show that Trade is bad in human play.

## Rerun after the timing-window fix (results-04, authoritative)

**Engine change** (`crates/ti4-engine/src/timing.rs`, uncommitted; operator rule):
- A seat that passes in two consecutive passes of a window closes it.
- A `reaction:` slot that leaves the seat's hand unchanged (opened, then backed out of) counts as a pass.

The first part alone (results-03) still left 61 loops, because the looping seat was "resolving" a no-op slot rather than passing. With both parts, all ti4-engine library tests pass (2961), including the new regression test.

**Run:** 40 origins × 3 arms × 16 dice; 1960/1960 complete with 0 guard failures; 40/40 determinism checks match. Took 9m59s on 30 workers.
- Output: `-confirm40-results-04.json` and `-summary-04.json`.

| Arm vs control | Pairs | ΔVP | 95% CI (40 origins) | Sign + / − / 0 |
|---|---|---|---|---|
| trade | 640 | −0.79 | [−1.13, −0.46] | 141/393/106 |
| military-support | 640 | +0.06 | [−0.19, +0.33] | 200/190/250 |

These match results-02 (−0.87 / +0.01), so excluding guard-failure games had not biased the earlier read-out.
