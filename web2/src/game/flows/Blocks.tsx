import { useState } from "react";
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
  Icon,
  InlineNote,
  Kbd,
  ListRow,
  Press,
  cx,
  useReadOnly,
} from "../../ui";
import { ActionButton, RichText, SeatSymbol, TECH_COLOR, useDispatch } from "../context";
import { OfferSelect } from "../OfferSelect";
import { useLink } from "../link";

type TokenState = "kept" | "added" | "removed";

/**
 * One command token, about the size of the text: a triangle, as on the command sheet (the fleet
 * pool points up). A token that stays is filled, an added one is filled in the accent, a removed
 * one is an outline with "−".
 */
function Triangle({ up, state }: { up?: boolean; state: TokenState }) {
  const points = up ? "10,1.5 18.5,16.5 1.5,16.5" : "1.5,1.5 18.5,1.5 10,16.5";
  // The centre of the triangle, where a sign is.
  const y = up ? 11.5 : 6.5;
  const sign = state === "removed" ? `M7,${y}h6` : null;
  return (
    <svg viewBox="0 0 20 18" className="h-3.5 w-4 flex-none" aria-hidden="true">
      <polygon
        points={points}
        strokeLinejoin="round"
        strokeWidth={1.5}
        className={
          state === "kept"
            ? "fill-muted stroke-muted"
            : state === "added"
              ? "fill-accent stroke-accent"
              : "fill-none stroke-muted"
        }
      />
      {sign && <path d={sign} strokeWidth={1.75} strokeLinecap="round" className="stroke-muted" />}
    </svg>
  );
}

/**
 * A pool of command tokens as one line: the name, the count ("3 → 5") in a fixed width, then a
 * triangle for each token. Only the tokens wrap, in their own column.
 *
 * With a counter the tokens are the control, as the tokens of a production are: a tap on an
 * added token (highlighted) takes it back, the dashed slot with "+" adds one, and a removed token
 * comes back with a tap. The slot has the size of a token. A token that the pool keeps (the least that the game offers) is no
 * control, and a token that stays can be taken only while none is added. The slot is not shown
 * when nothing more can be added. Padding, not the glyph, makes a token big enough for a finger.
 */
function PoolRow({ row }: { row: RowView }) {
  const dispatch = useDispatch();
  const { now, after, up } = row.tokens!;
  // Read-only, the tokens are no control.
  const readOnly = useReadOnly();
  const counter = row.control?.kind === "counter" && !readOnly ? row.control.counter : undefined;
  const kept = Math.min(now, after);
  const fewer = !!counter && after > (counter.min ?? 0);
  const more = !!counter && after < counter.max;
  const step = (value: number) =>
    counter && dispatch({ type: "flowCount", key: counter.id, value });
  const cell = "grid place-items-center rounded-sm px-1 py-1 touch:py-2";
  const button = (key: number, state: TokenState, label: string, value: number) => (
    <Press
      key={key}
      aria-label={label}
      className={cx(
        cell,
        state === "added" ? "bg-accent/15 hover:bg-accent/30" : "hover:bg-raised",
      )}
      onClick={() => step(value)}
    >
      <Triangle up={up} state={state} />
    </Press>
  );
  const tokens = Array.from({ length: Math.max(now, after) }, (_, index) => {
    const state: TokenState = index < kept ? "kept" : after > now ? "added" : "removed";
    const takes =
      fewer &&
      index >= (counter?.min ?? 0) &&
      (state === "added" || (state === "kept" && after <= now));
    const gives = more && state === "removed";
    return takes || gives ? (
      button(
        index,
        state,
        `${takes ? "Remove" : "Add"} ${counter!.label}`,
        after + (takes ? -1 : 1),
      )
    ) : (
      <span key={index} className={cx(cell, state === "added" && "bg-accent/15")}>
        <Triangle up={up} state={state} />
      </span>
    );
  });
  return (
    <div
      role="group"
      aria-label={now === after ? `${now} tokens` : `${now} tokens now, ${after} after`}
      className="row grid min-h-8 grid-cols-[7rem_3.75rem_minmax(0,1fr)] items-center gap-x-2 px-3.5 phone:px-2.5 [.row+&]:border-t [.row+&]:border-line/50"
    >
      <span className="flex min-w-0 items-center gap-1.5">
        <Icon name={up ? "token" : "tokenDown"} className="size-3.5 flex-none text-icon" />
        <strong className="truncate text-sm font-mid">
          <RichText value={row.title} />
        </strong>
      </span>
      <span className="text-sm whitespace-nowrap text-muted tabular-nums">
        {now}
        {now !== after && (
          <>
            {" → "}
            <strong className="font-semibold text-text">{after}</strong>
          </>
        )}
      </span>
      <span className="flex min-w-0 flex-wrap items-center">
        {tokens}
        {more && after >= now && (
          <Press
            aria-label={`Add ${counter!.label}`}
            // The size of a token: the border takes a pixel of the padding.
            className="grid place-items-center rounded-sm border border-dashed border-line-strong px-[3px] py-[3px] text-muted hover:bg-raised touch:py-[7px]"
            onClick={() => step(after + 1)}
          >
            <span className="grid h-3.5 w-4 place-items-center text-sm leading-none">+</span>
          </Press>
        )}
      </span>
    </div>
  );
}

