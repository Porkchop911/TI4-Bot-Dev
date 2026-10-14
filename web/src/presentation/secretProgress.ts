import type { ObjectiveProgressView, PlayerView } from "../protocol/types.ts";

/**
 * What the server's `secret_objective_progress` count measures, per secret alias. The numbers
 * come from the engine helper that backs the requirement itself (`ti4_engine::secrets::progress`),
 * so a full bar is a card that is legal to score; this table only words the count.
 */
export const SECRET_PROGRESS_UNITS: Readonly<Record<string, string>> = {
  ctr: "systems with at least one of your ships",
  eap: "PDS on the game board",
  fwm: "space docks on the game board",
  gamf: "dreadnoughts on the game board",
  faa: "cultural planets controlled",
  mrm: "hazardous planets controlled",
  mp: "industrial planets controlled",
  mlp: "technologies in your strongest color",
  mtm: "planets with one of your mechs",
  otf: "ground forces on your best planet without your space dock",
  csl: "systems where your ships share a rival's space dock",
  dhw: "relic fragments (purged when scored)",
  fsn: "action cards in hand (discarded when scored)",
  sb: "rivals' promissory notes in your play area",
  lsc: "systems adjacent to an anomaly with your ships",
  te: "systems adjacent to a rival home system with your ships",
  fc: "other players you are neighbors with",
  dfat: "units in the wormhole nexus",
  btgk: "wormhole types (alpha, beta) your ships sit on",
  ans: "faction technologies owned",
  dp: "laws in play",
  syc: "systems where you and a rival both control a planet",
  pem: "PRODUCTION in your best single system",
  sai: "legendary planets controlled",
  ose: "ships at Mecatol Rex while you control it",
  eh: "influence on planets you control",
  hrm: "resources on planets you control",
};

/**
 * Secrets that are met by something happening, not by something you have: no running count
 * exists, so they show no bar — the card is offered at the moment it happens.
 */
export const EVENT_SECRETS: ReadonlySet<string> = new Set([
  "baf",
  "bam",
  "btv",
  "dtd",
  "dtgs",
  "dts",
  "dyp",
  "fwp",
  "mew",
  "pe",
  "sar",
  "ttfd",
  "uf",
]);

export interface SecretProgressDisplay {
  have: number;
  threshold: number;
  satisfied: boolean;
  /** 0–100, for the bar. */
  percent: number;
  /** "systems with at least one of your ships". */
  unit: string;
  /** "4 / 6 systems with at least one of your ships". */
  text: string;
  /** The systems that count right now, when the server names them (Learn the Secrets). */
  systems: string[];
}

/** The viewer's progress toward one of their own secrets, or null when it has no count. */
export function secretProgressDisplay(
  alias: string,
  progress: ObjectiveProgressView | undefined,
): SecretProgressDisplay | null {
  if (!progress || progress.threshold <= 0) return null;
  const unit = SECRET_PROGRESS_UNITS[alias] ?? "toward the requirement";
  const have = Math.max(0, progress.have);
  return {
    have,
    threshold: progress.threshold,
    satisfied: progress.satisfied,
    percent: Math.min(100, Math.round((have / progress.threshold) * 100)),
    unit,
    text: `${have} / ${progress.threshold} ${unit}`,
    systems: [...(progress.systems ?? [])],
  };
}

/** A one-line note for a held secret that has no bar. */
export function secretNoProgressNote(alias: string): string | null {
  if (EVENT_SECRETS.has(alias)) return "Met by an event: offered the moment it happens";
  return null;
}

/** The progress the server sent for one of `player`'s secrets (only ever the viewer's own). */
export function secretProgressOf(
  player: Pick<PlayerView, "secret_objective_progress"> | undefined,
  alias: string,
): ObjectiveProgressView | undefined {
  return player?.secret_objective_progress?.[alias];
}
