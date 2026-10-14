import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { agendaCard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Casting a vote: the agenda card with both outcomes, and the running tally per outcome.
test("agenda: cast a vote", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "vote for which outcome",
      context: { subtype: "cast_vote", details: { agenda_card: agendaCard } },
      options: [
        { id: "FOR", label: "FOR", kind: "vote", payload: { current_votes: 12 } },
        { id: "AGAINST", label: "AGAINST", kind: "vote", payload: { current_votes: 8 } },
        { id: "decline", label: "Abstain", kind: "decline" },
      ],
    },
  });
  const modal = page.getByTestId("agenda-ballot-modal");
  await modal.waitFor();
  await shot(page, testInfo, "1-cast-vote");
});
