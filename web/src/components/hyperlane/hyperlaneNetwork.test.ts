import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it } from "vitest";
import {
  analyzeHyperlanes,
  clearHyperlaneCache,
  getHyperlaneNetwork,
  layoutHash,
  linkSentence,
  networkStats,
  tileKey,
  type NetTileInput,
  type RouteRecord,
} from "./hyperlaneNetwork.ts";
import { parseHyperlaneId } from "./hyperlaneGeometry.ts";

const t = (systemId: string, q: number, r: number): NetTileInput => ({ systemId, q, r });
const ids = (m: ReturnType<typeof analyzeHyperlanes>) =>
  m.connections.map((c) => [c.fromId, c.toId].sort().join("-")).sort();

describe("hyperlane routes on synthetic layouts", () => {
  it("joins two systems through a chain of straight lanes (SE to NW)", () => {
    const m = analyzeHyperlanes([t("A", 0, 0), t("83a", 0, 1), t("83a", 0, 2), t("83a", 0, 3), t("B", 0, 4)]);
    expect(ids(m)).toEqual(["A-B"]);
    expect(m.connections[0].segments).toEqual(["0,1:1-4", "0,2:1-4", "0,3:1-4"]);
    expect(m.connections[0].tiles).toEqual(["0,1", "0,2", "0,3"]);
    expect(m.usedDocks.size).toBe(2);
    expect(linkSentence(m, "0,0")).toBe("Connected through hyperlanes to system B");
    expect(m.openEnds.size).toBe(0);
  });

  it("does not join across a gap or a mismatched lane end", () => {
    const gap = analyzeHyperlanes([t("A", 0, 0), t("83a", 0, 1), t("B", 0, 3)]);
    expect(gap.connections).toEqual([]);
    expect([...gap.openEnds]).toEqual(["0,1:4".replace("4", "1")]);
    // 83a turned by 60 degrees has lanes 2-5, so it does not meet an SE/NW neighbour
    const bad = analyzeHyperlanes([t("A", 0, 0), t("83a", 0, 1), t("83a60", 0, 2), t("B", 0, 3)]);
    expect(bad.connections).toEqual([]);
    expect(bad.openEnds.size).toBeGreaterThan(0);
  });

  it("does not turn at a shared lane end: branches of a three-lane tile join the trunk, not each other", () => {
    // 88a lanes 1-5, 2-5, 3-5 (0-based 0-4, 1-4, 2-4) share the NW end.
    const m = analyzeHyperlanes([t("88a", 0, 0), t("X", 0, -1), t("A", -1, 1), t("B", 0, 1), t("C", 1, 0)]);
    expect(ids(m)).toEqual(["A-X", "B-X", "C-X"]);
    expect(m.junctions.size).toBe(1);
    for (const c of m.connections) expect(c.segments).toHaveLength(1);
  });

  it("Learn to Play: 39 reaches 41, 41 and 80 reach 75, and 39 and 80 are not joined", () => {
    const m = analyzeHyperlanes([
      t("39", 0, -1), t("85a", 1, -1), t("88a", 2, -1), t("80", 3, -1),
      t("41", 1, 0), t("75", 2, -2),
    ]);
    expect(ids(m)).toEqual(["39-41", "41-75", "75-80"]);
  });

  it("terminates on a loop and keeps the direct lane", () => {
    // 89a: a triangle of lanes 0-2, 0-4, 2-4.
    const m = analyzeHyperlanes([t("89a", 0, 0), t("A", -1, 1), t("B", 1, 0), t("C", 0, -1)]);
    expect(ids(m)).toEqual(["A-B", "A-C", "B-C"]);
    for (const c of m.connections) expect(c.segments).toHaveLength(1);
  });

  it("keeps one route per pair, the shortest, over longer detours through other tiles", () => {
    // 89a triangle whose ports 0 and 2 continue in a second 89a-type tile making a longer way.
    const m = analyzeHyperlanes([
      t("89a", 0, 0),
      t("A", -1, 1),
      t("B", 1, 0),
      t("C", 0, -1),
    ]);
    const keys = m.connections.map((c) => `${c.fromKey}|${c.toKey}`);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("is independent of input order", () => {
    const tiles = [t("88a", 0, 0), t("X", 0, -1), t("A", -1, 1), t("B", 0, 1), t("C", 1, 0), t("83a", 1, 1), t("D", 1, 2)];
    const forward = analyzeHyperlanes(tiles);
    const backward = analyzeHyperlanes([...tiles].reverse());
    expect(backward.connections).toEqual(forward.connections);
    expect([...backward.usedSegments].sort()).toEqual([...forward.usedSegments].sort());
  });

  it("has no systems linked by a map without hyperlane tiles", () => {
    const m = getHyperlaneNetwork([t("26", 0, 0), t("27", 1, 0)]);
    expect(m.connections).toEqual([]);
  });
});

describe("hyperlane routes on the real community maps", () => {
  const dir = resolve(process.cwd(), "e2e/screenshots/CO-community-maps/server-snapshot");
  type Tile = { system_id: string; q: number; r: number; special_area?: string | null };
  const layouts: { name: string; tiles: NetTileInput[] }[] = [];
  for (const file of readdirSync(dir).filter((f) => f.endsWith("p.json")).sort()) {
    const { previews } = JSON.parse(readFileSync(resolve(dir, file), "utf8")) as {
      previews: Record<string, { tiles: Tile[] }>;
    };
    for (const [alias, p] of Object.entries(previews)) {
      const tiles = p.tiles.filter((x) => !x.special_area).map((x) => t(x.system_id, x.q, x.r));
      if (tiles.some((x) => parseHyperlaneId(x.systemId))) layouts.push({ name: alias, tiles });
    }
  }

  it("finds hyperlane layouts", () => {
    expect(layouts.length).toBeGreaterThan(5);
  });

  it("every chosen route is a real chain of printed lanes, one route per pair", () => {
    for (const { name, tiles } of layouts) {
      const model = analyzeHyperlanes(tiles);
      const byKey = new Map(tiles.map((x) => [tileKey(x.q, x.r), x]));
      const drawn = model.connections.map((c) => [c.fromKey, c.toKey].sort().join("|"));
      expect(new Set(drawn).size, `${name}: one route per pair`).toBe(drawn.length);
      for (const c of model.connections) {
        expect(c.segments.length, name).toBeGreaterThan(0);
        for (const s of c.segments) {
          const [k, ab] = s.split(":");
          const [a, b] = ab.split("-").map(Number);
          expect(parseHyperlaneId(byKey.get(k)!.systemId)!.lanes.some((l) => l[0] === a && l[1] === b), `${name} ${s}`).toBe(true);
        }
      }
    }
  });
});

describe("memoisation and the generated table", () => {
  beforeEach(() => clearHyperlaneCache());
  const layout = [t("A", 0, 0), t("83a", 0, 1), t("B", 0, 2)];

  it("computes a layout once and returns the same object afterwards", () => {
    const before = { ...networkStats };
    const first = getHyperlaneNetwork(layout);
    const again = getHyperlaneNetwork([...layout].reverse());
    expect(again).toBe(first);
    expect(networkStats.computed - before.computed).toBe(1);
    expect(networkStats.hits - before.hits).toBe(1);
  });

  const dir = resolve(process.cwd(), "e2e/screenshots/CO-community-maps/server-snapshot");
  const tablePath = resolve(process.cwd(), "src/components/hyperlane/hyperlaneRoutes.generated.json");
  const fresh: Record<string, RouteRecord[]> = {};
  for (const file of readdirSync(dir).filter((f) => f.endsWith("p.json")).sort()) {
    const { previews } = JSON.parse(readFileSync(resolve(dir, file), "utf8")) as {
      previews: Record<string, { tiles: { system_id: string; q: number; r: number; special_area?: string | null }[] }>;
    };
    for (const p of Object.values(previews)) {
      const tiles = p.tiles.filter((x) => !x.special_area).map((x) => t(x.system_id, x.q, x.r));
      if (!tiles.some((x) => parseHyperlaneId(x.systemId))) continue;
      fresh[layoutHash(tiles)] = analyzeHyperlanes(tiles).connections.map(
        ({ fromId: _f, toId: _t, ...record }) => record,
      );
    }
  }
  const sorted = Object.fromEntries(Object.entries(fresh).sort(([a], [b]) => (a < b ? -1 : 1)));

  it("ships a table equal to a fresh computation (run `npm run hyperlane:table` to refresh)", () => {
    if (process.env.UPDATE_HYPERLANE_TABLE) writeFileSync(tablePath, JSON.stringify(sorted) + "\n");
    expect(JSON.parse(readFileSync(tablePath, "utf8"))).toEqual(sorted);
    expect(Object.keys(sorted).length).toBeGreaterThan(5);
  });

  it("answers a catalogue layout from the table without searching", () => {
    const [hash] = Object.keys(sorted);
    expect(hash).toBeTruthy();
    const dirFiles = readdirSync(dir).filter((f) => f.endsWith("p.json"));
    let tiles: NetTileInput[] | undefined;
    for (const file of dirFiles) {
      const { previews } = JSON.parse(readFileSync(resolve(dir, file), "utf8")) as {
        previews: Record<string, { tiles: { system_id: string; q: number; r: number; special_area?: string | null }[] }>;
      };
      for (const p of Object.values(previews)) {
        const cand = p.tiles.filter((x) => !x.special_area).map((x) => t(x.system_id, x.q, x.r));
        if (layoutHash(cand) === hash) tiles = cand;
      }
    }
    const before = { ...networkStats };
    const model = getHyperlaneNetwork(tiles!);
    expect(networkStats.fromTable - before.fromTable).toBe(1);
    expect(networkStats.computed - before.computed).toBe(0);
    expect(model.connections).toEqual(analyzeHyperlanes(tiles!).connections);
  });
});
