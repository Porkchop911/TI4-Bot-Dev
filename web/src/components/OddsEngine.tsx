import React, { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { EngineQuery } from "@ti4/shared/engine/odds.ts";
import { pageEngine, pageHasEngine } from "../protocol/pageEngine.ts";

/**
 * The engine that computes the odds of a combat (`spaceOdds`, `groundOdds` of
 * `@ti4/shared/engine/odds.ts`): `ready` with its query, `loading` while the page fetches it,
 * `failed` when it could not be loaded, `none` when this build or browser has none.
 */
export type OddsEngine =
  | { state: "ready"; query: EngineQuery }
  | { state: "loading" }
  | { state: "failed" }
  | { state: "none" };

const OddsEngineContext = createContext<OddsEngine>({ state: "none" });

export const useOddsEngine = (): OddsEngine => useContext(OddsEngineContext);

/** Gives the odds a fixed engine (tests). */
export const OddsEngineContextProvider = OddsEngineContext.Provider;

/**
 * The engine of the odds for a game screen. A game on this device passes `local`, the engine
 * that plays it (`EngineLink.query`, in its Worker). Otherwise the page loads its own engine
 * after the first render (`pageEngine`); the odds show once it is there.
 */
export const OddsEngineProvider: React.FC<{ local?: EngineQuery; children: React.ReactNode }> = ({
  local,
  children,
}) => {
  const [page, setPage] = useState<OddsEngine>(() =>
    pageHasEngine() ? { state: "loading" } : { state: "none" },
  );
  useEffect(() => {
    if (local || !pageHasEngine()) return;
    let live = true;
    pageEngine().then(
      (query) => live && setPage({ state: "ready", query }),
      (error: unknown) => {
        console.warn("No engine for the odds of a combat:", error);
        if (live) setPage({ state: "failed" });
      },
    );
    return () => {
      live = false;
    };
  }, [local]);
  const value = useMemo<OddsEngine>(
    () => (local ? { state: "ready", query: local } : page),
    [local, page],
  );
  return <OddsEngineContext.Provider value={value}>{children}</OddsEngineContext.Provider>;
};
