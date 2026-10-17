import type { BoardView } from "../../../src/protocol/types";
import { galleryBoard } from "../../../src/dev/galleryBoard";
import { actor } from "../../../src/dev/decisionGalleryCases";

/** `before` on the base commit (TI4_SHOT_PREFIX=before), `after` otherwise. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const inf = (owner: string, planet: string) => ({ unit_type: "infantry", owner, planet, damaged: false });

/** Mecatol Rex: the gallery infantry plus three of the viewer's on Jord, one rival's on the other planet. */
export const plagueBoard: BoardView = {
  ...galleryBoard,
  systems: {
    ...galleryBoard.systems,
    "18": {
      ...galleryBoard.systems["18"],
      units: [
        ...(galleryBoard.systems["18"]?.units ?? []),
        inf(actor, "jord"),
        inf(actor, "jord"),
        inf(actor, "jord"),
        inf("other_seat", "exhausted"),
      ],
    },
  },
};

export const plagueChoice = {
  prompt: "Plague: which planet",
  context: { subtype: "plague_pick_planet", source: { ActionCard: "plague" } },
  options: [
    { id: "18|jord", kind: "planet", label: "Jord", payload: { planet: "jord", system: "18" } },
    { id: "18|exhausted", kind: "planet", label: "Exhausted World", payload: { planet: "exhausted", system: "18" } },
  ],
};
