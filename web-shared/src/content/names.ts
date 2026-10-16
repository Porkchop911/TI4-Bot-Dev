// Pure helpers about the ids of the corpus. No text for a player: the names are the catalog's.

/**
 * An id written as words: underscores become spaces and each word starts in upper case, also
 * after a hyphen ("support_for_the_throne" → "Support For The Throne", "jol-nar" → "Jol-Nar"). A
 * fallback for an id the catalog does not name, in both apps.
 */
export function humanizeId(id: string): string {
  return id.replace(/_/g, " ").replace(/\b\w/g, (char) => char.toUpperCase());
}

/**
 * The factions of the base game that a table can seat, in the engine's order
 * (`ti4_engine::seating::LOBBY_FACTIONS`).
 */
export const BASE_FACTION_IDS: readonly string[] = [
  "sol",
  "hacan",
  "letnev",
  "xxcha",
  "jolnar",
  "l1z1x",
  "arborec",
  "mentak",
  "muaat",
  "naalu",
  "nekro",
  "saar",
  "sardakk",
  "winnu",
  "yin",
  "yssaril",
  "ghost",
];

/** The factions of Prophecy of Kings that a table can seat, in the engine's order. */
export const POK_FACTION_IDS: readonly string[] = [
  "argent",
  "cabal",
  "empyrean",
  "mahact",
  "naaz",
  "nomad",
  "titans",
];

/**
 * Every faction that a table can seat, base game first: what the engine offers
 * (`SetupOptions.faction_groups`).
 */
export const FACTION_IDS: readonly string[] = [...BASE_FACTION_IDS, ...POK_FACTION_IDS];

/** The Prophecy of Kings factions; every other faction of `FACTION_IDS` is of the base game. */
export const POK_FACTIONS: ReadonlySet<string> = new Set(POK_FACTION_IDS);
