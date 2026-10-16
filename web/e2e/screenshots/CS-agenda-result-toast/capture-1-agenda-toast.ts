import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// The engine's public agenda log, as the state carries it once a vote has finished.
const log = (records: Record<string, unknown>[]) => ({ agenda_log: records });
const RESULTS: Record<string, { record: Record<string, unknown>; expect: string[] }> = {
  "for-wins": {
    record: { round: 3, agenda: "revolution", name: "Anti-Intellectual Revolution", outcome: "for", votes: { for: 9, against: 4 }, law: true, enacted: true },
    expect: ["Anti-Intellectual Revolution: For wins", "Votes: For 9 · Against 4", "The law enters play."],
  },
  "against-wins": {
    record: { round: 3, agenda: "economic_equality", name: "Economic Equality", outcome: "against", votes: { for: 3, against: 7 } },
    expect: ["Economic Equality: Against wins", "Votes: Against 7 · For 3", "Directive resolved."],
  },
  election: {
    record: { round: 3, agenda: "minister_peace", name: "Minister of Peace", outcome: actor, votes: { [actor]: 6, [opponent.id]: 5 }, law: true, enacted: true },
    expect: ["Minister of Peace: You elected", "Votes: You 6 · Player 2 5", "The law enters play."],
  },
  tie: {
    record: { round: 3, agenda: "mutiny", name: "Mutiny", outcome: "against", votes: { for: 4, against: 4 }, speaker_decided: true },
    expect: ["Mutiny: Against wins", "Tied (For 4 · Against 4): the speaker decided.", "Directive resolved."],
  },
  "no-votes": {
    record: { round: 3, agenda: "incentive", name: "Incentive Program", outcome: "for", votes: {}, speaker_decided: true },
    expect: ["Incentive Program: For wins", "No votes cast: the speaker decided."],
  },
};

for (const [size, viewport] of Object.entries(SIZES)) {
  for (const [name, spec] of Object.entries(RESULTS)) {
    if (size !== "desktop" && name !== "for-wins" && name !== "election") continue;
    test(`agenda result toast ${name} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      // The viewer voted on this agenda: the result still toasts for them.
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
      await expect(toast.getByTestId("toast-agenda-tag")).toHaveText("Agenda");
      for (const part of spec.expect) await expect(toast).toContainText(part);
      // The same log in a later state does not toast twice.
      game.send({ ...update, type: "state_update", game_version: 42, state: log([spec.record]) });
      await page.waitForTimeout(700); // the toast slides in
      await expect(toast).toHaveCount(1);
      // The table's news, in the politics gold rather than a seat colour.
      await expect(toast).toHaveCSS("border-left-color", "rgb(234, 179, 8)");
      const box = (await toast.boundingBox())!;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
      expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
      await shot(page, testInfo, `${name}-${size}`);
    });
  }
}
