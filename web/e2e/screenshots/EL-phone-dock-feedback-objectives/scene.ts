import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** `before` on the base commit (TI4_SHOT_PREFIX=before), `after` otherwise. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
  "844x390": { width: 844, height: 390 },
  desktop: { width: 1440, height: 900 },
} as const;
export type SizeName = keyof typeof SIZES;
export const PHONES: SizeName[] = ["390x844", "360x740", "844x390"];
/** Portrait phones show the dock; a landscape phone is wider than 720 px and has none. */
export const PORTRAIT: SizeName[] = ["390x844", "360x740"];

export type Situation = "turn" | "decision";

/** The public alpha server (so the Feedback button exists) with a mocked game: the viewer's turn menu or a decision dialog. */
export async function openScene(page: Page, size: SizeName, situation: Situation) {
  await page.setViewportSize(SIZES[size]);
  await page.route("**/health", (route) =>
    route.fulfill({ json: { status: "ok", protocol_version: 3, commit: "51390594", public: true } }),
  );
  await page.route("**/api/access", (route) => route.fulfill({ json: { required: true, granted: true } }));
  const players = [playerWithHand({ technologies: ["amd", "st"] }), opponent];
  const table = {
    revealed_objectives: ["amass_wealth", "develop", "corner"],
    scored_objectives: { gallery_seat: ["amass_wealth"] },
  };
  if (situation === "turn") {
    await openMockedGame(page, {
      players,
      table,
      choice: {
        prompt: "action phase",
        options: [
          { id: "tactical", label: "take a tactical action", kind: "action" },
          { id: "strategic|pok3politics", label: "take the strategic action of 3. Politics", kind: "action" },
          { id: "pass", label: "pass", kind: "action" },
        ],
        details: {
          kind: "turn_menu",
          closing: false,
          tokens: { tactic: 3, fleet: 3, strategy: 2 },
          strategy_cards: [{ card: "pok3politics", used: false, option: "strategic|pok3politics" }],
          partners: [],
        },
      },
    });
    await page.getByTestId("turn-action-bar").waitFor();
  } else {
    await openMockedGame(page, {
      players,
      table,
      choice: {
        prompt: "Choose a command token to return",
        options: [
          { id: "a", label: "Tactic pool", kind: "choice" },
          { id: "b", label: "Fleet pool", kind: "choice" },
        ],
      },
    });
    // A modal decision covers the whole screen, dock included (for Players just the same); minimized
    // it leaves the dock and the map in view, which is the state a player switches panels in.
    await page.getByRole("button", { name: "Minimize decision" }).click();
    await page.getByRole("dialog", { name: /command token/i }).waitFor({ state: "hidden" }).catch(() => undefined);
  }
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}

/** Whether a tap at the element's centre would hit it (not covered by something else). */
export async function tappable(page: Page, testId: string): Promise<boolean> {
  return page.getByTestId(testId).evaluate((el) => {
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return hit === el || el.contains(hit);
  });
}
