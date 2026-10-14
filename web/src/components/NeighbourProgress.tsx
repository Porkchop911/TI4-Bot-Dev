import React from "react";
import type { ObjectiveProgressView } from "../protocol/types.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { seatInk } from "../presentation/playerDisplay.ts";
import { neighbourProgressDisplay } from "../presentation/neighbourProgress.ts";
import "./NeighbourProgress.css";

const GLYPH = { ahead: "▲", tied: "=", behind: "▼" } as const;

/**
 * Push Boundaries, per neighbour: "12/10" (our planets / theirs) in blue when ahead, red when
 * tied or behind, with the neighbour's seat symbol and a ▲ / = / ▼ so colour is never the only
 * cue; the summary says how many are beaten and how many the card needs.
 */
export const NeighbourProgress: React.FC<{
  progress: ObjectiveProgressView | undefined;
  /** Test id suffix, e.g. "push_boundaries-seat_a". */
  testId: string;
  className?: string;
  /** A leading label, e.g. "You:" when shown outside the player's own column. */
  heading?: string;
}> = ({ progress, testId, className, heading }) => {
  const display = usePlayerIdentity();
  const model = neighbourProgressDisplay(progress, display);
  if (!model) return null;
  return (
    <div
      className={`neighbour-progress${className ? ` ${className}` : ""}`}
      data-testid={`neighbour-progress-${testId}`}
    >
      {heading && <div className="neighbour-progress__heading">{heading}</div>}
      <ul className="neighbour-progress__rows" aria-label="Planets against each neighbor">
        {model.rows.map((row) => (
          <li
            key={row.player}
            className={`neighbour-progress__row neighbour-progress__row--${row.standing}`}
            data-testid={`neighbour-row-${testId}-${row.player}`}
            data-standing={row.standing}
            title={row.aria}
            aria-label={row.aria}
          >
            <span
              className="neighbour-progress__who"
              style={{ background: row.color, color: seatInk(row.color) }}
              aria-hidden="true"
            >
              {row.position ?? row.symbol}
            </span>
            <span className="neighbour-progress__count" aria-hidden="true">
              {row.text}
            </span>
            <span className="neighbour-progress__glyph" aria-hidden="true">
              {GLYPH[row.standing]}
            </span>
          </li>
        ))}
      </ul>
      <div
        className={`neighbour-progress__summary${
          model.ahead >= model.threshold ? " neighbour-progress__summary--met" : ""
        }`}
      >
        {model.summary}
      </div>
    </div>
  );
};
