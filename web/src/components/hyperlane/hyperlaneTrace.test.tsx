import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MapPreviewBoard } from "../MapPreviewBoard.tsx";
import { MapThumbnail } from "../MapThumbnail.tsx";
import { EDGE_DIRS, analyzeHyperlanes, tileKey } from "./hyperlaneNetwork.ts";
import { lanePath, parseHyperlaneId } from "./hyperlaneGeometry.ts";
import { deriveHexGeometry } from "../../presentation/boardPresentation.ts";
import type { MapPreviewDto } from "../../protocol/types.ts";

/**
 * The rule as the TTPG mod traces it (`adjacency-hyperlane.js`), written again here: from a system
 * edge enter the neighbouring lane tile; inside a tile take a printed lane that starts on the
 * entry edge to its other end; the neighbour across that edge is a system (adjacent) or another
 * lane tile (entered through the opposite edge). No turning at shared ends.
 */
function expectedPairs(tiles: { system_id: string; q: number; r: number }[]): Set<string> {
  const cells = new Map(tiles.map((t) => [`${t.q},${t.r}`, t]));
  const out = new Set<string>();
  for (const s of tiles) {
    if (parseHyperlaneId(s.system_id)) continue;
    const seen = new Set<string>();
    const queue: [string, number][] = [];
    const nb = (q: number, r: number, e: number) => cells.get(`${q + EDGE_DIRS[e][0]},${r + EDGE_DIRS[e][1]}`);
    for (let e = 0; e < 6; e++) {
      const n = nb(s.q, s.r, e);
      if (n && parseHyperlaneId(n.system_id)) queue.push([`${n.q},${n.r}`, (e + 3) % 6]);
    }
    while (queue.length) {
      const [key, entry] = queue.shift()!;
      if (seen.has(`${key}:${entry}`)) continue;
      seen.add(`${key}:${entry}`);
      const tile = cells.get(key)!;
      const lane = parseHyperlaneId(tile.system_id)!;
      for (const [a, b] of lane.lanes) {
        if (a !== entry && b !== entry) continue;
        const exit = a === entry ? b : a;
        const m = nb(tile.q, tile.r, exit);
        if (!m) continue;
        if (parseHyperlaneId(m.system_id)) queue.push([`${m.q},${m.r}`, (exit + 3) % 6]);
        else if (m !== s) out.add([`${s.q},${s.r}`, `${m.q},${m.r}`].sort().join("|"));
      }
    }
  }
  return out;
}

/** What is drawn, read back from the SVG: lane strokes as segments between world points. */
interface Drawn {
  kind: "path" | "stub";
  /** `q,r:a-b` for a printed lane, absent for a stub. */
  seg?: string;
  a: [number, number];
  b: [number, number];
}

function readDrawn(root: Element, centres: Map<string, string>): Drawn[] {
  const out: Drawn[] = [];
  const num = (v: string | null) => Number(v);
  const ink = (el: Element) => (el.getAttribute("stroke") ?? "").includes("--hyperlane-ink");
  for (const g of root.querySelectorAll('[data-testid="hyperlane-network"] g[transform]')) {
    const m = /translate\(([-\d.]+) ([-\d.]+)\)/.exec(g.getAttribute("transform") ?? "");
    if (!m) continue;
    const [tx, ty] = [Number(m[1]), Number(m[2])];
    const cell = centres.get(`${Math.round(tx)},${Math.round(ty)}`);
    for (const p of g.querySelectorAll(":scope > path")) {
      if (!ink(p)) continue;
      const n = p.getAttribute("d")!.match(/-?\d+(\.\d+)?/g)!.map(Number);
      // Which printed lane is this? Match its path string against the tile's lanes.
      const tile = [...centres.entries()].length ? cell : undefined;
      out.push({
        kind: "path",
        seg: `${tile}|${p.getAttribute("d")}`,
        a: [tx + n[0], ty + n[1]],
        b: [tx + n[n.length - 2], ty + n[n.length - 1]],
      });
    }
  }
  for (const l of root.querySelectorAll('[data-testid="hyperlane-network"] line')) {
    if (!ink(l)) continue;
    out.push({ kind: "stub", a: [num(l.getAttribute("x1")), num(l.getAttribute("y1"))], b: [num(l.getAttribute("x2")), num(l.getAttribute("y2"))] });
  }
  return out;
}

const near = (p: [number, number], q: [number, number], tol = 1) => Math.hypot(p[0] - q[0], p[1] - q[1]) <= tol;

/**
 * Walks drawn strokes only. A ship leaves a system along a stub, follows one drawn lane to its
 * end, leaves the tile along the stub there, crosses the gap on the neighbour's stub and takes one
 * lane of that tile. It never turns from one lane into another inside a tile.
 * Returns the systems whose edge midpoint the walk reaches.
 */
