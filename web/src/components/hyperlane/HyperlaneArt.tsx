import React from "react";
import {
  EDGE_ANGLES,
  EDGE_RADIUS,
  edgePoint,
  hyperlaneEdges,
  hyperlaneLabel,
  hyperlaneTitle,
  lanePath,
  parseHyperlaneId,
} from "./hyperlaneGeometry.ts";
import {
  portPoint,
  tileKey,
  type HyperlaneNetworkModel,
  type NetTileInput,
} from "./hyperlaneNetwork.ts";

const INK = "var(--hyperlane-ink, #c4b5fd)";
/** The tile colour, so the inner line of a lane reads as a hollow channel. */
const BED = "var(--hyperlane-bed, #1e1b4b)";

/**
 * Flat lane art for one hyperlane tile, centred on 0,0 (apply `translate` outside). Each lane is a
 * double line (ink outline around a channel in the tile colour), so it reads by shape and weight,
 * not by colour. A ring cap sits on every edge a lane reaches; it half-overlaps the neighbour, and
 * where that neighbour is a matching hyperlane the two halves close into one ring.
 */
const Lanes: React.FC<{ systemId: string; weight: number; frame: boolean }> = ({ systemId, weight, frame }) => {
  const tile = parseHyperlaneId(systemId);
  if (!tile) return null;
  const edges = hyperlaneEdges(tile);
  return (
    <g>
      {frame && (
        <polygon
          points={Array.from({ length: 6 }, (_, k) => {
            const a = Math.PI / 6 + (Math.PI / 3) * k;
            return `${(63 * Math.cos(a)).toFixed(1)},${(63 * Math.sin(a)).toFixed(1)}`;
          }).join(" ")}
          fill="none"
          stroke={INK}
          strokeWidth={1}
          strokeDasharray="2 5"
          strokeLinecap="round"
          opacity={0.4}
        />
      )}
      <g fill="none" strokeLinecap="round">
        {tile.lanes.map((pair) => (
          <path key={pair.join("-")} d={lanePath(pair)} stroke={INK} strokeWidth={9 * weight} />
        ))}
        {tile.lanes.map((pair) => (
          <path key={`in-${pair.join("-")}`} d={lanePath(pair)} stroke={BED} strokeWidth={3.6 * weight} />
        ))}
      </g>
      {edges.map((e) => {
        const p = edgePoint(e, EDGE_RADIUS - 1);
        return (
          <g key={e}>
            <circle cx={p.x} cy={p.y} r={7.5 * weight} fill={INK} />
            <circle cx={p.x} cy={p.y} r={3 * weight} fill={BED} />
          </g>
        );
      })}
    </g>
  );
};

/** Distance between neighbouring hexes' edges on every board (centres 150 apart, radius 70). */
const GAP = 150 - 2 * EDGE_RADIUS;

export interface NetworkTile extends NetTileInput {
  /** Centre in the drawing's coordinates. */
  cx: number;
  cy: number;
}

/**
 * The whole lane network as one drawing, to be placed after every tile fill and before nothing
 * but overlays. Pass order gives the seamless look: all lane outlines first, then all channels
 * (so lanes merge across tile edges and at junctions with one consistent double line), then the
 * ends: a ring dock on the system a lane reaches, a flat bar on an open end, a diamond where three
 * or more lanes meet. Every lane is drawn, so every chosen route is traceable along drawn lanes.
 */
