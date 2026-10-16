import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { actor, galleryBoard, opponent, playerWithHand, prefix, viewports } from "./scene";
import { shot } from "../_shared/shot";

// Bombardment with ground forces of two other players on the planet: the invader picks whose
// forces take the next hits. The engine's option labels carry the raw participant id.
const RIVAL_ID = `player_${"b793e54282b7a62cf6b1d7876eb".padEnd(64, "0")}`;
const OTHER_ID = `player_${"7b2a83550545c580c4b919fa03f".padEnd(64, "a")}`;

for (const v of viewports) {
  test(`bombardment-target-${v.id}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: v.width, height: v.height });
    const base = galleryBoard.systems["18"];
    const board = {
      ...galleryBoard,
      systems: {
        ...galleryBoard.systems,
        "18": {
          ...base,
          units: [
            { owner: RIVAL_ID, unit_type: "infantry", damaged: false, planet: "jord" },
            { owner: RIVAL_ID, unit_type: "mech", damaged: false, planet: "jord" },
            { owner: OTHER_ID, unit_type: "infantry", damaged: false, planet: "jord" },
          ],
          planets: { jord: { planet_id: "jord", exhausted: false } },
        },
      },
      invasion: {
        system_id: "18",
        invasion_seq: 1,
        invader: actor,
        phase: "bombardment",
        planets: ["jord"],
        current_planet: "jord",
        defender: null,
        ground_round: 0,
        odds_context: {},
      },
    };
    await openMockedGame(page, {
      players: [playerWithHand(), { ...opponent, id: RIVAL_ID }, { ...opponent, id: OTHER_ID, faction: "letnev" }],
      board: board as never,
      choice: {
        prompt: "whose units on jord take the bombardment's next hits (2 hits)",
        context: { subtype: "bombardment_target", target: { System: "18" } },
        options: [
          { id: RIVAL_ID, label: `${RIVAL_ID}'s units`, kind: "bombardment_target", payload: { system: "18", planet: "jord" } },
          { id: OTHER_ID, label: `${OTHER_ID}'s units`, kind: "bombardment_target", payload: { system: "18", planet: "jord" } },
        ] as never,
      },
    });
    await page.waitForTimeout(800);
    await shot(page, testInfo, `${prefix}-bombardment-target-${v.id}`);
  });
}
