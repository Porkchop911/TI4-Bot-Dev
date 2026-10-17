import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, withPlayer } from "../_shared/players";

const third = withPlayer(opponent, { id: "third_seat", faction: "yssaril" });

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** The option ids and labels the engine sends (crates/ti4-engine/src/starting_tech.rs). */
const NAMES: Record<string, string> = {
  aida: "AI Development Algorithm",
  amd: "Antimass Deflectors",
  det: "Dark Energy Tap",
  nm: "Neural Motivator",
  pa: "Psychoarchaeology",
  ps: "Plasma Scoring",
  sdn: "Scanlink Drone Network",
  st: "Sarween Tools",
};
const COLOUR: Record<string, string> = {
  aida: "WARFARE",
  amd: "PROPULSION",
  det: "PROPULSION",
  nm: "BIOTIC",
  pa: "BIOTIC",
  ps: "WARFARE",
  sdn: "CYBERNETIC",
  st: "CYBERNETIC",
};

const options = (ids: string[], owners: Record<string, string[]> = {}) =>
  ids.map((id) => ({
    id,
    kind: "starting_technology",
    label: NAMES[id],
    payload: { colour: COLOUR[id], prerequisites: 0, ...(owners[id] ? { owners: owners[id] } : {}) },
  }));

const context = { subtype: "starting_technology", source: { Content: "starting_technology" }, phase: "Strategy", round: 1 };

export interface Scene {
  prompt: string;
  ids: string[];
  details: Record<string, unknown>;
  owners?: Record<string, string[]>;
  held?: string[];
}

/** The Winnu: any one technology without prerequisites. */
export const WINNU: Scene = {
  prompt: "choose a starting technology",
  ids: ["aida", "amd", "det", "nm", "pa", "ps", "sdn", "st"],
  details: { pick: 1, of: 1, rule: "listed", faction: "winnu", held: [] },
};
/** The Argent Flight, first of two picks from three. */
export const ARGENT_FIRST: Scene = {
  prompt: "choose a starting technology (1 of 2)",
  ids: ["nm", "ps", "st"],
  details: { pick: 1, of: 2, rule: "listed", faction: "argent", held: [] },
};
/** The Argent Flight, second pick: the first is held, the third stays in the deck. */
export const ARGENT_SECOND: Scene = {
  prompt: "choose a starting technology (2 of 2)",
  ids: ["nm", "ps"],
  details: { pick: 2, of: 2, rule: "listed", faction: "argent", held: ["st"] },
};
/** The Council Keleres: after every other seat, from what the others hold. */
export const KELERES: Scene = {
  prompt: "choose a starting technology (1 of 2)",
  ids: ["amd", "nm", "ps", "st"],
  details: { pick: 1, of: 2, rule: "owned_by_others", faction: "keleresm", held: [] },
  owners: { amd: ["other_seat"], nm: ["third_seat"], ps: ["third_seat"], st: ["other_seat", "third_seat"] },
};

export async function openScene(page: Page, size: SizeName, scene: Scene) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    phase: "strategy",
    round: 1,
    players: [playerWithHand({ strategy_cards: [], technologies: scene.details.held as string[] }), opponent, third],
    choice: {
      prompt: scene.prompt,
      context,
      details: scene.details,
      options: options(scene.ids, scene.owners),
    },
  });
  await page.getByTestId("choice-option").first().waitFor();
}
