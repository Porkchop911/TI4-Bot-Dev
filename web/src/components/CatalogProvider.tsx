import React, { useEffect, useState } from "react";
import { CatalogError, loadCatalog } from "@ti4/shared/content/catalog.ts";
import type { Catalog } from "../protocol/types.ts";
import { catalogContent, setCatalog } from "../protocol/catalogStore.ts";
import { pageEngine, pageHasEngine } from "../protocol/pageEngine.ts";

/** The catalog from the engine of this page (the instance that also answers the odds). */
async function fromEngine(): Promise<Catalog> {
  if (!pageHasEngine()) {
    throw new CatalogError("engine", "This build or browser has no engine.");
  }
  return loadCatalog({ query: await pageEngine() });
}

/**
 * The printed cards: an online page asks the server (`GET /api/catalog`), an offline page its
 * engine. Each falls back to the other; the first error is the one kept.
 */
export async function loadPageCatalog(offline: boolean): Promise<Catalog> {
  const sources = offline ? [fromEngine, () => loadCatalog()] : [() => loadCatalog(), fromEngine];
  let first: unknown;
  for (const source of sources) {
    try {
      return await source();
    } catch (error) {
      first ??= error;
    }
  }
  throw first;
}

/** The load under way: one for the page, however often the provider mounts. */
let loading: Promise<void> | null = null;

/** What the person is told when no catalog could be loaded. */
function failureText(error: unknown): string {
  if (error instanceof CatalogError && error.kind === "http" && error.detail.status === 0) {
    return "The server could not be reached. Check your connection and try again.";
  }
  return "The card texts could not be loaded. Try again, or reload the page.";
}

/**
 * Loads the printed cards (`catalogStore.ts`) before the app renders: every content lookup of web
 * reads them. Shows a loading line meanwhile, and the problem with a Retry when none came.
 */
export const CatalogProvider: React.FC<{ offline: boolean; children: React.ReactNode }> = ({
  offline,
  children,
}) => {
  const [state, setState] = useState<"ready" | "loading" | { error: unknown }>(() =>
    catalogContent() === null ? "loading" : "ready",
  );
  useEffect(() => {
    if (state !== "loading") return;
    let live = true;
    loading ??= loadPageCatalog(offline)
      .then(setCatalog)
      .finally(() => {
        loading = null;
      });
    loading.then(
      () => live && setState("ready"),
      (error: unknown) => {
        console.error("No catalog of the printed cards:", error);
        if (live) setState({ error });
      },
    );
    return () => {
      live = false;
    };
  }, [state, offline]);

  if (state === "ready") return <>{children}</>;
  if (state === "loading") {
    return (
      <div className="game-loading" role="status" data-testid="catalog-loading">
        Loading the cards…
      </div>
    );
  }
  return (
    <div className="connection-problem" role="alert" data-testid="catalog-failed">
      <div className="connection-problem__card">
        <h2 className="connection-problem__title">The cards could not be loaded</h2>
        <p>{failureText(state.error)}</p>
        <div className="connection-problem__actions">
          <button
            type="button"
            className="button button--primary"
            data-testid="catalog-retry"
            onClick={() => setState("loading")}
          >
            Retry
          </button>
        </div>
      </div>
    </div>
  );
};
