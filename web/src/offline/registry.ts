import type { OfflineFeedbackLocal, OfflineLauncher } from "./types.ts";

/**
 * Where T5 plugs the real launcher in. The shell asks `getOfflineLauncher()`; with nothing
 * registered (a build without the wasm host) the "Play offline" entry is not shown.
 */
let launcher: OfflineLauncher | null = null;
const listeners = new Set<() => void>();

export function registerOfflineLauncher(next: OfflineLauncher | null): void {
  launcher = next;
  listeners.forEach((listener) => listener());
}

export const getOfflineLauncher = (): OfflineLauncher | null => launcher;

export function subscribeLauncher(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * The offline game currently on screen. The game screen sets it (T5) so the feedback sheet can
 * route reports into the queue and attach what replays the game.
 */
export interface ActiveOfflineGame {
  gameId: string;
  /** Read when a report is created: seed, the human's answers so far, engine hash. */
  local(): OfflineFeedbackLocal;
}
let active: ActiveOfflineGame | null = null;
const activeListeners = new Set<() => void>();
export const setActiveOfflineGame = (game: ActiveOfflineGame | null): void => {
  active = game;
  activeListeners.forEach((listener) => listener());
};
export const activeOfflineGame = (): ActiveOfflineGame | null => active;
export const subscribeActiveOfflineGame = (listener: () => void): (() => void) => {
  activeListeners.add(listener);
  return () => activeListeners.delete(listener);
};
