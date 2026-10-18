# Four-versus-six-player bot investigation - 2026-10-13

Operator asks whether the large four-player gap and small historical six-player gap are a player-count effect, and permits detailed focus on Hacan, Sol, Jol-Nar, L1Z1X, Letnev (operator typo letnex interpreted as Letnev), Xxcha. Cutoff remains after round four. New subagents restricted by operator to Luna/Terra; read-only audit/reviewer used Luna only. No new Sol agent spawned; older review history is retained.

## Final finding

The original four-versus-six comparison did not hold diplomacy fixed. The original six-player sample was diplomacy ON; the four-player server benchmark was OFF. A fresh diplomacy-enabled old-protocol run reproduced **all288 original first24-seed rows exactly**. On the same seeds/pool/bundle/round4, switching diplomacy OFF changes MLP-minus-rule from +.32639 to -1.34028VP/seat. MLP gains+1.63194VP(SE.11349) when enabled, while rule changes-.03472(SE.13936).

The large gap does not persist as a player-count distinction when matching that setting. Same21 successful core-faction seeds across the four conditions:

| After round4, VP/seat | Rule | MLP | Rule minus MLP |
|---|---:|---:|---:|
| Four players, diplomacy OFF | 5.2738 | 3.1429 | +2.1310 |
| Six players, diplomacy OFF | 4.3571 | 2.1825 | +2.1746 |
| Four players, diplomacy ON | 5.1310 | 5.0476 | +.0833 |
| Six players, diplomacy ON | 3.9603 | 4.3016 | -.3413 |

All24 ON pairs also show four near-parity (5.0417/5.0104, gap+.03125SE.21562) and six small MLP lead (3.9167/4.2847, gap-.36806SE.22207); neither ON campaign had a failure. ON/OFF four compares common21 pairs because OFF lost3seeds toguards; both six arms have24 complete pairs. Common21 player-count change in rule advantage: OFF -.04365SE.28181; ON+.42460SE.31820. These do not establish exact equivalence or rule out a modest count interaction, but they do not support the original large four-only penalty. Do not describe a small six-player MLP lead as a proven universal advantage.

### Mechanistic interpretation

The intervention changes the whole structured-diplomacy subsystem, not one feature alone: optional contacts, offers/counters/payment promises, political talks and relationship/deal facts become available to the model and rules. For same24 six-core pairs, enabling diplomacy adds MLP+2.14583VP(SE.17298), while rule falls-.36111(SE.12273). At four on common21 it addsMLP+1.90476(SE.20228), rule-.14286(SE.17215).

MLP six-core mean public-objective nominal points rise1.5833->2.1042, secretpoints .4444->.4583, and VP not accounted for by nominal scored-objective IDs rises.1111->1.7222. About1.61 of the2.15VP gain is outside that ledger. This residual is not itself a Support counter: other non-objective awards/losses can contribute.

Actual journals independently support a concrete path: applied Support for the Throne note transfers to MLP vs rule are226vs128 across48 six-core ON games; four ON games134vs97. Decoder follows each deal's latest offered/countered revision and immediate_applied combined proposer/recipient term index. These are **cumulative transfer events**, not net held Support or exact awarded VP. Source promissory.rs:20 and343-386 explicitly scores incoming Support and removes the point when returned. This shows a real scoring route, not merely correlation with a generic diplomacy flag. Other negotiated resource effects can improve objective conditions; no narrower deal/feature/Support-disabled intervention was done, so the exact decomposition of the flag effect remains unisolated.

Expansion deficit remains even when diplomacy closes VP gap. Full24 six ON: rule11.98planets vsMLP6.97, fleet printedcost20.09vs12.70, ground12.80vs9.43. Four ON: rule15.18planets vsMLP9.94, fleet22.89vs17.54. Consequently this checkpoint's competitive VP under diplomacy is not evidence of matching the rule bot's expansion/force development. Both bots usually take scoring opportunities they are actually offered; the OFF deficit reflects fewer opportunities/state achievements rather than habitual refusal of available VP. Leadership/Imperial differences detailed below remain observational pathways, not a card-value causal claim.

### Completeness, checks and reproducibility

