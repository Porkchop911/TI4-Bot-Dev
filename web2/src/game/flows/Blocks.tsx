import type {
  BlockView,
  DealItemView,
  DealSideView,
  MenuRowView,
  RowView,
  SeatRowView,
  TechCellView,
  TechColor,
} from "../../model";
import {
  Badge,
  Card,
  CardHeading,
  Counter,
  Gauge,
  Gauges,
  Hint,
  InlineNote,
  Kbd,
  ListRow,
  cx,
} from "../../ui";
import { ActionButton, RichText, SeatSymbol, TECH_COLOR, useDispatch } from "../context";
import { OfferSelect } from "../OfferSelect";
import { useLink } from "../link";

/**
 * A pool of command tokens, one triangle for each token. The fleet pool points up and the other
 * pools point down, as on the command sheet. A token that stays is filled, a token that is added
 * has "+", a token that is removed is an empty outline with "−": the shape tells them apart, not
 * only the colour.
 */
function TokenPool({ now, after, up }: { now: number; after: number; up?: boolean }) {
  const kept = Math.min(now, after);
  const points = up ? "10,1.5 18.5,16.5 1.5,16.5" : "1.5,1.5 18.5,1.5 10,16.5";
  // The centre of the triangle, where the sign is.
  const y = up ? 11.5 : 6.5;
  return (
    <span
      className="inline-flex items-center gap-2.5"
      role="img"
      aria-label={now === after ? `${now} tokens` : `${now} tokens now, ${after} after`}
    >
      <span className="inline-flex min-w-[216px] items-center gap-1 phone:min-w-0 phone:flex-wrap">
        {Array.from({ length: Math.max(now, after) }, (_, index) => {
          const added = index >= kept && after > now;
          const removed = index >= kept && !added;
          return (
            <svg
              key={index}
              viewBox="0 0 20 18"
              className="h-[18px] w-5 flex-none"
              aria-hidden="true"
            >
              <polygon
                points={points}
                strokeLinejoin="round"
                strokeWidth={1.5}
                className={
                  removed
                    ? "fill-none stroke-muted"
                    : added
                      ? "fill-accent stroke-accent"
                      : "fill-muted stroke-muted"
                }
              />
              {(added || removed) && (
                <path
                  d={`M7,${y}h6${added ? `M10,${y - 3}v6` : ""}`}
                  strokeWidth={1.75}
                  strokeLinecap="round"
                  className={added ? "stroke-canvas" : "stroke-muted"}
                />
              )}
            </svg>
          );
        })}
      </span>
      <span className="text-xs text-muted tabular-nums">
        {now}
        {now !== after && (
          <>
            {" → "}
            <strong className="text-base font-semibold text-text">{after}</strong>
          </>
        )}
      </span>
    </span>
  );
}

function Row({ row }: { row: RowView }) {
  const dispatch = useDispatch();
  const { linked, props } = useLink(row.link);
  const control = row.control;
  return (
    <ListRow
      icon={row.icon}
      lead={
        row.order !== undefined ? (
          <span className="grid size-[22px] place-items-center rounded-full border border-line text-xs text-muted tabular-nums">
            {row.order}
          </span>
        ) : undefined
      }
      title={
        <span className="inline-flex items-center gap-2">
          {/* Pool rows share one name column, so the token marks line up. */}
          <span className={cx(row.tokens && "inline-block min-w-[108px]")}>
            <RichText value={row.title} />
          </span>
          {row.hint && <Hint label="Rules">{row.hint}</Hint>}
        </span>
      }
      subtitle={row.subtitle}
      extra={row.tokens && <TokenPool {...row.tokens} />}
      linked={linked}
      {...props}
      controls={
        control &&
        (control.kind === "button" ? (
          <ActionButton view={control.button} size="sm" />
        ) : control.kind === "counter" ? (
          <Counter
            {...control.counter}
            onChange={(value) => dispatch({ type: "flowCount", key: control.counter.id, value })}
          />
        ) : (
          <Badge tone={control.pill.tone}>{control.pill.label}</Badge>
        ))
      }
    />
  );
}

/**
 * One choice: key, group, name, and its state at the right edge. The whole row is the control,
 * so every row has the same shape and a row that is not allowed is only dimmed.
 */
