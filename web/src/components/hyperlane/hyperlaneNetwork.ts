/**
 * The hyperlane network of a map: which systems the lanes link, and the one route drawn for each link.
 *
 * Rules matched (TI4 "Hyperlanes", as the TTPG mod traces them in `adjacency-hyperlane.js`): hyperlane
 * tiles are not systems; a ship that enters a tile through an edge leaves it only through the other
 * end of a printed lane that starts on that edge (lanes that share an end point, as on the
 * three-lane tiles, are separate lanes: entering by one branch does not lead out by another
 * branch); it then enters the neighbour across the exit edge, which carries it on only if that
 * tile has a lane end on the shared edge. Two systems that such a chain joins are adjacent. The
 * lane table (`hyperlaneGeometry.ts`) was cross-checked with TTPG's `system.data.js` (tile faces
 * 83-91, A = face up, B = face down; TTPG side i is engine edge -i mod 6, a turn of t adds t) and
 * against all 208 shared edges of the community layouts. The engine does not model hyperlane paths
 * (`crates/ti4-content/src/galaxy.rs`), so there is no engine adjacency to compare with.
 *
 * Algorithm
 *  1. Ports. Every hyperlane tile has up to six ports, one per edge (`q,r:edge`). A printed lane
 *     joins two ports of the same tile. Two ports on either side of a shared edge are joined
 *     (neighbour in the ring-1 direction of the edge, opposite edge `(e + 3) % 6`) when both tiles
 *     have a lane end there. Within a tile a path takes one lane only (no turning at a shared end).
 *  2. Starts. For each ordinary system S and edge e whose neighbour is a hyperlane tile with a lane
 *     end on the opposite edge, a depth-first search walks simple port paths (no port twice, so
 *     loops terminate) until a path leaves a hyperlane tile into an ordinary system T. Paths that
 *     come home to S are ignored. Edges that lead nowhere (no neighbour, or a hyperlane tile
 *     without a matching lane end) are reported as open ends.
 *  3. One route per pair. All candidate routes of the unordered pair {S, T} are written in a
 *     canonical direction (smaller `q,r` first). The shortest wins: fewest lane segments, then
 *     fewest tiles. Pairs are settled in order of that length (then by key); among equally short
 *     routes the one overlapping most segments already chosen wins, so routes share lanes; the
 *     last tie-break is the lexicographic order of the segment keys. The result does not depend
 *     on input order.
 *  4. The drawing is the union of the chosen routes: `usedSegments` are marked on the lane art,
 *     `docks` are the system edges a chosen route ends on. Every pair that any chain joins gets
 *     exactly one route, so nothing that is possible is hidden.
 */

import { EDGE_RADIUS, edgePoint, parseHyperlaneId } from "./hyperlaneGeometry.ts";

/** Ring-1 direction of each edge (engine `Hex::ring(1)` order), as q,r offsets. */
export const EDGE_DIRS: readonly (readonly [number, number])[] = [
  [-1, 1],
  [0, 1],
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
];

export interface NetTileInput {
  systemId: string;
  q: number;
  r: number;
}

export interface HyperlaneConnection {
  /** Canonical order: the smaller `q,r` first. */
  fromKey: string;
  toKey: string;
  fromId: string;
  toId: string;
  /** Segment keys in route order, `q,r:a-b` with a < b. */
  segments: string[];
  /** Hyperlane tile keys on the route, in order, without repeats. */
  tiles: string[];
  /** Edge of each end system the route docks on. */
  fromEdge: number;
  toEdge: number;
}

/** What the generated table stores per route: positions only, ids are filled from the layout. */
export type RouteRecord = Omit<HyperlaneConnection, "fromId" | "toId">;

export interface HyperlaneNetworkModel {
  connections: HyperlaneConnection[];
  usedSegments: Set<string>;
  /** Hyperlane tile key to its lane ends: ports `q,r:edge`. */
  lanePorts: Set<string>;
  /** Lane ends that touch an ordinary system: port key to the system's tile key. */
  docks: Map<string, string>;
  /** Docks a chosen route ends on. */
  usedDocks: Set<string>;
  /** Lane ends that touch no tile or a hyperlane without a matching lane end. */
  openEnds: Set<string>;
  /** Lane ends where three or more lanes meet (one entry per physical edge). */
  junctions: Set<string>;
  /** System tile key to the systems it is linked with, by system id. */
  partners: Map<string, { key: string; systemId: string }[]>;
}

