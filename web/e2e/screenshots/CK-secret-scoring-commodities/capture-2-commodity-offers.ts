import { expect, test, type Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

/** `before` is taken on the commit without the change. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };

// The viewing seat is Sol (commodity value 4) with 1 commodity and 2 trade goods.
const players = [playerWithHand({ commodities: 1, trade_goods: 2 }), opponent];
const standing = (have: number) => ({ commodities: { have, max: 4, trade_goods: 2 } });

async function open(page: Page, viewport: { width: number; height: number }, choice: MockChoice) {
  await page.setViewportSize(viewport);
  await openMockedGame(page, { board: galleryBoard, players, choice });
  await page.getByTestId("pending-choice-dialog").waitFor();
  await page.waitForTimeout(700);
}

const merchantStation = (have: number): MockChoice => ({
  prompt: "Merchant Station",
  context: { subtype: "merchant_station_choose_reward", source: { Content: "merchant_station" } },
  options: [
    { id: "replenish", label: "replenish commodities", kind: "explore" },
    { id: "convert", label: "convert commodities to trade goods", kind: "explore" },
  ],
  details: standing(have),
});

for (const [size, viewport] of [["desktop", DESKTOP], ["390x844", PHONE]] as const) {
  test(`merchant station ${size}`, async ({ page }, testInfo) => {
    await open(page, viewport, merchantStation(1));
    if (prefix === "after") {
      await expect(page.getByTestId("commodity-standing-have")).toContainText("1 / 4");
    }
    await shot(page, testInfo, `${prefix}-merchant-station-${size}`);
  });
}

test("abandoned warehouses, commodities full", async ({ page }, testInfo) => {
  await open(page, DESKTOP, {
    prompt: "Abandoned Warehouses",
    context: {
      subtype: "abandoned_warehouses_choose_reward",
      source: { Content: "abandoned_warehouses" },
    },
    options: [
      { id: "gain", label: "gain 2 commodities", kind: "explore" },
      { id: "convert", label: "convert up to 2 commodities to trade goods", kind: "explore" },
    ],
    details: standing(4),
  });
  if (prefix === "after") {
    await expect(page.getByTestId("commodity-standing-have")).toContainText("4 / 4 full");
  }
  await shot(page, testInfo, `${prefix}-abandoned-warehouses-full-desktop`);
});

test("trade secondary", async ({ page }, testInfo) => {
  await open(page, DESKTOP, {
    prompt: "spend a strategy token to replenish commodities",
    context: { subtype: "decision" },
    options: [
      { id: "no", label: "decline", kind: "strategy" },
      { id: "yes", label: "replenish", kind: "strategy" },
    ],
    details: {
      kind: "strategy_secondary",
      card: "pok5trade",
      played_by: opponent.id,
      tokens_left: 2,
      costs_token: true,
      ...standing(1),
    },
  });
  if (prefix === "after") {
    await expect(page.getByTestId("strategy-secondary-panel")).toContainText("1 / 4");
  }
  await shot(page, testInfo, `${prefix}-trade-secondary-desktop`);
});

test("hacan agent", async ({ page }, testInfo) => {
  await open(page, DESKTOP, {
    prompt: "Leader: gain 2 commodities or replenish another player",
    context: {
      subtype: "leader_hacanagent_branch",
      source: { FactionAbility: "hacanagent" },
    },
    options: [
      { id: "self", label: "gain 2 commodities", kind: "leader_hacanagent_branch" },
      { id: opponent.id, label: `replenish ${opponent.id}`, kind: "leader_hacanagent_branch" },
    ],
    details: standing(1),
  });
  if (prefix === "after") {
    await expect(page.getByTestId("commodity-standing-panel")).toContainText("1 / 4");
  }
  await shot(page, testInfo, `${prefix}-hacan-agent-desktop`);
});
