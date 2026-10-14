import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { actor, galleryBoard } from "../_shared/fixtures";

/** The gallery board where the viewer controls Jord, Lodor (#26) and Quann (#25). */
export const leadershipBoard = {
  ...galleryBoard,
  systems: {
    ...galleryBoard.systems,
    "26": { ...galleryBoard.systems["26"], planets: { lodor: { ...galleryBoard.systems["26"].planets.lodor, controlled_by: actor } } },
    "25": { ...galleryBoard.systems["25"], planets: { quann: { ...galleryBoard.systems["25"].planets.quann, controlled_by: actor } } },
  },
};

/** Leadership primary: three free tokens, tokens cost 3 influence; Jord pays 2, Lodor 3, Quann 1. */
export const leadershipChoice = {
  prompt: "gain a command token into which pool",
  nonce: "shots-leadership-map-payment",
  context: { subtype: "gain_command_token" },
  options: [
    { id: "tactic_tokens", label: "tactic pool", kind: "pool" },
    { id: "fleet_tokens", label: "fleet pool", kind: "pool" },
    { id: "strategic_tokens", label: "strategy pool", kind: "pool" },
  ],
  details: {
    kind: "command_tokens",
    mode: "gain",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 8,
    tokens_to_place: 3,
    purchase: {
      cost: 3,
      influence_available: 6,
      max: 2,
      trade_goods: 0,
      trade_good_worth: 1,
      planets: [
        { id: "jord", worth: 2 },
        { id: "lodor", worth: 3 },
        { id: "quann", worth: 1 },
      ],
    },
  },
};

/** Opens the Leadership panel, buys `bought` tokens and assigns every token to a pool. */
export async function openLeadership(page: Page, bought = 1) {
  await openMockedGame(page, { players: [playerWithHand({ trade_goods: 0 }), opponent], board: leadershipBoard, choice: leadershipChoice });
  await page.getByTestId("command-token-panel").waitFor();
  for (let i = 0; i < bought; i++) await page.getByTestId("token-buy-plus").click();
  for (let i = 0; i < 3 + bought; i++) await page.getByTestId("token-plus-tactic").click();
  await expect(page.getByTestId("token-remaining")).toContainText("0");
}

/** Minimises the panel so the map is the control and the bar confirms. */
export async function payOnMap(page: Page) {
  await page.getByTestId("token-pay-on-map").click();
  await page.getByTestId("token-payment-bar").waitFor();
}

/** Clicks planets on the map; Auto-pay's pick (Lodor for one token) toggles like any other. */
export async function clickPlanets(page: Page, ...planets: string[]) {
  for (const planet of planets) await page.getByTestId(`planet-${planet}`).click();
}
