# Handover: Phase 9 rules verification

Written 2026-08-31. Branch `wp/r01-review-viewer-contract`, head `55b8cb1`. Tree clean, all suites
green: **1,086 engine, 189 policy, 52 sim**. Clippy clean under `RUSTFLAGS="-D warnings"`.

Supersedes `plans/PI_HANDOFF_ENGINE_REMAINDER.md`, which is closed — everything it listed is done.

## Where things stand

Content coverage is 100% of everything in scope, and the reports say so themselves:

```
cargo run -p ti4-engine --example remaining          # prints nothing
cargo run -p ti4-engine --example coverage_report    # every in-scope row at 100%
```

action cards 142/142 · agendas 63/63 · laws 40/40 · exploration 80/80 · relics 24/24 ·
objectives 40+40 · leaders 19/19 · breakthroughs 6/6 · mech abilities 6/6 · reaction windows 0
unsupported.

**Coverage is not correctness.** That is the whole point of what follows.

## Phase 9: what is done and what it found

Forty of the 109 rule topics have now been checked against the rules text at
[tirules2.com](https://tirules2.com/) — pass 1 of the audit's method, the only pass that
establishes correctness. **39 remain unverified.**

**Twelve defects in thirty-eight in-scope topics.** That rate is the single most important number
in this document: it is close to the base rate `engine-rules-audit.md` warned about when it was
written, and it means the 39 unchecked rows should be read as "not checked", never as "probably
fine".

Fixed:

| Rule | What was wrong |
|---|---|
| 16.3 | fighters and ground forces share one capacity total; ground forces were **never** counted as excess, so six infantry on a four-capacity carrier reported nothing wrong |
| 59.5 | the nebula defender's +1 to each combat roll had no code at all — 59.1–59.4 were all present, which is what made it easy to miss |
| 15.7 | Non-Euclidean Shielding cancelled one hit instead of two |
| 63.2 | a planetary shield did not stop L1Z1X's Harrow |
| 95.5 | pickup from a system holding your own command token — unreachable until the Dominus Orb made it live |
| 68.10 | **blockaded space docks built ships**; the rule was enforced in the bot-facing helper and nowhere in the path that produces |
| Fragile | Jol-Nar's −1 applied in space only; `combat_modifier`'s `context` parameter had exactly one caller and it passed `"space"` |
| PDS II / Indomitus | "SPACE CANNON against ships in adjacent systems" — `space_cannon_offense` read only the activated system, so an upgraded PDS next door never fired |
| mech abilities | four of six in-scope mechs unimplemented **and counted by nothing** |
| 27.2a | the custodians token — and its victory point — was purchasable with six influence and no army |
| Warfare | "then, the active player can redistribute their command tokens" was absent |
| Thalnos scope | both cards say "during each combat round" and were firing on space cannon, barrage and bombardment |

## Open, deliberately

Three things found and left. Each has its reason recorded in the code beside the rule, not only
here.

1. **16.3c — excess removed at the *end* of combat.** `over_capacity` answers correctly; there is
   no caller after the shooting, so a carrier destroyed in a fight leaves its cargo standing and a
   stranded ground force can still invade. Wiring it there was tried and reverted: Crash Landing
   and three other cards move units from windows that settle *after* the combat window closes, and
   enforcing first removes what those cards are about to rescue. Needs an ordering change with its
   own review. See `fleet.rs`.

2. **81.5 — the status phase's own token redistribution.** `strategy_cards::redistribute_tokens`
   exists and works (it serves Warfare). Wiring it into the status phase asks every player up to a
   dozen questions every round, and a greedy decider answers by shuffling tokens for nothing — a
   status-phase test caught exactly that. It is a large change to the decision stream and to every
   trained policy, so it wants its own measurement. See `status.rs`.

3. **20.4/20.4a — command tokens limited by reinforcements.** No token supply pool exists, so the
   three pools can grow without bound. Modelling it means a supply count and a decision at every
   gain.

Plus two simplifications recorded rather than hidden: **95.1** allows pickup from each system a
ship moves *through* and this engine offers only the origin (narrower, not illegal); **68.3b** lets
a player produce one unit of a two-for-one pair at full cost, which is not offered.

## How to continue Phase 9

The method that produced twelve defects, in order:

1. `awk -F'|' '/^\| .* \| \? \|/ {print $2}' engine-rules-audit.md` — the unverified list.
2. Fetch `https://tirules2.com/R_<topic>` (lowercase, underscores) and read the numbered rules.
3. Grep the engine for each numbered sub-rule *individually*. **This is where the defects are.**
   Nebula 59.1–59.4 were all implemented and 59.5 was absent; Space Cannon had offence and defence
   and no adjacency. A topic that "looks done" is exactly the shape of the ones that were not.
4. Write the failing test first, then fix, then re-run the suite.

**Do not mark a row VERIFIED on a grep.** The audit distinguishes pass 1 (rules text) from pass 3
(the topic is mentioned somewhere), and only pass 1 counts. I marked Space Cannon verified on
offence and defence existing, then found the adjacency clause missing in the next batch; the row is
corrected and the mistake is worth not repeating.

## The defect class, still the most useful thing to know

Every count in this engine has at some point drifted from the code it counts. Eight instances now,
and the newest is the worst shape: **mech abilities were counted by nothing at all** — a mech's
ability is printed on the unit, not in `abilities.json`, so "faction abilities 14 of 14" never saw
them and four cards sat unimplemented without ever appearing as a gap. `unimplemented_mechs` now
counts them.

So: before trusting any count, grep the helper for a caller. And its mirror — before declaring
anything blocked, grep the model for the field you think is missing. `Galaxy::wormholes_off`,
`System::is_scar`, `GameState::strategy_card_goods`, the whole reroll-staging system and
`redistribute_tokens` were all built and uncalled at some point this week.

## Verification

```
RUSTFLAGS="-D warnings" cargo clippy -p ti4-engine -p ti4-model -p ti4-content -p ti4-policy -p ti4-sim --all-targets
cargo test -p ti4-engine     # 1,086
cargo test -p ti4-policy     # 189, ~200s
cargo test -p ti4-sim        # 52, includes the behavioural gate
```

`-D warnings` is what CI runs and is not optional. `cargo test -p ti4-sim` needs
`out/pools/full_np8_12_holdout.json`, which is untracked — copy it into any new worktree or
`fixture_capture_is_deterministic` fails for a reason unrelated to your change.

### The behavioural gate

Two checks: a *value* gate (metrics within recorded bounds) and a stricter *protocol-integrity*
gate (the recorded bounds must equal what the tree recomputes). The second is why the baseline
moves even when every value is still in range.

Re-baseline with `cargo run -p ti4-sim --example rebaseline_behavior`, which prints old against new
and changes nothing by itself. Record every move in `plans/evidence/M08-021.md` **with its cause**.
Currently **v26**.

Adding an event type dilutes all six `share_*` metrics by one uniform factor and leaves `vp_pace`,
`score_spread`, `faction_differentiation` and `completion` alone. A real change in play moves the
latter. Say which you are looking at — that distinction is the only thing keeping the gate
meaningful. Several fixes this session moved nothing at all, and that was worth stating plainly
rather than dressing up.

One live finding from the gate: **the behavioural suite plays the Thunder's Edge strategy cards**,
so `te6warfare` runs and base Warfare does not. Fixes to the base cards will not show in these
metrics. Worth knowing before concluding a change "had no effect".

## Scope

Six players. Base + PoK + Codices + Thunder's Edge. No variants, no galactic events. Faction content
outside sol, letnev, xxcha, hacan, jolnar and l1z1x is out of scope — Capture is marked out of
scope for exactly this reason (every capture effect in the corpus is Vuil'raith Cabal).

Engine and policy features only; reward shaping is not in scope. **Training is still paused by the
owner's decision.**

## One note on the dice cluster

Thalnos, the Crown of Thalnos and the Heart of Ixth were implemented twice, concurrently — once by
me on `wp/engine-completion`, once uncommitted in the main checkout. The owner chose the latter and
was right to: it carries a per-die `deltas` map, so Thalnos's +1 dies with the die a later reroll
replaces, and pooled unit types per roll entry, so "destroy each unit that rerolled and missed"
reaches all of them. Mine did neither. The reconciliation is `acb898b`; nothing was lost.
