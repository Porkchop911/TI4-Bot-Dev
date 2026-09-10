# VP sources and concrete training opportunities — 2026-09-09

## Measured result

The improvement is chiefly public-objective scoring, concentrated in spending and structures. It is not a general improvement in secret-objective or Mecatol play. The next experiments should test objective-conditioned preparation and secret selection, while retaining the simpler reward recipe as the current baseline. No training or production behavior was changed in this investigation.

720 candidate games per checkpoint: 20 Validation seed blocks 910001000–910001019, six rotations × six candidate seats, four-round horizon. Candidate and all five frozen opponents use temperature 0.001. Opponents: checkpoint-473312. Start: checkpoint-241428. New candidate: arm-vponly/checkpoint-43992. These are the same 20 blocks as the temperature sweep, not a new independent confirmation.

| VP source per candidate game | Start | vponly | Difference |
|---|---:|---:|---:|
| Public objectives, status | 1.5042 | 1.5778 | +0.0736 |
| Public objectives, Imperial | 0.3681 | 0.4333 | +0.0653 |
| Secret objectives, status | 0.5333 | 0.5389 | +0.0056 |
| Secret objectives, event windows | 0.0847 | 0.0833 | -0.0014 |
| Custodians | 0.1403 | 0.1278 | -0.0125 |
| Mecatol point from Imperial | 0.1056 | 0.1042 | -0.0014 |
| Support awards, gross | 0.9750 | 1.0083 | +0.0333 |
| Other recorded awards, net | 0.0736 | 0.0500 | -0.0236 |
| Unattributed net adjustment | -0.0139 | -0.0264 | -0.0125 |
| **Total actual VP** | **3.7708** | **3.8972** | **+0.1264** |

**Attribution limit:** objective awards are read from changes in scored-objective sets, with printed values from the content corpus, and matched to actual scoring choices. Other awards use the VP ledger. The ledger omits some mutations: event-secret scoring bypasses it, and `return_support` removes a point without a ledger entry. Card tracking recovers the former. The remaining discrepancy is explicitly retained above: 16 start games and 33 vponly games have a ±1 residual, net −10 and −19 points respectively. Do not call gross Support awards net retained Support, or assign the whole residual to Support without replay evidence. Public/secret labels identify the card corpus; a secret promoted to public by an agenda would require separate window classification.

The exact total means reproduce the previous frozen cross-play evaluation. Total objective points increase from 1,793 to 1,896 (+103), while total VP increases by 91. Public objectives explain +100 of those objective points; secrets explain only +3.

## Specific cards and areas of play

All counts below are scoring occurrences across 720 candidate games per arm. Every scored card in this panel is worth one point. These are **not conversion rates conditional on being revealed or drawn**; the diagnostic does not record that exposure denominator.

