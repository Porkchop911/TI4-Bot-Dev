# Community maps

`crates/ti4-host/src/maps/community_maps.json` adds 31 fixed, community-made galaxy layouts for
3 to 6 players to the map picker, next to the upstream AsyncTI4 templates in
`ti4-content/content/map_templates.json`. That upstream file is a pinned mirror (see
`CHECKSUMS.sha256`), so it is left alone. Both files use the same entry format (`alias`,
`playerCount`, `templateTiles` with `pos`/`staticTileId`/`home`/`playerNumber`). A community entry
also has `name` (what the picker shows), `descr`, `source` (URL) and `sourceEntry` (the entry's
name at that URL). Every position is fixed, so no seeded filler is used and the seed does not change
the board. Homes are numbered clockwise from north.

Only tile layouts were taken from these sources, as facts. No descriptions or art were copied.

## Sources

| Source | License / note | Maps |
|---|---|---|
| TI4-TTPG premade map list, [`map-string-db.json` @ 88d2fc3](https://github.com/TI4-Online/TI4-TTPG/blob/88d2fc302c623169a3196f96c063e57ddaa76fdc/src/lib/map-string/map-string-db.json) | The repo's software is released under the Unlicense; the map layouts belong to their authors (FFG, SCPT, BSFP, Chart the Galaxy entrants) | Learn to Play (PoK) 3/4/5/6p (FFG); Three's Company 3p, Race for Mecatol 5p, Won't You Be My Neighbor? 6p (FFG "Chart the Galaxy" contest entries); SCPT 2020 Knockout 5p; SCPT 2019 Final, 2020 Final, 2021 Preliminary/Semifinal/Final 6p; Clear Skies 6p (BSFP) |
| TI4 Map Atlas as shipped in TI4-TTPG, [`map-atlas-db.json` @ c373195](https://github.com/TI4-Online/TI4-TTPG/blob/c37319527abe42839b47a2d3bf703e11ad504922/src/lib/map-string/map-atlas-db.json) | Same as above; each entry credits its author | Balanced Spiral, Equidistant Diplomacy, Gravity (Jon) 6p; Outer Rim 4/5/6p, Small and Rich 5/6p, Rich Snowflake 3p, The Link 3p (DaveBot777); Expanse, Pyrrhic Victory 4p (Jon); Hurts Donut 3p (zombiebrains) |
| Board Game Arena forum, ["Premade Maps A, B, C, D on BGA"](https://forum.boardgamearena.com/viewtopic.php?p=261915) | Map D layouts by TomKy, as keeganw.github.io tile strings | BGA Balanced (Map D) 3/4/5/6p |

## Conversion

TTPG map strings list tiles from ring 1 outwards, each ring clockwise from north. That is the same
order as the `RNN` template positions. `0` is a home, `-1` is no tile, and `{X}` replaces Mecatol in
the centre. keeganw strings use the same order with Mecatol written first. They come out rotated
relative to TTPG, which does not matter for a symmetric board.

Hyperlanes needed a real translation. TTPG and AsyncTI4 number the sides and rotations of a hyperlane
tile differently, and in places pick a different (equivalent) tile. The mapping
`async_side = (ttpg_rotation - ttpg_side) mod 6` was taken from the hyperlane connection tables of
both projects (TTPG `system.data.js`, AsyncTI4 `hyperlanes.properties`). Of the 24 candidate mappings
(side mirrored or not, offset 0 to 5, rotation direction) it is the only one that reproduces all 18
hyperlanes of the standard 4- and 5-player frames of both projects. It also reproduces the 3-player
frame, which the two projects place 60 degrees apart. Each hyperlane in a community map was then
translated into the AsyncTI4 tile variant with the same connections. Note that the engine does not
model hyperlane paths (see `ti4_content::galaxy`), so rotations only affect the picture.

## Left out

Maps were left out if they:

- seat 7 or more players. Seats past the sixth reuse factions, so such tables cannot be seated yet.
- place the same tile twice (Beltway, A Tight Belt, The Trident, Triquetra, The Birnam Wood, ...).
- have no Mecatol Rex.
- leave holes exactly where the standard 5-player hyperlanes go, so the intended hyperlanes are
  ambiguous (Big Tech, Wargames).
- were kept out to keep the list short. The rest of the TTPG lists could be added the same way.

BGA maps A to C are not included. A is the base-game learn-to-play layout, and B and C are the BGA
developer's presets. Their 4-player hyperlane variants also place one hyperlane tile twice with the
same rotation.

## Checks

`crates/ti4-host/src/maps/community_tests.rs` checks each map:

- it is named, attributed and has a source;
- it has unique positions, no open slots, and exactly one home per seat;
- it builds with every tile id known to the content, and none repeats;
- the board is the same for any seed;
- the board is one connected piece, and every home touches at least two tiles;
- it starts a game headlessly through `create_game_with_template`;
- it is offered by `/api/maps` under its name without replacing the recommended default.
