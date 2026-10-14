import { autoResolveText } from "./autoResolveText.ts";
import type { AutoResolveNotification } from "../components/AutoResolveToast.tsx";

/** A toast that was on screen once, kept so it can be read again after it faded. */
export interface ToastHistoryEntry {
  id: string;
  kind: NonNullable<AutoResolveNotification["kind"]>;
  actor?: string;
  /** Small heading ("Invasion", "Agenda", "Turn"), when the toast had one. */
  tag?: string;
  /** The wording as the toast showed it, without the actor's name. */
  text: string;
}

export const MAX_TOAST_HISTORY = 60;

/** The wording a notification shows, whatever its kind. */
export function notificationText(n: AutoResolveNotification): string {
  const kind = n.kind ?? "auto-resolve";
  return kind === "auto-resolve"
    ? autoResolveText(n.decisionType, n.selectedValue, n.reason)
    : (n.text ?? "");
}

const TAGS: Partial<Record<NonNullable<AutoResolveNotification["kind"]>, string>> = {
  recap: "Turn",
};

/**
 * Adds the notifications to the history, oldest first. A toast that grew (merged actions of one
 * player) updates its entry in place instead of repeating; the list keeps the newest
 * {@link MAX_TOAST_HISTORY}.
 */
export function recordToasts(
  history: readonly ToastHistoryEntry[],
  notifications: readonly AutoResolveNotification[],
): ToastHistoryEntry[] {
  let next: ToastHistoryEntry[] | null = null;
  for (const n of notifications) {
    const kind = n.kind ?? "auto-resolve";
    const entry: ToastHistoryEntry = {
      id: n.id,
      kind,
      actor: n.actor,
      tag: n.tag ?? TAGS[kind],
      text: notificationText(n),
    };
    const list: ToastHistoryEntry[] = next ?? [...history];
    const at = list.findIndex((h) => h.id === n.id);
    if (at < 0) list.push(entry);
    else if (list[at].text === entry.text) continue;
    else list[at] = entry;
    next = list;
  }
  return next ? next.slice(-MAX_TOAST_HISTORY) : (history as ToastHistoryEntry[]);
}
