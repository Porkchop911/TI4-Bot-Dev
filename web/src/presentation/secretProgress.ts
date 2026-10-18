import type { BoardView, ObjectiveProgressView, PlayerView } from "../protocol/types.ts";

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
 * A one-line rule reminder under the bar for secrets whose count is easy to misread. Strengthen
 * Bonds counts only notes lying face-up in your play area, not the ones in your hand.
 */
export const SECRET_PROGRESS_HINTS: Readonly<Record<string, string>> = {
  sb: "Only another player's note lying face-up in your play area counts (Alliance, Support for the Throne, some faction notes). Notes in your hand do not.",
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

/**
 * Event secrets whose card says "Only 1 objective can be scored during or after each combat."
 * The engine caps exactly these per combat window (`EventScoreLimit::OnePerPlayer`); Make an
 * Example of Their World, Turn Their Fleets to Dust, Be a Martyr, Prove Endurance and Drive the
 * Debate print no such note and may be scored together.
 */
export const COMBAT_CAPPED_SECRETS: ReadonlySet<string> = new Set([
  "dtgs",
  "sar",
  "uf",
  "baf",
  "btv",
  "dts",
  "dyp",
  "fwp",
]);

export const COMBAT_CAP_TEXT = "Only 1 objective can be scored during or after each combat.";

export interface CombatCapNote {
  /** The notice for the top of the secret section, or null when no offered secret is capped. */
  notice: string | null;
  /** Offered capped secrets that are given up once `selected` is scored. */
  skipped: ReadonlySet<string>;
}

/**
 * What a scoring window should say about the one-per-combat cap, from the secrets it offers (not
 * Plots Within Plots options) and the one currently picked. Display only: the engine enforces it.
 */
export function combatCapNote(offered: readonly string[], selected: string | null): CombatCapNote {
  const capped = offered.filter((alias) => COMBAT_CAPPED_SECRETS.has(alias));
  if (capped.length === 0) return { notice: null, skipped: new Set() };
  if (capped.length === 1) return { notice: COMBAT_CAP_TEXT, skipped: new Set() };
  const skipped =
    selected && capped.includes(selected)
      ? new Set(capped.filter((alias) => alias !== selected))
      : new Set<string>();
  return {
    notice: `${COMBAT_CAP_TEXT} ${capped.length} of your secrets are met; scoring one closes the window for the other${capped.length > 2 ? "s" : ""}.`,
    skipped,
  };
}

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
  /** A rule reminder for counts that are easy to misread, or null. */
  hint: string | null;
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
    hint: SECRET_PROGRESS_HINTS[alias] ?? null,
    systems: [...(progress.systems ?? [])],
  };
}

/** A one-line note for a held secret that has no bar. */
export function secretNoProgressNote(alias: string): string | null {
  if (COMBAT_CAPPED_SECRETS.has(alias)) {
    return `Met by an event: offered the moment it happens. ${COMBAT_CAP_TEXT}`;
  }
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

/**
 * A held secret the map cannot serve for now. Seize an Icon needs a legendary planet; when none
 * is on the map the card is still legal to hold (it stays in the deck by the rules), so the sheet
 * says why it is stuck instead of showing a bare 0 / 1. The Wormhole Nexus and the Fracture are
 * off the main map: their planets count only once the system is in play (it holds something).
 * Null when the board has no tile list (nothing is known) or the card is not affected.
 */
export function secretUnreachableNote(
  alias: string,
  board: Pick<BoardView, "systems" | "map_tiles"> | null | undefined,
): string | null {
  if (alias !== "sai" || !board?.map_tiles || board.map_tiles.length === 0) return null;
  const onMap = board.map_tiles.some(
    (tile) =>
      (!tile.special_area || tile.system_id in board.systems) &&
      (tile.planets ?? []).some((planet) => planet.legendary),
  );
  if (onMap) return null;
  return "No legendary planet is on this map. One could still arrive only through Mirage (a frontier token) or by opening the Wormhole Nexus (Mallice); the card stays in the deck by the rules.";
}
