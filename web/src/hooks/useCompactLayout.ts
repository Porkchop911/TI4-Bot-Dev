import { useSyncExternalStore } from "react";

/** A phone in portrait (narrow) or landscape (short): the layouts where floating panels must stay small. */
export const COMPACT_LAYOUT_QUERY = "(max-width: 720px), (max-height: 500px)";

/** Wider than a portrait phone: the board column sits beside the side panels (a phone in landscape). */
export const WIDE_LAYOUT_QUERY = "(min-width: 721px)";

/** A touch screen without a mouse: finger as the only pointer, nothing to hover with. */
export const TOUCH_ONLY_QUERY = "(pointer: coarse) and (hover: none)";

/**
 * The shortest side of the physical screen (CSS px) up to which a touch-only device is a phone.
 * Phones are 320-430 px (foldables unfolded up to about 600); tablets (iPad mini 744 and up) stay on
 * the desktop layout.
 */
export const PHONE_SCREEN_MAX = 600;

export type LayoutOverride = "compact" | "desktop" | null;
export const LAYOUT_STORAGE_KEY = "ti4.layout.v1";

/** Everything the layout decision looks at, so it can be tested without a browser. */
export interface LayoutEnv {
  /** `(max-width: 720px), (max-height: 500px)` matches the layout viewport. */
  narrowViewport: boolean;
  /** `(pointer: coarse) and (hover: none)`. */
  touchOnly: boolean;
  /** `screen.width`/`screen.height` in CSS px (null when unknown). */
  screenWidth: number | null;
  screenHeight: number | null;
  override: LayoutOverride;
}

/**
 * The one phone layout decision. Compact when the layout viewport is phone sized (portrait width or
 * landscape height), OR the device is a touch-only phone-sized screen even though the browser
 * reports a wide layout viewport (Chrome/Safari "Request desktop site" lays the page out at about
 * 980 px). A manual override (`?layout=compact|desktop`, stored) beats both. Mouse/hover desktops
 * and tablets (shortest screen side above {@link PHONE_SCREEN_MAX}) are never compact by device.
 */
export function isCompactLayout(env: LayoutEnv): boolean {
  if (env.override === "compact") return true;
  if (env.override === "desktop") return false;
  if (env.narrowViewport) return true;
  if (!env.touchOnly) return false;
  const sides = [env.screenWidth, env.screenHeight].filter((n): n is number => typeof n === "number" && n > 0);
  return sides.length > 0 && Math.min(...sides) <= PHONE_SCREEN_MAX;
}

const mq = (query: string): boolean =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(query).matches
    : false;

/** Whether the media query matches; follows rotation and resizing. False where matchMedia is missing. */
export function useMatchMedia(query: string): boolean {
  return useSyncExternalStore(
    (notify) => {
      if (typeof window.matchMedia !== "function") return () => undefined;
      const list = window.matchMedia(query);
      list.addEventListener("change", notify);
      return () => list.removeEventListener("change", notify);
    },
    () => mq(query),
    () => false,
  );
}

export function readLayoutOverride(): LayoutOverride {
  try {
    const stored = window.localStorage.getItem(LAYOUT_STORAGE_KEY);
    return stored === "compact" || stored === "desktop" ? stored : null;
  } catch {
    return null;
  }
}

const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

/** Stores (or with null clears) the manual layout choice and re-evaluates everything. */
export function setLayoutOverride(value: LayoutOverride): void {
  try {
    if (value) window.localStorage.setItem(LAYOUT_STORAGE_KEY, value);
    else window.localStorage.removeItem(LAYOUT_STORAGE_KEY);
  } catch {
    // No storage (private mode): the choice lasts until reload through the URL parameter only.
  }
  applyLayoutAttribute();
  emit();
}

/** `?layout=compact|desktop` stores the override, `?layout=auto` clears it; the parameter is then dropped. */
function consumeLayoutParam(): void {
  try {
    const url = new URL(window.location.href);
    const param = url.searchParams.get("layout");
    if (param === null) return;
    url.searchParams.delete("layout");
    window.history.replaceState({}, "", url.pathname + url.search + url.hash);
    if (param === "compact" || param === "desktop") window.localStorage.setItem(LAYOUT_STORAGE_KEY, param);
    else if (param === "auto") window.localStorage.removeItem(LAYOUT_STORAGE_KEY);
  } catch {
    // Ignore malformed URLs / blocked storage.
  }
}