function reach(drawn: Drawn[], start: [number, number][], systems: Map<string, [number, number][]>): Set<string> {
  const touched = new Set<string>();
  // state: element index and the end we are about to leave it by
  type State = { el: number; at: [number, number] };
  const seen = new Set<string>();
  const queue: State[] = [];
  const key = (el: number, p: [number, number]) => `${el}@${p[0].toFixed(0)},${p[1].toFixed(0)}`;
  const arrive = (el: number, p: [number, number]) => {
    // the walk has just travelled element `el` and stands at p
    for (const [name, mids] of systems) if (mids.some((m) => near(m, p))) touched.add(name);
    const d = drawn[el];
    drawn.forEach((n, i) => {
      if (i === el) return;
      let ok = false;
      let far: [number, number] | null = null;
      if (d.kind === "path") {
        // after a lane: the stub at its end
        if (n.kind === "stub" && near(n.a, p)) { ok = true; far = n.b; }
      } else if (near(d.b, p) || near(d.a, p)) {
        const atPort = near(d.a, p);
        if (atPort) {
          // arrived at a tile port by a stub: take a lane that starts here
          if (n.kind === "path") {
            if (near(n.a, p)) { ok = true; far = n.b; }
            else if (near(n.b, p)) { ok = true; far = n.a; }
          }
        } else if (n.kind === "stub" && near(n.b, p)) {
          // arrived at the middle of a gap: continue on the neighbour's half
          ok = true; far = n.a;
        }
      }
      if (ok && far && !seen.has(key(i, far))) { seen.add(key(i, far)); queue.push({ el: i, at: far }); }
    });
  };
  // leave a system along any stub whose outer end is on its edge midpoints
  drawn.forEach((d, i) => {
    if (d.kind === "stub" && start.some((m) => near(m, d.b))) {
      seen.add(key(i, d.a));
      queue.push({ el: i, at: d.a });
    }
  });
  while (queue.length) {
    const { el, at } = queue.pop()!;
    arrive(el, at);
  }
  return touched;
}

const dir = resolve(process.cwd(), "e2e/screenshots/CO-community-maps/server-snapshot");
const rows: string[] = [];

describe("every linked pair can be traced along the drawn lanes", () => {
  for (const file of readdirSync(dir).filter((f) => f.endsWith("p.json")).sort()) {
    const { previews } = JSON.parse(readFileSync(resolve(dir, file), "utf8")) as {
      previews: Record<string, MapPreviewDto>;
    };
    for (const [alias, preview] of Object.entries(previews)) {
      const tiles = preview.tiles.filter((t) => !t.special_area);
      if (!tiles.some((t) => parseHyperlaneId(t.system_id))) continue;
      for (const view of ["preview", "thumbnail"] as const) {
        it(`${alias} (${view})`, () => {
          const expected = expectedPairs(tiles);
          const { container, unmount } = render(
            view === "preview" ? (
              <MapPreviewBoard preview={preview} />
            ) : (
              <MapThumbnail tiles={preview.tiles} seats={preview.seats} label={alias} />
            ),
          );
          const centres = new Map(tiles.map((t) => {
            const c = deriveHexGeometry(t.q, t.r).center;
            return [`${Math.round(c.x)},${Math.round(c.y)}`, tileKey(t.q, t.r)] as const;
          }));
          const drawn = readDrawn(container, centres);
          // System edge midpoints, from the hex polygons that are drawn.
          const systems = new Map<string, [number, number][]>();
          for (const poly of container.querySelectorAll("polygon")) {
            const host = poly.closest("[data-kind]") ?? poly;
            if (host.getAttribute("data-kind") === "hyperlane") continue;
            if (poly.closest('[data-testid="hyperlane-network"]')) continue;
            const pts = poly.getAttribute("points")!.split(" ").map((s) => s.split(",").map(Number) as [number, number]);
            const cx = pts.reduce((s, p) => s + p[0], 0) / 6;
            const cy = pts.reduce((s, p) => s + p[1], 0) / 6;
            const cell = centres.get(`${Math.round(cx)},${Math.round(cy)}`);
            if (!cell) continue;
            // Only the outer hex of a tile (radius 70), not decorations.
            if (Math.hypot(pts[0][0] - cx, pts[0][1] - cy) < 69) continue;
            systems.set(
              cell,
              pts.map((p, i) => {
                const q = pts[(i + 1) % 6];
                return [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2] as [number, number];
              }),
            );
          }
          // Walk from each system over drawn strokes only; every system reached must be an expected partner and vice versa.
          const found = new Set<string>();
          for (const [cell, mids] of systems) {
            const reached = reach(drawn, mids, systems);
            for (const other of reached) if (other !== cell) found.add([cell, other].sort().join("|"));
          }
          expect(found).toEqual(expected);
          // Every chosen route, walked on its own drawn lanes only, reaches its far system.
          const model = analyzeHyperlanes(tiles.map((t) => ({ systemId: t.system_id, q: t.q, r: t.r })));
          for (const c of model.connections) {
            const own = drawn.filter((d) => {
              if (d.kind === "path") {
                const [cell, dstr] = d.seg!.split("|");
                return c.segments.some((s) => {
                  const [k, ab] = s.split(":");
                  const [a, b] = ab.split("-").map(Number);
                  return k === cell && lanePath([a, b]) === dstr;
                });
              }
              return true; // stubs: the walk only uses those touching own lanes or the systems
            });
            const mids = systems.get(c.fromKey)!;
            expect(reach(own, mids, systems).has(c.toKey), `${alias} ${view} ${c.fromId} -> ${c.toId}`).toBe(true);
          }
          if (view === "preview") rows.push(`${alias}\t${expected.size}\t${found.size}\t${model.connections.length}`);
          unmount();
        });
      }
    }
  }
  it("writes the table", () => {
    if (process.env.PAIR_TABLE) writeFileSync(process.env.PAIR_TABLE, rows.join("\n"));
  });
});
