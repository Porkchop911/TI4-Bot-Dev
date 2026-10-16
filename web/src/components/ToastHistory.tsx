import React, { useState } from "react";
import { usePlayerIdentity, useParticipantText } from "../presentation/PlayerIdentity.tsx";
import type { ToastHistoryEntry } from "../presentation/toastHistory.ts";
import "./ToastHistory.css";

interface ToastHistoryProps {
  history: readonly ToastHistoryEntry[];
}

/**
 * A small "Recent notifications" button above the corner toasts that lists every toast of this
 * session again, newest first, so one that faded before it was read can be revisited.
 */
export const ToastHistory: React.FC<ToastHistoryProps> = ({ history }) => {
  const [open, setOpen] = useState(false);
  const display = usePlayerIdentity();
  const present = useParticipantText();
  if (history.length === 0) return null;
  return (
    <div className="toast-history">
      {open && (
        <ol className="toast-history__list" data-testid="toast-history-list" aria-label="Recent notifications">
          {[...history].reverse().map((entry) => {
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
      <button
        type="button"
        className="toast-history__toggle"
        data-testid="toast-history-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {open ? "Hide notifications" : `Recent notifications (${history.length})`}
      </button>
    </div>
  );
};
