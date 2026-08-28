# Space stations: rules audit

Thunder's Edge adds four space stations. The corpus carries them as ordinary planets carrying a
`SPACESTATION` planet type, and the engine treats them as planets nearly everywhere. The official
rules make them a distinct thing with its own control mechanism, and the difference is load-bearing
for training: **6.2% of measured opening clearances depend on a space station being taken by a move
the rules forbid outright.**

Rules text from [tirules2.com/R_space_stations](https://tirules2.com/R_space_stations), corroborated
by the [Twilight Imperium wiki](https://twilight-imperium.fandom.com/wiki/Thunder's_Edge). The
oracle (`D:/Projects/ti4-engine`) has the same `SPACESTATION` tag and no special handling either, so
this is inherited from the port source rather than introduced by it.

## The stations

| system | tile | planets on tile | station |
|---|---|---|---|
| Bellatrix/Tsion Station | 109 | `bellatrix`, `tsionstation` | Tsion Station |
| Tarana/Oluz Station | 111 | `tarana`, `oluzstation` | Oluz Station |
| Ordinian/Revelation - Bastion | 92 | `ordinian`, `revelation` | Revelation |
| The Watchtower | 117 | `thewatchtower` | The Watchtower |

Tile 117 is the only one whose system has **no** real planet, which matters for rule 14.

Each station is 1 resource / 1 influence except Revelation (1 / 2).

## Rule by rule

| # | Rule | Engine | Verdict |
|---|---|---|---|
| 1 | "Several systems contain space stations, which are mechanically similar to planets." | Treated as planets | informational |
| 2 | "A player gains control of a space station when they are the only player with units in that space station's system." | Control transfers by landing ground forces on it (`planet_control` written by invasion) | **WRONG** |
| 2a | "If another player moves ships into that system, they will not gain control ... unless they win the resulting space combat." | Not modelled | **MISSING** |
| 2b | "If the player who controls the space station moves their ships out ... they retain control until another player moves ships in." | Not modelled | **MISSING** |
| 4 | "The space station may be exhausted for resources or influence as though it were a planet." | `production::spendable_planets` includes it once controlled | **correct** (given correct control) |
| 5 | "Structures and ground forces cannot be committed to or placed on a space station." | `invasion::landable_planets` offers it to any invasion; `strategy_cards.rs:474` (Construction) offers PDS/space dock on every controlled planet including stations | **WRONG**, two paths |
| 6 | "Space stations do count as planets for the purpose of voting." | `vote::votable_planets` reads `planet_control`, so they count | **correct** |
| 7 | "Space Stations do not count as planets for the purpose of scoring objectives." | Counted by every planet-counting family (`NonHome`, `OnTheRim`, ...) | **WRONG** |
| 8 | "For each space station a player controls, their commodity value is increased by one." | No such effect | **MISSING** |
| 10 | "A player who controls one or more space stations may resolve transactions with other players who control one or more space stations." | `transactions::neighbours` is adjacency-only | **MISSING** |
| 12 | "A player may exhaust a space station at any time to convert any commodities that they have into trade goods." | No such action | **MISSING** |
| 14 | "If a system tile contains a space station, but no planets, then a frontier token will be placed in that system during game setup." | `exploration::frontier_systems` filters on `record.planets().is_empty()`; tile 117 lists `thewatchtower`, so it is non-empty and gets no token | **WRONG** |

Already correct, and worth noting because it shows the distinction was half-drawn already:
`galaxy::Planet::traits` excludes `SPACESTATION` from trait matching, so "control 4 planets of the
same trait" does not count stations. The type was known not to be a planet trait; nothing followed
that through to control, landing, or scoring.

## Measured cost to training

`cargo run --release -p ti4-mlp --example space_station_reliance -- --bundle
out/checkpoints/run-022/checkpoint-79044 --seeds 120 --seed-base 700000000 --temperature 0.25`

4,320 held-out seat-games on `full_np8_12_train.json`:

| | |
|---|---|
| maps with a station present | 53.1% |
| cleared seats holding a station | 9.5% |
| cleared seats that would fail without it | **6.2%** |
| clearance as measured | **95.3%** |
| clearance with stations not counting as planets | **89.4%** |

Every opening-clearance figure recorded in this project is inflated by roughly six points. The
champion's 95.8% is about 90%. The apparent saturation of five factions near 96% is an artifact:
they were being topped up by a free planet-and-system obtainable with one infantry.

## Pool interaction

Tile 117 placement, by pool:

| pool | contains 117 | distinct positions | adjacent to a home |
|---|---|---|---|
| `save52_e400_train.json.gz` | 100% (6136/6136) | 1 | **100%** |
| `save52_e400_holdout.json.gz` | 100% (2056/2056) | 1 | **100%** |
| `save52_noadj_train.json` | 100% (949/949) | 1 | **100%** |
| `full_np8_12_train.json` | 15.0% | 30 | 53.9% |
| `full_np8_12_holdout.json` | 14.9% | 30 | 53.7% |
| `full_np8_12_final.json` | 16.0% | 30 | 53.8% |

In every `save52_*` pool the Watchtower is fixed at one board position touching a home, in every
arrangement. Under the current (incorrect) rules that is a free planet and a free system next to
every seat, takeable with a single infantry. Runs on those pools are affected far more severely than
the `full_np8_12_*` runs this session used.

## Fix order

1. `invasion::landable_planets` — exclude `SPACESTATION`. Removes the illegal move and, with it,
   the inflated clearance.
2. Control by sole occupancy of the system, with 2a/2b, replacing control-by-landing.
3. Exclude stations from planet-counting objective families (rule 7). Extends the distinction
   `Planet::traits` already draws.
4. `frontier_systems` — a system whose only "planets" are stations is planetless for rule 14.
5. Construction (`strategy_cards.rs:474`) — no structures on stations.
6. Economy: +1 commodity per station, station-to-station transactions, exhaust-to-convert.

1–5 change what a policy can do and therefore invalidate existing clearance numbers. 6 is economy
detail that does not touch the opening.
