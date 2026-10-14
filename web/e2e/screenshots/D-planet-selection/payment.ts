import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { actor, galleryBoard } from "../_shared/fixtures";

/** The gallery board where the viewer also controls Lodor (#26) and Quann (#25). */
export const paymentBoard = {
  ...galleryBoard,
  systems: {
    ...galleryBoard.systems,
    "26": { ...galleryBoard.systems["26"], planets: { lodor: { ...galleryBoard.systems["26"].planets.lodor, controlled_by: actor } } },
    "25": { ...galleryBoard.systems["25"], planets: { quann: { ...galleryBoard.systems["25"].planets.quann, controlled_by: actor } } },
  },
};

/** Paying 5 resources for production: three planets (4, 3 and 1) and trade goods to spend. */
export const paymentChoice = {
  prompt: "pay 5 resources to produce",
  context: { subtype: "pay_resources", outstanding: [{ kind: "resources", amount: 5, paid: 0 }] },
  options: [
    { id: "exhaust|jord", label: "Exhaust Jord", kind: "pay", payload: { worth: 4, owed: 5, kind: "resources", planet: "jord", planet_name: "Jord" } },
    { id: "exhaust|lodor", label: "Exhaust Lodor", kind: "pay", payload: { worth: 3, owed: 5, kind: "resources", planet: "lodor", planet_name: "Lodor" } },
    { id: "exhaust|quann", label: "Exhaust Quann", kind: "pay", payload: { worth: 1, owed: 5, kind: "resources", planet: "quann", planet_name: "Quann" } },
    { id: "trade_good", label: "Spend one trade good", kind: "pay", payload: { worth: 1 } },
    { id: "decline", label: "Done", kind: "decline" },
  ],
};

/** Opens the mocked game with the payment drawer showing. */
export async function openPayment(page: Page) {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: paymentBoard, choice: paymentChoice });
  await page.getByTestId("payment-drawer").waitFor();
}

/** Minimises the list so the map is the control and the payment bar confirms. */
export async function pickOnMap(page: Page) {
  await page.getByTestId("pick-on-map-btn").click();
  await page.getByTestId("payment-bar").waitFor();
}
