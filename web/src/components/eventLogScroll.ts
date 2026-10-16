/** Scroll-follow decisions for the event log, kept pure because jsdom has no layout. */

/** A reader within this many px of the bottom counts as "at the newest entry". */
export const PIN_THRESHOLD_PX = 8;

export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

export const distanceToBottom = (m: ScrollMetrics): number =>
  m.scrollHeight - m.clientHeight - m.scrollTop;

export const isPinned = (m: ScrollMetrics): boolean => distanceToBottom(m) <= PIN_THRESHOLD_PX;

/**
 * Where the list should be scrolled after its content changed: the bottom while the reader is
 * pinned, otherwise undefined (leave the reader exactly where they are).
 */
export const followTarget = (pinned: boolean, m: ScrollMetrics): number | undefined =>
  pinned ? Math.max(0, m.scrollHeight - m.clientHeight) : undefined;

/** Count of events that arrived below the fold while the reader was scrolled up. */
export const nextUnseen = (
  unseen: number,
  pinned: boolean,
  previousLength: number,
  length: number,
): number => {
  if (pinned) return 0;
  return length > previousLength ? unseen + (length - previousLength) : unseen;
};

/**
 * What resets the reader's manual expansion. History generation is bumped by undo, redo and
 * restore (a different timeline); a reconnect re-sends the same generation with a fresh events
 * array and must keep the reader's place. Servers that send no generation fall back to the array
 * identity (the previous behaviour).
 */
export const logHistoryKey = (generation: number | undefined, events: unknown): unknown =>
  generation === undefined ? events : generation;

/** Where to put the list when it is shown again after being hidden. */
export const restoreTarget = (
  wasPinned: boolean,
  savedScrollTop: number,
  m: ScrollMetrics,
): number => {
  const max = Math.max(0, m.scrollHeight - m.clientHeight);
  return wasPinned ? max : Math.min(savedScrollTop, max);
};
