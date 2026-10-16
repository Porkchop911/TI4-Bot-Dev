import type { PaymentView } from "../../model";
import { Card, cx } from "../../ui";

/** The unit of a payment, as a word after the number. */
export const payUnit = (view: PaymentView) => (view.unit === "resource" ? "resources" : view.unit);

/** How a payment stands: short, exact or more than the cost. The words carry it, not only the colour. */
export function payState(view: Pick<PaymentView, "cost" | "paid">) {
  if (view.paid < view.cost) {
    return { tone: "text-red", border: "border-red/60", label: `${view.cost - view.paid} more` };
  }
  if (view.paid > view.cost) {
    return {
      tone: "text-orange",
      border: "border-orange",
      label: `⚠ ${view.paid - view.cost} wasted`,
    };
  }
  return { tone: "text-green", border: "border-green/60", label: "✓ Paid" };
}

/**
 * A payment that is recorded or planned, in one line. An open payment is not here: the board has
 * its controls, and the footer has its line.
 */
export function PaymentList({ view }: { view: PaymentView }) {
  return (
    <Card className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3.5 py-2">
      <h3 className="text-base font-semibold">{view.title}</h3>
      <span className="min-w-0 flex-1 text-sm text-muted">
        {view.summary || "No payment needed"}
      </span>
      <span className="text-xs text-muted tabular-nums">
        Cost {view.cost} {payUnit(view)} · Paying {view.paid}
        {view.paid > view.cost && (
          <span className={cx("font-semibold", payState(view).tone)}>
            {" "}
            · {payState(view).label}
          </span>
        )}
      </span>
    </Card>
  );
}
