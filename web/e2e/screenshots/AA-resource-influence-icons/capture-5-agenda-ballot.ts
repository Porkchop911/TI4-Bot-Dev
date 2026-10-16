import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { agendaCard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

const choice = {
  prompt: "exhaust a planet to vote FOR",
  context: { subtype: "vote_exhaust_planet", details: { agenda_card: agendaCard } },
  options: [
    { id: "mecatol_rex", label: "exhaust Mecatol Rex for 6 votes", kind: "vote_planet", payload: { votes: 6, planet_name: "Mecatol Rex" } },
    { id: "jord", label: "exhaust Jord for 2 votes", kind: "vote_planet", payload: { votes: 2, planet_name: "Jord" } },
    { id: "lodor", label: "exhaust Lodor for 1 vote", kind: "vote_planet", payload: { votes: 1, planet_name: "Lodor" } },
    { id: "decline", label: "Done", kind: "decline" },
  ],
};

// Each planet card shows its influence as icon + number (the votes it adds); a 1-vote planet reads "1 vote (influence)".
test("agenda ballot: influence per planet", async ({ page }, testInfo) => {
  await openMockedGame(page, { phase: "agenda", round: 3, players: [playerWithHand(), opponent], choice });
  const modal = page.getByTestId("agenda-ballot-modal");
  await modal.waitFor();
  await modal.getByText("Mecatol Rex").first().click();
  await shot(page, testInfo, "5a-agenda-planets", { of: modal, pad: 8 });
});

test("agenda ballot on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { phase: "agenda", round: 3, players: [playerWithHand(), opponent], choice });
  await page.getByTestId("agenda-ballot-modal").waitFor();
  await shot(page, testInfo, "5b-agenda-phone");
});
