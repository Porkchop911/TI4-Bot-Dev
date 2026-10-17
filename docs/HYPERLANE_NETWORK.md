# Hyperlane network (web, display only)

Code: `web/src/components/hyperlane/` (`hyperlaneNetwork.ts` analysis, `HyperlaneArt.tsx` drawing).

- Lane data (`hyperlaneGeometry.ts`) is draw-only. The engine does not model hyperlane paths
  (`crates/ti4-content/src/galaxy.rs`), so it has no adjacency to compare with. The web tests check
  the routes against an independent union-find over lane ports on every community map snapshot.
- Algorithm (header of `hyperlaneNetwork.ts`): ports per hex edge, lanes join ports inside a tile,
  matching lane ends join across a shared edge, a depth-first search from every system edge finds
  simple port paths to other systems. One route per unordered pair: fewest lane segments, then
  fewest tiles, then most overlap with routes already chosen, then lexicographic. Deterministic and
  independent of input order. The drawing is the union of the routes.
- Drawing: one layer after all tile fills (board, preview, thumbnails). Hexes are 29 px apart, so
  lanes are extended half the gap into a neighbouring lane tile, or all the way to a system, which
  makes the network continuous. Dock ring on systems, flat bar on
  open ends, diamond at junctions.
- Computed once: `getHyperlaneNetwork` memoises per layout (module cache, shared by board, preview,
  thumbnails, inspector). Catalogue layouts are in `hyperlaneRoutes.generated.json` (keyed by a hash
  of hyperlane tile ids and places plus which other places hold a system). A test fails when the
  table is stale; refresh with `cd web && npm run hyperlane:table`. Random or custom maps are
  computed on first draw and cached; nothing is stored in game state or save formats.

## Rule and verification

- Rule (same as the TTPG mod's `adjacency-hyperlane.js`): enter a lane tile through an edge, leave
  by the other end of a printed lane that starts there, cross into the neighbour, repeat. No turning
  at a shared lane end (three-lane tiles). Lane table cross-checked with TTPG `system.data.js`
  (side i is engine edge -i mod 6) and with all 208 shared edges of the community layouts.
- `hyperlaneTrace.test.tsx` reads the drawn SVG back and walks the strokes as a ship would, per
  layout, in preview and thumbnail, and compares with an independent TTPG-style oracle.
