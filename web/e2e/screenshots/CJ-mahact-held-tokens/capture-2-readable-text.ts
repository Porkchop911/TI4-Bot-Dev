import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { open, waiverLabel, SEAT_ALEX } from "./scene";

test("an engine event type in a prompt reads as words", async ({ page }, testInfo) => {
  await open(page, { width: 1440, height: 900 }, {
    prompt: "after TACTICAL_ACTION_ENDED",
    context: { subtype: "decision" },
    options: [
      { id: "use", label: "Use it", kind: "ability" },
      { id: "decline", label: "Decline", kind: "decline" },
    ],
  });
  await expect(page.getByText("after tactical action ended").first()).toBeVisible();
  await expect(page.getByText("TACTICAL_ACTION_ENDED")).toHaveCount(0);
  await shot(page, testInfo, "desktop-event-type");
});

test("Jae Mir Kan names the player and the system", async ({ page }, testInfo) => {
  await open(page, { width: 1440, height: 900 }, {
    prompt: "spend a strategy token to draw a secret objective",
    context: { subtype: "decision" },
    options: [
      { id: "no", label: "no", kind: "strategy" },
      { id: "yes", label: "yes", kind: "strategy" },
      { id: "follow|waived|0|agent|18", label: waiverLabel(SEAT_ALEX), kind: "strategy" },
    ],
  });
  await shot(page, testInfo, "desktop-waiver");
});
