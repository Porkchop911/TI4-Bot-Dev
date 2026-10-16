import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { actor, galleryBoard } from "../_shared/fixtures";

/** Which side of the change a run is: `before` is taken on the commit without planets-first. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const control = (systemId: string, planetId: string) => {
  const sys = galleryBoard.systems[systemId];
  const planets = sys.planets as Record<string, object>;
  return {
    ...sys,
    planets: {
      ...planets,
      [planetId]: { planet_id: planetId, exhausted: false, ...planets[planetId], controlled_by: actor },
    },
  };
};

/**
 * The viewer controls Jord (2 influence, 2 resources), Centauri (3 influence, 1 resource), Dal Bootha
 * and Rarron (2 influence, no resources each; Rarron's printed 3 is lowered to 2 for this scene).
 * One token costs 3: Centauri pays it exactly but exhausts a resource; Dal Bootha + Rarron waste 1
 * influence and no resource.
 */
export const planetsBoard = {
  ...galleryBoard,
  map_tiles: galleryBoard.map_tiles?.map((tile) =>
    tile.system_id === "29"
      ? { ...tile, planets: tile.planets?.map((p) => (p.id === "rarron" ? { ...p, influence: 2 } : p)) }
      : tile,
  ),
  systems: {
    ...galleryBoard.systems,
    "30": control("30", "centauri"),
    "29": control("29", "rarron"),
    "32": control("32", "dal_bootha"),
  },
};

export const PLANETS = [
  { id: "jord", worth: 2 },
  { id: "centauri", worth: 3 },
  { id: "dal_bootha", worth: 2 },
  { id: "rarron", worth: 2 },
];

const pools = { tactic: 3, fleet: 4, strategic: 2 };
const purchase = (tradeGoods = 0) => ({
  cost: 3,
  influence_available: 9 + tradeGoods,
  max: Math.floor((9 + tradeGoods) / 3),
  trade_goods: tradeGoods,
  trade_good_worth: 1,
  planets: PLANETS,
});

/** Leadership primary: three free tokens, then the purchase. */
export const primaryChoice = (tradeGoods = 0) => ({
  prompt: "gain a command token into which pool",
  nonce: "shots-bc-primary",
  context: { subtype: "gain_command_token" },
  options: [
    { id: "tactic_tokens", label: "tactic pool", kind: "pool" },
    { id: "fleet_tokens", label: "fleet pool", kind: "pool" },
    { id: "strategic_tokens", label: "strategy pool", kind: "pool" },
  ],
  details: { kind: "command_tokens", mode: "gain", pools, reinforcements: 8, tokens_to_place: 3, purchase: purchase(tradeGoods) },
});

/** Leadership secondary: only the purchase (nothing free). */
export const secondaryChoice = (tradeGoods = 0) => ({
  prompt: "spend 3 influence for a command token",
  nonce: "shots-bc-secondary",
  context: { subtype: "buy_token_with_influence" },
  options: [
    { id: "yes", label: "yes", kind: "yes" },
    { id: "no", label: "no", kind: "no" },
    { id: "tactic_tokens", label: "tactic pool", kind: "pool" },
    { id: "fleet_tokens", label: "fleet pool", kind: "pool" },
    { id: "strategic_tokens", label: "strategy pool", kind: "pool" },
  ],
  details: { kind: "strategy_secondary", card: "pok1leadership", mode: "buy", costs_token: false, pools, reinforcements: 8, tokens_to_place: 0, purchase: purchase(tradeGoods) },
});

export async function openChoice(page: Page, choice: ReturnType<typeof primaryChoice> | ReturnType<typeof secondaryChoice>) {
  await openMockedGame(page, { players: [playerWithHand({ trade_goods: 0 }), opponent], board: planetsBoard, choice });
  await page.getByTestId("command-token-panel").waitFor();
  await expect(page.getByTestId("command-token-panel")).toBeVisible();
}
