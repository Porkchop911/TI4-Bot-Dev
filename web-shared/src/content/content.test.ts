import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { CATALOG_PATH, CATALOG_QUERY, CatalogError, loadCatalog } from "./catalog.ts";
import {
  BASE_FACTION_IDS,
  FACTION_IDS,
  POK_FACTIONS,
  POK_FACTION_IDS,
  humanizeId,
} from "./names.ts";

const catalog = (content = "digest-a") => ({
  content,
  exploration: { cards: {}, attachments: {} },
  components: {},
  records: { factions: [{ id: "sol", name: "The Federation of Sol" }] },
});

const json = (value: unknown, status = 200) =>
  Promise.resolve({ ok: status < 400, status, json: () => Promise.resolve(value) });

const failure = (promise: Promise<unknown>) =>
  promise.then(
    () => {
      throw new Error("loaded");
    },
    (error: unknown) => error as CatalogError,
  );

describe("loadCatalog", () => {
  it("asks the engine when the page has one, and not the server", async () => {
    const asked: unknown[] = [];
    const loaded = await loadCatalog({
      query: (request) => {
        asked.push(request);
        return Promise.resolve(catalog());
      },
      fetch: () => {
        throw new Error("the server was asked");
      },
    });
    expect(asked).toEqual([CATALOG_QUERY]);
    expect(asked[0]).toEqual({ query: "catalog" });
    expect(loaded.records.factions?.[0].id).toBe("sol");
  });

  it("takes an engine that answers at once", async () => {
    expect((await loadCatalog({ query: () => catalog("x") })).content).toBe("x");
  });

  it("says when the engine refuses", async () => {
    const error = await failure(
      loadCatalog({
        query: () => Promise.reject(new Error("no such query")),
      }),
    );
    expect(error).toBeInstanceOf(CatalogError);
    expect(error.kind).toBe("engine");
  });

  it("gets GET /api/catalog without an engine", async () => {
    const urls: string[] = [];
    const loaded = await loadCatalog({
      fetch: (url) => {
        urls.push(url);
        return json(catalog());
      },
    });
    expect(urls).toEqual([CATALOG_PATH]);
    expect(CATALOG_PATH).toBe("/api/catalog");
    expect(loaded.content).toBe("digest-a");
  });

  it("asks the server at the given URL", async () => {
    const urls: string[] = [];
    await loadCatalog({
      url: "http://localhost:8080/api/catalog",
      fetch: (url) => {
        urls.push(url);
        return json(catalog());
      },
    });
    expect(urls).toEqual(["http://localhost:8080/api/catalog"]);
  });

  it("says the status of a server that answers with an error", async () => {
    const error = await failure(loadCatalog({ fetch: () => json({}, 503) }));
    expect([error.kind, error.detail.status]).toEqual(["http", 503]);
  });

  it("says when the server cannot be reached", async () => {
    const error = await failure(
      loadCatalog({ fetch: () => Promise.reject(new TypeError("Failed to fetch")) }),
    );
    expect([error.kind, error.detail.status]).toEqual(["http", 0]);
  });

  it("refuses what is not a catalog", async () => {
    for (const value of [null, [], { content: "x" }, { ...catalog(), records: undefined }]) {
      const error = await failure(loadCatalog({ query: () => value }));
      expect(error.kind).toBe("invalid");
    }
    const error = await failure(
      loadCatalog({
        fetch: () =>
          Promise.resolve({ ok: true, status: 200, json: () => Promise.reject(new SyntaxError()) }),
      }),
    );
    expect(error.kind).toBe("invalid");
  });

  it("checks the digest of the corpus when one is given", async () => {
    const same = await loadCatalog({ query: () => catalog("abc"), content: "abc" });
    expect(same.content).toBe("abc");
    const error = await failure(loadCatalog({ fetch: () => json(catalog("abc")), content: "def" }));
    expect(error.kind).toBe("digest");
    expect(error.detail).toEqual({ expected: "def", actual: "abc" });
  });
});

describe("humanizeId", () => {
  it.each([
    ["support_for_the_throne", "Support For The Throne"],
    ["jol-nar", "Jol-Nar"],
    ["l1z1x", "L1z1x"],
    ["sol", "Sol"],
    ["pok_trade", "Pok Trade"],
    ["action phase", "Action Phase"],
    ["Already", "Already"],
    ["", ""],
  ])("%s → %s", (id, name) => expect(humanizeId(id)).toBe(name));
});

describe("the faction ids", () => {
  // The engine's list (`ti4_engine::seating`): the base game first, then Prophecy of Kings.
  const seating = readFileSync(
    fileURLToPath(new URL("../../../crates/ti4-engine/src/seating.rs", import.meta.url)),
    "utf8",
  );
  const lobby = /LOBBY_FACTIONS: \[&str; \d+\] = \[([^\]]*)\]/.exec(seating)?.[1] ?? "";
  const engine = [...lobby.matchAll(/"([a-z0-9]+)"/g)].map((match) => match[1]);
  const base = Number(/LOBBY_BASE_FACTIONS: usize = (\d+)/.exec(seating)?.[1]);

  it("are the engine's, in its order", () => {
    expect(engine.length).toBeGreaterThan(0);
    expect(FACTION_IDS).toEqual(engine);
    expect(BASE_FACTION_IDS).toEqual(engine.slice(0, base));
    expect(POK_FACTION_IDS).toEqual(engine.slice(base));
  });

  it("know the Prophecy of Kings ones", () => {
    expect(POK_FACTIONS.has("nomad")).toBe(true);
    expect(POK_FACTIONS.has("sol")).toBe(false);
    expect(POK_FACTIONS.size).toBe(POK_FACTION_IDS.length);
  });
});
