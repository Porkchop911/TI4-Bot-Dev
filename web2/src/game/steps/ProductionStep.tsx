import type { ProductionView } from "../../model";
import { ProductionSummary } from "./production/ProductionSheet";

/** Step 5: the build and its payment are staged on the board. The footer has the payment line. */
export function ProductionStep({ view }: { view: ProductionView }) {
  return <ProductionSummary view={view.sheet} />;
}
