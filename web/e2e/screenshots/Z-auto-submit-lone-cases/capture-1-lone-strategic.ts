import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { loneStrategic, openLoneGame } from "./lone";

// The turn menu offers only the strategic action: the client sends it for the player, and the
// corner toast says so. (The mocked server then moves on, as the real one does.)
test("lone strategic action is taken for the player", async ({ page }, testInfo) => {
  const { advance, submissions } = await openLoneGame(page);
  advance(1, null); // the previous decision was answered: play moved forward
  advance(2, loneStrategic("n-lone-strategic"));
  await expect(page.getByTestId("corner-toast")).toContainText("Only one choice: take your strategic action");
  await expect.poll(() => submissions.length).toBe(1);
  expect(submissions[0]).toMatchObject({ option_id: "strategic", nonce: "n-lone-strategic" });
  advance(3, null);
  await shot(page, testInfo, "1-lone-strategic");
});
