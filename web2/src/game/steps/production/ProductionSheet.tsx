import type { BuildCountView, BuildGroupView, BuildSheetView, BuildUnitView } from "../../../model";
import { plural } from "../../../model";
import { Button, Gauge, Icon, Peek, cx } from "../../../ui";
import { SystemDetails } from "../../board/SystemDetails";
import { useDispatch } from "../../context";

// The columns of a unit row: name, cost, what is left in supply, then what is staged. Every row
// is its own grid, so the widths are fixed and the numbers line up over the rows.
const COLUMNS =
  "grid grid-cols-[7.5rem_2.75rem_2.25rem_minmax(0,1fr)] items-center gap-x-1.5 px-3 phone:grid-cols-[7rem_2.5rem_2rem_minmax(0,1fr)] phone:px-2.5";

/** What one purchase makes: one unit, or two for fighters and infantry ("1 for 2"). */
function perPurchase(unit: BuildUnitView) {
  const match = / for (\d+)$/.exec(unit.price);
  return match ? Number(match[1]) : 1;
}

/** The units staged of one kind, in a fixed place: zero is dimmed, so a glance finds the others. */
function Staged({ units }: { units: number }) {
  return (
    <span
      className={cx(
        "w-6 flex-none text-right text-sm tabular-nums",
        units > 0 ? "font-mid text-accent" : "text-muted opacity-50",
      )}
    >
      {units}
    </span>
  );
}

/**
 * What is staged of one unit in one place: the units, then a token for each purchase with the
 * icon of every unit it makes. A tap on a token takes one purchase back. The dashed slot adds
 * one and shows the units that it makes. It is not shown when nothing more can be added.
 */
function BuildTokens({ unit, count }: { unit: BuildUnitView; count: BuildCountView }) {
  const dispatch = useDispatch();
  const per = perPurchase(unit);
  const { id, value, max, label } = count.counter;
  const set = (next: number) => dispatch({ type: "setCount", key: id, value: next });
  const icons = (n: number, className?: string) =>
    Array.from({ length: n }, (_, index) => (
      <Icon key={index} name={unit.unit} className={cx("size-4 flex-none", className)} />
    ));
  return (
    <span className="flex items-center gap-1.5 py-0.5">
      <Staged units={count.made} />
      <span className="flex min-w-0 flex-wrap items-center gap-1">
        {Array.from({ length: value }, (_, step) => (
          <button
            key={step}
            type="button"
            aria-label={`Remove ${label}`}
            className="inline-flex h-7 items-center gap-0.5 rounded-md border border-line-strong bg-raised px-1.5 text-icon hover:border-text touch:h-9"
            onClick={() => set(value - 1)}
          >
            {icons(Math.max(1, Math.min(per, count.made - step * per)))}
          </button>
        ))}
        {value < max && (
          <button
            type="button"
            aria-label={`Add ${label}`}
            className="inline-flex h-7 items-center gap-0.5 rounded-md border border-dashed border-line px-1.5 text-icon hover:bg-raised touch:h-9"
            onClick={() => set(value + 1)}
          >
            <span className="text-sm leading-none text-muted">+</span>
            {icons(per, "opacity-50")}
          </button>
        )}
      </span>
    </span>
  );
}

/**
 * A unit as its card prints it: the name, the four values, the keywords and the printed special
 * rule. It is behind the name of the row. A section with nothing is not shown.
 */
