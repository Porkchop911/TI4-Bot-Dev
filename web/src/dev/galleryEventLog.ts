import type { GameEvent } from "../protocol/types.ts";
import { gallerySeating } from "./galleryBoard.ts";

const [alex, blair] = gallerySeating;

/** Public synthetic history shown through the real GameShell log. */
export const galleryEventLog: GameEvent[] = [
  {
    id: "start",
    timestamp: "10:00",
    visibility: { visibility: "public" as const },
    event: {
      kind: "game_initialized",
      round: 1,
      phase: "action",
      speaker: alex,
    },
  },
  ...(
    [
      "activation",
      "movement",
      "movement",
      "combat",
      "reactions",
      "combat",
      "invasion",
      "production",
    ] as const
  ).map((stage, index): GameEvent => ({
    id: `choice-${index + 1}`,
    timestamp: `10:0${index + 1}`,
    version: index + 1,
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    round: 1,
    phase: "action",
    action_id: "action_1",
    action_type: "tactical",
    action_actor: alex,
    actor: index === 4 ? blair : alex,
    stage,
    decision_count: index + 1,
    batch_id: index === 1 || index === 2 ? "batch-move" : undefined,
    detail: index === 4 ? "Blair reacted" : `Synthetic ${stage} choice`,
  })),
  {
    id: "second-round",
    timestamp: "11:00",
    visibility: { visibility: "public" as const },
    event: { kind: "phase_transition", round: 2, phase: "strategy" },
  },
  {
    id: "strategy",
    timestamp: "11:01",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    round: 2,
    phase: "strategy",
    actor: blair,
    decision_count: 9,
    detail: "Selected a strategy card",
  },
];