/**
 * Touch is the only way to point: coarse pointer and no hover, or (for browsers that report a mouse-ish
 * primary pointer in desktop-site mode) touch points present and no fine pointer or hover anywhere.
 */
function touchOnly(): boolean {
  if (mq(TOUCH_ONLY_QUERY)) return true;
  const points = typeof navigator !== "undefined" ? navigator.maxTouchPoints : 0;
  return points > 0 && !mq("(any-pointer: fine)") && !mq("(any-hover: hover)");
}

export function readLayoutEnv(): LayoutEnv {
  const hasScreen = typeof window !== "undefined" && typeof window.screen !== "undefined";
  return {
    narrowViewport: mq(COMPACT_LAYOUT_QUERY),
    touchOnly: touchOnly(),
    screenWidth: hasScreen ? window.screen.width : null,
    screenHeight: hasScreen ? window.screen.height : null,
    override: typeof window === "undefined" ? null : readLayoutOverride(),
  };
}

export const currentCompactLayout = (): boolean => isCompactLayout(readLayoutEnv());

const VIEWPORT_DEVICE = "width=device-width, initial-scale=1.0";

/**
 * Mirrors the decision onto `<html data-layout="compact|desktop">` (the stylesheet keys off it) and,
 * in desktop-site mode, asks the browser for a device-width viewport again (honoured by some
 * browsers; harmless where it is ignored).
 */
export function applyLayoutAttribute(): void {
  if (typeof document === "undefined") return;
  const compact = currentCompactLayout();
  const root = document.documentElement;
  const value = compact ? "compact" : "desktop";
  if (root.getAttribute("data-layout") !== value) root.setAttribute("data-layout", value);
  // Compact although the viewport is not phone sized (desktop-site mode, manual override): the
  // stylesheet's repeated phone rules key off this one; real phone viewports use the media queries alone.
  const forced = compact && !mq(COMPACT_LAYOUT_QUERY);
  if (forced !== root.hasAttribute("data-layout-forced")) {
    if (forced) root.setAttribute("data-layout-forced", "");
    else root.removeAttribute("data-layout-forced");
  }
  const meta = document.querySelector('meta[name="viewport"]');
  if (meta && compact && window.innerWidth > 720 && meta.getAttribute("content") !== VIEWPORT_DEVICE) {
    meta.setAttribute("content", VIEWPORT_DEVICE);
  }
}

let installed = false;
/** Called once before the first render: reads `?layout=`, sets the attribute and keeps it current. */
export function installLayoutAttribute(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;
  consumeLayoutParam();
  applyLayoutAttribute();
  const update = () => {
    applyLayoutAttribute();
    emit();
  };
  window.addEventListener("resize", update);
  window.addEventListener("orientationchange", update);
  window.addEventListener("storage", update);
  if (typeof window.matchMedia === "function") {
    for (const query of [COMPACT_LAYOUT_QUERY, TOUCH_ONLY_QUERY]) {
      window.matchMedia(query).addEventListener("change", update);
    }
  }
}

function subscribe(notify: () => void): () => void {
  listeners.add(notify);
  const list = typeof window.matchMedia === "function" ? window.matchMedia(COMPACT_LAYOUT_QUERY) : null;
  list?.addEventListener("change", notify);
  return () => {
    listeners.delete(notify);
    list?.removeEventListener("change", notify);
  };
}

/** True on the phone layout: a phone-sized viewport, a touch-only phone in desktop-site mode, or the manual override. */
export const useCompactLayout = (): boolean => useSyncExternalStore(subscribe, currentCompactLayout, () => false);

/** The board column sits beside the side panels: wider than a portrait phone and not the phone layout. */
export function useWideLayout(): boolean {
  const compact = useCompactLayout();
  const narrow = useMatchMedia(COMPACT_LAYOUT_QUERY);
  const wide = useMatchMedia(WIDE_LAYOUT_QUERY);
  // A phone in landscape is compact and wide; a phone in desktop-site mode is compact by device only.
  return wide && !(compact && !narrow);
}

/** The manual layout choice, for the menu sheet. */
export function useLayoutOverride(): LayoutOverride {
  return useSyncExternalStore(subscribe, readLayoutOverride, () => null);
}