export const HyperlaneNetwork: React.FC<{
  tiles: readonly NetworkTile[];
  model: HyperlaneNetworkModel;
  weight?: number;
  bed?: string;
}> = ({ tiles, model, weight = 1, bed }) => {
  const lanes = tiles
    .map((t) => ({ t, key: tileKey(t.q, t.r), tile: parseHyperlaneId(t.systemId) }))
    .filter((x): x is { t: NetworkTile; key: string; tile: NonNullable<typeof x.tile> } => x.tile !== null);
  if (lanes.length === 0) return null;
  const at = (key: string) => lanes.find((l) => l.key === key);
  const paths = (stroke: string, width: number, pick?: (seg: string) => boolean, extra?: object) =>
    lanes.map(({ t, key, tile }) => (
      <g key={key} transform={`translate(${t.cx} ${t.cy})`}>
        {tile.lanes
          .filter(([a, b]) => !pick || pick(`${key}:${a}-${b}`))
          .map((pair) => (
            <path key={pair.join("-")} d={lanePath(pair)} stroke={stroke} strokeWidth={width} {...extra} />
          ))}
      </g>
    ));
  const world = (port: string) => {
    const [key, e] = port.split(":");
    const l = at(key);
    return l ? { l, e: Number(e), ...portPoint(l.t.cx, l.t.cy, Number(e)) } : null;
  };
  const w = weight;
  // Hexes sit GAP apart on the board; lanes are extended across it so a lane runs on unbroken
  // into the next lane tile (half the gap from each side) or all the way to a system's edge.
  const reach = (port: string) => (model.docks.has(port) ? GAP : model.openEnds.has(port) ? 0 : GAP / 2);
  const stubPorts = [...model.lanePorts].filter((port) => reach(port) > 0);
  const stubs = (stroke: string, width: number, pick?: (port: string) => boolean, extra?: object) =>
    stubPorts
      .filter((port) => !pick || pick(port))
      .map((port) => {
        const p = world(port);
        if (!p) return null;
        const q = edgePoint(p.e, EDGE_RADIUS + reach(port));
        return (
          <line
            key={`${stroke}-${port}`}
            x1={p.x}
            y1={p.y}
            x2={p.x - edgePoint(p.e, EDGE_RADIUS).x + q.x}
            y2={p.y - edgePoint(p.e, EDGE_RADIUS).y + q.y}
            stroke={stroke}
            strokeWidth={width}
            {...extra}
          />
        );
      });
  return (
    <g
      data-testid="hyperlane-network"
      data-hyperlane-routes={model.connections.length}
      pointerEvents="none"
      aria-hidden="true"
      style={bed ? ({ "--hyperlane-bed": bed } as React.CSSProperties) : undefined}
    >
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        {paths(INK, 9 * w)}
        {stubs(INK, 9 * w)}
        {paths(BED, 3.6 * w)}
        {stubs(BED, 3.6 * w)}
      </g>
      {[...model.docks.keys()].map((port) => {
        const p = world(port);
        if (!p) return null;
        const out = edgePoint(p.e, GAP + 3 * w);
        const base = edgePoint(p.e, 0);
        const used = model.usedDocks.has(port);
        return (
          <g key={`dock-${port}`} data-testid="hyperlane-dock" data-used={used}>
            <circle cx={p.x + out.x - base.x} cy={p.y + out.y - base.y} r={7.5 * w} fill={INK} />
            <circle cx={p.x + out.x - base.x} cy={p.y + out.y - base.y} r={3.2 * w} fill={BED} />
          </g>
        );
      })}
      {[...model.openEnds].map((port) => {
        const p = world(port);
        if (!p) return null;
        const a = (EDGE_ANGLES[p.e] * Math.PI) / 180;
        const tx = -Math.sin(a) * 7.5 * w;
        const ty = Math.cos(a) * 7.5 * w;
        return (
          <line
            key={`open-${port}`}
            data-testid="hyperlane-open-end"
            x1={p.x - tx}
            y1={p.y - ty}
            x2={p.x + tx}
            y2={p.y + ty}
            stroke={INK}
            strokeWidth={3 * w}
            strokeLinecap="butt"
          />
        );
      })}
      {[...model.junctions].map((port) => {
        const p = world(port);
        if (!p) return null;
        const r = 6.5 * w;
        const shift = model.openEnds.has(port) ? { x: 0, y: 0 } : edgePoint(p.e, GAP / 2);
        return (
          <g
            key={`junction-${port}`}
            data-testid="hyperlane-junction"
            transform={`translate(${p.x + shift.x} ${p.y + shift.y}) rotate(45)`}
          >
            <rect x={-r * 0.7} y={-r * 0.7} width={r * 1.4} height={r * 1.4} fill={INK} />
            <rect x={-r * 0.35} y={-r * 0.35} width={r * 0.7} height={r * 0.7} fill={BED} />
          </g>
        );
      })}
    </g>
  );
};

/** Stand-alone HTML diagram for the inspector: a hex with its lanes and an accessible name. */
export const HyperlaneDiagram: React.FC<{ systemId: string; size?: number }> = ({ systemId, size = 72 }) => {
  const tile = parseHyperlaneId(systemId);
  if (!tile) return null;
  const label = hyperlaneLabel(tile);
  return (
    <svg
      width={size}
      height={size}
      viewBox="-76 -76 152 152"
      role="img"
      aria-label={label}
      data-testid="hyperlane-diagram"
      style={{ flex: "none" }}
    >
      <title>{hyperlaneTitle(tile)}</title>
      <polygon
        points={Array.from({ length: 6 }, (_, k) => {
          const a = Math.PI / 6 + (Math.PI / 3) * k;
          return `${(70 * Math.cos(a)).toFixed(1)},${(70 * Math.sin(a)).toFixed(1)}`;
        }).join(" ")}
        fill={BED}
        stroke="#334155"
        strokeWidth={2}
      />
      <Lanes systemId={systemId} weight={1} frame />
    </svg>
  );
};
