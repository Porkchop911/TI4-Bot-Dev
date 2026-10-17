import { test } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { playOfflineGame } from "./offlineSweep";

// One full offline game for the nightly offline sweep (tools/nightly-smoke/offline-run.sh), driven
// through the production build with no server. Parameters come from the environment (the runner
// sets them and records them in meta.json):
//   TI4_OFFLINE_GAME_SEED TI4_OFFLINE_CLICK_SEED TI4_OFFLINE_PLAYERS TI4_OFFLINE_POLICY (steer|random)
//   TI4_OFFLINE_SEAT (random|1..6|dealt) TI4_OFFLINE_FACTION (random|dealt|<id>)
//   TI4_OFFLINE_MAX_DECISIONS TI4_OFFLINE_MAX_SECONDS TI4_OFFLINE_STALL_SECONDS
//   TI4_OFFLINE_VIEWPORT (desktop|phone) TI4_OFFLINE_SHOT_CAP TI4_OFFLINE_TRACE_DIR
//   TI4_OFFLINE_RESUME_AT=<decision> (reload the page there and check the game comes back)
// Run by hand: cd web && TI4_OFFLINE_URL=http://127.0.0.1:<port> npm run test:e2e:offline-sweep
const env = (name: string, fallback: string) => process.env[`TI4_OFFLINE_${name}`] ?? fallback;
const phone = env("VIEWPORT", "desktop") === "phone";

test.use({ viewport: phone ? { width: 390, height: 844 } : { width: 1440, height: 900 }, hasTouch: phone });

test("plays one full offline game", async ({ page, context, baseURL }) => {
  const maxSeconds = Number(env("MAX_SECONDS", "1500"));
  test.setTimeout((maxSeconds + 240) * 1000);
  const traceDir = env("TRACE_DIR", join("test-results", "offline-sweep"));
  mkdirSync(traceDir, { recursive: true });
  const gameSeed = Number(env("GAME_SEED", String(1 + Math.floor(Math.random() * 4_000_000_000))));
  const report = await playOfflineGame(context, page, {
    baseUrl: baseURL!,
    gameSeed,
    clickSeed: Number(env("CLICK_SEED", String(gameSeed ^ 0x5bd1e995))),
    players: Number(env("PLAYERS", "4")),
    policy: env("POLICY", "steer") === "random" ? "random" : "steer",
    seat: env("SEAT", "random"),
    faction: env("FACTION", "random"),
    maxDecisions: Number(env("MAX_DECISIONS", "20000")),
    maxSeconds,
    stallSeconds: Number(env("STALL_SECONDS", "90")),
    viewport: phone ? "phone" : "desktop",
    resumeAt: env("RESUME_AT", "") === "" ? null : Number(env("RESUME_AT", "0")),
    shotCap: Number(env("SHOT_CAP", "30")),
    traceDir,
    log: (line) => console.log(line),
  });
  console.log(`RESULT ${report.outcome} decisions=${report.decisions} seconds=${report.seconds} findings=${report.findings.length} panics=${report.panics.length}`);
});
