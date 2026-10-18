import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// The engine's public agenda log (rules version 14): a card revealed and discarded without a vote
// is listed with `discarded` and a reason, then the replacement is voted on and logged as usual.
const log = (records: Record<string, unknown>[]) => ({ agenda_log: records });
const CASES: Record<string, { record: Record<string, unknown>; expect: string[] }> = {
  "no-choice": {
    record: { round: 3, agenda: "classified", name: "Classified Document Leaks", outcome: null, discarded: true, law: true, discard_reason: "no_choice" },
    expect: ["Classified Document Leaks", "Discarded: nothing to elect. The next agenda is revealed."],
  },
  replaced: {
    record: { round: 2, agenda: "standardization", name: "Standardization", outcome: null, discarded: true, discard_reason: "replaced" },
    expect: ["Standardization", "Discarded and replaced by the next agenda."],
  },
};

for (const [size, viewport] of Object.entries(SIZES)) {
  for (const [name, spec] of Object.entries(CASES)) {
    if (size !== "desktop" && name !== "no-choice") continue;
    test(`agenda discard toast ${name} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      const game = await openMockedGame(page, {
        board: galleryBoard,
        players: [playerWithHand({}), opponent],
        phase: "agenda",
        round: 3,
        state: log([]),
      });
      await page.waitForTimeout(500);
      const { events: _events, ...update } = game.snapshot;
      game.send({ ...update, type: "state_update", game_version: 41, state: log([spec.record]) });
      const toast = page.locator('[data-testid="corner-toast"][data-toast-kind="agenda"]');
      await toast.waitFor();
      for (const part of spec.expect) await expect(toast).toContainText(part);
      await page.waitForTimeout(700);
      const box = (await toast.boundingBox())!;
      expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
      await shot(page, testInfo, `${name}-${size}`);
    });
  }
}
