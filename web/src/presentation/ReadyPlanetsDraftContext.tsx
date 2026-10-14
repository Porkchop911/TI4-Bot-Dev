import React, { createContext, useCallback, useContext, useMemo, useState } from "react";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import {
  describeReadyPlanets,
  toggleReadyPlanet,
  validReadySelection,
  type ReadyPlanetsView,
} from "./readyPlanets.ts";

/** The planets picked on Diplomacy's ready question; nothing is sent before Confirm. */
export interface ReadyPlanetsDraftApi {
  /** Null whenever the pending decision is not a ready question. */
  view: ReadyPlanetsView | null;
  selected: string[];
  toggle: (planetId: string) => void;
  /** Replaces the selection (a prepared answer, or the plan being prepared). */
  set: (planetIds: readonly string[]) => void;
  reset: () => void;
}

/**
 * The picks and the question they belong to. Picks belong to one decision (a new nonce starts
 * over). When there is nothing to choose (as many planets as must be readied) all are picked
 * already, so the player only confirms.
 */
export function useReadyPlanetsDraft(
  choice: PendingChoiceDto | null | undefined,
  board?: BoardView | null,
): ReadyPlanetsDraftApi {
  const view = useMemo(() => describeReadyPlanets(choice, board), [choice, board]);
  const nonce = choice?.nonce ?? null;
  const [picked, setPicked] = useState<{ nonce: string | null; ids: string[] } | null>(null);
  const initial = useMemo(
    () => (view && view.planets.length <= view.required ? view.planets.map((planet) => planet.id) : []),
    [view],
  );
  const current = picked && picked.nonce === nonce ? picked.ids : initial;
  const toggle = useCallback(
    (planetId: string) => {
      if (view) setPicked({ nonce, ids: toggleReadyPlanet(view, current, planetId) });
    },
    [view, nonce, current],
  );
  const set = useCallback(
    (planetIds: readonly string[]) => {
      if (view) setPicked({ nonce, ids: validReadySelection(view, planetIds) });
    },
    [view, nonce],
  );
  const reset = useCallback(() => setPicked({ nonce, ids: [] }), [nonce]);
  return useMemo(
    () => ({ view, selected: view ? validReadySelection(view, current) : [], toggle, set, reset }),
    [view, current, toggle, set, reset],
  );
}

const ReadyPlanetsDraftContext = createContext<ReadyPlanetsDraftApi | null>(null);

export const ReadyPlanetsDraftProvider: React.FC<{
  value: ReadyPlanetsDraftApi;
  children: React.ReactNode;
}> = ({ value, children }) => (
  <ReadyPlanetsDraftContext.Provider value={value}>{children}</ReadyPlanetsDraftContext.Provider>
);

/** The shared ready picks (bar and map), or null outside a provider. */
export const useSharedReadyDraft = (): ReadyPlanetsDraftApi | null =>
  useContext(ReadyPlanetsDraftContext);
