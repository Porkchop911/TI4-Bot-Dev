/**
 * The engine file, as vite knows it. `web/scripts/build-wasm.mjs` copies the built
 * `ti4_wasm.wasm` into `./engine/`; without it (a build that skipped the wasm) the glob is empty,
 * and the app has no "Play offline". The glob only holds a loader: the file is not fetched, nor
 * is its URL part of the main bundle, until a game is opened.
 */
const loaders = import.meta.glob("./engine/ti4_wasm.wasm", { query: "?url", import: "default" }) as Record<
  string,
  () => Promise<string>
>;

export const hasEngineAsset = (): boolean => Object.keys(loaders).length > 0;

export async function engineUrl(): Promise<string> {
  const load = Object.values(loaders)[0];
  if (!load) throw new Error("This build has no offline engine");
  return load();
}

/** `ti4_wasm-<hash>.wasm` gives `<hash>`; a dev server has no hash. */
export function engineHashOf(url: string): string {
  const match = /ti4_wasm-([A-Za-z0-9_-]+)\.wasm/.exec(url);
  return match?.[1] ?? "dev";
}
