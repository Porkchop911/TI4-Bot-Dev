import React from "react";
import type { ProductionTotal as Total } from "../presentation/productionDraft.ts";
import { plural, productionTotalText } from "../presentation/productionTotalText.ts";

const titleCase = (unit: string) => unit.charAt(0).toUpperCase() + unit.slice(1);

export interface ProductionTotalProps {
  total: Total;
  /** Resources the player can spend (null when the question does not say). */
  available: number | null;
  /** One payment for all builds, or asked build by build. */
  singlePayment: boolean;
  /** Earlier builds of this production were already sent: the total covers only the ones staged now. */
  partial?: boolean;
}

/**
 * What everything staged in the builder costs: the total, the per-build costs, the cost reduction,
 * and how it compares with what the player can spend. Display only; the sum is `draftResourceCost`.
 */
export const ProductionTotal: React.FC<ProductionTotalProps> = ({ total, available, singlePayment, partial }) => {
  if (!total.builds) return null;
  const sum = total.total;
  const payLine =
    total.builds < 2
      ? null
      : singlePayment
        ? "You pay once for all builds."
        : "Each build asks for its own payment.";
  return (
    <div className="production-total" data-testid="production-total" role="status">
      <strong className="production-total__sum" data-testid="production-total-sum">
        {sum === null ? `${plural(total.builds, "build")} — cost unavailable` : productionTotalText(sum, total.builds, partial)}
      </strong>
      {sum !== null && (
        <ul className="production-total__lines" data-testid="production-total-lines">
          {total.lines.map((line) => (
            <li key={line.optionId}>
              {line.batches > 1 ? `${line.batches}× ` : ""}
              {titleCase(line.unit)}
              {line.exchange ? " (exchange)" : ""}
              <span className="production-total__cost">{line.printed}</span>
            </li>
          ))}
          {total.discount > 0 && (
            <li className="production-total__discount" data-testid="production-total-discount">
              Cost reduction<span className="production-total__cost">−{total.discount}</span>
            </li>
          )}
        </ul>
      )}
      {sum !== null && available !== null && (
        <span className="production-total__avail" data-testid="production-total-available">
          You have {available} available{available - sum >= 0 ? ` · ${available - sum} left` : ""}
        </span>
      )}
      {payLine && <span className="production-total__note text-muted">{payLine}</span>}
    </div>
  );
};
