import React, { createContext, useContext, useMemo } from "react";
import type { BoardView } from "../protocol/types.ts";

/** Resource value of every planet on the board by planet id (printed values, from the static tiles). */
export function planetResourceMap(board: BoardView | null | undefined): Map<string, number> {
  const map = new Map<string, number>();
  for (const tile of board?.map_tiles ?? []) {
    for (const planet of tile.planets ?? []) {
      if (typeof planet.resources === "number") map.set(planet.id, planet.resources);
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

/** Influence value of every planet on the board by planet id (printed values, from the static tiles). */
export function planetInfluenceMap(board: BoardView | null | undefined): Map<string, number> {
  const map = new Map<string, number>();
  for (const tile of board?.map_tiles ?? []) {
    for (const planet of tile.planets ?? []) {
      if (typeof planet.influence === "number") map.set(planet.id, planet.influence);
    }
  }
  return map;
}

export const PlanetInfluenceContext = createContext<ReadonlyMap<string, number>>(EMPTY);
export const usePlanetInfluence = (): ReadonlyMap<string, number> => useContext(PlanetInfluenceContext);
