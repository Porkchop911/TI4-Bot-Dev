# Alpha feedback triage, 2026-10-15

All 46 reports in `/var/lib/ti4-public/feedback/` (2026-10-12, 2026-10-13 and 2026-10-15), read with `tools/public/list_feedback.sh`. Testers are referred to by report id only.

How coverage was established: the commits of the alpha branches (alpha-w1 ... w8, alpha-e1 ... e3, alpha-trade-upstream, alpha-feedback-names) and their screenshot manifests (CD, CF, CG, CJ, DB, DC, DD, DM), matched against each report's text and context. Where the outcome was unclear, the report's game was replayed: server games with a scratch `state_at` replay (`replay_session_forced` on the first N recorded decisions of a copy of the store), offline reports with `cargo run -p ti4-wasm --example replay_save` (all four reproduced their position exactly).

## Summary

| Status | Reports |
|---|---|
| done (earlier branches) | 29 |
| done (this branch) | 4 |
| duplicate | 2 |
| needs-decision | 2 |
| needs-info | 2 |
| not a bug | 4 |
| not worked (idea) | 1 |
| no action | 2 |

Fixed on branch `alpha-feedback-0415`: fb_1792038952 (`e9247daf`), fb_1792039768 (`d8f6d1f7`, `95e04289`), fb_1791905479 (`3a8f8f8a`), fb_1791875585 (`577d1a74`). Screenshots: `web/e2e/screenshots/EG-alpha-feedback-0415`.

## Table

