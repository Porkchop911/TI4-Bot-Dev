// Snapshots the real server's map catalog and previews (3-6 players) into server-snapshot/<n>p.json, which the
// captures replay, so the pictured boards are the real community layouts. Rerun after changing
// crates/ti4-server/src/maps/community_maps.json:
//   cd web && node e2e/screenshots/CO-community-maps/fetch-real.mjs
// Builds and starts the server binary on 127.0.0.1:$TI4_FETCH_PORT (default 18765) with a
// throw-away data directory; honours CARGO_TARGET_DIR.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../../..");
const port = process.env.TI4_FETCH_PORT ?? "18765";
const base = `http://127.0.0.1:${port}`;
execFileSync("cargo", ["build", "-q", "-p", "ti4-server", "--bin", "server"], { cwd: root, stdio: "inherit" });
const bin = join(process.env.CARGO_TARGET_DIR ?? join(root, "target"), "debug", "server");
const dataDir = mkdtempSync(join(process.env.TMPDIR ?? tmpdir(), "ti4-co-"));
const server = spawn(bin, [], {
  env: { ...process.env, HOST: "127.0.0.1", PORT: port, TI4_DATA_DIR: dataDir },
  stdio: "ignore",
});
const get = async (path) => {
  const res = await fetch(base + path);
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`);
  return res.json();
};
try {
  for (let i = 0; ; i++) {
    try {
      await get("/api/maps?player_count=6");
      break;
    } catch (e) {
      if (i > 240) throw e;
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  for (const n of [3, 4, 5, 6]) {
    const templates = await get(`/api/maps?player_count=${n}`);
    const previews = {};
    for (const t of templates) previews[t.alias] = await get(`/api/maps/${encodeURIComponent(t.alias)}/preview?variant=0`);
    writeFileSync(join(here, "server-snapshot", `${n}p.json`), JSON.stringify({ templates, previews }) + "\n");
    console.log(`${n}p: ${templates.length} maps`);
  }
} finally {
  server.kill();
  rmSync(dataDir, { recursive: true, force: true });
}
