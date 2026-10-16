import { test } from "@playwright/test";
import { randomUiPlaythrough } from "./smokePlaythrough";
import { cardSetFromEnv, mapTemplateFromEnv, parseExpect, presetFromEnv } from "./smokePreset";

const envInt = (name: string, fallback: number) => {
  const value = process.env[name];
  return value ? Number.parseInt(value, 10) : fallback;
};

// Long-running and randomized, so it only runs on request: `npm run test:e2e:smoke`.
test.skip(!process.env.TI4_SMOKE, "set TI4_SMOKE=1 to run the random UI playthrough");

test("random UI playthrough keeps the game advancing", async ({ browser, request }, testInfo) => {
  const options = {
    playerCount: envInt("TI4_SMOKE_PLAYERS", 3),
    gameSeed: envInt("TI4_SMOKE_GAME_SEED", 42),
    clickSeed: envInt("TI4_SMOKE_CLICK_SEED", 1),
    startPreset: presetFromEnv(process.env.TI4_SMOKE_PRESET),
    cardSet: cardSetFromEnv(process.env.TI4_SMOKE_CARD_SET),
    mapTemplate: mapTemplateFromEnv(process.env.TI4_SMOKE_MAP_TEMPLATE),
    expect: parseExpect(process.env.TI4_SMOKE_EXPECT, presetFromEnv(process.env.TI4_SMOKE_PRESET)),
    maxDecisions: envInt("TI4_SMOKE_DECISIONS", 200),
    maxClicksPerDecision: envInt("TI4_SMOKE_CLICKS_PER_DECISION", 40),
    policy: process.env.TI4_SMOKE_POLICY === "steer" ? ("steer" as const) : ("random" as const),
    stopAtRound: process.env.TI4_SMOKE_ROUND ? envInt("TI4_SMOKE_ROUND", 0) : undefined,
    traceDir: process.env.TI4_SMOKE_TRACE_DIR || undefined,
  };
  test.setTimeout(60_000 + options.maxDecisions * 15_000);
  const lines: string[] = [];
  try {
    const report = await randomUiPlaythrough(browser, request, {
      ...options,
      log: (line) => {
        lines.push(line);
        if (process.env.TI4_SMOKE_VERBOSE) console.log(line);
      },
    });
    console.log(`smoke playthrough: ${JSON.stringify(report, null, 2)}`);
    await testInfo.attach("report.json", { body: JSON.stringify(report, null, 2) });
  } finally {
    await testInfo.attach("clicks.log", { body: lines.join("\n") });
    console.log(`last clicks:\n${lines.slice(-25).join("\n")}`);
  }
});
