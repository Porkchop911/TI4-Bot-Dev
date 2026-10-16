import React from "react";
import type { PendingChoiceDto } from "../protocol/types.ts";

interface NotOffered {
  unit: string;
  reason: string;
}

/** The engine's `details.not_offered`: units missing from a single-unit production, with why. */
export function notOffered(choice: Pick<PendingChoiceDto, "details">): NotOffered[] {
  const raw = choice.details?.not_offered;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) =>
    entry && typeof entry.unit === "string" && typeof entry.reason === "string"
      ? [{ unit: entry.unit, reason: entry.reason }]
      : [],
  );
}

const unitName = (id: string) => id.replace(/^[a-z]+_(?=mech|flagship|carrier|infantry)/, "").replace(/_/g, " ");

/** Above the options of a one-unit production: why the other units are not on the list. */
export const NotOfferedNote: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const missing = notOffered(choice);
  if (!missing.length) return null;
  return (
    <div className="not-offered" data-testid="not-offered">
      <div className="not-offered__title">Not available here</div>
      <ul className="not-offered__list">
        {missing.map((entry) => (
          <li key={entry.unit} data-testid={`not-offered-${entry.unit}`}>
            <strong>{unitName(entry.unit)}</strong>: {entry.reason}
          </li>
        ))}
      </ul>
    </div>
  );
};