| Area | Named objective and requirement | Start scores | vponly scores |
|---|---|---:|---:|
| Saving resources / tokens | **Erect a Monument** — Spend 8 resources. | 121 | 196 |
| Saving resources / tokens | **Lead From the Front** — Spend a total of 3 tokens from your tactic and/or strategy pools. | 184 | 192 |
| Saving resources / tokens | **Amass Wealth** — Spend 3 influence, 3 resources, and 3 trade goods. | 99 | 104 |
| Saving resources / tokens | **Negotiate Trade Routes** — Spend 5 trade goods. | 94 | 100 |
| Saving resources / tokens | **Sway the Council** — Spend 8 influence. | 115 | 110 |
| Structures / placement | **Build Defenses** — Have 4 or more structures. | 119 | 154 |
| Structures / placement | **Improve Infrastructure** — Have structures on 3 planets outside of your home system. | 19 | 36 |
| Structures / placement | **Fuel the War Machine** — Have 3 space docks on the game board. | 52 | 59 |
| Structures / placement | **Establish a Perimeter** — Have 4 PDS units on the game board. | 2 | 11 |
| Expansion / positioning | **Push Boundaries** — Control more planets than each of 2 of your neighbors. | 160 | 149 |
| Expansion / positioning | **Expand Borders** — Control 6 planets in non-home systems. | 46 | 55 |
| Expansion / positioning | **Corner the Market** — Control 4 planets that each have the same planet trait. | 67 | 74 |
| Expansion / positioning | **Found Research Outposts** — Control 3 planets that have technology specialties. | 57 | 56 |
| Expansion / positioning | **Intimidate Council** — Have 1 or more ships in 2 systems that are adjacent to Mecatol Rex's system. | 68 | 76 |
| Expansion / positioning | **Make History** — Have units in 2 systems that contain legendary planets, Mecatol Rex, or anomalies. | 56 | 34 |
| Technology paths | **Develop Weaponry** — Own 2 unit upgrade technologies. | 29 | 9 |
| Technology paths | **Diversify Research** — Own 2 technologies in each of 2 colors. | 28 | 24 |
| Technology paths | **Adapt New Strategies** — Own 2 faction technologies. 'Valefar Assimilator' technologies do not count toward this objective. | 2 | 0 |
| Secret planning / cards | **Form a Spy Network** — Discard 5 action cards. | 30 | 15 |
| Secret planning / cards | **Strengthen Bonds** — Have another player's promissory note in your play area. | 62 | 77 |
| Combat / expensive fleets | **Unveil Flagship** — Win a space combat in a system that contains your flagship. You cannot score this objective if your flagship is destroyed in the combat. | 2 | 2 |
| Combat / expensive fleets | **Gather a Mighty Fleet** — Have 5 dreadnoughts on the game board. | 1 | 0 |
| Combat / expensive fleets | **Betray a Friend** — Win a combat against a player whose promissory note you had in your play area at the start of your tactical action. | 2 | 0 |

Erect a Monument alone adds 75 scores (+0.1042 VP/game), compared with the total net gain of 91 points. That is an accounting contribution, not proof of which training change caused it. The five listed spend objectives supply 702 points (0.975/game), up from 613. The four listed structure objectives supply 260 (0.361/game), up from 192. Other sources offset part of these gains.

The policy changes strategy mix substantially: Construction rises from 185 to 905 picks (6.4% → 31.4% of 2,879 picks); Imperial from 482 to 685 (16.7% → 23.8%). Technology falls from 294 to 123 and Trade from 740 to 365. This supports a specialization hypothesis, not a recommendation to hard-code Construction.

## Stage I conversion when revealed

The denominator here is candidate-game exposure: one objective revealed in one candidate game is one
opportunity, and the numerator is whether that candidate scored that objective by the end of the
four-round game. All Stage I objectives in this table were revealed early enough to have at least one
normal scoring opportunity. The Stage II cards revealed after round four's scoring step are excluded.
Thus, for this table, literal `scored / revealed` and opportunity-adjusted conversion are identical.

These are descriptive rates from 20 map/seed clusters. The six rotations and six candidate positions
within a seed are correlated, and the objective exposure mix is not uniform. A rate does not say how
close failed games came to satisfying the requirement, nor whether pursuing the objective would have
been the best available plan.

