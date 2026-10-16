// What both transports read of a `ServerMessage`: the server sends them on a websocket
// (`wsTransport.ts`), and the engine of this page writes the same messages (`wasmTransport.ts`).
import type { PlanStopped } from "./transport";
import type {
  ReactionMode,
  ReactionModes,
  RejectionReason,
  ScriptStopped,
  SessionUpdate,
  StateMsg,
} from "./wire";

/** What a page says of an answer that the game did not take. */
export function rejection(reason: RejectionReason): string {
  switch (reason.reason) {
    case "stale_version":
    case "stale_nonce":
      return "The game went on before your answer arrived. Nothing was sent.";
    case "no_pending_choice":
      return "The game does not wait for this answer. Nothing was sent.";
    case "unauthorized_seat":
      return "The server does not take this answer from you. Nothing was sent.";
    case "unknown_option":
      return `The game does not offer "${reason.option_id}". Nothing was sent.`;
    case "validation_failed":
      return reason.message;
    case "answered_elsewhere":
      return "Your answer was given in another tab. Nothing was sent.";
    case "undo_barrier":
      return {
        revealed: "Something hidden was shown since your last decision. It cannot be taken back.",
        random: "Dice were rolled since your last decision. It cannot be taken back.",
        other_seat: "Another player decided since your last decision. It cannot be taken back.",
      }[reason.barrier];
  }
}

/** The part of a message of the game that is the update. `state` is large and not read. */
export const updateOf = (message: StateMsg): SessionUpdate => ({
  viewer: message.viewer,
  view: message.view,
  pending_choice: message.pending_choice,
  turn_status: message.turn_status,
  tactical: message.tactical,
  exploration: message.exploration,
  action_cards: message.action_cards,
  secret_objectives: message.secret_objectives,
  score: message.score,
  agendas: message.agendas,
  secondary: message.secondary,
  reaction: message.reaction,
  research: message.research,
  seats: message.seats,
});

/** The modes that are set, in the order of their keys: "always" is the mode of a key that is not there. */
export function modesOf(told: Record<string, ReactionMode>): ReactionModes {
  const modes: ReactionModes = {};
  for (const key of Object.keys(told).sort()) {
    const mode = told[key];
    if (mode === "never" || mode === "ownturn") {
      modes[key] = mode;
    }
  }
  return modes;
}

/** The windows of a message that were declined for this side, because it muted all of their cards: by note. */
export const passedOf = (message: StateMsg) =>
  (message.auto_resolved ?? []).flatMap((note) =>
    note.passed?.length ? [{ id: note.id, cards: note.passed, count: note.count ?? 1 }] : [],
  );

/**
 * Where a script that was sent stopped (`draft_applied`), counted as a plan counts: the game
 * counts in entries of the script, and a plan in the steps after the `lead` answers by id.
 */
export function planStoppedOf(
  stopped: ScriptStopped,
  sent: { destination: string; lead: number },
): PlanStopped {
  const { applied, remaining, interrupted, reason } = stopped;
  return {
    destination: sent.destination,
    applied: Math.max(0, applied - sent.lead),
    remaining: remaining.slice(Math.max(0, sent.lead - applied)) as PlanStopped["remaining"],
    interrupted,
    reason,
  };
}
