import type { MovementView, OriginView } from "../../../model";
import { plural, unitName } from "../../../model";
import { Card, CardHeading, Gauge, Gauges, Icon, cx } from "../../../ui";
import { SpaceOdds } from "../../combat/SpaceOdds";
import { useLink } from "../../link";

/** The fleet in the active system, before and after the staged movement, with its two limits. */
export function Arrival({ view }: { view: MovementView }) {
  const { linked, props } = useLink([`sys:${view.target.system}`]);
  return (
    <Card className={cx(linked && "bg-cyan/[.06]")} {...props}>
      <CardHeading title={`Arrives in ${view.target.label}`}>
        {view.excessShips > 0 && (
          <span className="text-orange">
            ⚠ {plural(view.excessShips, "ship")} over fleet supply · removed after movement
          </span>
        )}
      </CardHeading>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-center gap-x-5 gap-y-1.5 px-3.5 py-1.5 phone:grid-cols-1 phone:px-2.5">
        <div className="flex flex-wrap gap-x-4 gap-y-0.5 text-sm">
          {view.arrival.length ? (
            view.arrival.map((row) => (
              <span key={row.unit} className="inline-flex items-center gap-1.5 whitespace-nowrap">
                <Icon name={row.unit} className="size-4 text-icon" />
                {unitName(row.unit, row.after)}
                <strong className="font-mid tabular-nums">
                  {row.now === row.after ? row.after : `${row.now} → ${row.after}`}
                </strong>
              </span>
            ))
          ) : (
            <span className="text-muted">
              Nothing moves. You may activate a system without moving.
            </span>
          )}
        </div>
        <Gauges>
          {view.gauges.map((gauge) => (
            <Gauge key={gauge.label} {...gauge} />
          ))}
        </Gauges>
      </div>
      {view.odds && (
        <SpaceOdds view={view.odds} className="border-t border-line px-3.5 py-1.5 phone:px-2.5" />
      )}
    </Card>
  );
}

/** Units that are lost because the ships that carried them leave. The board shows what stays. */
export function Stays({ origin }: { origin: OriginView }) {
  return origin.warning ? (
    <div className="border-t border-line px-3.5 py-1 text-xs text-orange phone:px-2.5">
      ⚠ {origin.warning}
    </div>
  ) : null;
}