| Area | Stage I objective | Start scored / revealed | Start | vponly scored / revealed | vponly | Change |
|---|---|---:|---:|---:|---:|---:|
| Economy / spending | Erect a Monument | 121 / 216 | 56.0% | 196 / 216 | **90.7%** | **+34.7 pp** |
| Economy / spending | Sway the Council | 115 / 216 | 53.2% | 110 / 216 | 50.9% | −2.3 pp |
| Economy / spending | Negotiate Trade Routes | 94 / 216 | 43.5% | 100 / 216 | 46.3% | +2.8 pp |
| Economy / spending | Amass Wealth | 99 / 144 | 68.8% | 104 / 144 | **72.2%** | +3.5 pp |
| Economy / spending | Lead From the Front | 184 / 252 | 73.0% | 192 / 252 | **76.2%** | +3.2 pp |
| Technology | Develop Weaponry | 29 / 144 | 20.1% | 9 / 144 | **6.2%** | **−13.9 pp** |
| Technology | Diversify Research | 28 / 144 | 19.4% | 24 / 144 | 16.7% | −2.8 pp |
| Planet / area control | Expand Borders | 46 / 108 | 42.6% | 55 / 108 | **50.9%** | +8.3 pp |
| Planet / area control | Corner the Market | 67 / 144 | 46.5% | 74 / 144 | **51.4%** | +4.9 pp |
| Planet / area control | Found Research Outposts | 57 / 180 | 31.7% | 56 / 180 | 31.1% | −0.6 pp |
| Planet / area control | Discover Lost Outposts | 10 / 72 | 13.9% | 13 / 72 | 18.1% | +4.2 pp |
| Planet / area control | Push Boundaries | 160 / 252 | 63.5% | 149 / 252 | **59.1%** | −4.4 pp |
| Board position / spatial | Intimidate Council | 68 / 287 | 23.7% | 76 / 286 | 26.6% | +2.9 pp |
| Board position / spatial | Explore Deep Space | 0 / 180 | **0.0%** | 0 / 180 | **0.0%** | 0.0 pp |
| Board position / spatial | Populate the Outer Rim | 19 / 72 | 26.4% | 13 / 72 | 18.1% | −8.3 pp |
| Board position / spatial | Make History | 56 / 396 | 14.1% | 34 / 396 | **8.6%** | −5.6 pp |
| Military / fleet | Raise a Fleet | 6 / 108 | 5.6% | 7 / 108 | **6.5%** | +0.9 pp |
| Military / fleet | Engineer a Marvel | 51 / 108 | 47.2% | 46 / 108 | 42.6% | −4.6 pp |
| Infrastructure / production | Build Defenses | 119 / 216 | 55.1% | 154 / 216 | **71.3%** | **+16.2 pp** |
| Infrastructure / production | Improve Infrastructure | 19 / 144 | 13.2% | 36 / 144 | **25.0%** | **+11.8 pp** |

Aggregating exposures within each area gives a compact view of the policy's play style. These pooled
rates weight objectives by how often this fixed panel revealed them.

| Area | Start | vponly | Change |
|---|---:|---:|---:|
| Economy / spending | 58.7% | **67.2%** | **+8.5 pp** |
| Infrastructure / production | 38.3% | **52.8%** | **+14.4 pp** |
| Planet / area control | 45.0% | 45.9% | +0.9 pp |
| Military / fleet | 26.4% | 24.5% | −1.9 pp |
| Board position / spatial | 15.3% | **13.2%** | −2.1 pp |
| Technology | 19.8% | **11.5%** | **−8.3 pp** |

The strongest specific conversion is Erect a Monument at 90.7%; the clearest complete failure is
Explore Deep Space at 0/180. The weakest broader areas are technology and spatial positioning.
`vponly` improved by becoming much better at one resource objective and the structure family, while
technology and several spatial objectives regressed. This is stronger evidence for an
objective-conditioned curriculum than for simply continuing the current specialization unchanged.

## Ranked areas of improvement

1. **Coordinate spending and a public score each round.** The clearest existing strength is converting resources and tokens into public points. Improve the sequence: identify a revealed affordable target, reserve its actual payment, and schedule other spending and Imperial around it. Erect a Monument needs 8 resources; Sway the Council 8 influence; Lead From the Front 3 tactic/strategy tokens; Amass Wealth needs all three payment types. Training examples should distinguish spending that completes a scoring plan from spending that consumes its last required resources. Do not introduce a generic reward for hoarding.

   There are 1,145 status windows with a public offered and 1,136 public selections: **99.2% conversion when offered**, versus only 1.578 status public scores/game. Round-specific public selections are 35, 266, 412, 423. In round four, only 423/720 candidate games score a public through status. The main gap is reaching the scoring window with an eligible unscored card, not the final button press. Four status scores is a nominal ceiling, not an estimate of recoverable points: revelation, earlier Imperial scores, payments, home-system control and game end constrain it.

