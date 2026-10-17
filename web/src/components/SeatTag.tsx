import React from "react";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";

/**
 * A seat by its name, led by its colour and symbol (the symbol is the second channel for people who
 * cannot tell the colours apart). Presentation only: the id is never shown.
 */
export const SeatTag: React.FC<{ seat: string | null | undefined }> = ({ seat }) => {
  const display = usePlayerIdentity();
  const who = display(seat);
  return (
    <span className="seat-tag" data-testid="seat-tag" data-seat={seat ?? undefined}>
      <span className="seat-tag__mark" style={{ color: who.color }} aria-hidden="true">
        {who.symbol}
      </span>
      {who.label}
    </span>
  );
};