export const tileKey = (q: number, r: number) => `${q},${r}`;
const parseKey = (k: string) => k.split(",").map(Number) as [number, number];
export const portKey = (key: string, edge: number) => `${key}:${edge}`;
const segKey = (key: string, a: number, b: number) => `${key}:${Math.min(a, b)}-${Math.max(a, b)}`;
const keyLess = (a: string, b: string) => {
  const [aq, ar] = parseKey(a);
  const [bq, br] = parseKey(b);
  return aq - bq || ar - br;
};

interface Candidate {
  from: string;
  to: string;
  fromEdge: number;
  toEdge: number;
  segments: string[];
  tiles: string[];
}

const MAX_SEGMENTS = 30;
const MAX_STEPS = 50000;

export function analyzeHyperlanes(
  input: readonly NetTileInput[],
  /** Routes computed earlier (the precomputed table); skips the search. */
  preset?: readonly RouteRecord[],
): HyperlaneNetworkModel {
  const byKey = new Map<string, NetTileInput>();
  for (const t of input) byKey.set(tileKey(t.q, t.r), t);
  const lanesOf = new Map<string, [number, number][]>();
  for (const [k, t] of byKey) {
    const lane = parseHyperlaneId(t.systemId);
    if (lane) lanesOf.set(k, lane.lanes.map(([a, b]) => [a, b] as [number, number]));
  }
  const model: HyperlaneNetworkModel = {
    connections: [],
    usedSegments: new Set(),
    lanePorts: new Set(),
    docks: new Map(),
    usedDocks: new Set(),
    openEnds: new Set(),
    junctions: new Set(),
    partners: new Map(),
  };
  if (lanesOf.size === 0) return model;

  const portEdges = (k: string) => {
    const s = new Set<number>();
    for (const [a, b] of lanesOf.get(k) ?? []) {
      s.add(a);
      s.add(b);
    }
    return s;
  };
  const neighbour = (k: string, e: number) => {
    const [q, r] = parseKey(k);
    const [dq, dr] = EDGE_DIRS[e];
    return tileKey(q + dq, r + dr);
  };

  // Lane ends: what lies beyond each.
  for (const k of [...lanesOf.keys()].sort(keyLess)) {
    for (const e of portEdges(k)) {
      model.lanePorts.add(portKey(k, e));
      const n = neighbour(k, e);
      if (!byKey.has(n)) model.openEnds.add(portKey(k, e));
      else if (!lanesOf.has(n)) model.docks.set(portKey(k, e), n);
      else if (!portEdges(n).has((e + 3) % 6)) model.openEnds.add(portKey(k, e));
    }
  }

  // Junctions: lane ends meeting at one physical edge, counted on both sides.
  const degree = (k: string, e: number) =>
    (lanesOf.get(k) ?? []).filter(([a, b]) => a === e || b === e).length;
  for (const p of model.lanePorts) {
    const [k, es] = p.split(":");
    const e = Number(es);
    const n = neighbour(k, e);
    const other = lanesOf.has(n) ? degree(n, (e + 3) % 6) : 0;
    if (degree(k, e) + other >= 3) {
      const mine = portKey(k, e);
      const theirs = portKey(n, (e + 3) % 6);
      model.junctions.add(lanesOf.has(n) && theirs < mine ? theirs : mine);
    }
  }

  interface Canon extends Candidate {
    cost: number;
  }
  const chosen: Canon[] = [];
  if (preset) {
    for (const c of preset) {
      chosen.push({
        from: c.fromKey,
        to: c.toKey,
        fromEdge: c.fromEdge,
        toEdge: c.toEdge,
        segments: [...c.segments],
        tiles: [...c.tiles],
        cost: c.segments.length,
      });
      c.segments.forEach((x) => model.usedSegments.add(x));
    }
  } else {
    // Candidate routes per unordered pair.
    const pairs = new Map<string, Candidate[]>();
    let steps = 0;
    for (const [sKey] of [...byKey].sort(([a], [b]) => keyLess(a, b))) {
      if (lanesOf.has(sKey)) continue;
      for (let e = 0; e < 6; e++) {
        const first = neighbour(sKey, e);
        if (!lanesOf.has(first) || !portEdges(first).has((e + 3) % 6)) continue;
        steps = 0;
        const seen = new Set<string>();
        const segs: string[] = [];
        const tiles: string[] = [];
        const walk = (k: string, entry: number) => {
          // Arrived at tile k through port `entry`.
          const here = portKey(k, entry);
          if (seen.has(here) || steps++ > MAX_STEPS) return;
          seen.add(here);
          const pushed = tiles[tiles.length - 1] !== k;
          if (pushed) tiles.push(k);
          for (const [a, b] of lanesOf.get(k)!) {
            if (a !== entry && b !== entry) continue;
            const out = a === entry ? b : a;
            if (seen.has(portKey(k, out)) || segs.length >= MAX_SEGMENTS) continue;
            segs.push(segKey(k, a, b));
            seen.add(portKey(k, out));
            // Leave through `out`: into a system, or on into the next hyperlane tile.
            const n = neighbour(k, out);
            if (byKey.has(n) && !lanesOf.has(n)) {
              if (n !== sKey) {
                const cand: Candidate = {
                  from: sKey,
                  to: n,
                  fromEdge: e,
                  toEdge: (out + 3) % 6,
                  segments: [...segs],
                  tiles: [...tiles],
                };
                const id = [sKey, n].sort(keyLess).join("|");
                const list = pairs.get(id) ?? [];
                list.push(cand);
                pairs.set(id, list);
              }
            } else if (byKey.has(n) && portEdges(n).has((out + 3) % 6)) {
              walk(n, (out + 3) % 6);
            }
            seen.delete(portKey(k, out));
            segs.pop();
          }
          if (pushed) tiles.pop();
          seen.delete(here);
        };
        walk(first, (e + 3) % 6);
      }
    }

    // Canonical direction, then one route per pair.
    const canon = (c: Candidate): Canon => {
      if (keyLess(c.from, c.to) <= 0) return { ...c, cost: c.segments.length };
      return {
        from: c.to,
        to: c.from,
        fromEdge: c.toEdge,
        toEdge: c.fromEdge,
        segments: [...c.segments].reverse(),
        tiles: [...c.tiles].reverse(),
        cost: c.segments.length,
      };
    };
    const lessSegs = (a: string[], b: string[]) => {
      for (let i = 0; i < Math.min(a.length, b.length); i++) {
        if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1;
      }
      return a.length - b.length;
    };
    const best = [...pairs.entries()].map(([id, list]) => {
      const all = list.map(canon);
      const min = Math.min(...all.map((c) => c.cost + c.tiles.length / 100));
      return { id, cands: all.filter((c) => c.cost + c.tiles.length / 100 === min), min };
    });
    best.sort((a, b) => a.min - b.min || (a.id < b.id ? -1 : 1));
    for (const { cands } of best) {
      let pick: Canon | undefined;
      let pickShared = -1;
      for (const c of [...cands].sort((a, b) => lessSegs(a.segments, b.segments))) {
        const shared = c.segments.filter((s) => model.usedSegments.has(s)).length;
        if (shared > pickShared) {
          pick = c;
          pickShared = shared;
        }
      }
      if (!pick) continue;
      chosen.push(pick);
      pick.segments.forEach((s) => model.usedSegments.add(s));
    }
  }
  chosen.sort((a, b) => keyLess(a.from, b.from) || keyLess(a.to, b.to));
  for (const c of chosen) {
    const fromId = byKey.get(c.from)!.systemId;
    const toId = byKey.get(c.to)!.systemId;
    model.connections.push({
      fromKey: c.from,
      toKey: c.to,
      fromId,
      toId,
      segments: c.segments,
      tiles: c.tiles,
      fromEdge: c.fromEdge,
      toEdge: c.toEdge,
    });
    // The system's own edge faces the lane tile; its dock is the lane tile's port on that edge.
    for (const [sys, edge] of [[c.from, c.fromEdge], [c.to, c.toEdge]] as const) {
      const lane = neighbour(sys, edge);
      model.usedDocks.add(portKey(lane, (edge + 3) % 6));
    }
    const add = (a: string, b: string, id: string) => {
      const list = model.partners.get(a) ?? [];
      list.push({ key: b, systemId: id });
      model.partners.set(a, list);
    };
    add(c.from, c.to, toId);
    add(c.to, c.from, fromId);
  }
  for (const list of model.partners.values()) {
    list.sort((a, b) => a.systemId.localeCompare(b.systemId, undefined, { numeric: true }));
  }
  return model;
}

