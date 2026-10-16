import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView, PlacedUnitView } from "../../../src/protocol/types";

// Tactical action into Mecatol Rex (#18) from the Sol home system (#01) and a gravity rift (#41).
// Every system in the move dialog reads "#<tile> [<name>]"; "All from here" stages every ship of an
// origin and the cargo its holds can take, and sends nothing until Commit Moves.
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

const planet = (id: string, label: string) => ({
  id,
  label,
  resources: 2,
  influence: 2,
  traits: [],
  tech_specialties: [],
});
const unit = (unit_type: string, extra: Partial<PlacedUnitView> = {}): PlacedUnitView => ({
  owner: actor,
  unit_type,
  damaged: false,
  ...extra,
});
const board: BoardView = {
  active_system: "18",
  map_tiles: [
    { system_id: "18", label: "Mecatol Rex", q: 0, r: 0, planets: [planet("mr", "Mecatol Rex")] },
    { system_id: "01", label: "Jord - Sol", q: 0, r: 1, planets: [planet("jord", "Jord")] },
    { system_id: "41", label: "Gravity Rift", q: 1, r: 0, anomalies: ["gravity rift"], planets: [] },
  ],
  systems: {
    "18": { system_id: "18", command_tokens: [actor], planets: {}, units: [] },
    "01": {
      system_id: "01",
      command_tokens: [],
      planets: {},
      units: [
        unit("sol_carrier"),
        unit("sol_carrier"),
        unit("dreadnought"),
        unit("fighter"),
        unit("fighter"),
        unit("fighter"),
        unit("infantry"),
        ...Array.from({ length: 7 }, () => unit("infantry", { planet: "jord" })),
        unit("mech", { planet: "jord" }),
        unit("spacedock", { planet: "jord" }),
      ],
    },
    "41": {
      system_id: "41",
      command_tokens: [],
      planets: {},
      units: [
        unit("carrier"),
        unit("destroyer"),
        unit("destroyer"),
        unit("fighter"),
        unit("fighter"),
        ...Array.from({ length: 4 }, () => unit("infantry")),
      ],
    },
  },
};
const choice = {
  prompt: "Move ships into Mecatol Rex",
  context: { subtype: "movement_step", target: { System: "18" } },
  options: [
    { id: "move|01|0", label: "Sol Carrier", kind: "move", payload: { origin: "01", unit: "sol_carrier", capacity: 6 } },
    { id: "move|01|1", label: "Dreadnought", kind: "move", payload: { origin: "01", unit: "dreadnought", capacity: 1 } },
    { id: "move|01|2", label: "Fighter", kind: "move", payload: { origin: "01", unit: "fighter" } },
    { id: "move|41|0", label: "Destroyer", kind: "move", payload: { origin: "41", unit: "destroyer" } },
    { id: "move|41|1", label: "Carrier", kind: "move", payload: { origin: "41", unit: "carrier", capacity: 4 } },
    { id: "move|41|2", label: "Fighter", kind: "move", payload: { origin: "41", unit: "fighter" } },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

async function open(page: import("@playwright/test").Page, size: { width: number; height: number }) {
  const sent: string[] = [];
  await page.setViewportSize(size);
  page.on("request", (request) => {
    if (request.method() === "POST" && !/lobby/.test(request.url())) sent.push(request.url());
  });
  await openMockedGame(page, {
    board,
    players: galleryPlayers,
    choice,
    onClientMessage: (message) => sent.push(message.type),
  });
  await page.getByTestId("tactical-movement-tray").waitFor();
  return sent;
}

for (const [name, size] of Object.entries(SIZES)) {
  test(`system names ${name}`, async ({ page }, testInfo) => {
    await open(page, size);
    const tray = page.getByTestId("tactical-movement-tray");
    await expect(tray).toContainText("Destination: #18 [Mecatol Rex]");
    await expect(page.getByTestId("origin-title-01")).toHaveText("From #01 [Sol home system]");
    await expect(page.getByTestId("origin-title-41")).toHaveText("From #41 [Gravity rift]");
    await expect(page.getByTestId("rally-all-01")).toBeEnabled();
    await expect(page.getByTestId("rally-all-41")).toBeEnabled();
    if (name === "390x844") {
      const width = await page.evaluate(() => document.documentElement.scrollWidth);
      expect(width).toBeLessThanOrEqual(390);
      const box = await page.getByTestId("rally-all-01").boundingBox();
      expect(box && box.x + box.width).toBeLessThanOrEqual(390);
    }
    await shot(page, testInfo, `1-names-${name}`, { of: tray, pad: 8 });
  });

  test(`all from here staged ${name}`, async ({ page }, testInfo) => {
    const sent = await open(page, size);
    const tray = page.getByTestId("tactical-movement-tray");
    await page.getByTestId("rally-all-01").click();
    // Two Sol carriers (6 each) and a dreadnought (1): 13 slots for 3 fighters, the infantry in
    // space, 7 infantry and the mech on Jord (12 units); the space dock stays.
    await expect(page.getByTestId("rally-count-01-sol_carrier")).toHaveText("2");
    await expect(page.getByTestId("rally-count-01-dreadnought")).toHaveText("1");
    await expect(page.getByTestId("rally-count-cargo-01-fighter-space")).toHaveText("3");
    await expect(page.getByTestId("rally-count-cargo-01-infantry-space")).toHaveText("1");
    await expect(page.getByTestId("rally-count-cargo-01-infantry-jord")).toHaveText("7");
    await expect(page.getByTestId("rally-count-cargo-01-mech-jord")).toHaveText("1");
    await expect(page.getByTestId("rally-count-41-destroyer")).toHaveText("0");
    await expect(page.getByTestId("rally-all-01")).toBeDisabled();
    await expect(page.getByTestId("commit-moves-btn")).toHaveText("Commit Moves (15)");
    // Staged only: nothing reaches the server until Commit Moves.
    await page.waitForTimeout(300);
    expect(sent.filter((s) => s !== "subscribe" && s !== "ping")).toEqual([]);
    await page.getByTestId("origin-group-01").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `2-all-from-sol-${name}`, { of: tray, pad: 8 });
  });

  test(`all from here capacity limited ${name}`, async ({ page }, testInfo) => {
    const sent = await open(page, size);
    const tray = page.getByTestId("tactical-movement-tray");
    // One carrier (4 slots) for 2 fighters and 4 infantry: fighters first, then 2 infantry; the
    // other 2 infantry stay in the rift. Sol keeps its own staging untouched.
    await page.getByTestId("rally-inc-01-dreadnought").click();
    await page.getByTestId("rally-all-41").click();
    await expect(page.getByTestId("rally-count-41-carrier")).toHaveText("1");
    await expect(page.getByTestId("rally-count-41-destroyer")).toHaveText("2");
    await expect(page.getByTestId("rally-count-cargo-41-fighter-space")).toHaveText("2");
    await expect(page.getByTestId("rally-count-cargo-41-infantry-space")).toHaveText("2");
    await expect(page.getByTestId("cargo-capacity-gauge-41")).toHaveText("Cargo: 4 loaded / 4 capacity");
    await expect(page.getByTestId("rally-inc-cargo-41-infantry-space")).toBeDisabled();
    await expect(page.getByTestId("rally-all-41")).toBeDisabled();
    await expect(page.getByTestId("rally-count-01-dreadnought")).toHaveText("1");
    await expect(page.getByTestId("rally-count-01-sol_carrier")).toHaveText("0");
    await expect(page.getByTestId("rally-all-01")).toBeEnabled();
    await expect(page.getByTestId("commit-moves-btn")).toHaveText("Commit Moves (8)");
    await page.waitForTimeout(300);
    expect(sent.filter((s) => s !== "subscribe" && s !== "ping")).toEqual([]);
    await page.getByTestId("origin-group-41").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `3-two-origins-${name}`, { of: tray, pad: 8 });
  });
}