function MenuRow({ row, grouped }: { row: MenuRowView; grouped: boolean }) {
  const dispatch = useDispatch();
  return (
    <div
      className={cx(
        "relative grid min-h-9 items-center gap-x-3 px-3.5 py-1.5 touch:min-h-12 [&+&]:border-t [&+&]:border-line/50",
        // A phone has no keys: the column of the key is not there.
        grouped
          ? "grid-cols-[30px_104px_minmax(0,1fr)_auto] phone:grid-cols-[68px_minmax(0,1fr)_auto]"
          : "grid-cols-[30px_minmax(0,1fr)_auto] phone:grid-cols-[minmax(0,1fr)_auto]",
        // On a phone the setting of a reaction is under the text of its card, which has the width.
        row.offer && "phone:grid-cols-1 phone:gap-y-1.5",
        !row.disabled && "focus-within:bg-white/5 hover:bg-white/5",
        row.selected && "shadow-[inset_2px_0_0_var(--color-gold)]",
      )}
    >
      <span
        className={cx(
          "justify-self-start phone:hidden",
          row.disabled ? "text-faint" : "text-accent",
        )}
      >
        {row.key && <Kbd>{row.key}</Kbd>}
      </span>
      {grouped && <span className="text-xs text-muted">{row.group}</span>}
      <span className="flex min-w-0 flex-wrap items-center gap-x-2">
        <button
          type="button"
          disabled={row.disabled}
          aria-pressed={row.selected}
          aria-keyshortcuts={row.key}
          onClick={() => dispatch(row.intent)}
          className="truncate text-left text-base font-mid outline-none after:absolute after:inset-0 enabled:cursor-pointer disabled:text-faint phone:whitespace-normal focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-accent"
        >
          {row.title}
        </button>
        {row.hint && (
          <span className="relative z-1 inline-flex">
            <Hint label={`Rules: ${row.title}`}>{row.hint}</Hint>
          </span>
        )}
        {row.help && row.help.length > 0 && (
          <span className="relative z-1 inline-flex">
            <Hint label={`Rules: ${row.title}`}>
              {row.help.map((item) => (
                <p key={item.title}>
                  <strong className="block">{item.title}</strong>
                  {item.text}
                </p>
              ))}
            </Hint>
          </span>
        )}
        {row.text && (
          <span className={cx("w-full text-xs", row.disabled ? "text-faint" : "text-muted")}>
            {row.text}
          </span>
        )}
      </span>
      <span className="flex items-center gap-2.5">
        <span className={cx("text-xs tabular-nums", row.disabled ? "text-faint" : "text-muted")}>
          {row.state}
        </span>
        {row.selected && <Badge tone="draft">Selected</Badge>}
        {row.offer && (
          // Above the button of the row, which covers the row.
          <span className="relative z-1 inline-flex">
            <OfferSelect offer={row.offer} />
          </span>
        )}
      </span>
    </div>
  );
}

/**
 * A card of a deal. Staged with a tap: a staged card has "✓" and the gold frame. Its printed text
 * is behind "?".
 */
function DealCard({ item }: { item: DealItemView }) {
  const dispatch = useDispatch();
  const on = !!item.toggle?.on;
  const name = (
    <>
      {on && <span className="text-gold">✓ </span>}
      <span className="font-mid">{item.name}</span>
    </>
  );
  return (
    <span
      className={cx(
        "relative inline-flex max-w-full min-h-7 items-center gap-1.5 rounded-md border px-2 py-0.5 text-sm touch:min-h-10",
        on
          ? "border-gold bg-gold/[.12]"
          : item.toggle
            ? "border-line-strong bg-white/[.03] focus-within:bg-white/[.07] hover:bg-white/[.07]"
            : "border-line",
      )}
    >
      {item.toggle ? (
        <button
          type="button"
          aria-pressed={on}
          onClick={() => dispatch(item.toggle!.intent)}
          className="min-w-0 truncate text-left outline-none after:absolute after:inset-0 enabled:cursor-pointer focus-visible:after:rounded-md focus-visible:after:outline-2 focus-visible:after:outline-accent phone:whitespace-normal"
        >
          {name}
        </button>
      ) : (
        <span className="min-w-0 truncate phone:whitespace-normal">{name}</span>
      )}
      {item.help && (
        <span className="relative z-1 inline-flex flex-none">
          <Hint label={`Text: ${item.name}`}>{item.help.text}</Hint>
        </span>
      )}
    </span>
  );
}