/** "Connected through hyperlanes to system 27 and 41", or null when nothing is linked. */
export function linkSentence(model: HyperlaneNetworkModel, key: string): string | null {
  const ids = (model.partners.get(key) ?? []).map((p) => p.systemId);
  if (ids.length === 0) return null;
  const list = ids.length === 1 ? ids[0] : `${ids.slice(0, -1).join(", ")} and ${ids[ids.length - 1]}`;
  return `Connected through hyperlanes to system${ids.length > 1 ? "s" : ""} ${list}`;
}

/** Connections that involve this tile as an end system or as a lane tile on the route. */
export const connectionsAt = (model: HyperlaneNetworkModel, key: string | null): HyperlaneConnection[] =>
  key === null
    ? []
    : model.connections.filter((c) => c.fromKey === key || c.toKey === key || c.tiles.includes(key));

/** World point of a port, given the tile centre. */
export const portPoint = (cx: number, cy: number, edge: number, radius = EDGE_RADIUS) => {
  const p = edgePoint(edge, radius);
  return { x: cx + p.x, y: cy + p.y };
};

// ---- memoisation: one analysis per distinct tile arrangement -------------------------------

import PRECOMPUTED from "./hyperlaneRoutes.generated.json";

/** Stable key of a layout: every tile with its id and position, sorted. */
export function layoutKey(input: readonly NetTileInput[]): string {
  return input
    .map((t) => `${t.systemId}@${t.q},${t.r}`)
    .sort()
    .join(";");
}

