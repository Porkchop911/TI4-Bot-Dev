import type { PaymentView } from "../../model";

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
