import type { Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { fallbackCases, galleryCases } from "../../../src/dev/decisionGalleryCases";
import { hitAssignmentBoard } from "../_shared/fixtures";
import { measureMap, type OverlayMetrics } from "./measure";

export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
  "844x390": { width: 844, height: 390 },
} as const;
export type SizeName = keyof typeof SIZES;

export const allCases = [...galleryCases, ...fallbackCases];

/** Opens one gallery decision in the real app (mocked server) as the deciding seat. */
export async function openCase(page: Page, title: string, size: SizeName) {
  const c = allCases.find((x) => x.title === title)!;
  await page.setViewportSize(SIZES[size]);
  const choice: MockChoice = {
    prompt: c.choice.prompt,
    context: c.choice.context as Record<string, unknown>,
    options: c.choice.options,
    ...(c.choice.details ? { details: c.choice.details as Record<string, unknown> } : {}),
  };
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice,
    ...(c.boardId === "hit_assignment" ? { board: hitAssignmentBoard } : {}),
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}

/** Clicks a minimize-like control (test id or visible text) of whatever covers the map. */
export async function minimize(page: Page, control: string | null): Promise<boolean> {
  if (!control) return false;
  const target = page
    .locator(`[data-testid="${control}"]`)
    .or(page.getByRole("button", { name: control, exact: false }))
    .first();
  if (!(await target.count())) return false;
  try {
    await target.click({ timeout: 2000 });
  } catch {
    return false;
  }
  await page.waitForTimeout(300);
  return true;
}

export interface Row {
  title: string;
  size: string;
  open: OverlayMetrics;
  minimized?: OverlayMetrics;
}

export async function audit(page: Page, title: string, size: SizeName): Promise<Row> {
  await openCase(page, title, size);
  const open = await measureMap(page);
  const row: Row = { title, size, open };
  if (open.covered > 0.15 && (await minimize(page, open.minimizeControl))) row.minimized = await measureMap(page);
  return row;
}