function UnitCard({ unit }: { unit: BuildUnitView }) {
  const values = [
    ["Cost", unit.price],
    ["Combat", unit.combat],
    ["Move", unit.move],
    ["Capacity", unit.capacity],
  ];
  const section = "border-t border-line/50 px-3 py-2";
  return (
    <>
      <div className="flex items-center gap-1.5 px-3 py-2">
        <Icon name={unit.unit} className="size-4 flex-none text-icon" />
        <strong className="text-sm font-mid">{unit.name}</strong>
      </div>
      <dl className={cx(section, "grid grid-cols-4 gap-x-1 text-center")}>
        {values.map(([word, value]) => (
          <div key={word} className="flex flex-col-reverse">
            <dt className="text-2xs text-muted">{word}</dt>
            <dd className="text-sm font-mid whitespace-nowrap tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      {unit.abilities.length > 0 && (
        <ul className={cx(section, "text-2xs font-mid tracking-wide")}>
          {unit.abilities.map((ability) => (
            <li key={ability}>{ability}</li>
          ))}
        </ul>
      )}
      {unit.rules && <p className={cx(section, "whitespace-pre-line")}>{unit.rules}</p>}
    </>
  );
}

/**
 * One kind of unit as printed, with what is staged. A unit that is not produced here is dimmed.
 * `inline`: its tokens are in its own row. Else it has a line for each place.
 */
function UnitRow({ unit, inline }: { unit: BuildUnitView; inline: boolean }) {
  const off = !!unit.unavailable;
  const dim = off && "opacity-45";
  return (
    <>
      <div className={cx(COLUMNS, "min-h-8 border-t border-line/50 touch:min-h-10")}>
        <Peek
          label={`${unit.name}, printed values`}
          trigger={
            <>
              <Icon name={unit.unit} className="size-4 flex-none text-icon" />
              <strong
                className="min-w-0 truncate font-mid underline decoration-dotted decoration-muted/60 underline-offset-4 phone:whitespace-normal"
                title={unit.name}
              >
                {unit.name}
              </strong>
            </>
          }
          buttonClassName={() => cx("flex min-w-0 items-center gap-1.5 text-left", dim)}
          width={264}
          flush
        >
          <UnitCard unit={unit} />
        </Peek>
        <span className={cx("justify-self-end text-xs tabular-nums", dim)}>{unit.price}</span>
        <span className={cx("justify-self-end text-xs tabular-nums", dim)}>{unit.left ?? "–"}</span>
        {/* A word, not only the dimmed row: why the unit is not produced here. */}
        {unit.unavailable ? (
          <span
            className="truncate text-xs whitespace-nowrap text-muted"
            title={unit.unavailable.reason}
          >
            {unit.unavailable.word}
          </span>
        ) : inline ? (
          <span className="flex flex-wrap gap-1">
            {unit.counts.map((count) => (
              <BuildTokens key={count.counter.id} unit={unit} count={count} />
            ))}
          </span>
        ) : (
          // A unit with a line for each place: the total here, the places below.
          <Staged units={unit.counts.reduce((sum, count) => sum + count.made, 0)} />
        )}
      </div>
      {!inline &&
        unit.counts.map((count) => (
          <div key={count.counter.id} className={cx(COLUMNS, "min-h-8 touch:min-h-10")}>
            <span className="col-span-3 truncate pl-[22px] text-xs text-muted">{count.place}</span>
            <BuildTokens unit={unit} count={count} />
          </div>
        ))}
    </>
  );
}

/** The ships, or the ground forces: a heading and one row for each kind of unit. */
function Group({ group, first }: { group: BuildGroupView; first: boolean }) {
  // Units that all go to one place say it once, in the heading, as the ships do.
  const places = new Set(group.units.flatMap((unit) => unit.counts.map((count) => count.place)));
  const inline = !group.byPlace || places.size <= 1;
  const place = group.byPlace && places.size === 1 ? ([...places][0] ?? null) : null;
  return (
    <div role="group" aria-label={group.title}>
      <div
        className={cx(
          COLUMNS,
          "min-h-6 bg-raised/50 text-xs text-muted",
          !first && "border-t border-line/50",
        )}
      >
        <span className="truncate">{group.title}</span>
        <span className="justify-self-end">Cost</span>
        <span className="justify-self-end">Left</span>
        <span className="truncate">{place}</span>
      </div>
      {group.units.map((unit) => (
        <UnitRow key={unit.id} unit={unit} inline={inline} />
      ))}
    </div>
  );
}

/** "3 units · 5 resources", with what the discount took off. */
function Bill({ view }: { view: BuildSheetView }) {
  if (view.made === 0) {
    return <>Nothing is produced</>;
  }
  return (
    <>
      <strong className="font-mid text-text">
        {plural(view.made, "unit")} · {plural(view.cost, "resource")}
      </strong>
      {view.discount > 0 && ` · discount ${view.discount}`}
    </>
  );
}

/**
 * The build of a production with its controls, on the board, at the system that produces. On a
 * phone it is a sheet over the lower part of the map. The payment follows, on the map.
 */
export function ProductionSheet({ view }: { view: BuildSheetView }) {
  const dispatch = useDispatch();
  return (
    <div
      role="group"
      aria-label={`Produce in ${view.label}`}
      className="absolute bottom-2.5 left-2.5 z-2 max-h-[80%] w-[540px] overflow-auto rounded-lg border border-line bg-surface/97 text-sm phone:inset-x-0 phone:bottom-0 phone:z-3 phone:max-h-[70%] phone:w-auto phone:rounded-none phone:border-x-0 phone:border-b-0 phone:bg-surface"
    >
      <div className="sticky top-0 z-1 bg-surface">
        <div className="flex items-center gap-2 border-b border-line py-0.5 pr-1 pl-3 phone:pl-2.5">
          <h3 className="truncate text-base font-semibold phone:text-sm" title={view.label}>
            {view.label}
          </h3>
          <span role="status" className="min-w-0 flex-1 truncate text-xs text-muted">
            <Bill view={view} />
          </span>
          <Button
            size="sm"
            tone="quiet"
            disabled={view.made === 0}
            onClick={() => dispatch({ type: "resetOrigin", system: view.system })}
          >
            Reset
          </Button>
          {/* Closes the sheet. With something to pay, the map is then the payment. */}
          <button
            type="button"
            aria-label="Done with the build"
            title={
              view.made > 0 && view.cost > 0
                ? "Done with the build · the payment is next"
                : "Done with the build · closes the sheet"
            }
            className="grid size-7 flex-none place-items-center rounded-md text-icon hover:bg-raised touch:size-9"
            onClick={() => dispatch({ type: "markOrigin", system: view.system })}
          >
            <Icon name="check" />
          </button>
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 border-b border-line px-3 py-1.5 phone:px-2.5">
          {view.gauges.map((gauge) => (
            <Gauge key={gauge.label} {...gauge} />
          ))}
        </div>
      </div>
      {view.groups.map((group, index) => (
        <Group key={group.title} group={group} first={index === 0} />
      ))}
      {/* What is in the system, as the inspector has it. It scrolls with the units. */}
      {view.details && (
        <SystemDetails
          view={view.details}
          className="border-t border-line px-3 pb-2.5 pt-1.5 text-xs phone:px-2.5"
        />
      )}
    </div>
  );
}

/** In the panel: what is produced in each place, and the limits. The controls are on the board. */
export function ProductionSummary({ view }: { view: BuildSheetView }) {
  const dispatch = useDispatch();
  const row =
    "row flex min-h-8 w-full flex-wrap items-baseline gap-x-2.5 px-3.5 py-1 text-left hover:bg-raised [.row+&]:border-t [.row+&]:border-line/50 phone:px-2.5";
  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-lg border border-line">
        <div className="border-b border-line px-3.5 py-1.5 text-xs text-muted phone:px-2.5">
          {view.stage === "build" ? (
            "Choose the units on the map, at the system that produces."
          ) : (
            <>
              Choose on the map what pays. <Bill view={view} />
            </>
          )}
        </div>
        {view.places.map((place) => (
          <button
            key={place.id}
            type="button"
            className={row}
            onClick={() => dispatch({ type: "inspectSystem", system: view.system })}
          >
            <strong className="font-mid">{place.name}</strong>
            <span className="text-xs text-muted">
              {place.builds ? (
                <strong className="font-mid text-text">{place.builds}</strong>
              ) : (
                "Nothing is produced"
              )}
            </span>
          </button>
        ))}
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-x-4 gap-y-3">
        {view.gauges.map((gauge) => (
          <Gauge key={gauge.label} {...gauge} />
        ))}
      </div>
    </div>
  );
}
