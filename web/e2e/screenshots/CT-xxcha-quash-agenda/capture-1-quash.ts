import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import {
  ARCHIVED_SECRET,
  MUTINY,
  options,
  politicalFavor,
  quash,
  resolvedWindow,
  vetoWindow,
} from "./scene";

const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;

/** The card sits above the options and is fully inside the viewport. */
async function expectAgendaCard(
  page: import("@playwright/test").Page,
  name: string,
  eyebrow: string,
  size: { width: number; height: number },
) {
  const card = page.getByTestId("decision-agenda-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText(eyebrow);
  await expect(page.getByTestId("decision-agenda-name")).toContainText(name);
  const box = (await card.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(size.width + 1);
  const firstOption = (await page
    .getByTestId("choice-option")
    .first()
    .boundingBox())!;
  expect(box.y, "the agenda is above the options").toBeLessThan(firstOption.y);
}

// What the engine sent before the fix: the Quash question with no agenda on it. Rendered by this
// client, it shows exactly what players saw then (the dialog never had anything else to show).
test("before: Quash without the agenda", async ({ page }, testInfo) => {
  await page.setViewportSize(SIZES.desktop);
  await openMockedGame(page, options(quash(null)));
  await page.getByTestId("choice-option").first().waitFor();
  await expect(page.getByTestId("decision-agenda-card")).toHaveCount(0);
  await page.waitForTimeout(400);
  await shot(page, testInfo, "before-quash-desktop");
});

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`Quash on Mutiny ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, options(quash(MUTINY)));
    await page.getByTestId("choice-option").first().waitFor();
    await expectAgendaCard(page, "Mutiny", "Revealed agenda", viewport);
    await expect(page.getByTestId("decision-agenda-card")).toContainText(
      'Against: Each player that voted "For" loses 1 victory point.',
    );
    // The phase strip names it too.
    await expect(page.getByTestId("agenda-strip-text")).toContainText("Mutiny");
    expect(
      await page
        .getByTestId("choice-option")
        .evaluateAll((rows) =>
          rows.map((r) => r.getAttribute("data-option-id")),
        ),
    ).toEqual(["use", "decline"]);
    await page.waitForTimeout(400);
    await shot(page, testInfo, `quash-mutiny-${size}`);
  });
}

test("Quash on the replacement (an election)", async ({ page }, testInfo) => {
  await page.setViewportSize(SIZES.desktop);
  await openMockedGame(page, options(quash(ARCHIVED_SECRET)));
  await page.getByTestId("choice-option").first().waitFor();
  await expectAgendaCard(
    page,
    "Archived Secret",
    "Revealed agenda",
    SIZES.desktop,
  );
  await expect(page.getByTestId("decision-agenda-name")).toContainText(
    "Elect Player",
  );
  await page.waitForTimeout(400);
  await shot(page, testInfo, "quash-replacement-desktop");
});

test("Political Favor on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize(SIZES["390x844"]);
  await openMockedGame(page, options(politicalFavor(MUTINY)));
  await page.getByTestId("choice-option").first().waitFor();
  await expectAgendaCard(page, "Mutiny", "Revealed agenda", SIZES["390x844"]);
  await page.waitForTimeout(400);
  await shot(page, testInfo, "political-favor-390x844");
});

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`Veto window ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, options(vetoWindow(MUTINY)));
    await page.getByTestId("choice-option").first().waitFor();
    await expectAgendaCard(page, "Mutiny", "Revealed agenda", viewport);
    await page.waitForTimeout(400);
    await shot(page, testInfo, `veto-window-${size}`);
  });
}

test("Confusing Legal Text window on the resolved agenda", async ({
  page,
}, testInfo) => {
  await page.setViewportSize(SIZES.desktop);
  await openMockedGame(page, options(resolvedWindow(ARCHIVED_SECRET)));
  await page.getByTestId("choice-option").first().waitFor();
  await expectAgendaCard(page, "Archived Secret", "Agenda", SIZES.desktop);
  await expect(page.getByTestId("decision-agenda-card")).not.toContainText(
    "Revealed",
  );
  await page.waitForTimeout(400);
  await shot(page, testInfo, "confusing-window-desktop");
});
