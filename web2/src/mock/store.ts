// A tiny store around the dummy's world, so React re-renders after each intent.
import { useMemo, useSyncExternalStore } from "react";
import type { GameSession, Intent } from "../model";
import { examples } from "./data";
import { selectShell } from "./select/shell";
import {
  createWorld,
  currentState,
  loadExample,
  reduce,
  resetExample,
  setViewer,
  type World,
} from "./world";

export interface MockStore {
  world: World;
  version: number;
  subscribe: (listener: () => void) => () => void;
  /** Runs a change on the world, then tells React. */
  update: (change: (world: World) => void) => void;
}

export function createMockStore(start?: { example?: string; viewer?: string }): MockStore {
  const listeners = new Set<() => void>();
  const store: MockStore = {
    world: createWorld(),
    version: 0,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    update(change) {
      change(store.world);
      store.version++;
      for (const listener of listeners) {
        listener();
      }
    },
  };
  if (start?.example && examples[start.example]) {
    loadExample(store.world, start.example);
  }
  if (start?.viewer && start.viewer !== "sol") {
    setViewer(store.world, start.viewer);
  }
  return store;
}

/** The mock implementation of `GameSession`, plus the controls of the demo bar. */
export function useMockSession(store: MockStore) {
  const version = useSyncExternalStore(store.subscribe, () => store.version);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the world changes in place; the version says when
  return useMemo(() => {
    const world = store.world;
    const session: GameSession = {
      view: selectShell(world),
      dispatch: (intent: Intent) => store.update((w) => reduce(w, intent)),
      // The demo has no tool of a local game.
      capabilities: { stepping: false, switchSeat: false, scenarios: false, save: false },
    };
    return {
      session,
      demo: {
        example: currentState(world).example as string,
        viewer: world.viewer,
        tip: (currentState(world).tip as string) || "",
        loadExample: (example: string) => store.update((w) => loadExample(w, example)),
        setViewer: (viewer: string) => store.update((w) => setViewer(w, viewer)),
        reset: () => store.update(resetExample),
      },
    };
  }, [store, version]);
}
