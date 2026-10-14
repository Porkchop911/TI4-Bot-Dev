import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, pushAll, recapAlreadyOn, selection, step, OTHER } from "../X-turn-recap-toast/recap";
import { thirdPlayer } from "../G-corner-notifications/corner";

// With the turn recap on, a third seat's Sabotage inside another player's action used to be folded
// away with the rest of that action. Card plays and scoring now toast at once.
test("cards: a card play inside a recapped turn", async ({ page }, testInfo) => {
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent, thirdPlayer] });
  pushAll(game, [
    selection("t-0", "action_50", OTHER, "tactical"),
    step("t-1", "action_50", OTHER, "activated #27", { stage: "activation" }),
    step("t-2", "action_50", "third_seat", "played Sabotage", { stage: "reactions" }),
    step("t-3", "action_50", "third_seat", "scored objective expand_borders", { stage: "effects" }),
  ]);
  await expect(page.getByTestId("corner-toast").first()).toBeVisible();
  await shot(page, testInfo, "3-card-in-recapped-turn", cornerCrop(page));
});
