import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { shot } from "../_shared/shot";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// Freelancers with the fleet supply full and no planet to land on: only fighters are on the list,
// and the dialog says why the other units are not.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`one-unit production says why units are missing ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, {
      phase: "action",
      players: [playerWithHand({}), opponent],
      choice: {
        prompt: "Freelancers: produce one unit in 25",
        context: { subtype: "produce_one_unit", target: { System: "25" }, optional: true },
        details: {
          not_offered: [
            { unit: "sol_infantry", reason: "no place to put it in this system (ground forces need a planet you control)" },
            { unit: "cruiser", reason: "the fleet supply in this system is full" },
            { unit: "destroyer", reason: "the fleet supply in this system is full" },
            { unit: "dreadnought", reason: "the fleet supply in this system is full" },
          ],
        },
        options: [
          { id: "build|fighter|1", kind: "produce", label: "produce 1x fighter for 1", payload: { unit: "fighter", cost: 1, count: 1 } },
          { id: "decline", kind: "decline", label: "Decline", payload: {} },
        ],
      },
    });
    const note = page.getByTestId("not-offered");
    await note.waitFor();
    await expect(page.getByTestId("not-offered-destroyer")).toContainText("fleet supply in this system is full");
    await expect(page.getByTestId("not-offered-sol_infantry")).toContainText("planet you control");
    await shot(page, testInfo, `3-not-offered-${size}`);
  });
}
