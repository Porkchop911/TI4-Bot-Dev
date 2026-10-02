import { expect, test } from "@playwright/test";
import { fallbackCases, galleryCases } from "../src/dev/decisionGalleryCases.ts";

test("synthetic log opens only the active path and allows browsing earlier stages and rounds", async ({
  page,
}) => {
  await page.goto("/dev/decisions");
  await page.getByRole("button", { name: new RegExp(`^${galleryCases[0].title}`) }).click();
  await page.getByTestId("event-log-toggle").click();
  const log = page.getByTestId("event-log-list");
  await expect(log.getByRole("button", { name: /Round 2/ })).toBeVisible();
  await expect(log).toContainText("Synthetic production choice");
  await expect(log).not.toContainText("Blair reacted");
  await log.getByRole("button", { name: /Reactions/ }).click();
  await expect(log).toContainText("Blair reacted");
  await log.getByRole("button", { name: /Round 2/ }).click();
  await log.getByRole("button", { name: /Strategy phase/ }).click();
  await expect(log).toContainText("Selected a strategy card");
});

test("all current workflow kinds open a rendered preview", async ({ page }) => {
  await page.goto("/dev/decisions");
  for (const item of galleryCases) {
    await page.getByRole("button", { name: new RegExp(`^${item.title}`) }).click();
    await expect(page.getByTestId("game-container")).toBeAttached();
    await page.getByText("Gallery debug details · synthetic fixture").click();
    await expect(page.getByLabel("Gallery debug details")).toContainText(
      `Workflow: ${item.workflow}`,
    );
    await expect(page.getByTestId("ti4-board-svg")).toBeAttached();
    await page.getByRole("button", { name: "All decisions" }).click();
  }
});

test("dev gallery exposes all workflows and an actionable empty-state fallback", async ({
  page,
}) => {
  await page.goto("/dev/decisions");
  await expect(page.getByText(/Workflow kinds \(18\)/)).toBeVisible();
  await expect(page.getByText("Fallbacks and boundary states (6)")).toBeVisible();
  await page.getByRole("button", { name: /Empty movement Explicit finish/i }).click();
  await expect(page.getByTestId("tactical-movement-tray")).toBeVisible();
  await page.getByTestId("commit-moves-btn").click();
  await page.getByText("Gallery debug details · synthetic fixture").click();
  await expect(page.getByText(/Local submission: done_moving/)).toBeVisible();
  await page.getByText("Gallery debug details · synthetic fixture").click();
  await page.getByRole("button", { name: "Minimize decision" }).click();
  await page.getByRole("button", { name: "All decisions" }).click();
  await page
    .getByRole("button", { name: /Missing movement finish Missing explicit finish/i })
    .click();
  await page.getByTestId("commit-moves-btn").click();
  await expect(page.getByTestId("tactical-movement-tray").getByRole("alert")).toBeVisible();
});

test("all fallback examples open with their boundary clearly labeled", async ({ page }) => {
  await page.goto("/dev/decisions");
  for (const item of fallbackCases) {
    await page.getByRole("button", { name: new RegExp(`^${item.title}`) }).click();
    await page.getByText("Gallery debug details · synthetic fixture").click();
    await expect(page.getByLabel("Gallery debug details")).toContainText(item.fallback!);
    await page.getByRole("button", { name: "All decisions" }).click();
  }
});

test("ready offered payment planets are targetable but exhausted planets are not", async ({
  page,
}) => {
  await page.goto("/dev/decisions");
  await page.getByRole("button", { name: /^payment pay_resources/i }).click();
  await expect(page.getByTestId("planet-jord")).toHaveAttribute("data-target-candidate", "true");
  await expect(page.getByTestId("planet-exhausted")).not.toHaveAttribute(
    "data-target-candidate",
    "true",
  );
  await expect(page.getByTestId("payment-drawer")).not.toContainText("Unknown participant");
});
