import type { PaymentView } from "../../model";
import { Button, Counter, cx } from "../../ui";
import { ActionButton, useDispatch } from "../context";
import { payState, payUnit } from "../steps/PaymentList";

/**
 * The open payment, on the board where the player chooses the planets: how it stands, and every
 * control of it. Trade goods are not on the board, so they have a counter here, and one button
 * that pays what the planets leave.
 */
export function PaymentBar({ view }: { view: PaymentView }) {
  const dispatch = useDispatch();
  const state = payState(view);
  const goods = view.goods;
  return (
    <div
      role="group"
      aria-label="Payment"
      className={cx(
        "pointer-events-auto absolute top-11 left-1/2 z-2 flex -translate-x-1/2 items-center gap-2.5 rounded-md border bg-surface px-3 py-1.5 whitespace-nowrap shadow-lg phone:inset-x-2.5 phone:top-14 phone:translate-x-0 phone:flex-wrap phone:gap-y-1.5 phone:px-2.5",
        state.border,
      )}
    >
      <span role="status" className="flex items-baseline gap-2">
        <span className="text-xs text-muted">Paying</span>
        <strong className="text-lg leading-none font-semibold tabular-nums">
          {view.paid} / {view.cost}
        </strong>
        <span className="text-xs text-muted">{payUnit(view)}</span>
        <span className={cx("min-w-[68px] text-sm font-semibold", state.tone)}>{state.label}</span>
      </span>
      {goods && (
        <span className="flex items-center gap-2 border-l border-line pl-2.5 text-xs text-muted phone:order-last phone:w-full phone:border-l-0 phone:pl-0">
          Trade goods
          <Counter
            value={goods.value}
            max={goods.max}
            label="trade good"
            onChange={(value) => dispatch({ type: "setPayment", source: goods.id, value })}
          />
          <Button
            size="sm"
            tone="quiet"
            disabled={goods.rest === null}
            onClick={() =>
              goods.rest !== null &&
              dispatch({ type: "setPayment", source: goods.id, value: goods.rest })
            }
          >
            Pay rest
          </Button>
        </span>
      )}
      <span className="flex items-center gap-1.5 border-l border-line pl-2.5 phone:ml-auto phone:border-l-0 phone:pl-0">
        {view.actions.map((action) => (
          <ActionButton key={action.label} view={action} size="sm" />
        ))}
      </span>
    </div>
  );
}
