// @vitest-environment node
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { hasJspi } from "@ti4/shared/engine/jspi.ts";
import { groundOdds, pageQuery, spaceOdds } from "@ti4/shared/engine/odds.ts";
import { inlinePort } from "@ti4/shared/engine/port.ts";
import { LocalTransport } from "./LocalTransport.ts";

/** The engine of this build (`node scripts/build-wasm.mjs`); without it the test is skipped. */
const WASM = fileURLToPath(new URL("./engine/ti4_wasm.wasm", import.meta.url));

describe.skipIf(!existsSync(WASM) || !hasJspi())("the odds of an offline game", () => {
  it("are answered by the engine that plays it, with the numbers of the page's engine", async () => {
    const module = await WebAssembly.compile(readFileSync(WASM));
    const transport = new LocalTransport({
      gameId: "off_odds",
      createPort: async () => inlinePort(module),
      config: { setup: { game_id: "off_odds", seed: 7, players: 3, human_seat: 0 } },
    });
    try {
      await transport.begin();
      await transport.ready;
      const query = (request: object) => transport.queryEngine(request);
      const space = {
        attacker: { faction: "sol", units: { dreadnought: 1, carrier: 1, fighter: 2 }, damaged: { carrier: 1 } },
        defender: { faction: "letnev", units: { cruiser: 1, destroyer: 1 } },
        in_progress: true,
      };
      const odds = await spaceOdds(query, space);
      expect(odds.simulations).toBe(2000);
      expect(odds).toEqual(await spaceOdds(pageQuery(module), space));
      const ground = {
        attacker: { faction: "sol", units: { infantry: 3 } },
        defender: { faction: "letnev", units: { infantry: 1 }, guns: { pds: 1 } },
        simulations: 2000,
      };
      const landing = await groundOdds(query, ground);
      expect(landing.attacker_win_rate).toBeGreaterThan(0.5);
      expect(landing).toEqual(await groundOdds(pageQuery(module), ground));
    } finally {
      transport.close();
    }
  });
});
