import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { groundOdds, pageQuery, spaceOdds } from "./odds.ts";

const SPACE = {
  attacker: { units: { cruiser: 2 } },
  defender: { units: { fighter: 2 } },
};
const GROUND = {
  attacker: { faction: "sol", units: { sol_infantry: 3 } },
  defender: { faction: "letnev", units: { infantry: 1 }, guns: { pds: 1 } },
  simulations: 500,
};

describe("the odds queries", () => {
  it("ask the engine with the query and the sides", async () => {
    const asked: object[] = [];
    const query = (request: object) => {
      asked.push(request);
      return { simulations: 1 };
    };
    await spaceOdds(query, SPACE);
    await groundOdds(async (request) => query(request), GROUND);
    expect(asked).toEqual([
      { query: "space_odds", ...SPACE },
      { query: "ground_odds", ...GROUND },
    ]);
  });

  it("reject with the error of the engine", async () => {
    const refused = () => {
      throw new Error("space odds: unknown unit");
    };
    await expect(spaceOdds(refused, SPACE)).rejects.toThrow("space odds: unknown unit");
  });
});

// The engine file of web2 (`web2/scripts/build-wasm.sh`).
const WASM = fileURLToPath(new URL("../../../web2/src/session/ti4.wasm", import.meta.url));

describe.skipIf(!existsSync(WASM))("the odds of an engine on this thread", () => {
  it("are the same for the same sides, from one instance", async () => {
    const query = pageQuery(await WebAssembly.compile(readFileSync(WASM)));
    const space = await spaceOdds(query, SPACE);
    expect(space.attacker_fielded).toEqual({ cruiser: 2 });
    expect(await spaceOdds(query, SPACE)).toEqual(space);
    const ground = await groundOdds(query, GROUND);
    expect(ground.simulations).toBe(500);
    expect(ground.attacker_win_rate).toBeGreaterThan(0.5);
    await expect(
      spaceOdds(query, { attacker: { units: { no_such_ship: 1 } }, defender: SPACE.defender }),
    ).rejects.toThrow("space odds:");
    // An error leaves the instance as it was.
    expect(await spaceOdds(query, SPACE)).toEqual(space);
  });
});
