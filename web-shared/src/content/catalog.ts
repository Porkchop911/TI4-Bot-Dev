// The printed cards (`Catalog`), from the engine of the page or from the server: the same JSON.
import type { Catalog } from "../wire/index.ts";

/** The question of the engine for the catalog (`host_api::query`). */
export const CATALOG_QUERY = { query: "catalog" } as const;

/** Where the server serves the catalog. */
export const CATALOG_PATH = "/api/catalog";

export interface CatalogSource {
  /**
   * A query of the engine of the page (`EngineLink.query`, or `Host.query` on the main thread).
   * When given, the catalog comes from the engine and the server is not asked.
   */
  query?: (request: typeof CATALOG_QUERY) => unknown;
  /** The fetch of the page (default: the global one). Used only without `query`. */
  fetch?: (url: string) => Promise<Pick<Response, "ok" | "status" | "json">>;
  /** The URL of the catalog of the server (default `CATALOG_PATH`, on this origin). */
  url?: string;
  /**
   * The digest of the corpus that the catalog must be of: `content` of a snapshot of the server,
   * or of the save of a game. A catalog of another one is refused (`CatalogError` "digest").
   */
  content?: string;
}

/**
 * Why no catalog was loaded. `kind` is for the app, which shows its own text; `message` is for
 * the developer.
 * - `engine`: the engine refused the query;
 * - `http`: the server answered with an error (`status`), or could not be reached (`status` 0);
 * - `invalid`: what came is not a catalog;
 * - `digest`: the catalog is of another corpus than `content` (`expected`, `actual`).
 */
export class CatalogError extends Error {
  constructor(
    readonly kind: "engine" | "http" | "invalid" | "digest",
    message: string,
    readonly detail: { status?: number; expected?: string; actual?: string } = {},
  ) {
    super(message);
    this.name = "CatalogError";
  }
}

/**
 * The catalog: from the engine when the page has one loaded (`source.query`), else from the
 * server (`GET /api/catalog`).
 */
export async function loadCatalog(source: CatalogSource = {}): Promise<Catalog> {
  const value = source.query ? await fromEngine(source.query) : await fromServer(source);
  const catalog = checked(value);
  if (source.content !== undefined && catalog.content !== source.content) {
    throw new CatalogError(
      "digest",
      `The catalog is of the corpus ${catalog.content}, not of ${source.content}.`,
      { expected: source.content, actual: catalog.content },
    );
  }
  return catalog;
}

async function fromEngine(query: NonNullable<CatalogSource["query"]>): Promise<unknown> {
  try {
    return await query(CATALOG_QUERY);
  } catch (error) {
    throw new CatalogError("engine", `The engine gave no catalog: ${String(error)}`);
  }
}

async function fromServer(source: CatalogSource): Promise<unknown> {
  const url = source.url ?? CATALOG_PATH;
  const get = source.fetch ?? ((at: string) => fetch(at));
  let response: Awaited<ReturnType<typeof get>>;
  try {
    response = await get(url);
  } catch (error) {
    throw new CatalogError("http", `${url} could not be reached: ${String(error)}`, { status: 0 });
  }
  if (!response.ok) {
    throw new CatalogError("http", `${url} answered ${response.status}.`, {
      status: response.status,
    });
  }
  try {
    return await response.json();
  } catch (error) {
    throw new CatalogError("invalid", `${url} sent no JSON: ${String(error)}`);
  }
}

/** What an app relies on: the digest and the three parts. */
function checked(value: unknown): Catalog {
  const catalog = value as Partial<Catalog> | null;
  const isObject = (part: unknown) => typeof part === "object" && part !== null;
  if (
    !isObject(catalog) ||
    typeof catalog?.content !== "string" ||
    !isObject(catalog.exploration) ||
    !isObject(catalog.components) ||
    !isObject(catalog.records)
  ) {
    throw new CatalogError(
      "invalid",
      "This is not a catalog (content, exploration, components, records).",
    );
  }
  return catalog as Catalog;
}