2. **Select feasible secrets and prepare them deliberately.** Secrets yield 0.622 VP/game, nearly unchanged from 0.618, while 2.058 secrets remain in hand on average (start 2.049). Prioritize retention/discard decisions and trajectories toward an already held secret. Form a Spy Network falls 30 → 15 scores and remains held in 66 final hands: inspect whether action-card spending prevents reaching five cards. Mine Rare Metals is scored 12 times and held unscored 60 times: inspect hazardous-planet targeting. Adapt New Strategies is never scored and remains held 60 times: inspect whether the two faction technologies are feasible within the remaining horizon before retaining it. These are investigation targets, not verified near-completions; final hands can contain recent draws.

   Do not force combat merely to clear a difficult secret. Gather a Mighty Fleet (five dreadnoughts), Unveil Flagship (win with surviving flagship), and Betray a Friend (combat against a qualifying note partner) are held unscored 68, 59 and 59 times, but scored 0, 2 and 0 times. Better selection/discard may beat pursuing these expensive or contingent plans in a four-round episode. Scoreable secrets are usually taken: 388 selections from 407 eligible status windows.

3. **Make the structures approach conditional and exploit overlap.** Build Defenses, Improve Infrastructure, Fuel the War Machine and Establish a Perimeter cover different placement/type requirements. Four structures alone does not imply three space docks, four PDS, or structures outside home. Train joint public-plus-secret plans when the relevant cards are visible/held, and compare Construction choices when no structure objective is relevant. The current frequency jump may be useful specialization, or may be excessive in other objective draws. Scoring counts alone cannot decide.

4. **Recover purposeful technology development.** Develop Weaponry drops 29 → 9, Diversify Research 28 → 24, and Adapt New Strategies 2 → 0. Test objective-conditioned technology paths and faction-specific examples; do not restore a flat reward for owning technology. The latter could recreate the auxiliary-return problem that motivated vponly. These cards require particular upgrades/colors/faction technologies, not a generic tech count. This is a plausible secondary improvement, not evidence that every game should buy more technology.

5. **Coordinate Imperial with an actual scoring plan; defer broad combat emphasis.** Imperial public points rise 0.368 → 0.433, but Mecatol Imperial points stay about 0.104/game and custodians decline slightly. There are 685 Imperial drafts and 312 public scoring offers; 380 draft-rounds have no recorded Imperial public offer (counts are not a strict one-use funnel because other effects can produce offers). A pick can still be valuable for its secret draw or Mecatol point, so this is a targeting diagnostic, not 380 wasted picks. Test timing/selection given scoreability, available secrets and Mecatol control. For military improvement, start with objective-directed positioning such as Intimidate Council and specific expansion targets; current data do not establish that generic aggression will produce more VP.

6. **Small targeted correction: round-one score refusals.** vponly declines 29 of 1,925 scoring asks; all 29 occur in round one. Fifteen are Strengthen Bonds. Accepting every offered card represents only 29 immediate one-point opportunities across 720 games (0.0403/game), not a guaranteed final-VP improvement. Some may be scored later, and changing a decision changes the subsequent game. Inspect these examples for training interference, but the measured budget is much smaller than the eligibility gap.

## What this suggests for training

Keep the checkpoint surface fixed. The first bounded experiment should compare the current vponly recipe against training that gives more learning coverage to **existing** scoring-related trajectories: saving for a named public, choosing/retaining an achievable secret, and completing overlapping structure objectives. Use properly on-policy sampling or a separately specified imitation objective; naively oversampling PPO rows changes the optimization objective. Reward design should remain tied to actual points, with any added shaping tested for terminal bias.

Evaluate paired seeds against frozen opponents and also a varied opponent panel, reporting the above source/card breakdown alongside VP. Roughly one point/game is gross Support awards; the engine offers reciprocal Support swaps. That is a legitimate source, but a high rate against one cooperative benchmark should not be mistaken for general objective competence. The next comparisons need both training-seed replication and broader objective/map exposure. Do not pick a recipe solely for raising one card count.

