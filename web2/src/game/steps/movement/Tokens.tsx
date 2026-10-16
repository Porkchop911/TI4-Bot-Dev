import type { CargoSourceView, MoveShipView, OriginView, ShipUnitView } from "../../../model";
import { plural } from "../../../model";
import { Button, Icon, Pill, cx } from "../../../ui";
import { RichText, useDispatch } from "../../context";
import { useLink } from "../../link";

/** The kind of unit that a tap on a slot loads: the chosen one, or the first that is left. */
export const loadSource = (origin: OriginView, chosen: string | null) => {
  const can = (source: CargoSourceView) => !source.reason && source.left > 0;
  return (
    origin.cargo.find((source) => source.id === chosen && can(source)) ??
    origin.cargo.find(can) ??
    null
  );
};

/** One kind of unit in one place, with the count that is left. The chosen one has "▸". */
export function SourceChip({
  source,
  pressed,
  onSelect,
  verb = "Load",
}: {
  source: CargoSourceView;
  pressed: boolean;
  onSelect: () => void;
  /** What a tap does with the kind: the ships load it, or it lands. */
  verb?: "Load" | "Land";
}) {
  // A pickup lights its system on the map while the pointer or the focus is on it.
  const { props } = useLink(source.site ? [`sys:${source.site.system}`] : undefined);
  const where = source.site
    ? verb === "Load"
      ? `picked up on the way at ${source.site.label}`
      : `${source.place}, ${source.site.label}`
    : source.place;
  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={!!source.reason || source.left === 0}
      aria-label={`${verb} ${source.name}, ${where}, ${source.left} left`}
      title={`${source.name} · ${where}${source.reason ? ` · ${source.reason}` : ""}`}
      className={cx(
        "inline-flex min-h-[26px] flex-none items-center gap-1 rounded-md border px-1.5 text-xs whitespace-nowrap disabled:opacity-45 touch:min-h-9 touch:px-2",
        pressed ? "border-accent bg-accent/15 text-text" : "border-line text-muted",
      )}
      onClick={onSelect}
      {...props}
    >
      {/* A sign, not only the colour: this kind is the one that a slot loads. */}
      {pressed && <span aria-hidden="true">▸</span>}
      {source.site && <Icon name="pickup" className="size-3.5 text-cyan" />}
      {/* The icon names the unit; the name is the tooltip and the accessible name. */}
      <Icon name={source.unit} className="size-4" />
      <strong className="font-mid text-text tabular-nums">{source.left}</strong>
    </button>
  );
}

/**
 * What the ships of the system can load, one chip for each kind of unit. One chip is chosen: a
 * tap on a slot of any ship loads that kind, and "Fill" loads it on every ship that has room.
 * The row is one line, so it can stay in view.
 */
export function CargoRow({
  origin,
  source,
  onSelect,
}: {
  origin: OriginView;
  source: CargoSourceView | null;
  onSelect: (id: string) => void;
}) {
  const dispatch = useDispatch();
  // The place is said once, before the chips of that place.
  const placeOf = (item: CargoSourceView) => `${item.site?.system ?? ""}:${item.place}`;
  const opens = (index: number) =>
    index === 0 || placeOf(origin.cargo[index - 1]) !== placeOf(origin.cargo[index]);
  const canFill =
    !!source &&
    origin.ships.some((ship) => ship.units.some((unit) => unit.hold?.accepts[source.id]));
  return (
    <div
      role="group"
      aria-label="Load"
      className="flex items-center gap-1.5 border-b border-line py-1 pr-1.5 pl-3 phone:pl-2.5"
    >
      <span className="text-xs text-muted phone:hidden">Load</span>
      <span className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none]">
        {origin.cargo.length === 0 && <span className="text-xs text-faint">Nothing to load</span>}
        {origin.cargo.map((item, index) => (
          <span key={item.id} className="contents">
            {opens(index) && (
              <span
                className={cx(
                  "flex-none text-xs whitespace-nowrap text-muted",
                  index > 0 && "ml-1 border-l border-line pl-2",
                )}
              >
                {item.site && `#${item.site.system} `}
                {item.place}
              </span>
            )}
            <SourceChip
              source={item}
              pressed={item.id === source?.id}
              onSelect={() => onSelect(item.id)}
            />
          </span>
        ))}
      </span>
      <Button
        size="sm"
        tone="quiet"
        disabled={!canFill}
        aria-label={source ? `Fill with ${source.name}` : "Fill"}
        title={
          source
            ? `Load ${source.name} on every ship that leaves, until the holds are full`
            : undefined
        }
        onClick={() =>
          dispatch({ type: "fillHold", key: `origin:${origin.system}`, source: source?.id })
        }
      >
        Fill
      </Button>
    </div>
  );
}

