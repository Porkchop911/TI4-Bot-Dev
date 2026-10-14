import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { activation, openLoneGame } from "./lone";

// The tactical action is open and exactly one system can be activated: it is activated for the
// player, with a toast naming the system.
test("lone system activation is taken for the player", async ({ page }, testInfo) => {
  const { advance, submissions } = await openLoneGame(page);
  advance(1, null);
  advance(2, activation(["37"], "n-lone-activation"));
  await expect(page.getByTestId("corner-toast")).toContainText("Only one choice: activate system 37");
  await expect.poll(() => submissions.length).toBe(1);
  expect(submissions[0]).toMatchObject({ option_id: "37", nonce: "n-lone-activation" });
  advance(3, null);
  await shot(page, testInfo, "2-lone-activation");
});