Nine new48-attempt arms: core4OFF, core6OFF, curated6OFF, core6per-seatOFF, legacy-pool core6OFF, wide-pool6OFF, wide-pool6ON, core4ON, core6ON. Total432 attempted games,7guard failures retained,425 usable outcomes;210 complete seed pairs across arms. Failures are4core4OFF,1curated6OFF,2legacycoreOFF. No failures in the decisivewide controls or matchedON arms. The latter use one checkpoint/temp/cutoff and24balanced core seeds percount, then strictsame21seed matrix avoids exclusion mismatch. All are cutoff standings, not game wins.

CPU only, Cargo-j4/Rayon8, pinned CPU libtorch; noGPU/network/engine/model edits. Two tests and build passed after final toggle; exactoriginal4seed1round4 remains4/3/4/7, core faction/speaker balance regression passes. Formatting/diff checks passed. Independent reviews and recomputation performed by Luna only in this investigation, honoring operator restriction. Original artifacts never overwritten; new filenames preserve every raw log/output and each runner/settings revision.

Authoritative artifacts in existingout/rule-bot: four-v-six-20261013-factorial-common-summary.json, diplomacy-summary.json, diplomacy-forensics.json, primary-summary.json, counter-summary.json, corresponding9rawarm files/logs, campaign/settings/exit manifests and finalresults checksum manifest. The original wide first24 reconstructions useexact pool bytes (examples/reviewer pool hash equals main out/pools). A renamed independent manuscript is unnecessary; this evidence is the persistent report.

**Historical investigation notes below:** interim hypotheses and pending-status passages are superseded by the completed Final finding above. All nine experiment arms and the final Luna review are complete.

## Initial hypotheses and controls

1. Protocol effect: old six-player rollout uses DEFAULT/FULL source scope and Python validation map pool, while server benchmark uses POK setup/decks/game, generated boards, and removes map-impossible objectives/secrets. Prediction: six players under the four-player server protocol need not reproduce the old near-equality.
2. Faction coverage: old wide includes TE/new factions, new server roster is curated POK. Prediction: fixed six well-trained factions may shift relative scores compared with broad roster; core comparisons remove poor faction coverage as an explanation.
3. Player-count/map crowding: fewer seats share the generated map, changing expansion/objective availability and competition. Prediction: controlled core four-versus-six changes per-seat holdings/scoring even with the same checkpoint, source scope, cards and seed family.
4. Rule fallback stream: old rule decider uses per-seat decision count; server rule decider uses global count advanced by all seats. Prediction: a per-seat-counter ablation on otherwise identical positions can measure whether this explains the gap; until tested, do not assert quantitative effect.

Primary matching: same checkpoint 16448 as earlier, CPU temperature0.001, four-round horizon, server POK setup, explicit TE strategy cards, generated map family, speaker seed modulo players, all seats counted, alternating bot types and both swaps. Core roster rotates [hacan,sol,jolnar,l1z1x,letnev,xxcha] using zero=seed-1 and rotation=(zero%6+floor(zero/6)%6)%6. This block rotation balances faction identities and six-player speaker roles over seeds1..24. Four-player subset is firstfour of corresponding six-player assignment; seeds1..24 balance each faction16times at4 and24times at6 before swaps. Two versus two / three versus three, as appropriate. Twenty-four paired seeds for each new core comparison; separate six-player curated-roster control can use the exact first24 seeds of prior four-player data. No claim of holding an identical map between different player counts: home placement/player occupancy necessarily changes; generated family/seed held fixed.

New telemetry is factual and does not alter choices: map systems/planets, objective score ledger and reveals, chosen decisions keyed player|round|subtype|offeredcount, existing round holdings. Printed fleet metrics retain prior caveats. MLP errors/unused statuses remain failures; cutoff positions and completepairs only qualify. Decision stats can distinguish declining a scoreable objective (offer >1) from lacking any eligible objective, and strategycard selection/technology/action patterns.

## Verified protocol confounds from Luna source audit

Old rollout: Game.with_sources(DEFAULT=FULL), initial setup FULL, pool map FULL. Server: setup and map/deploy POK; Game.with_table defaults POK. TE strategycards are selected in both, but fullsource adds TE action/relic/tile/faction universe. Public/secretobjective catalogs have no TE-source records, so no invented TE-objective catalog effect. Server map_feasibility.apply filters map-impossible objectives/secrets and redraws public objectives/replaces impossible initial secrets; training seated path lacks this call. Curated server roster excludes TE factions. Initial MLP baseline built with DEFAULT against POK state is a confirmed source mismatch in original runner, though countfields may be unaffected; no live-MLP observation-wide source claim made.

