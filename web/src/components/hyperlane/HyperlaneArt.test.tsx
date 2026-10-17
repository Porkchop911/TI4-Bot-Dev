import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HyperlaneDiagram } from "./HyperlaneArt.tsx";
import {
  EDGE_RADIUS,
  HYPERLANE_FACES,
  edgePoint,
  hyperlaneEdges,
  hyperlaneLabel,
  hyperlaneTitle,
  lanePath,
  parseHyperlaneId,
} from "./hyperlaneGeometry.ts";
import { MapThumbnail } from "../MapThumbnail.tsx";

const systems = JSON.parse(
  readFileSync(resolve(process.cwd(), "../crates/ti4-content/content/systems.json"), "utf8"),
) as { id: string; isHyperlane?: boolean; aliases?: string[] }[];
const corpus = systems.filter((s) => s.isHyperlane);

describe("hyperlane geometry", () => {
  it("knows every hyperlane tile id of the content, and only those", () => {
    expect(corpus).toHaveLength(108);
    for (const s of corpus) {
      const tile = parseHyperlaneId(s.id);
      expect(tile, s.id).not.toBeNull();
      expect(tile!.lanes.length, s.id).toBeGreaterThan(0);
      for (const [a, b] of tile!.lanes) {
        expect(a).toBeGreaterThanOrEqual(0);
        expect(b).toBeLessThanOrEqual(5);
        expect(a).toBeLessThan(b);
      }
    }
    expect(parseHyperlaneId("18")).toBeNull();
    expect(parseHyperlaneId("83c")).toBeNull();
    expect(parseHyperlaneId("83a7")).toBeNull();
    expect(parseHyperlaneId("83a100")).toBeNull();
    expect(parseHyperlaneId("92a")).toBeNull();
    expect(parseHyperlaneId(undefined)).toBeNull();
  });

  it("gives the map-string alias (trailing 0-5) the same lanes as the corpus id", () => {
    for (const s of corpus) {
      for (const alias of s.aliases ?? []) {
        expect(parseHyperlaneId(alias), `${alias} for ${s.id}`).toEqual(parseHyperlaneId(s.id));
      }
    }
    expect(parseHyperlaneId("83A2")).toEqual(parseHyperlaneId("83a120"));
  });

  it("turns lanes by one edge per 60 degrees", () => {
    for (const face of Object.keys(HYPERLANE_FACES)) {
      for (let turn = 0; turn < 6; turn++) {
        const id = turn === 0 ? face : `${face}${turn * 60}`;
        const tile = parseHyperlaneId(id)!;
        expect(tile.turn).toBe(turn);
        const expected = HYPERLANE_FACES[face]
          .map(([a, b]) => [(a + turn) % 6, (b + turn) % 6].sort((x, y) => x - y))
          .sort((p, q) => p[0] - q[0] || p[1] - q[1]);
        expect(tile.lanes).toEqual(expected);
      }
    }
  });

  it("draws each lane between the midpoints of its two edges", () => {
    for (const s of corpus) {
      for (const pair of parseHyperlaneId(s.id)!.lanes) {
        const d = lanePath(pair);
        const nums = d.match(/-?\d+(\.\d+)?/g)!.map(Number);
        const [ax, ay] = [nums[0], nums[1]];
        const [bx, by] = [nums[nums.length - 2], nums[nums.length - 1]];
        const a = edgePoint(pair[0]);
        const b = edgePoint(pair[1]);
        expect(ax).toBeCloseTo(a.x, 1);
        expect(ay).toBeCloseTo(a.y, 1);
        expect(bx).toBeCloseTo(b.x, 1);
        expect(by).toBeCloseTo(b.y, 1);
        expect(Math.hypot(a.x, a.y)).toBeCloseTo(EDGE_RADIUS, 5);
      }
    }
  });

  it("labels edges counted from 1", () => {
    expect(hyperlaneLabel(parseHyperlaneId("83a")!)).toBe("Hyperlane connecting edges 2-5");
    expect(hyperlaneLabel(parseHyperlaneId("90a")!)).toBe("Hyperlane connecting edges 2-6 and 3-5");
    expect(hyperlaneLabel(parseHyperlaneId("83b")!)).toBe("Hyperlane connecting edges 1-3, 1-4 and 4-6");
    expect(hyperlaneTitle(parseHyperlaneId("83a")!)).toContain("SE to NW");
    expect(hyperlaneEdges(parseHyperlaneId("83b")!)).toEqual([0, 2, 3, 5]);
  });
});

describe("hyperlane edges on the real community maps", () => {
  // Ring-1 order of the engine's Hex::ring: edge k looks at this neighbour.
  const DIRS = [[-1, 1], [0, 1], [1, 0], [1, -1], [0, -1], [-1, 0]];
  const dir = resolve(process.cwd(), "e2e/screenshots/CO-community-maps/server-snapshot");
  const files = readdirSync(dir).filter((f) => f.endsWith("p.json"));

  it("agree between neighbouring hyperlane tiles: a lane end on one side meets a lane end on the other", () => {
    let pairs = 0;
    for (const file of files) {
      const { previews } = JSON.parse(readFileSync(resolve(dir, file), "utf8")) as {
        previews: Record<string, { tiles: { system_id: string; q: number; r: number; hyperlane?: boolean }[] }>;
      };
      for (const [alias, preview] of Object.entries(previews)) {
        const lanes = new Map(
          preview.tiles
            .filter((t) => t.hyperlane)
            .map((t) => [`${t.q},${t.r}`, new Set(hyperlaneEdges(parseHyperlaneId(t.system_id)!))]),
        );
        for (const [key, edges] of lanes) {
          const [q, r] = key.split(",").map(Number);
          DIRS.forEach(([dq, dr], k) => {
            const other = lanes.get(`${q + dq},${r + dr}`);
            if (!other) return;
            const back = DIRS.findIndex(([a, b]) => a === -dq && b === -dr);
            expect(edges.has(k), `${alias} ${key} edge ${k}`).toBe(other.has(back));
            pairs++;
          });
        }
      }
    }
    expect(pairs).toBeGreaterThan(100);
  });
});

describe("hyperlane art", () => {
  const wrap = (el: React.ReactNode) => render(<svg>{el}</svg>);

  it("offers a stand-alone diagram with an accessible name", () => {
    wrap(<HyperlaneDiagram systemId="83a60" />);
    expect(screen.getByRole("img", { name: "Hyperlane connecting edges 3-6" })).toBeInTheDocument();
  });

  it("draws one network layer for the thumbnail, once, and none without hyperlanes", () => {
    const { unmount } = render(
      <MapThumbnail
        label="Test layout"
        seats={[]}
        tiles={[
          { system_id: "83a120", label: "Hyperlane", q: 1, r: 0, hyperlane: true },
          { system_id: "26", label: "Lodor", q: 0, r: 1, planets: [] },
        ]}
      />,
    );
    expect(screen.getAllByTestId("hyperlane-network")).toHaveLength(1);
    unmount();
    render(<MapThumbnail label="Plain" seats={[]} tiles={[{ system_id: "26", label: "Lodor", q: 0, r: 1 }]} />);
    expect(screen.queryByTestId("hyperlane-network")).toBeNull();
  });
});
