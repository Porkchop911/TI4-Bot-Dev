import type { PlayerView } from "../../../src/protocol/types";
import { actor, galleryPlayers } from "./fixtures";

export const me = galleryPlayers[0];
export const opponent = galleryPlayers[1];

/** A copy of a gallery player with overrides, e.g. held cards or tokens. */
export function withPlayer(base: PlayerView, overrides: Partial<PlayerView>): PlayerView {
  return { ...base, ...overrides };
}

/** The viewing player with a private hand worth showing. */
export const playerWithHand = (overrides: Partial<PlayerView> = {}): PlayerView =>
  withPlayer(me, {
    victory_points: 3,
    action_cards_count: 4,
    held_action_cards: ["sabo1", "direct_hit", "decoy", "skilled_retreat"],
    secret_objectives_count: 1,
    held_secret_objectives: ["faa"],
    strategy_cards: ["pok3politics"],
    ...overrides,
  });

export { actor };