| Report | Date | Summary | Category | Status |
|---|---|---|---|---|
| fb_1791812197_c2347de5 | 2026-10-12 | "Test" (a tester checking the button) | other | no action (test report) |
| fb_1791833447_abcbc2e5 | 2026-10-12 | "Lovely" (praise) | other | no action |
| fb_1791833658_0b2fb8cc | 2026-10-12 | Make toasts revisitable | ui | done in `eaa5fe14` (alpha-w1-toasts: notification history) |
| fb_1791833790_1bfe77f5 | 2026-10-12 | Build menu still in two columns | ui | done in `3204c105` (alpha-w2-production: single-column build menu) |
| fb_1791833851_33189063 | 2026-10-12 | Objectives, events, feedback and action panels all close differently | ui | done in `2cf0e2f6` (alpha-w3-panels) |
| fb_1791834000_041442c0 | 2026-10-12 | "Kaputt gemacht" ("broke it"), no detail | bug | needs-info: no description; the game (game_935f...) replays with matching decision hashes, so nothing to reproduce |
| fb_1791834003_f79d20ff | 2026-10-12 | Technologies should be filterable by colour / researchable now | ui | done in `4a4e153a` (alpha-w4-info) |
| fb_1791875585_49772e92 | 2026-10-13 | Back to create-lobby button (spectator on /games/<id>, phone) | ui | done in `577d1a74` (this branch): Back to start on the lobby page and on a gone/unreachable game |
| fb_1791895952_493b9f81 | 2026-10-13 | No movement dialog when no unit can move | ui | done in `23e1017d` (alpha-w3-panels) |
| fb_1791896114_b2088e79 | 2026-10-13 | Trade goods clipped on the strategy cards | ui | done in `3204c105` (alpha-w2-production) |
| fb_1791896292_a9ea523e | 2026-10-13 | Space combat dialog does not name the system | ui | done in `4a4e153a` (alpha-w4-info) |
| fb_1791896411_d72ebf1b | 2026-10-13 | Bot attacked system 69 without invasion forces | idea (bot) | not worked: rule-bot planning; see Ideas |
| fb_1791896524_8df7826f | 2026-10-13 | Picked two carriers to Mecatol Rex; one carrier arrived, no infantry landed | bug | needs-info: replaying game_a36f... (decisions 305-323) shows one sol_carrier2 at home, 8 units loaded and 3 infantry committed to MR. This game no longer replays hash-identical on the current engine, so the live position may differ; the movement dialog's per-type counts were fixed earlier (BU) |
| fb_1791896580_ce317cf5 | 2026-10-13 | Turn beep: switchable, once per turn | ui | done in `4119bbe7` (alpha-w3-panels) |
| fb_1791896856_a6946645 | 2026-10-13 | Salvage should say how many commodities it takes | ui | done in `5af29be2` (alpha-w4-info) |
| fb_1791897348_6ce45c63 | 2026-10-13 | Same bot played a much better turn earlier (system 69) | idea (bot) | duplicate of fb_1791896411 (bot planning) |
| fb_1791897455_bb8286d0 | 2026-10-13 | Asked about Reverse Engineer without seeing the card; every action card play should toast | ui | done in `eaa5fe14`, `62ef32e0`, `77876836` (alpha-w1-toasts) |
| fb_1791897894_21cf5900 | 2026-10-13 | Clicking an anomaly should explain it | ui | done in `4a4e153a` (alpha-w4-info) |
| fb_1791898055_a71fe4af | 2026-10-13 | Production: split resources into planets vs trade goods | ui | done in `3204c105` (alpha-w2-production) |
| fb_1791898083_0025001a | 2026-10-13 | Why do I have 7 trade goods? | ui | done in `3204c105` (trade good sources in the production dialog) |
| fb_1791898357_3a1a2c99 | 2026-10-13 | Sabotage window should show the full target of the card | ui | done in `77876836` (alpha-w1-toasts) |
| fb_1791898693_f7d558a5 | 2026-10-13 | Could only build fighters despite resources | bug | done in `d820cf99` (alpha-w2-production: one-unit production says why a unit is not offered) |
| fb_1791899346_d5eaae7e | 2026-10-13 | Right player column not scrollable | ui | done in `2cf0e2f6` (alpha-w3-panels) |
| fb_1791900631_8409386e | 2026-10-13 | Legendary planet abilities in the system overview | idea | done in `4a4e153a` (alpha-w4-info) |
| fb_1791900942_9ef9a96d | 2026-10-13 | Last player sheet not fully visible | ui | done in `2cf0e2f6` (alpha-w3-panels) |
| fb_1791901120_d66c152b | 2026-10-13 | Dismiss the history (redo) bar | ui | done in `eaeadfdf` (alpha-w3-panels) |
| fb_1791901208_d2dbc9b3 | 2026-10-13 | What about Thunder's Edge breakthroughs? | ui | done in `460ae447` (alpha-w5-breakthrough-explore) |
| fb_1791901264_9fce66f0 | 2026-10-13 | Frontier tokens do not work | bug | done in `0a35de39`, `d1733ab1` (alpha-e2-exploration) and `460ae447` (map markers, toasts). Replay of game_d26c... decision 202-205: Dark Energy Tap did explore system 50 (Enigmatic Device), the old client said nothing happened |
| fb_1791901298_febc7568 | 2026-10-13 | Frontier token not explored although DET researched | bug | duplicate of fb_1791901264 (same game, same move) |
| fb_1791901506_7183b793 | 2026-10-13 | Toasts when people score objectives | ui | done in `eaa5fe14` (alpha-w1-toasts) |
| fb_1791901529_0a2e1543 | 2026-10-13 | Bots should make themselves speaker (Politics) | idea (bot) | needs-decision: the rule bot answers politics_choose_speaker at random (decision 81 of game_d26c...: Mentak bot gave it to Mahact). A one-line bot rule fixes it, but bot answers are recomputed when an offline save is resumed, so any bot change makes in-progress offline saves that hit that decision report an engine mismatch |
| fb_1791901761_cca7b9cf | 2026-10-13 | Trade system like upstream (porkchop) | idea | done in `c47e201d` (alpha-trade-upstream: free-form amounts) |
| fb_1791901951_774387d2 | 2026-10-13 | No notice on drawing an exploration; Enigmatic Device offers 36 component actions | bug | done in `0a35de39`, `d1733ab1` (alpha-e2-exploration) and exploration toasts (`460ae447`) |
| fb_1791902555_e867b436 | 2026-10-13 | So many units in system #02, bot bug? | idea (bot) | not a bug: replay at decision 343 shows Mentak's home with 4 infantry, 2 mechs, 3 fighters, all legal. Bot keeps ground forces at home; see Ideas |
| fb_1791902685_5ce5c66b | 2026-10-13 | Jae Mir Kan (Mahact agent) only offers the bot's token | rules | done in `7326b873` (alpha-e1-mahact-notes): correct per the card (the active player's token); the option now names the system by its planets |
| fb_1791903023_fdcd28d2 | 2026-10-13 | End turn button should sit next to the other buttons | ui | done in `2503e9d5` (alpha-w3-panels) |
| fb_1791904110_8f2d675f | 2026-10-13 | Ground combat: mechs should absorb hits before infantry die (or let me choose) | rules | needs-decision: replay of game_d26c... decisions 497-499 on Bakal: Sol lost 2 infantry with an undamaged mech because the defender's Mentak mech (Moll Terminus) forbids other players' SUSTAIN DAMAGE on its planet, so the engine applied the rules correctly. Ground hits are assigned automatically (sustain first, then the cheapest unit); see Decisions |
| fb_1791904262_91f4dfd3 | 2026-10-13 | Holder of my Military Support cannot use it as a component action | rules | no fix needed: Military Support is not an ACTION; it fires at the start of the Sol player's turn, and the holder used it at decision 525 (placed 2 infantry on Jord) |
| fb_1791904377_98ac0433 | 2026-10-13 | Gravity rift rolls should show in a toast and the log | bug | done in `81b96ec0`, `95fd3804`, `ca537308` (alpha-e3-public-rolls) |
| fb_1791904677_1f93e3e3 | 2026-10-13 | Hovering other players' units should show their stats | ui | done in `4a4e153a` (alpha-w4-info) |
| fb_1791905092_9aa9bf02 | 2026-10-13 | Mahact does not gain a command token after winning a ground combat | bug | done in `f4105287`, `d816f8a9` (alpha-e1-mahact-notes). Replay: Edict did fire at decision 561 (Sol's token into the Mahact fleet pool); it was invisible because foreign tokens do not count in the Mahact pools. The sheet now lists held tokens |
| fb_1791905479_5dc6762e | 2026-10-13 | Mahact's Scepter of Dominion seems broken: no tokens added to Mecatol Rex | bug | done in `3a8f8f8a` (this branch). Replay at decision 613: the only foreign token in the Mahact pool was the holder's own, so nobody places one (rules-correct for the Thunder's Edge text), but every option promised "each other player places a command token". Labels now say who places one, or nobody |
| fb_1792038952_6cb9e7a1 | 2026-10-15 | Plague: each planet should show infantry instead of resources | ui | done in `e9247daf` (this branch) |
| fb_1792038973_74215545 | 2026-10-15 | Why does [0.0.0] have so much infantry? (offline) | idea (bot) | not a bug: replay (offline save, position reproduced) shows the L1Z1X bot's home with 10 infantry and 4 mechs, all legal; same bot behaviour as fb_1791902555 |
| fb_1792039064_b13cab22 | 2026-10-15 | Why did the bot not take Mecatol Rex? (offline) | idea (bot) | not a bug: at the report's position Mecatol Rex was the tester's own (Sol: 2 infantry, carrier, destroyer, fighter); the bots chose not to attack; see Ideas |
| fb_1792039768_b08b6d58 | 2026-10-15 | Commander should show what it does when unlocked/used (offline) | ui | done in `d8f6d1f7`, `95e04289` (this branch): unlock toast with the printed ability. Replay: Claire Gibson was unlocked silently in round 2 and offered twice in round 3 as a reaction (that window already shows the ability text) |

## Needs a decision

- **Bots and the speaker (fb_1791901529).** Recommendation: let the rule bot name itself on Politics (`politics_choose_speaker`), but batch it with the other bot changes below and ship them together with an engine-build bump, because resuming an offline save recomputes bot answers: a changed bot answer makes an in-progress offline game that reached such a decision report an engine mismatch. Server games are not affected (they replay recorded decisions).
- **Ground combat casualties (fb_1791904110).** The engine assigns ground hits itself: an undamaged unit with SUSTAIN DAMAGE absorbs first, otherwise the cheapest ground force dies. In the reported fight the Mentak mech's Moll Terminus switched off the Sol mech's SUSTAIN DAMAGE, so the result was right. Recommendation: no rules change; (a) show the reason in the ground combat summary ("Moll Terminus: no Sustain Damage on this planet"), small UI work (about half a day); (b) whether players should pick ground casualties themselves (LRR 42 lets them) is a product decision: it adds one decision per hit and changes decision streams (new engine option, replay-compatible only behind a new decision subtype).
- Not found in the feedback files: the topics "enemy hand promissory notes visibility" and an "Overrule / Thunder's Edge ruling" are not raised by any report here, so they are not in this table.

## Ideas (bot behaviour)

fb_1791896411, fb_1791897348 (attack without invasion forces), fb_1791902555, fb_1792038973 (ground forces pile up at home), fb_1792039064 (does not contest a human's Mecatol Rex). All are rule-bot planning, not engine bugs. Rough effort: 2 to 3 days for a "ship idle ground forces to takeable planets / pair attacks with transports" pass in `ti4-rule-bot`, plus the offline-save caveat above.
