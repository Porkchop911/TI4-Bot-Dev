import { expect, test } from "@playwright/test";
import { asked, both, clickText, openLateGame, press, sheet } from "./scene";
import { startWeb2, type Web2Server } from "./web2";

// A real engine game: Sol plays Plagiarize, and Yin is asked whether to Sabotage it. Plagiarize has
// no preview in the engine, so the played card still says "Not resolved yet."; the Sabotage row
// says what Sabotage would do.
let server: Web2Server;
test.beforeAll(async () => {
  server = await startWeb2();
});
test.afterAll(() => server?.stop());

test("sabotage window over Plagiarize (no preview of the played card)", async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  await openLateGame(page, server);
  await clickText(sheet(page), "Play an action card or use a component");
  await clickText(sheet(page), "Plagiarize");
  await press(sheet(page), "Play Plagiarize");
  await asked(page, "Not resolved yet.");
  const step = sheet(page);
  await expect(step).toContainText(/Cancels Sol.s Plagiarize\. The card stays spent and its effect does not happen\./);
  await expect(step).toContainText("Not resolved yet.");
  await both(page, testInfo, "1-sabotage-plagiarize");
});
