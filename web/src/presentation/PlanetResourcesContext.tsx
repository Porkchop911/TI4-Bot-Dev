import React, { createContext, useContext, useMemo } from "react";
import type { BoardView } from "../protocol/types.ts";
import { type PlanetBoost, wireBoosts } from "@ti4/shared/planets/values.ts";

/** What raises each planet above its printed values, by planet id (planets with no boost are absent). */
export function planetBoostMap(board: BoardView | null | undefined): Map<string, PlanetBoost[]> {
  const map = new Map<string, PlanetBoost[]>();
  for (const system of Object.values(board?.systems ?? {})) {
    for (const [id, planet] of Object.entries(system.planets ?? {})) {
      const boosts = wireBoosts(planet);
      if (boosts.length > 0) map.set(id, boosts);
    }
  }
  return map;
}

const sum = (boosts: readonly PlanetBoost[] | undefined, key: "resources" | "influence"): number =>
  (boosts ?? []).reduce((total, b) => total + b[key], 0);

/** Resource value of every planet on the board by planet id: printed (static tiles) plus boosts. */
export function planetResourceMap(board: BoardView | null | undefined): Map<string, number> {
  const map = new Map<string, number>();
  const boosts = planetBoostMap(board);
  for (const tile of board?.map_tiles ?? []) {
    for (const planet of tile.planets ?? []) {
      if (typeof planet.resources === "number") {
        map.set(planet.id, planet.resources + sum(boosts.get(planet.id), "resources"));
      }
    }
  }
  return map;
}

const EMPTY: ReadonlyMap<string, number> = new Map();
export const PlanetResourcesContext = createContext<ReadonlyMap<string, number>>(EMPTY);

/** Makes planet resource values available to the influence payment UIs (Leadership, Auto-pay). */
export const PlanetResourcesProvider: React.FC<{
  board: BoardView | null | undefined;
  children: React.ReactNode;
}> = ({ board, children }) => {
  const map = useMemo(() => planetResourceMap(board), [board]);
  return <PlanetResourcesContext.Provider value={map}>{children}</PlanetResourcesContext.Provider>;
};

export const usePlanetResources = (): ReadonlyMap<string, number> => useContext(PlanetResourcesContext);

/** Influence value of every planet on the board by planet id: printed (static tiles) plus boosts. */
export function planetInfluenceMap(board: BoardView | null | undefined): Map<string, number> {
  const map = new Map<string, number>();
  const boosts = planetBoostMap(board);
  for (const tile of board?.map_tiles ?? []) {
    for (const planet of tile.planets ?? []) {
      if (typeof planet.influence === "number") {
        map.set(planet.id, planet.influence + sum(boosts.get(planet.id), "influence"));
      }
    }
  }
  return map;
}

export const PlanetInfluenceContext = createContext<ReadonlyMap<string, number>>(EMPTY);
export const usePlanetInfluence = (): ReadonlyMap<string, number> => useContext(PlanetInfluenceContext);

const NO_BOOSTS: ReadonlyMap<string, PlanetBoost[]> = new Map();
export const PlanetBoostContext = createContext<ReadonlyMap<string, PlanetBoost[]>>(NO_BOOSTS);
/** Boosts by planet id, for the payment and vote UIs that know a planet id but not the board. */
export const usePlanetBoosts = (): ReadonlyMap<string, PlanetBoost[]> => useContext(PlanetBoostContext);
