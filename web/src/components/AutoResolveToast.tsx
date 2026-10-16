import React, { useEffect, useState } from "react";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { autoResolveText } from "../presentation/autoResolveText.ts";
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
 * another player did, with their name and seat colour. Clicking dismisses early.
 */
export const AutoResolveToast: React.FC<AutoResolveToastProps> = ({
  notification,
  onDismiss,
}) => {
  const [isDismissing, setIsDismissing] = useState(false);
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
      title="Click to dismiss"
      className={`auto-resolve-toast auto-resolve-toast--${kind} ${isDismissing ? "dismissing" : ""}`}
      style={actor ? ({ "--toast-accent": actor.color } as React.CSSProperties) : undefined}
      onClick={() => setIsDismissing(true)}
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
  /** Rendered below the toasts (the notification history button). */
  footer?: React.ReactNode;
}

/**
 * Stack of toasts in the bottom-left corner. The container is always rendered, because a polite
 * live region is only announced reliably when it exists before its content does.
 */
export const AutoResolveToastContainer: React.FC<
  AutoResolveToastContainerProps
> = ({ notifications, onDismiss, bottomOffset, footer }) => (
  <div
    className="auto-resolve-toast-container"
    data-testid="corner-toasts"
    role="log"
    aria-live="polite"
    aria-relevant="additions"
    aria-label="Game notifications"
    style={bottomOffset === undefined ? undefined : { bottom: bottomOffset }}
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
