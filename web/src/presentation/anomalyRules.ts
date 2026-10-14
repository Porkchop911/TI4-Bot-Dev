/** What each anomaly does, in a sentence or two (Living Rules Reference, "Anomalies"). */
const ANOMALY_RULES: Record<string, { name: string; text: string }> = {
  nebula: {
    name: "Nebula",
    text:
      "A ship can only move into a nebula if it is the active system, and cannot move through one. " +
      "A ship that begins its movement in a nebula has a move value of 1. " +
      "The defender applies +1 to its combat rolls in a nebula.",
  },
  supernova: {
    name: "Supernova",
    text: "Ships cannot move through or into a supernova.",
  },
  "asteroid field": {
    name: "Asteroid field",
    text: "Ships cannot move through or into an asteroid field.",
  },
  "gravity rift": {
    name: "Gravity rift",
    text:
      "A ship moving out of or through a gravity rift applies +1 to its move value. " +
      "Each ship that does so rolls a die after it moves: on a 1 to 3 it is destroyed.",
  },
  "entropic scar": {
    name: "Entropic scar",
    text:
      "Units in an entropic scar cannot use unit abilities, and units outside cannot use them against units in one. " +
      "At the start of the status phase, a player with ships in a scar may spend a command token from their strategy pool to gain a faction technology.",
  },
};

/** The rule for one anomaly kind as the board names it ("gravity rift"); null when unknown. */
export function describeAnomaly(kind: string): { name: string; text: string } | null {
  return ANOMALY_RULES[kind.trim().toLowerCase().replace(/_/g, " ")] ?? null;
}
