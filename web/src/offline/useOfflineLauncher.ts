import { useSyncExternalStore } from "react";
import {
  activeOfflineGame,
  getOfflineLauncher,
  subscribeActiveOfflineGame,
  subscribeLauncher,
} from "./registry.ts";
import type { OfflineLauncher } from "./types.ts";

/** The registered launcher (null when this build has none), live. */
export const useOfflineLauncher = (): OfflineLauncher | null =>
  useSyncExternalStore(subscribeLauncher, getOfflineLauncher);

/** Whether an offline game is on screen (the feedback button then exists even without a server). */
export const useOfflineGameActive = (): boolean =>
  useSyncExternalStore(subscribeActiveOfflineGame, () => activeOfflineGame() !== null);