/** One ship. A tap moves it or takes it back. */
function Token({
  ship,
  unit,
  editing,
}: {
  ship: MoveShipView;
  unit: ShipUnitView;
  editing: boolean;
}) {
  const dispatch = useDispatch();
  const can = unit.moves || unit.canMove;
  return (
    <button
      type="button"
      disabled={!editing || !can}
      aria-pressed={unit.moves}
      aria-label={unit.label}
      title={[
        unit.move,
        unit.moves ? "Moves · tap to keep it here" : can ? "Stays · tap to move it" : unit.reason,
      ]
        .filter(Boolean)
        .join(" · ")}
      className={cx(
        "relative grid size-8 flex-none place-items-center rounded-md border touch:size-10",
        unit.moves
          ? "border-accent bg-accent/15 text-text"
          : can
            ? "border-line-strong text-icon hover:bg-raised"
            : "border-dashed border-line text-faint",
      )}
      onClick={() => dispatch({ type: "setCount", key: unit.key, value: unit.moves ? 0 : 1 })}
    >
      <Icon name={ship.unit} className="size-5" />
      {/* A sign, not only the colour: the arrow says that this ship moves. */}
      {unit.moves && (
        <span className="absolute -top-1.5 -right-1.5 grid size-3.5 place-items-center rounded-full bg-accent text-[#171d27]">
          <Icon name="arrow" className="size-2.5" />
        </span>
      )}
    </button>
  );
}

/** The hold of one ship: a slot for each unit that fits. A tap loads the chosen kind or unloads. */
function Slots({
  unit,
  source,
  editing,
}: {
  unit: ShipUnitView;
  source: CargoSourceView | null;
  editing: boolean;
}) {
  const dispatch = useDispatch();
  const hold = unit.hold!;
  const accept = source ? hold.accepts[source.id] : undefined;
  const why = !source
    ? "Nothing left to load"
    : source.site
      ? `This ship does not pass ${source.site.label}`
      : `No ${source.name} left`;
  return (
    <span className="inline-flex items-center gap-[3px]">
      {Array.from({ length: hold.capacity }, (_, index) => {
        const slot = hold.slots[index];
        const box = cx(
          "relative grid size-[22px] place-items-center rounded-[3px] border touch:size-8",
          slot ? "border-accent/60 bg-accent/10 text-text" : "border-dashed border-line-strong",
        );
        const inside = slot && (
          <>
            <Icon name={slot.unit} className="size-3.5" />
            {/* The same sign as on the chip: this unit is picked up on the way. */}
            {slot.site && (
              <Icon name="pickup" className="absolute -top-1 -right-1 size-3 text-cyan" />
            )}
          </>
        );
        const name =
          slot && `${slot.name}, ${slot.place}${slot.site ? `, picked up at ${slot.site}` : ""}`;
        if (!editing) {
          return (
            <span key={index} className={box} title={name || "Empty slot"}>
              {inside}
            </span>
          );
        }
        return (
          <button
            key={index}
            type="button"
            className={cx(box, "disabled:opacity-45")}
            disabled={!slot && !accept}
            aria-label={
              slot
                ? `Unload ${name} from ${unit.label}`
                : `Load ${source?.name ?? "a unit"} on ${unit.label}`
            }
            title={slot ? `${name} · tap to unload` : accept ? `Load ${source!.name}` : why}
            onClick={() =>
              dispatch(
                slot
                  ? { type: "setCount", key: slot.key, value: slot.value }
                  : { type: "setCount", ...accept! },
              )
            }
          >
            {inside}
          </button>
        );
      })}
    </span>
  );
}

/** Gravity Drive on one ship: +1 move for one ship of the action. The path follows. */
function Boost({ unit, editing }: { unit: ShipUnitView; editing: boolean }) {
  const dispatch = useDispatch();
  const boost = unit.boost!;
  if (!editing) {
    return boost.on ? <Pill tone="staged">+1 Gravity Drive</Pill> : null;
  }
  return (
    <button
      type="button"
      aria-pressed={boost.on}
      disabled={boost.locked || (!boost.on && !!boost.reason)}
      aria-label={`Gravity Drive on ${unit.label}`}
      title={
        boost.locked
          ? "This ship cannot move without Gravity Drive"
          : (boost.reason ??
            "+1 move for one ship of this action. The ship takes the path with no rift roll.")
      }
      className={cx(
        "inline-flex min-h-[22px] items-center gap-1 rounded-[3px] border px-1.5 text-xs whitespace-nowrap disabled:opacity-60 touch:min-h-8",
        boost.on ? "border-gold bg-gold/15 text-text" : "border-line text-muted",
      )}
      onClick={() => dispatch({ type: "setBoost", ship: boost.on ? null : unit.key })}
    >
      {/* A sign, not only the colour: the check says that this ship has it. */}
      {boost.on && <Icon name="check" className="size-3" />}
      +1 Gravity Drive
    </button>
  );
}

