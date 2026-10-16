import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

const log = (records: Record<string, unknown>[]) => ({ exploration_log: records });
const RESULTS: Record<string, { records: Record<string, unknown>[]; expect: string[] }> = {
  "own-trade-good": {
    records: [{ round: 2, player: actor, deck: "HAZARDOUS", card: "cm1", planet: "mehar_xull", outcome: "resolved", result: "gained 1 trade good, removed 1 infantry from the planet" }],
    expect: ["explored Mehar Xull (hazardous): Core Mine", "gained 1 trade good, removed 1 infantry from the planet"],
  },
  "own-declined": {
    records: [{ round: 2, player: actor, deck: "HAZARDOUS", card: "vfs1", planet: "mehar_xull", outcome: "resolved", result: "" }],
    expect: ["Volatile Fuel Source", "no infantry removed, nothing gained"],
  },
  "other-fragment": {
    records: [{ round: 2, player: opponent.id, deck: "CULTURAL", card: "crf1", planet: "mehar_xull", outcome: "fragment", result: "took a cultural relic fragment" }],
    expect: ["took a cultural relic fragment"],
  },
  "other-attachment": {
    records: [{ round: 2, player: opponent.id, deck: "HAZARDOUS", card: "mw", planet: "mehar_xull", outcome: "attached:miningworld", result: "" }],
    expect: ["attached Mining World"],
  },
  "frontier": {
    records: [{ round: 2, player: actor, deck: "FRONTIER", card: "ent", planet: null, system: "67", outcome: "resolved", result: "gained 1 command token, gained 2 trade goods" }],
    expect: ["frontier token in system 67", "gained 2 trade goods"],
  },
};

for (const [size, viewport] of Object.entries(SIZES)) {
  for (const [name, spec] of Object.entries(RESULTS)) {
    test(`exploration toast ${name} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      const game = await openMockedGame(page, {
        board: galleryBoard,
        players: [playerWithHand({}), opponent],
        state: log([]),
      });
      await page.waitForTimeout(500);
      // The next state update carries the finished exploration, as after the engine resolved it.
      const { events: _events, ...update } = game.snapshot;
      game.send({ ...update, type: "state_update", game_version: 41, state: log(spec.records) });
      const toast = page.getByTestId("corner-toast").first();
      await toast.waitFor();
      for (const part of spec.expect) await expect(toast).toContainText(part);
      // The toast stays inside the viewport and clear of the top map controls.
      await page.waitForTimeout(700); // the toast slides in
      const box = (await toast.boundingBox())!;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
      expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
      await shot(page, testInfo, `after-toast-${name}-${size}`);
    });
  }
}
