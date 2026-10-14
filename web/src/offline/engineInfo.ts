import { useSyncExternalStore } from "react";

/**
 * The engine file this page has loaded for offline play (null before any game was opened or
 * resumed). The banner above an offline game and the build footer show `shortEngine(...)`, so a
 * feedback screenshot names the engine as well as the page build.
 */
let loaded: string | null = null;
const listeners = new Set<() => void>();

export function setLoadedEngine(wasmSha256: string | null): void {
  if (loaded === wasmSha256) return;
  loaded = wasmSha256;
  listeners.forEach((listener) => listener());
}

export const getLoadedEngine = (): string | null => loaded;

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => void listeners.delete(listener);
};

/** The sha-256 of the loaded engine file, live. */
export const useLoadedEngine = (): string | null => useSyncExternalStore(subscribe, getLoadedEngine);

/** The first 8 hex digits, as shown to people. */
export const shortEngine = (wasmSha256: string): string => wasmSha256.slice(0, 8);

/** "engine ab12cd34", or null when no engine is loaded. */
export const engineLabel = (wasmSha256: string | null): string | null =>
  wasmSha256 ? `engine ${shortEngine(wasmSha256)}` : null;
