import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

const SCIENCES = {
  name: "Minister of Sciences",
  yes_outcome: "The elected player gains this card.",
  no_outcome: "",
};
const NAMES = ["Ada", "Ben"];
const players = () => [playerWithHand(), opponent];
const nicknames = (ids: string[]) => Object.fromEntries(ids.map((id, i) => [id, NAMES[i] ?? id]));

test("Minister of Sciences: choose who to elect", async ({ page }, testInfo) => {
  const ps = players();
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: ps,
    nicknames: nicknames(ps.map((p) => p.id)),
    choice: {
      prompt: "vote for which outcome",
      context: { subtype: "cast_vote", details: { agenda_card: SCIENCES } },
      options: [
        ...ps.map((p) => ({ id: p.id, label: p.id, kind: "vote", payload: { current_votes: 0 } })),
        { id: "decline", label: "Abstain", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("agenda-ballot-modal").waitFor();
  for (const [i, p] of ps.entries()) {
    await expect(page.getByTestId(`vote-outcome-opt-${p.id}`)).toContainText(NAMES[i]);
  }
  await shot(page, testInfo, "1-cast-vote-elect-player");
});

test("Elect player tie: the speaker picks a player", async ({ page }, testInfo) => {
  const ps = players();
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: ps,
    nicknames: nicknames(ps.map((p) => p.id)),
    choice: {
      prompt: "speaker breaks the tie",
      context: { subtype: "vote_tiebreak", details: { agenda_card: SCIENCES } },
      options: ps.map((p) => ({ id: p.id, label: p.id, kind: "tiebreak" })),
    },
  });
  await page.getByTestId("agenda-ballot-modal").waitFor();
  await expect(page.getByTestId(`tiebreak-opt-${ps[1].id}`)).toContainText("Ben");
  await shot(page, testInfo, "2-tiebreak-elect-player");
});

test("Committee Formation: elect which player", async ({ page }, testInfo) => {
  const ps = players();
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: ps,
    nicknames: nicknames(ps.map((p) => p.id)),
    choice: {
      prompt: "minister_sciences: elect which player",
      context: { subtype: "committee" },
      options: ps.map((p) => ({ id: p.id, label: `elect ${p.id}`, kind: "player" })),
    },
  });
  await page.getByTestId("choice-option").first().waitFor();
  await expect(page.getByTestId("choice-option").nth(1)).toContainText("elect Ben");
  await shot(page, testInfo, "3-committee-formation");
});
