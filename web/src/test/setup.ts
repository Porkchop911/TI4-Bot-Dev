import "@testing-library/jest-dom/vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { setCatalog } from "../protocol/catalogStore.ts";

// The printed cards, as the engine gives them (`fixtures/catalog/catalog.json`, written by
// `cargo wire-web`): what `CatalogProvider` loads before the app renders.
setCatalog(JSON.parse(readFileSync(path.resolve(__dirname, "../../../fixtures/catalog/catalog.json"), "utf8")));

function createStorage(): Storage {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value);
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
    key: (index: number) => Object.keys(store)[index] ?? null,
    get length() {
      return Object.keys(store).length;
    },
  };
}

const mockLocalStorage = createStorage();
Object.defineProperty(globalThis, "localStorage", {
  value: mockLocalStorage,
  writable: true,
  configurable: true,
});
if (typeof window !== "undefined") {
  Object.defineProperty(window, "localStorage", {
    value: mockLocalStorage,
    writable: true,
    configurable: true,
  });
}