/**
 * A transaction: one card for each side, the seat and what it holds now in the heading. An amount
 * is a counter (or its count, read-only); the cards are chips under the amounts.
 */
function Deal({ sides }: { sides: DealSideView[] }) {
  const dispatch = useDispatch();
  return (
    <div className="grid grid-cols-2 gap-2.5 phone:grid-cols-1">
      {sides.map((side) => {
        const amounts = side.items.filter((item) => item.counter || item.count !== undefined);
        const cards = side.items.filter((item) => !item.counter && item.count === undefined);
        const groups = [...new Set(cards.map((item) => item.group ?? ""))];
        return (
          <Card key={side.title} className="min-w-0">
            <CardHeading title={side.title}>
              <span className="inline-flex min-w-0 items-center gap-1.5">
                <SeatSymbol seat={side.seat} />
                <span className="max-w-[150px] truncate font-mid text-text" title={side.name}>
                  {side.name}
                </span>
                <span className="tabular-nums">· {side.holds}</span>
              </span>
            </CardHeading>
            {amounts.map((item) => (
              <div
                key={item.name}
                className="flex min-h-9 items-center justify-between gap-3 px-3.5 py-1 [&+&]:border-t [&+&]:border-line/50 phone:px-2.5"
              >
                <span className="text-sm font-mid">{item.name}</span>
                {item.counter ? (
                  <Counter
                    {...item.counter}
                    onChange={(value) =>
                      dispatch({ type: "flowCount", key: item.counter!.id, value })
                    }
                  />
                ) : (
                  <strong className="text-base font-semibold tabular-nums">{item.count}</strong>
                )}
              </div>
            ))}
            {groups.map((group, at) => (
              <div
                key={group}
                className={cx(
                  "grid grid-cols-[112px_minmax(0,1fr)] items-baseline gap-x-2.5 px-3.5 py-1.5 phone:grid-cols-1 phone:gap-y-1 phone:px-2.5",
                  (at > 0 || amounts.length > 0) && "border-t border-line/50",
                )}
              >
                <span className="text-xs text-muted">{group}</span>
                <span className="flex flex-wrap gap-1.5">
                  {cards
                    .filter((item) => (item.group ?? "") === group)
                    .map((item) => (
                      <DealCard key={item.name} item={item} />
                    ))}
                </span>
              </div>
            ))}
            {!side.items.length && <div className="px-3.5 py-2 text-xs text-muted">Nothing</div>}
          </Card>
        );
      })}
    </div>
  );
}

const TECH_NAME: Record<TechColor, string> = { B: "Blue", G: "Green", Y: "Yellow", R: "Red" };

const STATUS_TONE: Record<SeatRowView["status"]["tone"], string> = {
  live: "text-cyan",
  draft: "text-gold",
  done: "text-green",
  quiet: "text-muted",
  alert: "text-red",
};

/**
 * The seat order of a strategy card: the primary, then the secondaries, each with what it did.
 * The row of the viewer opens into the editor in place. The sign of a state carries it, not only
 * the colour.
 */
