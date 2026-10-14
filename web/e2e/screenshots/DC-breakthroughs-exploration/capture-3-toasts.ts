import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { board, players, actor, opponent } from "./_fixtures";

// One toast per finished exploration (card name and what was gained) and one per unlocked breakthrough.
const CASES: Record<string, { records: Record<string, unknown>[]; expect: string[] }> = {
  "enigmatic-device": {
    records: [{ round: 2, player: actor, deck: "FRONTIER", card: "ed2", planet: null, system: "39", outcome: "resolved" }],
    expect: ["frontier token in system 39", "Enigmatic Device", "kept faceup in the play area for later"],
  },
  attachment: {
    records: [{ round: 2, player: opponent.id, deck: "CULTURAL", card: "ds", planet: "lodor", outcome: "attached:dysonsphere" }],
    expect: ["attached Dyson Sphere"],
  },
  gain: {
    records: [{ round: 2, player: actor, deck: "HAZARDOUS", card: "cm1", planet: "mehar_xull", outcome: "resolved", result: "gained 1 trade good, removed 1 infantry from the planet" }],
    expect: ["Core Mine", "gained 1 trade good"],
  },
};

for (const [name, spec] of Object.entries(CASES)) {
  test(`exploration toast ${name}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const { list, state } = players(null);
    const game = await openMockedGame(page, { board, players: list, state: { ...state, exploration_log: [], frontier_tokens: [] } });
    await page.waitForTimeout(500);
    const { events: _events, ...update } = game.snapshot;
    game.send({ ...update, type: "state_update", game_version: 41, state: { ...state, exploration_log: spec.records, frontier_tokens: [] } });
    const toast = page.getByTestId("corner-toast").first();
    await toast.waitFor();
    for (const part of spec.expect) await expect(toast).toContainText(part);
    await page.waitForTimeout(700);
    await shot(page, testInfo, `toast-${name}`);
  });
}

test("breakthrough unlock toast", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const locked = players(null);
  const game = await openMockedGame(page, { board, players: locked.list, state: locked.state });
  await page.waitForTimeout(500);
  const { events: _events, ...update } = game.snapshot;
  game.send({ ...update, type: "state_update", game_version: 41, state: players("solbt").state });
  const toast = page.getByTestId("corner-toast").first();
  await toast.waitFor();
  await expect(toast).toContainText("unlocked the breakthrough Bellum Gloriosum");
  await page.waitForTimeout(700);
  await shot(page, testInfo, "toast-breakthrough");
});
