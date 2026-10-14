import { useSyncExternalStore } from "react";

/** The same width as the `phone:` variant in `styles/theme.css`. */
const QUERY = "(max-width: 720px)";

function subscribe(listener: () => void) {
  const media = matchMedia(QUERY);
  media.addEventListener("change", listener);
  return () => media.removeEventListener("change", listener);
}

/** True on a portrait phone: the shell shows one pane at a time. */
export function usePhone(): boolean {
  return useSyncExternalStore(subscribe, () => matchMedia(QUERY).matches);
}