Historical six-player raw artifact lacks invocation flags, maps, error details and explicit horizon. Three missing games mean it is incomplete. Its observed +0.273 VP advantage forMLP is not a clean player-count control.

## Resources / authorization

P0/P1/P2: existing benchmark example/tests, new local evidence/state/index records and bounded ignored outputs in existing out/rule-bot. No new directories, dependencies, engine changes, guard weakening, GPU, network, remote actions or Python reference access. Checkpoint/libtorch read from main repo; writes only rule-bot worktree. Cargo-j4 / Rayon8 preserves memory headroom. Per-game default cutoff4, cap200000steps/300seconds; unfinished cutoff is an error. Output filenames create_new; expected percampaign summaries/telemetry under20MB. Cancellation only owned process. One Cargo coordinator.

## Historical checks and progress

- Regression exact old four-player seed1round4 scores unchanged: PASS.
- Core subset/rotation balance regression: PASS; all six factions16four-seatappearances across24seeds, six assignment hasall6, fourprefixequals6prefix.
- `cargo test -j4 -p ti4-mlp --example four_player_bench -- --test-threads=8`: 2/2 PASS, 3.76seconds; existing deadcodewarning unrelated.
- Build PASS; core6smoke running. Source/settings/hash manifests and full logs retained for every new campaign; no results invented.

## Historical next steps (completed)

Finish six-player core smoke, run matched core4/core6 and six-curated controls; measure completepairs/initial hashes and selected mechanisms. Run a focused counter or feasibility ablation only if results warrant it. Independent review by Luna and actual outcomes pending. Preserve prior cutoff results and recurring-loop bug report; compute reservation held only for this package.

Primary campaigns finished: core4 48attempts/44valid/21completepairs/4guards; core6 48/48/24pairs/0guards; curated6 48/47/23pairs/1guard. Paired rule-minusMLP VP: core4 +2.13095 (SE.27256), core6 +2.13889 (SE.11642), curated6 +2.03623 (SE.17750). These preliminary computations must be independently checked; four-versus-six common-seed comparison still pending. Results warrant two focused controls: six-core per-seat counter, and fresh legacy-style pool six-core experiment with false diplomacy and the same bundle/temp/round4. No engine or policy change.

## Controlled results and independently checked mechanisms (pre-forensic)

Luna independently re-read all primary raw JSONL, validated pair initial hashes, and recomputed metrics. Common21 core seeds avoid unequal failure exclusion:

| Core setup | Rule VP/seat | MLP VP/seat | Rule advantage | SE |
|---|---:|---:|---:|---:|
| 4 players, common21 pairs | 5.2738 | 3.1429 | 2.1310 | .2726 |
| 6 players, common21 pairs | 4.3571 | 2.1825 | 2.1746 | .1309 |

Difference in gaps, four minus six, paired by seed: -.04365 VP, SE .28181. This is no evidence that the rule advantage disappears at six players in the controlled setup. Absolute scoring falls for both with six seats. Actual map grid has37systems in both; common21 mean main-map planets61.71 at4 vs62.10 at6 (about15.43 vs10.35 per seat). Homes/geometry/speaker necessarily differ; this is a matched-family/seed comparison rather than literally identical maps between different seat counts.

All six core factions favor rule at both counts. Common21 four/six rule advantages by faction: Hacan2.00/1.95, Sol2.07/2.43, Jol-Nar2.36/2.14, L1Z1X1.85/1.76, Letnev2.79/2.38, Xxcha1.71/2.38. Four samples perbot/faction13..15; six21 each. These are descriptive faction outcomes, not proof that a particular ability is defective.

Mechanisms observed on common21 pairs:

