import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { agendaCard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The vote ended level: the speaker breaks the tie.
test("agenda: speaker breaks a tie", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "speaker breaks the tie",
      context: { subtype: "vote_tiebreak", details: { agenda_card: agendaCard } },
      options: [
        { id: "FOR", label: "FOR", kind: "tiebreak" },
        { id: "AGAINST", label: "AGAINST", kind: "tiebreak" },
      ],
    },
  });
  await page.getByTestId("agenda-ballot-modal").waitFor();
  await shot(page, testInfo, "3-tiebreak");
});
