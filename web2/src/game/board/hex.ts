import type { SystemId, TileView } from "../../model";

export const HEX_R = 46;

export interface Point {
  x: number;
  y: number;
}
export interface Frame {
  w: number;
  h: number;
  cx: number;
  cy: number;
}

/** Centre of a system from its axial coordinates. Compare `GalaxyLayout.placements`. */
export const hexCenter = (tile: { q: number; r: number }): Point => ({
  x: HEX_R * Math.sqrt(3) * (tile.q + tile.r / 2),
  y: HEX_R * 1.5 * tile.r,
});

export const hexPoints = ({ x, y }: Point, radius: number) =>
  Array.from({ length: 6 }, (_, index) => {
    const angle = ((index * 60 - 30) * Math.PI) / 180;
    return `${(x + radius * Math.cos(angle)).toFixed(1)},${(y + radius * Math.sin(angle)).toFixed(1)}`;
  }).join(" ");

const adjacent = (a: { q: number; r: number }, b: { q: number; r: number }) => {
  const dq = a.q - b.q;
  const dr = a.r - b.r;
  return Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr)) === 1;
};

/** A line through the systems of a path. A hop between systems that do not touch, a wormhole, is an arc. */
export function routePath(path: SystemId[], tiles: Record<SystemId, TileView>) {
  return path
    .map((id, index) => {
      const { x, y } = hexCenter(tiles[id]);
      if (!index) {
        return `M${x.toFixed(1)} ${y.toFixed(1)}`;
      }
      const before = tiles[path[index - 1]];
      if (adjacent(before, tiles[id])) {
        return `L${x.toFixed(1)} ${y.toFixed(1)}`;
      }
      const from = hexCenter(before);
      const mx = (from.x + x) / 2;
      const my = (from.y + y) / 2;
      return `Q${(mx + (y - from.y) * 0.18).toFixed(1)} ${(my - (x - from.x) * 0.18).toFixed(1)} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(" ");
}

export function frame(tiles: TileView[], padX = HEX_R * 1.4, padY = HEX_R * 1.7): Frame {
  const points = tiles.map(hexCenter);
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return {
    w: Math.max(...xs) - Math.min(...xs) + padX * 2,
    h: Math.max(...ys) - Math.min(...ys) + padY * 2,
    cx: (Math.max(...xs) + Math.min(...xs)) / 2,
    cy: (Math.max(...ys) + Math.min(...ys)) / 2,
  };
}

/** The frame of the whole board. */
export const boardFrame = (tiles: TileView[]) => frame(tiles, HEX_R * 0.87 + 14, HEX_R + 44);
