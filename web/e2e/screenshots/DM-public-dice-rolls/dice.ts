import type { GameEvent, PublicRoll } from "../../../src/protocol/types";

let n = 0;

/** A public `dice_rolled` log entry, as the server writes it after a step that rolled. */
export function rollEntry(roll: Omit<PublicRoll, "index">, extra: Partial<GameEvent> = {}): GameEvent {
  n += 1;
  const place = roll.system ? ` in system ${roll.system}` : "";
  const outcome = roll.outcome ? ` (${roll.outcome})` : "";
  return {
    id: `dice-${n}`,
    timestamp: "10:07",
    visibility: { visibility: "public" as const },
    event: { kind: "dice_rolled" },
    round: 2,
    phase: "action",
    actor: roll.roller,
    detail: `${roll.roller} rolled ${roll.label}${place}: ${roll.faces.join(", ")}${outcome}`,
    roll: { index: n, ...roll },
    ...extra,
  } as GameEvent;
}

export const riftDestroyed = (seat: string): Omit<PublicRoll, "index"> => ({
  roller: seat,
  reason: "gravity rift",
  label: "Gravity rift",
  faces: [2],
  hits_on: 4,
  hits: 0,
  system: "67",
  outcome: "ship destroyed",
});

export const riftSurvives = (seat: string): Omit<PublicRoll, "index"> => ({
  ...riftDestroyed(seat),
  faces: [8],
  hits: 1,
  outcome: "ship survives",
});

export const combatRound = (seat: string, faces: number[], hits: number): Omit<PublicRoll, "index"> => ({
  roller: seat,
  reason: "space combat",
  label: "Space combat",
  faces,
  hits_on: 7,
  hits,
  system: "18",
  outcome: hits === 0 ? "no hits" : hits === 1 ? "1 hit" : `${hits} hits`,
});
