/** Pure rules of the toast swipe: how far a drag must go, and how it ends. Kept apart from the DOM so they can be tested. */

/** A drag shorter than this is a tap (or a click with a shaky hand), not a swipe. */
export const SWIPE_SLOP_PX = 8;
/** A release this far from where the drag started dismisses the toast. */
export const SWIPE_DISMISS_PX = 72;
/** A fast flick dismisses from a shorter distance (px per ms). */
export const SWIPE_FLING_VELOCITY = 0.45;
export const SWIPE_FLING_MIN_PX = 24;

export type SwipeEnd = "dismiss-left" | "dismiss-right" | "snap-back";

export interface SwipeSample {
  /** Horizontal distance from the pointer's start (negative: to the left). */
  dx: number;
  /** Time since the pointer went down, ms. */
  elapsedMs: number;
}

/** Whether a drag of `dx` has become a swipe (and the toast should follow the finger). */
export const isSwipe = (dx: number): boolean => Math.abs(dx) >= SWIPE_SLOP_PX;

/** How a swipe ends when the pointer is released. */
export function swipeEnd({ dx, elapsedMs }: SwipeSample): SwipeEnd {
  const distance = Math.abs(dx);
  const velocity = distance / Math.max(1, elapsedMs);
  const far = distance >= SWIPE_DISMISS_PX;
  const flung = distance >= SWIPE_FLING_MIN_PX && velocity >= SWIPE_FLING_VELOCITY;
  if (!far && !flung) return "snap-back";
  return dx < 0 ? "dismiss-left" : "dismiss-right";
}

/** The toast fades as it is dragged away: 1 at rest, about 0.25 at the dismiss distance. */
export const swipeOpacity = (dx: number): number =>
  Math.max(0.2, 1 - (Math.abs(dx) / (SWIPE_DISMISS_PX * 1.3)) * 0.75);

/** Keys that dismiss a focused toast (Enter and Space already do, as it is a button). */
export const isDismissKey = (key: string): boolean => key === "Escape" || key === "Delete" || key === "Backspace";
