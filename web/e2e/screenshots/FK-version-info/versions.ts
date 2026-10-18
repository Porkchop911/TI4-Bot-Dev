import { expect, type Page } from "@playwright/test";
import { SIZES, startGame } from "../DO-offline-game/offlineGame";

export { SIZES, startGame };

/** Rewrites the stored save of an offline game (the page is reloaded afterwards to play it again). */
export async function patchStoredSave(page: Page, gameId: string, patch: Record<string, number | null>) {
  const done = await page.evaluate(
    ({ id, fields }) =>
      new Promise<boolean>((resolve) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => resolve(false);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("games", "readwrite");
          const store = tx.objectStore("games");
          const get = store.get(id);
          get.onsuccess = () => {
            const record = get.result;
            if (!record) return resolve(false);
            for (const [key, value] of Object.entries(fields)) {
              if (value === null) delete record.save[key];
              else record.save[key] = value;
            }
            store.put(record);
          };
          tx.oncomplete = () => {
            db.close();
            resolve(true);
          };
          tx.onerror = () => {
            db.close();
            resolve(false);
          };
        };
      }),
    { id: gameId, fields: patch },
  );
  expect(done).toBe(true);
}

/** Opens the game's own list of versions: the phone menu sheet or the desktop dialog. */
export async function openVersions(page: Page, size: string) {
  // The decision dialog sits over the header: fold it away first, as the other offline captures do.
  const fold = page.getByTestId("minimize-choice-button");
  if (await fold.count()) await fold.click();
  if (size === "desktop") await page.getByTestId("version-info-button").click();
  else await page.getByTestId("top-menu-button").click();
  await expect(page.getByTestId("version-info")).toBeVisible();
  await page.getByTestId("version-info").scrollIntoViewIfNeeded();
}