- Expansion/forces: four rule15.73planets vsMLP9.65, fleet printedcost23.38vs13.98, ground13.73vs9.37. Six rule12.30vsMLP7.06planets, fleet21.00vs12.36, ground12.44vs9.17. Substantial expansion/force deficit accompanies the MLP VP deficit in both counts.
- Cumulative scored-objective ledger perseat: four public2.75vs2.13, secret1.19vs.80; six public2.50vs1.62, secret.85vs.44. Counts classified by embedded catalogs; counts are not exact causal VP decomposition and do not identify which round/source scored each ID.
- Strategy draft: engine setup gives2cards/player at4,1at6. On common21, four ruleImperial74/582picks12.7%, Leadership13/582=2.2%; MLPImperial53/575=9.2%, Leadership150/575=26.1%. Six ruleImperial74/499=14.8%, Leadership7/499=1.4%; MLPImperial37/501=7.4%, Leadership162/501=32.3%. Consistent token-focused/card-selection split is observed, but no card-choice intervention was done, so not a causal claim of Leadership being bad.
- Imperial score opportunities: four56rule vs31MLP, six66vs18; MLP declined once in each setting, rule never. These prompts exist only when a scoreable public objective is available; do not confuse them with all Imperial activations, its Mecatol point or secret draw. Status scoring likewise shows neither bot declining actual score_objective prompts: four258rulevs207MLP, six341vs235. Thus missing scoring opportunities, rather than widespread refusal to take offered VP, is the visible pattern.

Counter ablation: six-core per-seat counter24pairs, no failures, all starting hashes exactly match global-counter arm. Rule4.40278/MLP2.19444, gap2.20833SE.14054; global gap2.13889. Paired change+.06944SE.16785. Counter indexing does not explain the large gap here.

Fresh legacy pool core control (fullsources/no feasibility/per-seat count, same candidate bundle/temp/explicit4rounds, diplomacyfalse, seeds1..24):48attempts46complete/22pairs; rule-minusMLP gap1.5303SE.1336 (exact recount pending). SYSTEM_ACTIVATED seed9true and AGENDA_REVEALED seed23true failures retained. Public source/geometry/roster/IDs/speaker differences mean this is a protocol-family control, not one-factor causal ablation.

The original broad-roster comparison is now being checked at its first24 seeds900000000..900000023 and explicit4rounds with both diplomacy settings, because retained artifact lacks flags. Original first24 means rule3.10417/MLP3.43056. Fresh wide/diplomacyfalse after4 result has rule3.13889/MLP1.79861, gap1.34028SE.102 (no failures). It does not reproduce original MLP scores. Do not infer an unrecorded horizon, temperature or checkpoint solely from this difference; diplomacytrue forensic control pending.

Guard reporting: core4 seed9true trace independently confirms nested reaction/innerdecline on L1Z1X with zero model failures. Core4 SYSTEM_ACTIVATED errors at12false,13false/true and curated6 at21false share the guard but have rolled-back/incomplete decision tails; their precise roots are not confirmed. No guard or reaction behavior changed.

## Decisive forensic result: original six-player sample had diplomacy enabled

Both wide controls use same first24 original seeds900000000..23, same candidate checkpoint16448, validation pool bytes, temp.001, round4 cutoff and per-seat rule stream. Diplomacy ON reproduced every one of the original288 seat rows exactly, including faction,VP,openingclearance and highest-score flag. This reconstructs the sampled original setting rather than merely guessing its missing CLI flags. Diplomacy OFF did not reproduce it.

| Original wide six-player setup, 24 complete seed pairs | Rule VP/seat | MLP VP/seat |
|---|---:|---:|
| Diplomacy OFF | 3.1389 | 1.7986 |
| Diplomacy ON (all288originalrowsmatch) | 3.1042 | 3.4306 |

Same-seed ON-minusOFF: MLP +1.63194VP(SE.11349), rule -.03472(SE.13936). This is a one-flag experiment within the old six-player protocol. The old near-equality and new four-player rule dominance were not holding diplomacy fixed. Historical 200seedfullrun remains397/400games, but these first24seedpairs are complete and exactly reproduced.

Now test residual count effect with diplomacy ON in the matched server core4/core6 arms, same24seeds/rosterrotation/settings. New runner flag uses the exact existing rollout DiplomacyState.for_players(seating_order,true), before initialhash and baselines; no policy/rule change, end-state telemetry retained. Olddefaultflagfalse unchanged; tests2/2pass3.90seconds and buildpass. Luna independent code/reconstruction review pending. This revision has its own sourcehash manifest and does not overwrite prior artifacts.

Final package status: all requested investigative measurements are complete with failures explicitly retained. Final Luna accounting/doc review passed; no task process remains running. Compute reservation released. Remaining uncertainty is exact within-diplomacy causal decomposition and the SYSTEM_ACTIVATED loop roots, not whether the original sampled six-player protocol was diplomacy-enabled. No training changes, checkpoint promotion, engine fix, GUI or full-workspace qualification.
