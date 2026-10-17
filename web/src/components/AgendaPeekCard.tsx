import React, { useEffect, useState } from "react";
import type { AgendaPeek } from "../presentation/agendaPeek.ts";
import "./ActionCardPickers.css";

const BAR_SELECTOR = '[data-testid="turn-action-bar"]';

/**
 * Space between the bottom of the screen and the top of the turn bar (the bar's height plus what
 * sits below it), so the preview stays above the bar and never covers its buttons. Without a bar:
 * `fallback`. The bar's size changes with its content, so this re-reads it a few times a second.
 */
function useClearAboveTurnBar(fallback: number, active: boolean): number {
  const [clear, setClear] = useState(fallback);
  useEffect(() => {
    if (!active) return;
    const read = () => {
      const bar = document.querySelector<HTMLElement>(BAR_SELECTOR);
      const rect = bar?.getBoundingClientRect();
      const next = rect && rect.height > 0 ? Math.max(fallback, Math.round(window.innerHeight - rect.top + 8)) : fallback;
      setClear((previous) => (previous === next ? previous : next));
    };
    read();
    const timer = window.setInterval(read, 400);
    window.addEventListener("resize", read);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("resize", read);
    };
  }, [fallback, active]);
  return clear;
}

/**
 * Insider Information: the top agendas of the deck, for the one player who looked. It starts as a
 * small chip above the turn bar (next agenda and how many follow); tapping it opens the cards, at
 * most half the screen high. Escape or the close button dismisses it; it comes back for a new peek
 * and goes away when the top of the deck moves.
 */
export const AgendaPeekCard: React.FC<{ peek: AgendaPeek | null }> = ({ peek }) => {
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const visible = peek !== null && dismissed !== peek.key;
  const clear = useClearAboveTurnBar(16, visible);
  useEffect(() => {
    if (!peek) setDismissed(null);
  }, [peek]);
  // A new peek starts collapsed again.
  useEffect(() => {
    setOpen(false);
  }, [peek?.key]);
  // Open, Escape closes the card (the chip only reacts while it has focus, so it never eats the
  // Escape of the turn menu or a dialog).
  useEffect(() => {
    if (!visible || !open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) setDismissed(peek?.key ?? null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, open, peek?.key]);
  if (!peek || !visible) return null;
  const first = peek.entries[0];
  const more = peek.entries.length - 1;
  const style = { "--peek-bottom": `${clear}px` } as React.CSSProperties;
  if (!open) {
    return (
      <aside
        className="peek peek--chip"
        style={style}
        onKeyDown={(event) => {
          if (event.key === "Escape") setDismissed(peek.key);
        }}
        data-testid="agenda-peek"
        role="region"
        aria-label="Top of the agenda deck, only you can see this"
      >
        <button
          type="button"
          className="peek__chip-button"
          data-testid="agenda-peek-toggle"
          aria-expanded={false}
          aria-label={`Show the top of the agenda deck (only you can see this): next is ${first?.name ?? "unknown"}`}
          onClick={() => setOpen(true)}
        >
          <span className="peek__place">1</span>
          <span className="peek__chip-text">
            Next agenda: <strong>{first?.name}</strong>
            {more > 0 ? ` +${more}` : ""}
          </span>
        </button>
        <button
          type="button"
          className="button button--secondary button--icon peek__chip-close"
          data-testid="agenda-peek-close"
          aria-label="Close the agenda preview"
          onClick={() => setDismissed(peek.key)}
        >
          ×
        </button>
      </aside>
    );
  }
  return (
    <aside className="peek" style={style} data-testid="agenda-peek" role="region" aria-label="Top of the agenda deck, only you can see this">
      <header className="peek__header">
        <div>
          <strong>Top of the agenda deck</strong>
          <div className="text-muted peek__private">Insider Information: only you can see this</div>
        </div>
        <div className="peek__header-actions">
          <button
            type="button"
            className="button button--secondary button--icon"
            data-testid="agenda-peek-toggle"
            aria-expanded
            aria-label="Collapse the agenda preview"
            onClick={() => setOpen(false)}
          >
            −
          </button>
          <button
            type="button"
            className="button button--secondary button--icon"
            data-testid="agenda-peek-close"
            aria-label="Close the agenda preview"
            onClick={() => setDismissed(peek.key)}
          >
            ×
          </button>
        </div>
      </header>
      <ol className="peek__list">
        {peek.entries.map((entry) => (
          <li key={entry.place} className="peek__entry" data-testid="agenda-peek-entry">
            <div className="peek__name">
              <span className="peek__place" aria-label={entry.place === 1 ? "Next to be revealed" : `Number ${entry.place} in the deck`}>
                {entry.place}
              </span>
              <strong>{entry.name}</strong>
              {entry.tags.map((tag) => (
                <span key={tag} className="politics-panel__tag">
                  {tag}
                </span>
              ))}
            </div>
            {entry.text.map((line) => (
              <p key={line} className="politics-panel__text">
                {line}
              </p>
            ))}
          </li>
        ))}
      </ol>
    </aside>
  );
};
