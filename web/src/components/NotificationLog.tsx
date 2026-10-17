import React, { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { usePlayerIdentity, useParticipantText } from "../presentation/PlayerIdentity.tsx";
import type { ToastHistoryEntry } from "../presentation/toastHistory.ts";
import "./NotificationLog.css";

/**
 * The notification log of a game (offline or online, seat or spectator): an icon button in the row of the
 * Technologies / Objectives buttons that opens the list of every toast of the session, newest
 * first. The corner toasts feed it; the button and the list can sit anywhere in the page, so the
 * state lives in a context. Without the provider (isolated tests, embeds) nothing here renders and the
 * "Recent notifications" pill under the corner toasts is the fallback.
 */

interface Anchor {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

interface NotificationLogState {
  /** True inside the provider of a game page. */
  inline: boolean;
  entries: readonly ToastHistoryEntry[];
  setEntries: (entries: readonly ToastHistoryEntry[]) => void;
  open: boolean;
  /** Entries that arrived since the log was last open. */
  unread: number;
  toggle: (trigger: HTMLElement) => void;
  close: () => void;
}

const OFF: NotificationLogState = {
  inline: false,
  entries: [],
  setEntries: () => {},
  open: false,
  unread: 0,
  toggle: () => {},
  close: () => {},
};

const NotificationLogContext = createContext<NotificationLogState>(OFF);

export const useNotificationLog = (): NotificationLogState => useContext(NotificationLogContext);

const anchorOf = (el: HTMLElement): Anchor => {
  const r = el.getBoundingClientRect();
  return { top: r.top, bottom: r.bottom, left: r.left, right: r.right };
};

export const NotificationLogProvider: React.FC<{ inline: boolean; children: React.ReactNode }> = ({ inline, children }) => {
  const [entries, setEntries] = useState<readonly ToastHistoryEntry[]>([]);
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [seen, setSeen] = useState<ReadonlySet<string>>(() => new Set());
  const trigger = useRef<HTMLElement | null>(null);

  // While the list is open everything in it counts as read.
  useEffect(() => {
    if (!open) return;
    setSeen((old) => (entries.every((e) => old.has(e.id)) ? old : new Set([...old, ...entries.map((e) => e.id)])));
  }, [open, entries]);

  const toggle = useCallback((el: HTMLElement) => {
    trigger.current = el;
    setAnchor(anchorOf(el));
    setOpen((v) => !v);
  }, []);
  const close = useCallback(() => {
    setOpen(false);
    trigger.current?.focus?.();
  }, []);

  // A resize or rotation moves the button the list hangs from.
  useEffect(() => {
    if (!open) return;
    const reanchor = () => trigger.current && setAnchor(anchorOf(trigger.current));
    window.addEventListener("resize", reanchor);
    return () => window.removeEventListener("resize", reanchor);
  }, [open]);

  const unread = useMemo(() => entries.filter((e) => !seen.has(e.id)).length, [entries, seen]);
  const value = useMemo<NotificationLogState>(
    () => ({ inline, entries, setEntries, open, unread, toggle, close }),
    [inline, entries, open, unread, toggle, close],
  );
  return (
    <NotificationLogContext.Provider value={value}>
      {children}
      {inline && open && anchor && <NotificationLogPanel entries={entries} anchor={anchor} onClose={close} />}
    </NotificationLogContext.Provider>
  );
};

/** The icon-only toggle. Renders nothing outside the provider. */
export const NotificationLogButton: React.FC<{ testId?: string }> = ({ testId = "toast-history-toggle" }) => {
  const log = useNotificationLog();
  if (!log.inline) return null;
  const name = log.unread > 0 ? `Notifications, ${log.unread} new` : "Notifications";
  return (
    <button
      type="button"
      className="button button--secondary notification-log-button"
      data-testid={testId}
      aria-label={name}
      title={name}
      aria-expanded={log.open}
      aria-controls="notification-log-panel"
      aria-haspopup="true"
      data-unread={log.unread > 0 ? log.unread : undefined}
      onClick={(event) => log.toggle(event.currentTarget)}
    >
      <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path
          d="M6 17V11a6 6 0 1 1 12 0v6l1.6 2H4.4L6 17zM10 21h4"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      {log.unread > 0 && (
        <span className="notification-log-button__badge" data-testid="toast-history-unread" aria-hidden="true">
          {log.unread > 9 ? "9+" : log.unread}
        </span>
      )}
    </button>
  );
};

const MARGIN = 8;
const PANEL_WIDTH = 360;

const NotificationLogPanel: React.FC<{ entries: readonly ToastHistoryEntry[]; anchor: Anchor; onClose: () => void }> = ({
  entries,
  anchor,
  onClose,
}) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const ref = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLOListElement | null>(null);
  const lastCount = useRef(entries.length);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onDown = (event: PointerEvent) => {
      const target = event.target as Element | null;
      if (!target) return;
      // The button that opened the list toggles it itself.
      if (ref.current?.contains(target) || target.closest('[aria-controls="notification-log-panel"]')) return;
      onClose();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [onClose]);

  // Newest first: a new entry arrives at the top, and a reader at the top stays there.
  useLayoutEffect(() => {
    if (entries.length !== lastCount.current && listRef.current && listRef.current.scrollTop < 40) listRef.current.scrollTop = 0;
    lastCount.current = entries.length;
  }, [entries.length]);

  const below = anchor.top < window.innerHeight / 2;
  // Right-aligned with the button, kept inside the window.
  const width = Math.min(PANEL_WIDTH, window.innerWidth - 2 * MARGIN);
  const style: React.CSSProperties = {
    width,
    left: Math.min(Math.max(MARGIN, anchor.right - width), window.innerWidth - width - MARGIN),
    ...(below ? { top: anchor.bottom + MARGIN } : { bottom: window.innerHeight - anchor.top + MARGIN }),
  };
  return (
    <div
      id="notification-log-panel"
      ref={ref}
      className="notification-log-panel"
      data-testid="toast-history-panel"
      role="region"
      aria-label="Recent notifications"
      style={style}
    >
      {entries.length === 0 ? (
        <p className="notification-log-panel__empty" data-testid="toast-history-empty">
          No notifications yet.
        </p>
      ) : (
        <ol ref={listRef} className="notification-log-panel__list" data-testid="toast-history-list" aria-label="Recent notifications, newest first">
          {[...entries].reverse().map((entry) => {
            const actor = entry.actor ? display(entry.actor) : null;
            return (
              <li
                key={entry.id}
                className="toast-history__item"
                data-testid="toast-history-item"
                style={actor ? ({ "--toast-accent": actor.color } as React.CSSProperties) : undefined}
              >
                {entry.tag && <span className="toast-recap-tag">{entry.tag}</span>}
                {actor && <strong className="toast-history__actor">{actor.label} </strong>}
                <span>{present(entry.text)}</span>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
};