function SeatList({ rows }: { rows: SeatRowView[] }) {
  return (
    <div className="rounded-lg border border-line bg-surface/40">
      {rows.map((row) => (
        <div key={row.order} className="[&+&]:border-t [&+&]:border-line/50">
          <div
            className={cx(
              // On a phone the name and the state are one line; what the seat did is the line under it.
              "grid min-h-6 grid-cols-[16px_16px_132px_150px_minmax(0,1fr)] items-baseline gap-x-2.5 px-3.5 py-0.5 text-sm phone:grid-cols-[16px_16px_minmax(0,1fr)_auto] phone:px-2.5 phone:py-1",
              row.open && "bg-gold/[.06] shadow-[inset_2px_0_0_var(--color-gold)]",
            )}
          >
            <span className="text-xs text-faint tabular-nums">{row.order}</span>
            <SeatSymbol seat={row.seat} />
            <span
              className={cx("truncate", row.you ? "font-semibold" : "font-mid")}
              title={row.name}
            >
              {row.name}
            </span>
            <span className={cx("text-xs whitespace-nowrap", STATUS_TONE[row.status.tone])}>
              {row.status.sign} {row.status.label}
            </span>
            <span
              className={cx(
                "min-w-0 phone:col-start-3 phone:col-end-[-1] phone:empty:hidden",
                row.status.tone === "quiet" && "text-muted",
              )}
            >
              {row.text}
            </span>
          </div>
          {row.open && (
            <div className="space-y-1.5 border-l-2 border-gold/60 py-1 pr-2.5 pl-3 phone:pr-1.5 phone:pl-1.5">
              <Blocks blocks={row.open} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

/**
 * One technology: name and prerequisites. A filled symbol is a prerequisite that the player has,
 * a hollow one is missing. The whole cell is the control; the printed text is behind "?".
 */
function TechCell({ cell }: { cell: TechCellView }) {
  const dispatch = useDispatch();
  const closed = cell.state === "closed";
  const missing = cell.needs.filter((need) => need.missing).length;
  return (
    <div
      className={cx(
        "relative flex min-h-[26px] items-center gap-1.5 rounded-md border px-2 py-0.5 touch:min-h-10",
        cell.selected
          ? "border-gold bg-gold/[.12]"
          : cell.state === "open"
            ? "border-line-strong bg-white/[.03] focus-within:bg-white/[.07] hover:bg-white/[.07]"
            : "border-line/60",
      )}
    >
      <button
        type="button"
        disabled={cell.state !== "open"}
        aria-pressed={cell.selected}
        aria-label={`${cell.name}${cell.state === "owned" ? ", researched" : closed ? `, ${missing} prerequisite${missing === 1 ? "" : "s"} missing` : ""}`}
        onClick={() => dispatch(cell.intent)}
        className={cx(
          // A phone has no hover for the full name: the name takes a second line.
          "min-w-0 flex-1 truncate text-left text-xs font-mid outline-none after:absolute after:inset-0 enabled:cursor-pointer focus-visible:after:rounded-md focus-visible:after:outline-2 focus-visible:after:outline-accent phone:whitespace-normal",
          closed ? "text-faint" : cell.state === "owned" ? "text-muted" : "text-text",
        )}
      >
        {/* Outside the colour columns the cell names its colour with a mark. */}
        {cell.color && (
          <span
            className="mr-1.5 inline-block size-[9px] rounded-[2px] align-[-1px]"
            style={{ background: TECH_COLOR[cell.color] }}
            title={TECH_NAME[cell.color]}
          />
        )}
        {cell.state === "owned" && <span className="text-green">✓ </span>}
        {cell.name}
      </button>
      <span className="flex flex-none gap-[3px]" aria-hidden="true">
        {cell.needs.map((need, index) => (
          <span
            key={index}
            className="size-[9px] rounded-full border"
            style={{
              borderColor: TECH_COLOR[need.color],
              background: need.missing ? "transparent" : TECH_COLOR[need.color],
              opacity: cell.state === "owned" ? 0.45 : 1,
            }}
          />
        ))}
      </span>
      <span className="relative z-1 inline-flex flex-none">
        <Hint label={`Text: ${cell.name}`}>{cell.text}</Hint>
      </span>
    </div>
  );
}

/**
 * Content for actions that have no dedicated view yet: strategy cards and component actions.
 * The data source sends blocks; this renders them with the shared primitives.
 */
export function Blocks({ blocks }: { blocks: BlockView[] }) {
  return (
    <>
      {blocks.map((block, index) => {
        switch (block.kind) {
          case "note":
            return (
              <InlineNote key={index} tone={block.tone} strong={block.strong}>
                {block.text}
              </InlineNote>
            );
          case "source":
            // The card that asks, as printed: the player decides by this text.
            return (
              <div key={index} className="border-l-2 border-line-strong py-0.5 pl-3">
                <div className="flex items-baseline gap-2">
                  <strong className="text-base font-semibold">{block.name}</strong>
                  <span className="text-xs text-muted">{block.type}</span>
                </div>
                <p className="text-sm text-muted">{block.text}</p>
              </div>
            );
          case "gauges":
            return (
              <Gauges key={index}>
                {block.gauges.map((gauge) => (
                  <Gauge key={gauge.label} {...gauge} />
                ))}
              </Gauges>
            );
          case "seats":
            return <SeatList key={index} rows={block.rows} />;
          case "techs":
            return (
              <Card key={index} className="space-y-1.5 p-2">
                {/* What is chosen, and its price. The selected cell is the control. */}
                <div className="flex flex-wrap items-baseline gap-x-3 px-1.5">
                  <h3 className="text-base font-semibold">{block.title}</h3>
                  <span className="min-w-0 flex-1 text-sm">
                    {block.chosen.length ? (
                      block.chosen.join("  +  ")
                    ) : (
                      <span className="text-muted">No technology chosen</span>
                    )}
                  </span>
                  {block.aside && <span className="text-xs text-muted">{block.aside}</span>}
                </div>
                <div className="grid grid-cols-4 gap-x-2.5 phone:grid-cols-2 phone:gap-y-2.5">
                  {block.columns.map((column) => (
                    <div key={column.color} className="space-y-1">
                      <div
                        className="border-b-2 pb-0.5 text-xs font-semibold"
                        style={{ borderColor: TECH_COLOR[column.color] }}
                      >
                        {column.label}
                      </div>
                      {column.cells.map((cell) => (
                        <TechCell key={cell.id} cell={cell} />
                      ))}
                    </div>
                  ))}
                </div>
                {block.bands.map((band) => (
                  <div key={band.label} className="space-y-1">
                    <div className="border-b border-line pb-0.5 text-xs font-semibold">
                      {band.label}
                    </div>
                    {/* A short row has wider cells, so a long name with its marks has room. */}
                    <div
                      className={cx(
                        "grid gap-x-2.5 gap-y-1",
                        band.cells.length > 4 ? "grid-cols-5" : "grid-cols-3",
                        "phone:grid-cols-2",
                      )}
                    >
                      {band.cells.map((cell) => (
                        <TechCell key={cell.id} cell={cell} />
                      ))}
                    </div>
                  </div>
                ))}
              </Card>
            );
          case "menu":
            return (
              <Card key={index}>
                {block.title && <CardHeading title={block.title} />}
                {block.rows.map((row, index) => (
                  <MenuRow
                    // A long list has rows without a shortcut key, so the key does not identify a row.
                    key={index}
                    row={row}
                    grouped={block.rows.some((item) => item.group)}
                  />
                ))}
              </Card>
            );
          case "card":
            return (
              <Card key={index}>
                <CardHeading title={block.title} bad={block.bad}>
                  {block.aside}
                </CardHeading>
                {block.rows.every((row) => row.order !== undefined) ? (
                  // A list in seat order: two columns, read down the left column first.
                  <div
                    className="grid grid-flow-col grid-cols-2 phone:grid-flow-row phone:grid-cols-1 [&>.row]:border-t-0"
                    style={{
                      gridTemplateRows: `repeat(${Math.ceil(block.rows.length / 2)}, auto)`,
                    }}
                  >
                    {block.rows.map((row, at) => (
                      <Row key={at} row={row} />
                    ))}
                  </div>
                ) : (
                  block.rows.map((row, at) => <Row key={at} row={row} />)
                )}
              </Card>
            );
          case "deal":
            return <Deal key={index} sides={block.sides} />;
          case "draft":
            // A choice that is prepared before the player's seat is reached. The badge states it.
            return (
              <Card key={index}>
                <CardHeading title={block.title}>
                  <span className="inline-flex items-center gap-2">
                    <Badge tone={block.pill.tone}>{block.pill.label}</Badge>
                    {block.hint && <Hint label={`About ${block.title}`}>{block.hint}</Hint>}
                  </span>
                </CardHeading>
                {block.text && <div className="px-3.5 py-2 text-xs">{block.text}</div>}
                {block.blocks.length > 0 && (
                  <div className="space-y-2.5 p-2.5">
                    <Blocks blocks={block.blocks} />
                  </div>
                )}
              </Card>
            );
        }
        return null;
      })}
    </>
  );
}
