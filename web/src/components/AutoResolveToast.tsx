import React, { useEffect, useState } from "react";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { autoResolveText } from "../presentation/autoResolveText.ts";
import { isDismissKey, swipeOpacity } from "../presentation/swipeDismiss.ts";
import { useSwipeDismiss } from "../hooks/useSwipeDismiss.ts";
import "./AutoResolveToast.css";

export interface AutoResolveNotification {
  id: string;
  /** "auto-resolve" (default): the game took the only legal option. "action": another player acted. "recap": a summary of another player's finished turn. "agenda": the result of an agenda vote, for every seat. "invasion": the result of a bombardment, Space Cannon Defense or a whole invasion. "notice": something the viewer's own play did not do and why (Imperial scored nothing). */
  kind?: "auto-resolve" | "action" | "recap" | "invasion" | "agenda" | "notice";
  decisionType: string;
  selectedValue: string;
  /** Auto-resolve toasts: why there was only one choice (default "the only legal option"). */
  reason?: string;
  /** Action toasts: the acting seat and what they did, without their name. */
  actor?: string;
  text?: string;
  /** Invasion toasts: the small heading ("Bombardment", "Invasion"). */
  tag?: string;
  /** Overrides how long the toast stays (ms). */
  durationMs?: number;
  /** Bumped when a toast absorbs another action, so its timer starts over. */
  revision?: number;
}

/** How long a toast stays, in ms. */
export const TOAST_DURATION_MS = { "auto-resolve": 3500, action: 5000, recap: 8000, invasion: 10000, agenda: 10000, notice: 12000 } as const;

interface AutoResolveToastProps {
  notification: AutoResolveNotification;
  onDismiss: (id: string) => void;
}

/**
 * A corner toast. Auto-resolved decisions say what was taken and why; action toasts say what
 * another player did, with their name and seat colour. Clicking, swiping it sideways (touch or
 * mouse drag) or pressing Escape / Delete while it has focus dismisses it early. A dismissed toast
 * stays in the notification history.
 */
export const AutoResolveToast: React.FC<AutoResolveToastProps> = ({
  notification,
  onDismiss,
}) => {
  const [isDismissing, setIsDismissing] = useState(false);
  const [swipedOut, setSwipedOut] = useState<"left" | "right" | null>(null);
  const swipe = useSwipeDismiss((end) => {
    if (end === "snap-back") return;
    setSwipedOut(end === "dismiss-left" ? "left" : "right");
    setIsDismissing(true);
  }, isDismissing);
  // Once dragged, the toast's own entry animation must not hold the transform any more.
  const [touched, setTouched] = useState(false);
  useEffect(() => {
    if (swipe.dragging) setTouched(true);
  }, [swipe.dragging]);
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const kind = notification.kind ?? "auto-resolve";

  useEffect(() => {
    if (isDismissing) {
      // Give the animation time to finish before removing from DOM
      const animationTimer = setTimeout(() => onDismiss(notification.id), 300);
      return () => clearTimeout(animationTimer);
    }
    const timer = setTimeout(() => setIsDismissing(true), notification.durationMs ?? TOAST_DURATION_MS[kind]);
    return () => clearTimeout(timer);
  }, [notification.id, notification.revision, notification.durationMs, isDismissing, kind, onDismiss]);

  const actor = notification.actor ? display(notification.actor) : null;

  return (
    <button
      type="button"
      data-testid="corner-toast"
      data-toast-kind={kind}
      title="Click or swipe to dismiss"
      className={`auto-resolve-toast auto-resolve-toast--${kind} ${isDismissing ? "dismissing" : ""}${
        swipedOut ? ` dismissing--${swipedOut}` : ""
      }${swipe.dragging ? " auto-resolve-toast--dragging" : ""}${touched ? " auto-resolve-toast--touched" : ""}`}
      style={
        {
          ...(actor ? { "--toast-accent": actor.color } : {}),
          ...(swipe.offset !== 0
            ? { "--swipe-x": `${swipe.offset}px`, transform: `translateX(${swipe.offset}px)`, opacity: swipeOpacity(swipe.offset) }
            : {}),
        } as React.CSSProperties
      }
      onClick={() => setIsDismissing(true)}
      onKeyDown={(event) => {
        if (isDismissKey(event.key)) {
          event.preventDefault();
          setIsDismissing(true);
        }
      }}
      {...swipe.handlers}
    >
      {kind === "invasion" || kind === "agenda" || kind === "notice" ? (
        <>
          <span className="toast-recap-tag" data-testid={`toast-${kind}-tag`}>
            {notification.tag ?? (kind === "agenda" ? "Agenda" : kind === "notice" ? "Note" : "Invasion")}
          </span>
          <span className="toast-value toast-value--lines">{present(notification.text ?? "")}</span>
        </>
      ) : (kind === "action" || kind === "recap") && actor ? (
        <>
          {kind === "recap" && <span className="toast-recap-tag">Turn</span>}
          <span className="toast-actor" data-testid="toast-actor">
            <span aria-hidden="true" className="toast-actor__symbol">
              {actor.symbol}
            </span>
            {actor.label}
          </span>{" "}
          <span className="toast-value">{present(notification.text ?? "")}</span>
        </>
      ) : (
        <>
          <span className="toast-value">
            {autoResolveText(notification.decisionType, notification.selectedValue, notification.reason)}
          </span>
        </>
      )}
    </button>
  );
};

interface AutoResolveToastContainerProps {
  notifications: AutoResolveNotification[];
  onDismiss: (id: string) => void;
  /** Extra space above the bottom edge, so toasts clear bars and overlays down there. */
  bottomOffset?: number;
  /** The column stops this far up (a control near the top); the oldest toasts are cut off. */
  maxHeight?: number;
  /** Rendered below the toasts (the notification history button). */
  footer?: React.ReactNode;
  /**
   * Single-player games: the column sits below the turn bar, its menus and the dialogs (they
   * paint over a toast, never the other way round), so no control is ever covered by a toast.
   */
  underControls?: boolean;
}

/**
 * Stack of toasts in the bottom-left corner. The container is always rendered, because a polite
 * live region is only announced reliably when it exists before its content does.
 */
export const AutoResolveToastContainer: React.FC<
  AutoResolveToastContainerProps
> = ({ notifications, onDismiss, bottomOffset, maxHeight, footer, underControls }) => (
  <div
    className={`auto-resolve-toast-container${underControls ? " auto-resolve-toast-container--under" : ""}`}
    data-testid="corner-toasts"
    role="log"
    aria-live="polite"
    aria-relevant="additions"
    aria-label="Game notifications"
    style={
      bottomOffset === undefined && maxHeight === undefined
        ? undefined
        : {
            ...(bottomOffset === undefined ? {} : { bottom: bottomOffset }),
            // Clipped from the top: the newest toasts and the history pill stay at the bottom.
            ...(maxHeight === undefined ? {} : { maxHeight, overflow: "hidden", justifyContent: "flex-end" }),
          }
    }
  >
    {notifications.map((notification) => (
      <AutoResolveToast
        key={notification.id}
        notification={notification}
        onDismiss={onDismiss}
      />
    ))}
    {footer}
  </div>
);
