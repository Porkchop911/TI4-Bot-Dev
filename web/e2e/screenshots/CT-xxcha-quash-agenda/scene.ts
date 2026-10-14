import type { MockChoice, MockGameOptions } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

// Printed agendas as the engine's agenda_details sends them (crates/ti4-content agendas.json).
export const MUTINY = {
  id: "mutiny",
  name: "Mutiny",
  type: "Directive",
  target: "For/Against",
  text1: 'For: Each player that voted "For" gains 1 victory point.',
  text2: 'Against: Each player that voted "For" loses 1 victory point.',
};
export const ARCHIVED_SECRET = {
  id: "secret",
  name: "Archived Secret",
  type: "Directive",
  target: "Elect Player",
  text1: "The elected player draws 1 secret objective.",
  text2: "",
};

const useOrDecline = (alias: string) => [
  { id: "use", label: `use ${alias}`, kind: "promissory_note" },
  { id: "decline", label: "decline", kind: "decline" },
];

/** Quash as the engine asks it: subtype and source "quash", options use/decline. */
export const quash = (agenda: Record<string, string> | null): MockChoice => ({
  prompt:
    "Quash: spend 1 strategy token to discard this agenda and reveal the next",
  context: {
    subtype: "quash",
    source: { Content: "quash" },
    phase: "agenda",
    round: 3,
  } as never,
  ...(agenda ? { details: { agenda } } : {}),
  options: useOrDecline("quash"),
});

export const politicalFavor = (agenda: Record<string, string>): MockChoice => ({
  prompt: "Political Favor: discard this agenda and reveal the next",
  context: {
    subtype: "favor",
    source: { Content: "favor" },
    phase: "agenda",
    round: 3,
  } as never,
  details: { agenda },
  options: useOrDecline("favor"),
});

/** The action-card window on the reveal (Veto and the other "when an agenda is revealed" cards). */
export const vetoWindow = (agenda: Record<string, string>): MockChoice => ({
  prompt: "when AGENDA_REVEALED",
  context: {
    subtype: "reaction_when_AGENDA_REVEALED",
    optional: true,
    trigger: {
      kind: "agenda_revealed",
      event_type: "AGENDA_REVEALED",
      event_id: 12,
      relation: "when",
      agenda: agenda.id,
    },
  } as never,
  details: { agenda },
  options: [
    {
      id: "action_card:p1:veto:AGENDA_REVEALED:when",
      label: "Play Veto",
      kind: "ability",
      payload: { card: "veto", card_name: "Veto" },
    },
    { id: "decline", label: "Pass", kind: "decline" },
  ],
});

/** Confusing Legal Text's window, on the agenda being resolved. */
export const resolvedWindow = (agenda: Record<string, string>): MockChoice => ({
  prompt: "when AGENDA_RESOLVED",
  context: {
    subtype: "reaction_when_AGENDA_RESOLVED",
    optional: true,
    trigger: {
      kind: "agenda_resolved",
      event_type: "AGENDA_RESOLVED",
      event_id: 30,
      relation: "when",
      agenda: agenda.id,
    },
  } as never,
  details: { agenda },
  options: [
    {
      id: "action_card:p1:confusing:AGENDA_RESOLVED:when",
      label: "Play Confusing Legal Text",
      kind: "ability",
      payload: { card: "confusing", card_name: "Confusing Legal Text" },
    },
    { id: "decline", label: "Pass", kind: "decline" },
  ],
});

export const options = (choice: MockChoice): MockGameOptions => ({
  phase: "agenda",
  round: 3,
  players: [playerWithHand({ faction: "xxcha" }), opponent],
  choice,
});
