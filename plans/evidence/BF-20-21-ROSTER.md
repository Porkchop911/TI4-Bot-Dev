# BF-20 / BF-21 -- opt-in wide roster and seeded faction draw

Operator decisions 2026-10-08: roster = all choosable implemented factions; opt-in only (the six-faction
default is untouched everywhere); BF-23 dropped (authored bots deprecated).

## Roster (`seating::wide_roster(content, sources)`)

Derived: the six `IN_SCOPE_FACTIONS`, then `factions::MODULES` order, minus `NOT_CHOOSABLE_DURING_SETUP`
(Obsidian), minus factions absent from `factions::catalogue(content, sources)`. The three Keleres variants
collapse to one `"keleres"` family entry. Exact list (30 entries under `FULL`/default sources), asserted by
`the_wide_roster_is_exactly_this_list`:

sol, hacan, letnev, xxcha, jolnar, l1z1x | arborec, argent, bastion\*, cabal, crimson\*, deepwrought\*, empyrean,
firmament\*, ghost, keleres, mahact, mentak, muaat, naalu, naaz, nekro, nomad, ralnel\*, saar, sardakk, titans,
winnu, yin, yssaril.  (\* Thunder's Edge: present only when sources include `thunders_edge`; under `POK` the
roster is 25. Expands to 32 seatable aliases with keleresm/keleresx/keleresa.)

## Opt-in design

`seating::FactionRoster { InScope (Default), Wide }`; `seating::seat_roster(roster, content, players, sources, seed)`.
`InScope` returns exactly `seat_in_scope(players)` and ignores the seed (test
`the_default_roster_is_the_in_scope_six_and_ignores_the_seed`). `IN_SCOPE_FACTIONS` and `seat_in_scope` are
unchanged. Sim: `ti4_sim::run::Table::seated_with_roster(.., roster, seed)`; `Table::seated` unchanged.
Smoke: `crates/ti4-sim/examples/wide_roster_smoke.rs`. `seeded_faction_assignments` (BF21a) is unchanged.

## BF-21 draw (`seating::seat_wide`)

* Shuffle the roster on a fresh `GameRng(seed)` domain `seating:wide-roster:v1`; players (input order) take
  the first entries. Same seed -> same map; fixture: seed 0 for players a..f is
  deepwrought, letnev, ghost, mentak, ralnel, arborec.
* RNG separation: dedicated domains (`seating:wide-roster:v1`, `seating:keleres-variant:v1`) on their own
  `GameRng`; no map (`galaxy`/`map`), dice, deck or exploration stream is drawn or shifted (test asserts the
  derived seeds differ and `map_filler` for the seed is unchanged).
* Uniqueness: never reuses. Table larger than what can be seated -> `InsufficientCandidates` (a 31-seat table
  on the 30-entry roster, and a roster-sized table where the Tribuni forces a skip, are both refused).
* Keleres: the engine's own rule decides the variant set -- `tribuni_variants_available(already seated)` (a
  variant needs its base Mentak/Xxcha/Argent unplayed), `validate_tribuni` (one Keleres seat). When the family
  entry is drawn, the variant is picked uniformly among the legal ones on `seating:keleres-variant:v1`; if none
  is legal the entry is skipped; a base faction taken by the drawn variant is skipped when it comes up. 400
  seeds: all three variants occur, every table passes `validate_tribuni`.
* TE gating: TE factions only with `thunders_edge` in sources. Crimson (Sorrow/118) and Firmament (96a)
  deploy through `seating::deploy` (`every_wide_faction_deploys_through_seating_with_its_own_home`; 50 boards
  built from seeds 0..50).

## Smoke

`cargo run --release -p ti4-sim --example wide_roster_smoke -q -j1 -- 0 40 10`: 40 seeds, 6 seats each, all
games finished, determinism replay on every 10th, all 32 aliases (incl. each Keleres variant) seated:
`failures=0`, `missing=[]` (out/bf20-21-wide-smoke.log). Workers = 12.

## Consumers audited (`IN_SCOPE_FACTIONS` / `seat_in_scope`) -- none changed

| Consumer | Status |
|---|---|
| engine `factions/mod.rs` tests, `seating.rs` tests, `obsidian.rs` test | assert on the six; unchanged |
| engine `examples/coverage_report.rs` | uses the six; unchanged |
| `ti4-sim` `run.rs::Table::seated`, `baseline.rs`, `examples/{ledger,objectives,prompts,scope}` | default six; unchanged (new `seated_with_roster` is additive) |
| `ti4-sim/examples/base_faction_soak.rs` | `seat_in_scope` + one replaced seat; unchanged |
| `ti4-policy` `bot.rs:2398`, `battle.rs` FACTIONS test | six; **BF-22** |
| `ti4-training` `rollout.rs`, `stage1.rs`, `teacher_corpus.rs`, examples `bench_generation`, `heads` | six; **BF-22** |
| `ti4-mlp` `capture_offline_pilot.rs` (own const) | six; **BF-22** |
| `ti4-review` `lib.rs` roster test | six; **BF-22** |
| `tools/bench_generation.py` | own tuple; unchanged |

## BF-22 follow-ups (not done: other sessions own those crates)

1. Policy battle `FACTIONS` width / faction one-hot and vocabulary: schema version bump, old-checkpoint
   rejection; decide how the Keleres variants and the 32 aliases map to feature slots.
2. Training `rollout.rs`/`stage1.rs`/`teacher_corpus.rs`, mlp `capture_offline_pilot`, policy `bot.rs`: add an
   opt-in roster knob (config field / CLI flag) calling `seat_roster(FactionRoster::Wide, ..)` with the game
   seed; keep the six as default; record the roster in dataset/checkpoint metadata.
3. Replayer/review (BF-24) presentation for the new factions.
4. Game runtimes outside the sim `Table` that need Keleres/TE seating must call `build_board` + `deploy` (as
   the smoke does) after the draw.

## Notes

* `rustfmt` on `seating.rs` also reflowed three pre-existing long lines (file was git-clean); no semantic change.
* `game.rs` and its other-session hunks were not touched.
