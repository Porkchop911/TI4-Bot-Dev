import { hasJspi } from "@ti4/shared/engine/jspi.ts";
import { pageQuery, type EngineQuery } from "@ti4/shared/engine/odds.ts";
import { hasEngineAsset } from "../offline/local/engineAsset.ts";

/** This page can run its engine: the build has the file, and the browser has JSPI. */
export const pageHasEngine = (): boolean => hasEngineAsset() && hasJspi();

let loading: Promise<EngineQuery> | null = null;

/**
 * The engine of this page on the main thread (`pageQuery`), for the questions that need no game:
 * the catalog and the odds of a combat. The file is fetched and compiled once per page (the
 * games on this device use the same module), and one instance answers every question. Nothing of
 * it is fetched before the first call.
 */
export function pageEngine(): Promise<EngineQuery> {
  if (!pageHasEngine()) return Promise.reject(new Error("This build or browser has no engine."));
  loading ??= import("../offline/local/session.ts").then(async ({ loadEngine }) =>
    pageQuery((await loadEngine()).module),
  );
  loading.catch(() => {
    loading = null;
  });
  return loading;
}
