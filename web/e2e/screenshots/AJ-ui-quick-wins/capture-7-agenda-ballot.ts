import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { agendaCard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Two planets staged: one "Staged: +8 votes" line, a check mark on staged rows, "N votes" on each
// row, and one action row (Reset as a link, Finish without voting more, Cast votes).
test("agenda ballot: staged planets", async ({ page }, testInfo) => {
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
  await modal.getByTestId("planet-card-mecatol_rex").click();
  await modal.getByTestId("planet-card-jord").click();
  await shot(page, testInfo, "7-agenda-ballot");
});