function Row({ row }: { row: RowView }) {
  const dispatch = useDispatch();
  const { linked, props } = useLink(row.link);
  const control = row.control;
  // A pool of command tokens is one line of its own, with its tokens as the control.
  if (row.tokens) {
    return <PoolRow row={row} />;
  }
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
          <RichText value={row.title} />
          {row.hint && <Hint label="Rules">{row.hint}</Hint>}
        </span>
      }
      subtitle={row.subtitle}
      linked={linked}
      {...props}
      // A button of a row ("Change") stays at its end on a phone, so that a summary is one line.
      inline={control?.kind === "button"}
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
 * so every row has the same shape and a row that is not allowed is only dimmed. The printed text
 * of a card is a second line in the whole width of the row, under the name and the state.
 */
function MenuRow({ row, grouped }: { row: MenuRowView; grouped: boolean }) {
  const dispatch = useDispatch();
  return (
    <div
      className={cx(
        "relative grid min-h-9 items-center gap-x-3 gap-y-0.5 px-3.5 py-1.5 touch:min-h-12 [&+&]:border-t [&+&]:border-line/50",
        // A phone has no keys: the column of the key is not there.
        grouped
          ? "grid-cols-[30px_104px_minmax(0,1fr)_auto] phone:grid-cols-[68px_minmax(0,1fr)_auto]"
          : "grid-cols-[30px_minmax(0,1fr)_auto] phone:grid-cols-[minmax(0,1fr)_auto]",
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
        <Press
          disabled={row.disabled}
          aria-pressed={row.selected}
          aria-keyshortcuts={row.key}
          onClick={() => dispatch(row.intent)}
          className="truncate text-left text-base font-mid outline-none after:absolute after:inset-0 enabled:cursor-pointer disabled:text-faint phone:whitespace-normal focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-accent"
        >
          {row.title}
        </Press>
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
      </span>
      <span className="flex items-center gap-2.5">
        <span className={cx("text-xs tabular-nums", row.disabled ? "text-faint" : "text-muted")}>
          {row.state}
        </span>
        {row.selected && <Badge tone="draft">Selected</Badge>}
        {row.offer && (
          // Above the button of the row, which covers the row.
          <span className="relative z-1 inline-flex">
            <OfferSelect offer={row.offer} compact />
          </span>
        )}
      </span>
      {row.text && (
        // From the column of the name to the right edge; a phone has no column of the key.
        <span
          className={cx(
            "text-xs",
            grouped
              ? "[grid-column:3/-1] phone:[grid-column:2/-1]"
              : "[grid-column:2/-1] phone:[grid-column:1/-1]",
            row.disabled ? "text-faint" : "text-muted",
          )}
        >
          {row.text}
        </span>
      )}
    </div>
  );
}

/**
 * A choice of one about a card, on the line of its name: a segmented toggle. The chosen part has
 * "✓" and the tint; a choice made again takes it back, as the row of a menu does.
 */
function Choices({ label, rows }: { label: string; rows: MenuRowView[] }) {
  const dispatch = useDispatch();
  return (
    <div
      role="group"
      aria-label={label}
      className="inline-flex flex-none overflow-hidden rounded-md border border-line"
    >
      {rows.map((row) => (
        <Press
          key={row.title}
          disabled={row.disabled}
          aria-pressed={!!row.selected}
          aria-keyshortcuts={row.key || undefined}
          onClick={() => dispatch(row.intent)}
          className="inline-flex items-center gap-1.5 px-2.5 py-1 text-sm whitespace-nowrap text-muted enabled:cursor-pointer enabled:hover:bg-white/5 disabled:text-faint aria-pressed:bg-raised aria-pressed:text-accent touch:min-h-10 touch:px-3.5 [&+&]:border-l [&+&]:border-line"
        >
          {row.key && <Kbd>{row.key}</Kbd>}
          {row.selected && <span aria-hidden>✓</span>}
          {row.title}
        </Press>
      ))}
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
        <Press
          aria-pressed={on}
          onClick={() => dispatch(item.toggle!.intent)}
          className="min-w-0 truncate text-left outline-none after:absolute after:inset-0 enabled:cursor-pointer focus-visible:after:rounded-md focus-visible:after:outline-2 focus-visible:after:outline-accent phone:whitespace-normal"
        >
          {name}
        </Press>
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
function SeatList({ title, rows }: { title?: string; rows: SeatRowView[] }) {
  const list = (
    <ul
      aria-label={title ?? "Seats"}
      className={title ? undefined : "rounded-lg border border-line bg-surface/40"}
    >
      {rows.map((row, at) => (
        <SeatRow key={`${row.order}|${row.seat}|${at}`} row={row} />
      ))}
    </ul>
  );
  // A list of a phase of the Log has a title: one of its parts ("Picks", "Votes").
  return title ? (
    <Card>
      <CardHeading title={title} />
      {list}
    </Card>
  ) : (
    list
  );
}

/**
 * One seat of a strategy card. A row with details (a resolved action of the Log) opens with a
 * click to what the seat did, line by line ("▸" closed, "▾" open): a view toggle, not a choice.
 */
function SeatRow({ row }: { row: SeatRowView }) {
  const [open, setOpen] = useState(false);
  const details = row.details?.length ? row.details : null;
  const grid = cx(
    // On a phone the name and the state are one line; what the seat did is the line under it.
    "grid min-h-6 w-full grid-cols-[16px_16px_132px_150px_minmax(0,1fr)] items-baseline gap-x-2.5 px-3.5 py-0.5 text-left text-sm phone:grid-cols-[16px_16px_minmax(0,1fr)_auto] phone:px-2.5 phone:py-1",
    row.open && "bg-gold/[.06] shadow-[inset_2px_0_0_var(--color-gold)]",
    details && "cursor-pointer hover:bg-white/5",
  );
  const cells = (
    <>
      <span className="text-xs text-faint tabular-nums">{row.order}</span>
      <SeatSymbol seat={row.seat} />
      <span className={cx("truncate", row.you ? "font-semibold" : "font-mid")} title={row.name}>
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
        {details && (
          <span aria-hidden="true" className="mr-1.5 text-muted">
            {open ? "▾" : "▸"}
          </span>
        )}
        {row.text}
      </span>
    </>
  );
  return (
    <li className="[&+&]:border-t [&+&]:border-line/50">
      {details ? (
        <button type="button" className={grid} aria-expanded={open} onClick={() => setOpen(!open)}>
          {cells}
        </button>
      ) : (
        <div className={grid}>{cells}</div>
      )}
      {details && open && (
        <ul
          aria-label="What it did"
          className="space-y-0.5 border-l-2 border-line-strong py-1 pr-2.5 pl-3 text-sm text-muted phone:pr-1.5 phone:pl-1.5"
        >
          {details.map((line, index) => (
            <li key={index}>{line}</li>
          ))}
        </ul>
      )}
      {row.open && (
        <div className="space-y-1.5 border-l-2 border-gold/60 py-1 pr-2.5 pl-3 phone:pr-1.5 phone:pl-1.5">
          <Blocks blocks={row.open} />
        </div>
      )}
    </li>
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
      <Press
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
      </Press>
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
                <div className="flex items-center gap-2">
                  {/* The kind takes a second line before the choices do. */}
                  <div className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2">
                    <strong className="text-base font-semibold">{block.name}</strong>
                    <span className="text-xs text-muted">{block.type}</span>
                  </div>
                  {block.choices && <Choices label={block.name} rows={block.choices} />}
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
            return <SeatList key={index} title={block.title} rows={block.rows} />;
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