None of these interventions has an established causal VP uplift from this census. The measured reward-arm gain on these 20 maps is +0.1264; the larger existing 100-map greedy comparison found +0.1450. No extra training was run here. Four rounds restrict the relevance of expensive combat/technology plans and later stage-II objectives; this panel has no two-point objective scores. A longer horizon is a separate training experiment, not an automatic fix.

## Evidence and scope

New diagnostic: `crates/ti4-mlp/examples/vp_sources.rs`. Initial generated evidence:
`out/vp-sources-20260909/{start,vponly,verify}.jsonl`, `summary.json`, `analyse.py`,
`manifest.json`, and `vp_sources-frozen.exe`. The reveal-denominator extension generated
`{start,vponly}-reveals.jsonl`, `reveals-verify.jsonl`, and
`vp_sources-reveals-frozen.exe` in the same directory. The initial manifest hashes its executable,
diagnostic, bundles, map pool and initial raw outputs; it predates the reveal-denominator extension.
CPU libtorch 2.9.1, 24 Rayon workers; no ppo_update.exe was present at the initial process check. No
CUDA job was launched. Shared tree was dirty; existing source files were read only. No staging,
commits, resets or fixture regeneration.

Validation: release build succeeds; both diagnostic versions each replay 36 candidate games with
capture enabled/disabled and assert byte-identical ordered-choice, event and final-state SHA-256
hashes (108 comparisons per version). Both reveal-aware 720-game arms finish without inference errors
or step-cap failures and reproduce the exact earlier means, 3.770833 and 3.897222. One game per arm
ends before the fourth strategy draft; it is a genuine game end, not truncation. No production
optimization or training modification is claimed; the full unrelated engine suite was not rerun.

The wrapper delegates unchanged production MLP decisions. Scored-set differences establish awards, rather than assuming every selected card paid successfully. Per-round tracking reads state after each step and never mutates game state. Scoring-window subtype separates status and event-secret awards. Hash evidence proves this observer is inert on its replay sample; it does not prove completeness of the VP ledger.

## Full scored-card census