/** One ship that moves: its token, its own hold, and what its path means for it. */
function UnitRow({
  ship,
  unit,
  source,
  editing,
  route,
  places,
}: {
  ship: MoveShipView;
  unit: ShipUnitView;
  source: CargoSourceView | null;
  editing: boolean;
  /** False when the heading of the kind names the path: every ship that moves takes it. */
  route: boolean;
  /** The ships of the system can load at more than one place: the row says where its cargo is from. */
  places: boolean;
}) {
  const from = new Map<string, number>();
  for (const slot of (places && unit.hold?.slots) || []) {
    const place = slot.site ? `${slot.site}, ${slot.place}` : slot.place;
    from.set(place, (from.get(place) ?? 0) + 1);
  }
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
      <Token ship={ship} unit={unit} editing={editing} />
      {unit.hold && <Slots unit={unit} source={source} editing={editing} />}
      {from.size > 0 && (
        <span className="text-xs text-muted">
          {[...from].map(([place, count]) => `${place} ${count}`).join(" · ")}
        </span>
      )}
      {unit.move && <span className="text-xs text-muted">{unit.move}</span>}
      {unit.boost && <Boost unit={unit} editing={editing} />}
      {unit.riftRoll && (
        <Pill
          tone="loss"
          title="One die for each rift the ship leaves. The ship and what it carries are destroyed on 1–3."
        >
          ⚄ Rift roll
        </Pill>
      )}
      <span className={cx("text-xs", unit.invalid ? "text-red" : "text-faint")}>
        {unit.invalid ? unit.reason : route && unit.route}
      </span>
    </div>
  );
}

/** One kind of ship: what the ships have in common, a row for each that moves, then those that stay. */
function Kind({
  ship,
  source,
  editing,
  places,
}: {
  ship: MoveShipView;
  source: CargoSourceView | null;
  editing: boolean;
  places: boolean;
}) {
  const { linked, props } = useLink(ship.link);
  const stays = ship.units.filter((unit) => !unit.moves);
  const moving = ship.units.filter((unit) => unit.moves);
  // One path for every ship that moves is said once, in the heading. Gravity Drive gives a ship
  // a path of its own: then each row names its path.
  const shared = moving.every((unit) => unit.route === moving[0].route) ? moving[0]?.route : "";
  // The ships of a kind that stay have the same reason, if they have one.
  const reason = ship.reason ?? stays.find((unit) => unit.reason)?.reason;
  return (
    <div
      className={cx(
        "flex flex-col gap-1 border-line/50 px-3 py-1 not-first:border-t phone:px-2.5",
        linked && "bg-cyan/[.06]",
      )}
      {...props}
    >
      <div className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted">
        <strong className="text-sm font-mid text-text">{ship.name}</strong>
        <RichText value={ship.facts} />
        {ship.total > 1 && (
          <span>
            {ship.count} of {plural(ship.total, "ship")}
          </span>
        )}
        {shared && <span>{shared}</span>}
      </div>
      {moving.map((unit) => (
        <UnitRow
          key={unit.key}
          ship={ship}
          unit={unit}
          source={source}
          editing={editing}
          route={!shared}
          places={places}
        />
      ))}
      {stays.length > 0 && (
        <div className="flex flex-wrap items-center gap-x-[3px] gap-y-1">
          {stays.map((unit) => (
            <Token key={unit.key} ship={ship} unit={unit} editing={editing} />
          ))}
          {reason && <span className="ml-1.5 text-xs text-faint">{reason}</span>}
        </div>
      )}
    </div>
  );
}

/** The fleet of one system: for each kind of ship, a row for each ship with its own hold. */
export function OriginTokens({
  origin,
  source = null,
  editing = false,
}: {
  origin: OriginView;
  /** The kind of unit that a tap on a slot loads. */
  source?: CargoSourceView | null;
  editing?: boolean;
}) {
  // A read-only view has no load row: there the row of a ship always says where its cargo is from.
  const places =
    !editing || new Set(origin.cargo.map((item) => item.place + item.site?.system)).size > 1;
  return (
    <div>
      {origin.ships.map((ship) => (
        <Kind key={ship.key} ship={ship} source={source} editing={editing} places={places} />
      ))}
    </div>
  );
}

/** A recorded movement, read-only: one band for each system that ships left. */
export function MovementTokens({ origins }: { origins: OriginView[] }) {
  return (
    <div className="rounded-lg border border-line">
      {origins.map((origin) => (
        <section
          key={origin.system}
          aria-label={origin.label}
          className="grid grid-cols-[190px_minmax(0,1fr)] border-line not-first:border-t phone:grid-cols-1"
        >
          <header className="border-r border-line px-3 py-1.5 phone:border-r-0 phone:border-b phone:border-line/50 phone:px-2.5">
            <h3 className="truncate text-base font-semibold" title={origin.label}>
              {origin.label}
            </h3>
            <div className="text-xs text-muted">
              {plural(origin.away, "system")} away
              {origin.leaves && <strong className="font-mid text-text"> · {origin.leaves}</strong>}
            </div>
            {origin.warning && <div className="text-xs text-orange">⚠ {origin.warning}</div>}
          </header>
          <OriginTokens origin={origin} />
        </section>
      ))}
    </div>
  );
}
