import {
  createContext,
  createElement,
  useContext,
  useSyncExternalStore,
  type ReactNode,
} from "react";

/** The same width as the `phone:` variant in `styles/theme.css`. */
const QUERY = "(max-width: 720px)";

function subscribe(listener: () => void) {
  const media = matchMedia(QUERY);
  media.addEventListener("change", listener);
  return () => media.removeEventListener("change", listener);
}

const Scope = createContext(false);

/**
 * Content in the phone layout on any screen: the step sheet over the board on desktop. Its wrapper
 * has `data-phone`, which the `phone:` variant matches as it matches the width of a phone, and
 * `usePhone()` is true inside it. It is the same layout, not another breakpoint.
 */
export function PhoneScope({ className, children }: { className?: string; children: ReactNode }) {
  return createElement(
    Scope.Provider,
    { value: true },
    createElement("div", { "data-phone": "", className }, children),
  );
}

/** True on a portrait phone: the shell shows one pane at a time. */
export function usePhoneScreen(): boolean {
  return useSyncExternalStore(subscribe, () => matchMedia(QUERY).matches);
}

/**
 * True when content is in the phone layout: on a portrait phone, and inside a `PhoneScope`. The
 * shell itself (the panes, the toolbar) asks `usePhoneScreen()`.
 */
export function usePhone(): boolean {
  const scoped = useContext(Scope);
  return usePhoneScreen() || scoped;
}
