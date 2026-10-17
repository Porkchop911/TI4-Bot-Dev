/**
 * Hyperlane tiles: which hex edges each tile joins.
 *
 * The content corpus only flags a tile `isHyperlane` (ids `83a`..`91b`, plus `<id><degrees>` for
 * the five turned copies, e.g. `83a120`); it carries no lane data, and neither does the engine
 * (`crates/ti4-content/src/galaxy.rs`). This table is draw-only: it never feeds game rules.
 *
 * Edges are numbered 0..5 in the order of the engine's `Hex::ring(1)`: edge 0 faces the
 * south-west neighbour, then SE, E, NE, NW, W (the engine's ring order, which runs
 * counter-clockwise on the screen). That is the same
 * order the map templates use for ring positions, so a tile's edge `k` looks at the neighbour that
 * sits at ring-1 slot `k + 1` of that tile. A tile turned by `d` degrees has every edge shifted by
 * `d / 60`. The edge pairs are the printed lanes of the nine two-sided PoK tiles (face A or B).
 */

export type EdgePair = readonly [number, number];

/** Lanes of every face at rotation 0. */
export const HYPERLANE_FACES: Readonly<Record<string, readonly EdgePair[]>> = {
  "83a": [[1, 4]],
  "83b": [[0, 2], [0, 3], [3, 5]],
  "84a": [[2, 5]],
  "84b": [[0, 3], [0, 4], [1, 3]],
  "85a": [[1, 5]],
  "85b": [[0, 2], [0, 3], [3, 5]],
  "86a": [[1, 5]],
  "86b": [[0, 3], [0, 4], [1, 3]],
  "87a": [[0, 2], [2, 4], [2, 5]],
  "87b": [[0, 2], [0, 3]],
  "88a": [[0, 4], [1, 4], [2, 4]],
  "88b": [[0, 2], [0, 3], [3, 5]],
  "89a": [[0, 2], [0, 4], [2, 4]],
  "89b": [[0, 3], [0, 4]],
  "90a": [[1, 5], [2, 4]],
  "90b": [[0, 3], [0, 4]],
  "91a": [[0, 3], [0, 4], [1, 3]],
  "91b": [[0, 2], [0, 3]],
};

export interface HyperlaneTile {
  face: string;
  /** Steps of 60 degrees in edge-number order, 0..5. */
  turn: number;
  /** Sorted, normalised edge pairs after the turn. */
  lanes: EdgePair[];
}

/** Hex apothem for the board's radius-70 hexes: where an edge midpoint sits from the centre. */
export const EDGE_RADIUS = 70 * Math.cos(Math.PI / 6);

/** Screen angle (degrees, 0 = east, growing downwards as in SVG) of each edge midpoint. */
export const EDGE_ANGLES = [120, 60, 0, 300, 240, 180] as const;
const COMPASS = ["SW", "SE", "E", "NE", "NW", "W"] as const;

/**
 * Reads `83a`, `83a120` (corpus ids) and `83a2` / `83A2` (map-string spelling: trailing 0-5).
 * Returns null for anything that is not one of the 108 hyperlane ids.
 */
export function parseHyperlaneId(id: string | null | undefined): HyperlaneTile | null {
  const m = /^(8[3-9]|9[01])([ab])(\d{0,3})$/.exec((id ?? "").trim().toLowerCase());
  if (!m) return null;
  const face = `${m[1]}${m[2]}`;
  const tail = m[3];
  let turn = 0;
  if (tail !== "") {
    if (tail.length === 1) {
      turn = Number(tail);
      if (turn > 5) return null;
    } else {
      const deg = Number(tail);
      if (deg % 60 !== 0 || deg >= 360) return null;
      turn = deg / 60;
    }
  }
  const base = HYPERLANE_FACES[face];
  if (!base) return null;
  const lanes = base.map(([a, b]) => {
    const x = (a + turn) % 6;
    const y = (b + turn) % 6;
    return (x < y ? [x, y] : [y, x]) as EdgePair;
  });
  lanes.sort((p, q) => p[0] - q[0] || p[1] - q[1]);
  return { face, turn, lanes };
}

/** Distinct edges a tile touches, ascending. */
export const hyperlaneEdges = (tile: HyperlaneTile): number[] =>
  [...new Set(tile.lanes.flat())].sort((a, b) => a - b);

/** Point on the hex outline at an edge midpoint, relative to the hex centre. */
export function edgePoint(edge: number, radius = EDGE_RADIUS): { x: number; y: number } {
  const a = (EDGE_ANGLES[edge] * Math.PI) / 180;
  return { x: radius * Math.cos(a), y: radius * Math.sin(a) };
}

const f = (n: number) => Number(n.toFixed(1));

/**
 * SVG path of one lane, relative to the hex centre. Opposite edges give a straight line through
 * the middle; edges two apart bend round the centre (control point at the centre); neighbouring
 * edges, which no printed tile uses, bow gently inwards.
 */
export function lanePath([a, b]: EdgePair): string {
  const p = edgePoint(a);
  const q = edgePoint(b);
  const gap = Math.min((b - a + 6) % 6, (a - b + 6) % 6);
  if (gap === 3) return `M${f(p.x)} ${f(p.y)}L${f(q.x)} ${f(q.y)}`;
  const k = gap === 2 ? 0 : 0.35;
  return `M${f(p.x)} ${f(p.y)}Q${f(k * (p.x + q.x))} ${f(k * (p.y + q.y))} ${f(q.x)} ${f(q.y)}`;
}

/** "1-4 and 2-5" with edges counted from 1. */
export function describeLanes(tile: HyperlaneTile): string {
  const parts = tile.lanes.map(([a, b]) => `${a + 1}-${b + 1}`);
  return parts.length > 1 ? `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}` : parts[0];
}

/** `Hyperlane connecting edges 1-4 and 2-5` (or `edges 1-3, 1-4 and 4-6`). */
export function hyperlaneLabel(tile: HyperlaneTile): string {
  return `Hyperlane connecting edges ${describeLanes(tile)}`;
}

/** Longer hover text with compass directions as drawn on the board. */
export function hyperlaneTitle(tile: HyperlaneTile): string {
  const dirs = tile.lanes.map(([a, b]) => `${COMPASS[a]} to ${COMPASS[b]}`).join("; ");
  return `${hyperlaneLabel(tile)} (${dirs}). Ships may travel along a lane to the neighbouring tile at either end.`;
}

export const edgeName = (edge: number): string => COMPASS[edge];