| Objective | Deck | Start | vponly | Delta |
|---|---|---:|---:|---:|
| Erect a Monument | Public | 121 | 196 | +75 |
| Lead From the Front | Public | 184 | 192 | +8 |
| Build Defenses | Public | 119 | 154 | +35 |
| Push Boundaries | Public | 160 | 149 | -11 |
| Sway the Council | Public | 115 | 110 | -5 |
| Amass Wealth | Public | 99 | 104 | +5 |
| Negotiate Trade Routes | Public | 94 | 100 | +6 |
| Strengthen Bonds | Secret | 62 | 77 | +15 |
| Intimidate Council | Public | 68 | 76 | +8 |
| Corner the Market | Public | 67 | 74 | +7 |
| Fuel the War Machine | Secret | 52 | 59 | +7 |
| Found Research Outposts | Public | 57 | 56 | -1 |
| Expand Borders | Public | 46 | 55 | +9 |
| Engineer a Marvel | Public | 51 | 46 | -5 |
| Improve Infrastructure | Public | 19 | 36 | +17 |
| Make History | Public | 56 | 34 | -22 |
| Destroy Heretical Works | Secret | 39 | 29 | -10 |
| Hoard Raw Materials | Secret | 26 | 28 | +2 |
| Establish Hegemony | Secret | 25 | 28 | +3 |
| Threaten Enemies | Secret | 28 | 27 | -1 |
| Dictate Policy | Secret | 21 | 27 | +6 |
| Diversify Research | Public | 28 | 24 | -4 |
| Seize an Icon | Secret | 14 | 21 | +7 |
| Learn the Secrets of the Cosmos | Secret | 19 | 19 | +0 |
| Foster Cohesion | Secret | 20 | 17 | -3 |
| Form a Spy Network | Secret | 30 | 15 | -15 |
| Discover Lost Outposts | Public | 10 | 13 | +3 |
| Populate the Outer Rim | Public | 19 | 13 | -6 |
| Monopolize Production | Secret | 14 | 12 | -2 |
| Mine Rare Metals | Secret | 5 | 12 | +7 |
| Prove Endurance | Secret | 9 | 12 | +3 |
| Establish a Perimeter | Secret | 2 | 11 | +9 |
| Stake Your Claim | Secret | 17 | 11 | -6 |
| Drive the Debate | Secret | 13 | 10 | -3 |
| Develop Weaponry | Public | 29 | 9 | -20 |
| Cut Supply Lines | Secret | 4 | 9 | +5 |
| Raise a Fleet | Public | 6 | 7 | +1 |
| Produce en Masse | Secret | 6 | 6 | +0 |
| Turn Their Fleets to Dust | Secret | 5 | 4 | -1 |
| Spark a Rebellion | Secret | 4 | 3 | -1 |
| Occupy the Seat of the Empire | Secret | 2 | 3 | +1 |
| Forge an Alliance | Secret | 12 | 3 | -9 |
| Unveil Flagship | Secret | 2 | 2 | +0 |
| Control the Region | Secret | 4 | 1 | -3 |
| Become a Martyr | Secret | 0 | 1 | +1 |
| Make an Example of Their World | Secret | 4 | 1 | -3 |
| Betray a Friend | Secret | 2 | 0 | -2 |
| Adapt New Strategies | Secret | 2 | 0 | -2 |
| Demonstrate Your Power | Secret | 1 | 0 | -1 |
| Gather a Mighty Fleet | Secret | 1 | 0 | -1 |

## Most frequently held unscored secrets (vponly)

Counts are final hands, not failed scoring offers, draw-normalized difficulty or guaranteed recoverable VP.

| Secret | Final hands | Requirement |
|---|---:|---|
| Gather a Mighty Fleet | 68 | Have 5 dreadnoughts on the game board. |
| Form a Spy Network | 66 | Discard 5 action cards. |
| Mine Rare Metals | 60 | Control 4 hazardous planets. |
| Adapt New Strategies | 60 | Own 2 faction technologies. 'Valefar Assimilator' technologies do not count toward this objective. |
| Unveil Flagship | 59 | Win a space combat in a system that contains your flagship. You cannot score this objective if your flagship is destroyed in the combat. |
| Betray a Friend | 59 | Win a combat against a player whose promissory note you had in your play area at the start of your tactical action. |
| Destroy Their Greatest Ship | 52 | Destroy another player's war sun or flagship. |
| Occupy the Fringe | 51 | Have 9 or more ground forces on a planet that does not contain 1 of your space docks. |
| Occupy the Seat of the Empire | 48 | Control Mecatol Rex and have 3 or more ships in its system. |
| Darken the Skies | 48 | Win a combat in another player's home system. |
| Fight with Precision | 44 | Destroy the last of a player's fighters in the active system during the anti-fighter barrage step. |
| Spark a Rebellion | 43 | Win a combat against a player who has the most victory points. |
| Cut Supply Lines | 42 | Have 1 or more ships in the same system as another player's space dock. |
| Learn the Secrets of the Cosmos | 41 | Have 1 or more ships in 3 systems that are each adjacent to an anomaly. |
| Drive the Debate | 40 | You or a planet you control are elected by an agenda. |
| Mechanize the Military | 40 | Have 1 mech on each of 4 planets. |
| Brave the Void | 37 | Win a combat in an anomaly. |
| Stake Your Claim | 37 | Control a planet in a system that contains a planet controlled by another player. |
| Make an Example of Their World | 36 | Destroy the last of a player's ground forces on a planet during the bombardment step. |
| Turn Their Fleets to Dust | 36 | Destroy the last of a player's non-fighter ships in the active system during the space cannon offense step. |
