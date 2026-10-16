import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { planetsBoard, PLANETS } from "../BC-leadership-planets-first/scene";

/** `before` shots are taken with the headline change reverted and committed under before/. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const pools = { tactic: 3, fleet: 4, strategic: 2 };
const options = [
  { id: "tactic_tokens", label: "tactic pool", kind: "pool" },
  { id: "fleet_tokens", label: "fleet pool", kind: "pool" },
  { id: "strategic_tokens", label: "strategy pool", kind: "pool" },
];

const gain = (nonce: string, tokens: number, extra: object = {}) => ({
  prompt: "gain a command token into which pool",
  nonce,
  context: { subtype: "gain_command_token" },
  options,
  details: { kind: "command_tokens", mode: "gain", pools, reinforcements: 8, tokens_to_place: tokens, ...extra },
});

/** Status phase: two tokens, no purchase. */
export const statusChoice = () => gain("shots-bp-status", 2);
/** Leadership primary: three free tokens and the purchase. */
export const leadershipChoice = () =>
  gain("shots-bp-leadership", 3, {
    purchase: { cost: 3, influence_available: 9, max: 3, trade_goods: 0, trade_good_worth: 1, planets: PLANETS },
  });
/** A single token (an effect that gains exactly one). */
export const singleChoice = () => gain("shots-bp-single", 1);

export async function openChoice(page: Page, choice: ReturnType<typeof gain>) {
  await openMockedGame(page, { players: [playerWithHand({ trade_goods: 0 }), opponent], board: planetsBoard, choice });
  await page.getByTestId("command-token-panel").waitFor();
  await expect(page.getByTestId("command-token-panel")).toBeVisible();
}
