import type { ProductionView } from "../../model";
import {
  Card,
  CardHeading,
  CardText,
  Counter,
  Gauge,
  Gauges,
  InlineNote,
  ListRow,
  Quantity,
  Segmented,
} from "../../ui";
import { useDispatch } from "../context";
import { Skipped } from "./BattleSteps";
import { PaymentList } from "./PaymentList";
import { ProductionSummary } from "./production/ProductionSheet";

/** Step 5: build a cart, pay, and place. Limits are gauges, so an over-limit build is visible at once. */
export function ProductionStep({ view }: { view: ProductionView }) {
  const dispatch = useDispatch();
  if (view.skipped) {
    return <Skipped step="Production" reason={view.skipped} />;
  }
  if (view.sheet) {
    // The build and its payment are staged on the board. The footer has the payment line.
    return <ProductionSummary view={view.sheet} />;
  }
  return (
    <>
      <Card>
        <CardHeading title={view.title}>{view.subtitle}</CardHeading>
        {view.rows.length === 0 && <CardText>Nothing produced.</CardText>}
        {view.rows.map((row) => (
          <ListRow
            key={row.unit}
            icon={row.unit}
            title={row.name}
            subtitle={row.subtitle}
            controls={
              row.counter ? (
                <>
                  {row.placement && (
                    <Segmented
                      label={`${row.name} placement`}
                      options={row.placement.options}
                      value={row.placement.selected}
                      onChange={(place) =>
                        dispatch({ type: "setPlacement", unit: row.unit, place })
                      }
                    />
                  )}
                  <Counter
                    {...row.counter}
                    onChange={(value) =>
                      dispatch({ type: "setCount", key: row.counter!.id, value })
                    }
                  />
                </>
              ) : (
                row.quantity !== null && <Quantity count={row.quantity} />
              )
            }
          />
        ))}
      </Card>
      <Gauges>
        {view.gauges.map((gauge) => (
          <Gauge key={gauge.label} {...gauge} />
        ))}
      </Gauges>
      {/* An open payment is on the board and in the footer. */}
      {!view.payment.editable && <PaymentList view={view.payment} />}
      {view.done && (
        <InlineNote tone="success" strong={view.done.strong}>
          {view.done.text}
        </InlineNote>
      )}
    </>
  );
}