/**
 * Routes depend only on the hyperlane tiles (id and place) and on which other places hold a
 * system, not on which system, so the table is keyed by that shape.
 */
export function shapeKey(input: readonly NetTileInput[]): string {
  return input
    .map((t) => (parseHyperlaneId(t.systemId) ? `${t.systemId.toLowerCase()}@${t.q},${t.r}` : `s@${t.q},${t.r}`))
    .sort()
    .join(";");
}

/** FNV-1a of the shape key: the key of the generated table. */
export function layoutHash(input: readonly NetTileInput[]): string {
  let h = 0x811c9dc5;
  for (const ch of shapeKey(input)) {
    h ^= ch.charCodeAt(0);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(36);
}

const cache = new Map<string, HyperlaneNetworkModel>();
const CACHE_LIMIT = 48;
export const networkStats = { computed: 0, fromTable: 0, hits: 0 };
const table = PRECOMPUTED as Record<string, RouteRecord[]>;

/**
 * The model of a layout, computed once. Board, preview, thumbnails and inspector share this cache
 * (same input arrangement gives the same object). Layouts in the generated table
 * (`hyperlaneRoutes.generated.json`, made by `npm run hyperlane:table`, checked for staleness by a
 * test) skip the route search. Display-only: nothing here is stored in game state.
 */
export function getHyperlaneNetwork(input: readonly NetTileInput[]): HyperlaneNetworkModel {
  if (!input.some((t) => parseHyperlaneId(t.systemId))) return EMPTY;
  const key = layoutKey(input);
  const hit = cache.get(key);
  if (hit) {
    networkStats.hits++;
    return hit;
  }
  const preset = table[layoutHash(input)];
  const model = analyzeHyperlanes(input, preset);
  if (preset) networkStats.fromTable++;
  else networkStats.computed++;
  if (cache.size >= CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
  cache.set(key, model);
  return model;
}

export const clearHyperlaneCache = () => cache.clear();

const EMPTY: HyperlaneNetworkModel = analyzeHyperlanes([]);
