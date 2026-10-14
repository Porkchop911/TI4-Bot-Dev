import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { agendaCard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// After choosing FOR the player spends influence: planets are staged into a basket with their
// vote worth, and the total is shown before anything is exhausted.
test("agenda: spend influence on votes", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "exhaust a planet to vote FOR",
      context: { subtype: "vote_exhaust_planet", details: { agenda_card: agendaCard } },
      options: [
        { id: "mecatol_rex", label: "exhaust Mecatol Rex for 6 votes", kind: "vote_planet", payload: { votes: 6, planet_name: "Mecatol Rex" } },
        { id: "jord", label: "exhaust Jord for 2 votes", kind: "vote_planet", payload: { votes: 2, planet_name: "Jord" } },
        { id: "lodor", label: "exhaust Lodor for 3 votes", kind: "vote_planet", payload: { votes: 3, planet_name: "Lodor" } },
        { id: "decline", label: "Done", kind: "decline" },
      ],
    },
  });
  const modal = page.getByTestId("agenda-ballot-modal");
  await modal.waitFor();
  await modal.getByText("Mecatol Rex").first().click();
  await modal.getByText("Jord").first().click();
  await shot(page, testInfo, "2-spend-influence");
});
