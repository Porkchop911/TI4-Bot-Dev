import {
  createContext,
  useContext,
  useMemo,
  useSyncExternalStore,
  type ReactNode,
  type SyntheticEvent,
} from "react";
import type { LinkToken } from "../model";

/** A list row and its system, planet, or route on the board light up together. */
interface LinkStore {
  active: ReadonlySet<LinkToken>;
  set: (tokens: readonly LinkToken[]) => void;
  subscribe: (listener: () => void) => () => void;
}

function createLinkStore(): LinkStore {
  const listeners = new Set<() => void>();
  const store: LinkStore = {
    active: new Set(),
    set(tokens) {
      if (!tokens.length && !store.active.size) {
        return;
      }
      store.active = new Set(tokens);
      for (const listener of listeners) {
        listener();
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return store;
}

const LinkContext = createContext<LinkStore>(createLinkStore());

export function LinkProvider({ children }: { children: ReactNode }) {
  const store = useMemo(createLinkStore, []);
  return <LinkContext.Provider value={store}>{children}</LinkContext.Provider>;
}

/**
 * `linked` is true while the pointer or the focus is on any element that shares a token.
 * Spread `props` on the element. The innermost linked element under the pointer wins.
 */
export function useLink(tokens: readonly LinkToken[] | undefined) {
  const store = useContext(LinkContext);
  const key = tokens?.join(" ") ?? "";
  const linked = useSyncExternalStore(
    store.subscribe,
    () => !!key && key.split(" ").some((token) => store.active.has(token)),
  );
  const props = useMemo(() => {
    if (!key) {
      return {};
    }
    const on = (event: SyntheticEvent) => {
      event.stopPropagation();
      store.set(key.split(" "));
    };
    const off = (event: SyntheticEvent) => {
      event.stopPropagation();
      store.set([]);
    };
    // A tap sends a mouse-over with no mouse-out after it: the light would stay.
    const hover = matchMedia("(hover: hover)").matches ? { onMouseOver: on, onMouseOut: off } : {};
    return { "data-link": key, ...hover, onFocus: on, onBlur: off };
  }, [store, key]);
  return { linked, props };
}
