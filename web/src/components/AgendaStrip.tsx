import React, { createContext, useContext, useLayoutEffect, useRef, useState } from "react";
import { agendaStripText, type AgendaStrip as AgendaStripModel } from "../presentation/agendaStrip.ts";
import "./AgendaStrip.css";

/** The minimized decision pill's agenda wording ("Voting: Minister of Peace — For/Against"), when there is one. */
const AgendaPillContext = createContext<string | null>(null);
export const AgendaPillProvider = AgendaPillContext.Provider;
export const useAgendaPill = (): string | null => useContext(AgendaPillContext);

/**
 * One line under the top bar for the whole agenda phase: which agenda, For/Against or Elect, whose
 * vote it is. Tapping it opens the printed text and the tally under the line (the strip grows and the
 * dialogs start below it, so nothing is covered). It publishes its height on the document
 * (`--agenda-strip-bottom`, `data-agenda-strip`) so dialogs, the connection chip and the like keep clear.
 */
export const AgendaStrip: React.FC<{ strip: AgendaStripModel }> = ({ strip }) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const hasMore =
    strip.text.length > 0 || strip.outcomes.length > 0 || Boolean(strip.tally) || Boolean(strip.speaker) || Boolean(strip.type);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const root = document.documentElement;
    const publish = () => {
      const rect = el.getBoundingClientRect();
      root.style.setProperty("--agenda-strip-bottom", `${Math.ceil(rect.bottom)}px`);
      root.style.setProperty("--agenda-strip-height", `${Math.ceil(rect.height)}px`);
    };
    root.dataset.agendaStrip = "on";
    publish();
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(publish) : null;
    observer?.observe(el);
    if (el.parentElement) observer?.observe(el.parentElement);
    window.addEventListener("resize", publish);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", publish);
      delete root.dataset.agendaStrip;
      root.style.removeProperty("--agenda-strip-bottom");
      root.style.removeProperty("--agenda-strip-height");
    };
  }, []);

  const line = agendaStripText(strip);
  return (
    <div ref={ref} className="agenda-strip" data-testid="agenda-strip" data-open={open || undefined}>
      <button
        type="button"
        className="agenda-strip__line"
        data-testid="agenda-strip-toggle"
        aria-expanded={hasMore ? open : undefined}
        aria-controls="agenda-strip-details"
        disabled={!hasMore}
        onClick={() => setOpen((value) => !value)}
        title={line}
      >
        <span className="agenda-strip__icon" aria-hidden="true">
          ⚖
        </span>
        <span className="agenda-strip__text" data-testid="agenda-strip-text">
          {line}
        </span>
        {hasMore && (
          <span className="agenda-strip__chevron" aria-hidden="true">
            {open ? "▴" : "▾"}
          </span>
        )}
      </button>
      {open && hasMore && (
        <div className="agenda-strip__details" id="agenda-strip-details" data-testid="agenda-strip-details">
          {strip.name && (
            <p className="agenda-strip__name">
              <strong>{strip.name}</strong>
              {strip.type ? ` · ${strip.type}` : ""}
              {strip.kindLabel ? ` · ${strip.kindLabel}` : ""}
            </p>
          )}
          {strip.text.map((paragraph) => (
            <p key={paragraph} className="agenda-strip__text-line">
              {paragraph}
            </p>
          ))}
          {strip.outcomes.length > 0 && <p>Outcomes: {strip.outcomes.join(" / ")}</p>}
          {strip.tally && <p data-testid="agenda-strip-tally">Votes so far: {strip.tally}</p>}
          {strip.speaker && <p>Speaker: {strip.speaker} (votes last, breaks ties)</p>}
        </div>
      )}
    </div>
  );
};
