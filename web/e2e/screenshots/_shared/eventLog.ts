import type { Page } from "@playwright/test";

/** Opens the event log drawer and expands every round, phase and action node. */
export async function openLogFully(page: Page) {
  await page.getByTestId("event-log-toggle").click();
  const list = page.getByTestId("event-log-list");
  await list.waitFor();
  for (let i = 0; i < 6; i++) {
    const closed = list.locator('button[aria-expanded="false"]');
    if (!(await closed.count())) break;
    await closed.first().click();
  }
}
