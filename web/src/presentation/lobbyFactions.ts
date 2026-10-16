import { findFactionRecord } from "./factionInfo.ts";
import type { LobbyDto, LobbySlot } from "../protocol/types.ts";

/** Printed name without the article ("The Barony of Letnev" -> "Barony of Letnev"). */
export function lobbyFactionName(id: string): string {
  const name = findFactionRecord(id)?.name ?? id;
  return name.replace(/^The\s+/i, "");
}

/** Two-letter monogram for the faction badge (the project ships no faction art). */
const MONOGRAM: Readonly<Record<string, string>> = {
  sol: "SO", hacan: "HA", letnev: "LE", xxcha: "XX", jolnar: "JN", l1z1x: "L1",
  arborec: "AB", mentak: "ME", muaat: "MU", naalu: "NL", nekro: "NK", saar: "SA",
  sardakk: "SK", winnu: "WI", yin: "YI", yssaril: "YS", ghost: "CR", argent: "AF",
  cabal: "VC", empyrean: "EM", mahact: "MH", naaz: "NZ", nomad: "NO", titans: "UT",
};
export function factionMonogram(id: string): string {
  return MONOGRAM[id] ?? id.slice(0, 2).toUpperCase();
}

/** Badge colours per faction: distinct hues, readable text on each. */
export const FACTION_HUE: Readonly<Record<string, string>> = {
  sol: "#38bdf8", hacan: "#f59e0b", letnev: "#ef4444", xxcha: "#22c55e", jolnar: "#a78bfa",
  l1z1x: "#f472b6", arborec: "#4ade80", mentak: "#fb923c", muaat: "#f87171", naalu: "#fbbf24",
  nekro: "#a3a3a3", saar: "#d6a770", sardakk: "#b45309", winnu: "#e9d5ff", yin: "#e2e8f0",
  yssaril: "#84cc16", ghost: "#2dd4bf", argent: "#fca5a5", cabal: "#c026d3", empyrean: "#818cf8",
  mahact: "#fda4af", naaz: "#a8a29e", nomad: "#67e8f9", titans: "#93c5fd",
};

/** The Prophecy of Kings factions of the pool; every other faction is base game. */
export const POK_FACTIONS: ReadonlySet<string> = new Set([
  "argent", "cabal", "empyrean", "mahact", "naaz", "nomad", "titans",
]);

/** Pool split into the base game and Prophecy of Kings, each in the server's order. */
export function groupFactions(
  pool: readonly string[],
): { label: string; factions: string[] }[] {
  const base = pool.filter((f) => !POK_FACTIONS.has(f));
  const pok = pool.filter((f) => POK_FACTIONS.has(f));
  return [
    { label: "Base game", factions: base },
    { label: "Prophecy of Kings", factions: pok },
  ].filter((g) => g.factions.length > 0);
}

/** The factions the server lets a seat choose; empty on a server that predates the choice. */
export function lobbyFactionPool(lobby: LobbyDto): readonly string[] {
  return lobby.factions ?? [];
}

/** Who holds each chosen faction, by faction id -> seat (except the seat being edited). */
export function takenFactions(
  lobby: LobbyDto,
  exceptPosition?: number,
): Map<string, LobbySlot> {
  const taken = new Map<string, LobbySlot>();
  for (const slot of lobby.slots) {
    if (slot.faction && slot.position !== exceptPosition) taken.set(slot.faction, slot);
  }
  return taken;
}
