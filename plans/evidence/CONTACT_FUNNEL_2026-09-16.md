# Contact funnel under training conditions — 2026-09-16

Measured with `crates/ti4-mlp/examples/contact_funnel.rs` (new, uncommitted): the trainer's regime,
not a reviewer session. Same bundle, pool, seating and horizon as a PPO update; counters live in the
deciders, so nothing is written to disk.

`checkpoint-212544`, `full_np8_12_train.json`, 4 rounds, temperature 2.5, 4 seed blocks x 6
rotations = 24 games per run, no errors, no truncation.

| | block A (base 1261000000) | block B (base 1261000100) |
|---|---|---|
| contact options offered in action decisions | 44,383 | 43,986 |
| action decisions listing a contact | 14,421 | 14,251 |
| contacts opened | 13,487 | 13,313 |
| **menu with nothing admissible** | 5,964 (44.2%) | 6,208 (46.6%) |
| menu with signals only | 1,599 (11.9%) | 1,674 (12.6%) |
| menu with deals available | 5,924 (43.9%) | 5,431 (40.8%) |
| proposed a deal | 4,429 (32.8%) | 4,006 (30.1%) |
| sent a signal | 2,589 (19.2%) | 2,570 (19.3%) |
| **declined a menu that had deals** | 185 (1.4%) | 209 (1.6%) |
| responses | 9,037 | 8,144 |
| accepted / countered / refused | 3,867 / 4,608 / 562 | 3,411 / 4,138 / 595 |
| bundles, signals per opened contact | 3.7, 0.7 | 3.0, 0.7 |
| largest menu | 36 | 35 |

## What it corrects

- **The earlier "86% of contacts produce nothing" was wrong.** That figure came from one round at
  temperature 0.5 and merged three categories. The genuinely dead share -- no bundles and no signals
  -- is 44-47%. Voluntary declines of a menu that had deals are 1.4-1.6%, so Astra's concern that a
  filter would remove learning opportunities is a small risk, but the removable share is also about
  half what was claimed.
- **The policy does refuse deals.** 6.2-7.3% of responses are refusals, and counters (51%) outnumber
  accepts (43%). The earlier "never refuses, 0 of 57" came from greedy and temperature-0.5 samples,
  not the training regime.

## What it implies for the entry filter

Scaling 24 games to an update's 96: dead menus are roughly 24,000 decisions against a measured
diplomacy surcharge of about 117,000 decisions per update (237,220 with diplomacy against 119,832
without). That is about a fifth of the surcharge and a tenth of all decisions -- on the order of 2-3
seconds of a 33.8-second update, not a halving.

A second, unmeasured saving: about 44,000 dead contact options per 24 games also sit in action
decision menus, where every option is scored. Per-option inference cost is not captured by decision
counts.

Astra's constraint stands: suppression must preserve `TRANSACTION_OPENED`, the timing window,
Black Market widening and the transaction allowance, and must not leak the other seat's private
cards. That is a legality change needing review, not a filter on the candidate list.
