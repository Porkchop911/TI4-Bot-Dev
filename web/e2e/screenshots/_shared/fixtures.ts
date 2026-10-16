// Re-exports of the app's synthetic gallery fixtures, so captures share one realistic board.
export { galleryBoard, galleryPlayers, hitAssignmentBoard } from "../../../src/dev/galleryBoard";
export { actor } from "../../../src/dev/decisionGalleryCases";
export { galleryEventLog } from "../../../src/dev/galleryEventLog";

/** An agenda card as the engine puts it into a voting decision's context details. */
export const agendaCard = {
  name: "Sling Relay",
  yes_outcome: "All players may move their ships in non-home systems.",
  no_outcome: "Players cannot move ships during this agenda phase.",
};

import { galleryCases } from "../../../src/dev/decisionGalleryCases";

/** The gallery's activation decision: about thirty systems the player may activate. */
export const systemActivationOptions = galleryCases.find((c) => c.workflow === "system_activation")!.choice.options;

import { fallbackCases } from "../../../src/dev/decisionGalleryCases";

const galleryChoice = (title: string) =>
  [...galleryCases, ...fallbackCases].find((c) => c.title === title)!.choice;

/** Decisions from the dev gallery, as { prompt, context, options } ready for openMockedGame. */
export const galleryDecision = (title: string) => {
  const { prompt, context, options, details } = galleryChoice(title);
  return { prompt, context: context as Record<string, unknown>, options, ...(details ? { details } : {}) };
};

import type { BoardView, CombatView, PlacedUnitView } from "../../../src/protocol/types";
import { actor } from "../../../src/dev/decisionGalleryCases";
import { galleryBoard } from "../../../src/dev/galleryBoard";

const RIVAL = "other_seat";
const unit = (unit_type: string, owner: string, damaged = false): PlacedUnitView => ({ unit_type, owner, damaged });
const times = (n: number, make: () => PlacedUnitView) => Array.from({ length: n }, make);

/** Fleets in system 18 before a space combat: the viewer's mixed fleet against a rival's cruiser and destroyers. */
export const combatStartFleets: PlacedUnitView[] = [
  ...times(8, () => unit("fighter", actor)),
  ...times(2, () => unit("destroyer", actor)),
  ...times(2, () => unit("dreadnought", actor)),
  ...times(2, () => unit("carrier", actor)),
  ...times(2, () => unit("cruiser", RIVAL)),
  ...times(2, () => unit("destroyer", RIVAL)),
];

/** The same system after the fight: the rival is wiped out, the viewer lost two fighters and damaged a dreadnought. */
export const combatEndFleets: PlacedUnitView[] = [
  ...times(6, () => unit("fighter", actor)),
  ...times(2, () => unit("destroyer", actor)),
  unit("dreadnought", actor, true),
  unit("dreadnought", actor),
  ...times(2, () => unit("carrier", actor)),
];

/** A finished space combat in system 18, as the board shows it with `phase: "complete"`. */
export function completedCombatBoard(overrides: Partial<CombatView> = {}): BoardView {
  const base = galleryBoard.systems["18"];
  return {
    ...galleryBoard,
    combat: {
      system_id: "18",
      round: 2,
      phase: "complete",
      attacker: actor,
      defender: RIVAL,
      round_start: combatStartFleets,
      attacker_hits: 4,
      defender_hits: 2,
      dice_rolls: [
        { player: actor, unit: "dreadnought", roll: 8, target: 5, hit: true },
        { player: actor, unit: "dreadnought", roll: 3, target: 5, hit: false },
        { player: actor, unit: "dreadnought", roll: 6, target: 5, hit: true },
        { player: actor, unit: "destroyer", roll: 9, target: 9, hit: true },
        { player: actor, unit: "destroyer", roll: 4, target: 9, hit: false },
        { player: actor, unit: "carrier", roll: 7, target: 9, hit: false },
        { player: actor, unit: "fighter", roll: 10, target: 9, hit: true },
        { player: RIVAL, unit: "cruiser", roll: 8, target: 7, hit: true },
        { player: RIVAL, unit: "cruiser", roll: 5, target: 7, hit: false },
        { player: RIVAL, unit: "destroyer", roll: 9, target: 9, hit: true },
      ],
      ...overrides,
    },
    systems: { ...galleryBoard.systems, "18": { ...base, units: combatEndFleets } },
  };
}

import type { GameEvent } from "../../../src/protocol/types";

/** A short public log: round 2 action phase with one other-player action card play. */
export function actionCardEventLog(by = "other_seat", text = "Played Direct Hit on your Dreadnought in Mecatol Rex"): GameEvent[] {
  return [
    { id: "start", timestamp: "10:00", visibility: { visibility: "public" as const }, event: { kind: "game_initialized", round: 2, phase: "action", speaker: actor } },
    {
      id: "choice-1",
      timestamp: "10:04",
      version: 2,
      visibility: { visibility: "public" as const },
      event: { kind: "decision_resolved" },
      round: 2,
      phase: "action",
      action_id: "action_7",
      action_type: "tactical",
      action_actor: by,
      actor: by,
      stage: "reactions",
      decision_count: 2,
      detail: text,
    } as GameEvent,
  ];
}
