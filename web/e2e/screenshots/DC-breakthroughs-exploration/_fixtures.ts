import type { BoardView, PlayerView } from "../../../src/protocol/types";
import { galleryBoard, galleryPlayers, actor } from "../_shared/fixtures";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

export { actor, opponent };

/** Lodor carries Dyson Sphere (an attachment the cultural deck left there). */
export const board: BoardView = {
  ...galleryBoard,
  systems: {
    ...galleryBoard.systems,
    "26": {
      ...galleryBoard.systems["26"],
      planets: { lodor: { planet_id: "lodor", controlled_by: actor, exhausted: false, attachments: ["dysonsphere"] } },
    },
  },
};

/** What the engine's exploration_log holds after a few turns of exploring. */
export const exploration_log = [
  { round: 1, player: actor, deck: "CULTURAL", card: "ds", planet: "lodor", outcome: "attached:dysonsphere" },
  { round: 1, player: actor, deck: "HAZARDOUS", card: "cm1", planet: "mehar_xull", outcome: "resolved", result: "gained 1 trade good, removed 1 infantry from the planet" },
  { round: 2, player: opponent.id, deck: "CULTURAL", card: "crf1", planet: "wellon", outcome: "fragment", result: "took a cultural relic fragment" },
  { round: 2, player: actor, deck: "FRONTIER", card: "ed2", planet: null, system: "39", outcome: "resolved" },
];

export const players = (breakthrough: string | null = "solbt"): { list: PlayerView[]; state: Record<string, unknown> } => ({
  list: [
    playerWithHand({ faction: "sol", leaders: {} }),
    withPlayer(galleryPlayers[1], { faction: "letnev" }),
  ],
  state: {
    exploration_log,
    frontier_tokens: ["41"],
    players: [
      { id: actor, breakthrough },
      { id: opponent.id, breakthrough: null },
    ],
  },
});
