import { type ReactNode, useState } from "react";
import type {
  ComponentCardView,
  PlayerPickView,
  PlayerRowView,
  PlayerSort,
  PlayerTableView,
} from "../../model";
import { Hint, Icon, cx, type IconName } from "../../ui";
import { SeatSymbol, useDispatch, useSeat, useSeats } from "../context";

/** "Agent exhausted · Commander locked": the cards by name, with the state of one that is not ready. */
const cardLine = (cards: ComponentCardView[], name: (card: ComponentCardView) => string) =>
  cards
    .map((card) => `${name(card)}${card.state && card.state !== "ready" ? ` ${card.state}` : ""}`)
    .join(" · ");

/** The icons of a card, in the order of the card: the legend of the header lists them. */
const LEGEND: [IconName, string][] = [
  ["star", "Victory points"],
  ["speaker", "Speaker"],
  ["resources", "Resources: ready / total"],
  ["influence", "Influence: ready / total"],
  ["planet", "Planets"],
  ["coin", "Trade goods"],
  ["commodity", "Commodities / limit"],
  ["tokenDown", "Tactic pool · strategy pool"],
  ["token", "Fleet pool"],
  ["card", "Action cards in hand"],
  ["secret", "Secret objectives: scored / held"],
];

/** A symbol that stands for a word: the word is its accessible name and its tooltip. */
function Sign({ name, label, className }: { name: IconName; label: string; className?: string }) {
  return (
    <span role="img" aria-label={label} title={label} className="inline-flex flex-none">
      <Icon name={name} className={cx("size-4", className)} />
    </span>
  );
}

/** One value of a seat: its icon and its value. The full name is the tooltip and the accessible name of the icon. */
function Stat({
  icon,
  label,
  title = label,
  children,
}: {
  icon: IconName;
  label: string;
  /** The tooltip, when it says more than the name: the format of the value. */
  title?: string;
  children: ReactNode;
}) {
  return (
    <span title={title} className="inline-flex flex-none items-center gap-1">
      <span role="img" aria-label={label} className="inline-flex">
        <Icon name={icon} className="size-4 text-faint" />
      </span>
      <span>{children}</span>
    </span>
  );
}

/** A command token: the fleet pool points up, the tactic and the strategy pool point down. */
function Pool({ label, count, up }: { label: string; count: number; up?: boolean }) {
  return (
    <span title={label} className="inline-flex items-center gap-0.5">
      <span role="img" aria-label={label} className="inline-flex">
        <Icon name={up ? "token" : "tokenDown"} className="size-3.5 text-faint" />
      </span>
      {count}
    </span>
  );
}

const BADGE = "flex-none rounded-sm px-1 text-2xs leading-4 font-strong";

function Card({
  row,
  column,
  pick,
  onOpen,
}: {
  row: PlayerRowView;
  /** The column of the grid: the hover sheet goes left of the cards, over the board. */
  column: number;
  pick: PlayerPickView | null;
  onOpen: () => void;
}) {
  const seat = useSeat(row.seat);
  const dispatch = useDispatch();
  // While the task asks for a player, a click on a card chooses that player.
  const option = pick?.seats[row.seat];
  const allowed = !!option && option.reason === null;
  const chosen = !!pick?.chosen.includes(row.seat);
  const act = pick ? () => allowed && dispatch({ type: "pickSeat", seat: row.seat }) : onOpen;
  const [tactic, fleet, strategy] = row.tokens;
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: a click anywhere on the card is a larger target; the name button is the control for the keyboard
    <li
      onClick={act}
      className={cx(
        // Desktop: the strategy cards are the left column. A phone has them as the top row.
        "group relative grid min-w-0 grid-cols-[96px_minmax(0,1fr)] gap-x-2.5 gap-y-1.5 rounded-md border px-2 py-1.5 [grid-template-areas:'sc_id'_'sc_val'] focus-within:bg-white/5 phone:grid-cols-1 phone:gap-y-1 phone:[grid-template-areas:'sc'_'id'_'val']",
        // The card of the active player stands out: ▶ and a frame in the accent.
        row.turn === "now" ? "border-accent outline outline-accent" : "border-line",
        (!pick || allowed) && "cursor-pointer hover:bg-white/5",
        pick && !allowed && "opacity-45",
        allowed && !chosen && "shadow-[inset_3px_0_0_var(--color-text)]",
        chosen && "shadow-[inset_3px_0_0_var(--color-gold)]",
        row.isMe && !pick && "bg-cyan/[.07]",
        row.passed ? "text-faint" : row.isMe ? "text-text" : "text-muted",
      )}
    >
      {/*
        The strategy cards are in the same place on every card: the number large, the name with it.
        Two cards (four players or fewer) share the place; a seat with none has an empty place.
      */}
      <div
        role="group"
        aria-label="Strategy cards"
        className={cx(
          "flex min-h-12 min-w-0 flex-col justify-center gap-1 rounded-md border px-2 py-1 [grid-area:sc] phone:min-h-[30px] phone:flex-row phone:items-center phone:justify-start phone:gap-3 phone:py-0.5",
          row.strategyCards.length ? "border-line bg-white/[.03]" : "border-dashed border-line",
        )}
      >
        {row.strategyCards.length === 0 && (
          <span className="text-center text-faint phone:text-left" title="No strategy card">
            –
          </span>
        )}
        {row.strategyCards.map((card) => (
          <span
            key={card.number}
            className={cx(
              "flex min-w-0 phone:flex-row phone:items-baseline phone:gap-1.5",
              row.strategyCards.length > 1 ? "items-baseline gap-1.5" : "flex-col",
              card.used && "opacity-50",
            )}
            title={`Strategy card ${card.number} ${card.name}${card.used ? " · used" : ""}`}
          >
            <span
              className={cx(
                "leading-none font-strong text-gold tabular-nums phone:text-xl",
                row.strategyCards.length > 1 ? "text-lg" : "text-[26px]",
              )}
            >
              {card.number}
            </span>{" "}
            <span className="truncate text-xs text-text">
              {card.name}
              {card.used ? " ✓" : ""}
            </span>
          </span>
        ))}
      </div>
      {/* Desktop: one line. A phone breaks it after the victory points. */}
      <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 self-start text-sm [grid-area:id]">
        <span className="w-3.5 flex-none text-center text-muted">
          {row.turn === "now" ? (
            <span className="text-accent" title="Active player">
              ▶
            </span>
          ) : row.turn === "next" ? (
            <span title="Next player">›</span>
          ) : null}
        </span>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            act();
          }}
          disabled={!!pick && !allowed}
          aria-pressed={pick ? chosen : undefined}
          aria-label={
            pick
              ? `${pick.verb} ${seat.name}, ${seat.faction}${option?.reason ? `. ${option.reason}` : ""}`
              : `${seat.name}, ${seat.faction}${row.isMe ? ", you" : ""}${row.turn === "now" ? ", active player" : ""}${row.speaker ? ", speaker" : ""}${row.passed ? ", passed" : ""}. Open player sheet.`
          }
          className="flex min-w-[120px] flex-[1_1_0] items-center gap-1.5 overflow-hidden text-left focus-visible:outline-offset-1 phone:min-w-[72px]"
        >
          <SeatSymbol seat={row.seat} />
          <span
            className={cx(
              "max-w-[150px] flex-none truncate font-strong phone:max-w-full phone:flex-initial",
              row.passed ? "text-faint" : "text-text",
            )}
            title={seat.name}
          >
            {seat.name}
          </span>
          <span className="min-w-0 truncate text-muted phone:hidden" title={seat.faction}>
            {seat.faction}
          </span>
        </button>
        <span
          title="Victory points of 10"
          className="order-last inline-flex flex-none items-center gap-1 rounded-full bg-raised px-2 font-strong text-text tabular-nums phone:order-none"
        >
          <span role="img" aria-label="Victory points" className="inline-flex">
            <Icon name="star" className="size-3.5 text-gold" />
          </span>
          <span>{row.victoryPoints}</span>
        </span>
        <span className="hidden h-0 basis-full phone:block" />
        <span className="hidden min-w-0 flex-1 truncate text-xs text-muted phone:block">
          {seat.faction}
        </span>
        {row.isMe && <span className={cx(BADGE, "bg-cyan/15 text-cyan")}>You</span>}
        {row.passed && (
          <span className={cx(BADGE, "border border-line-strong text-muted")}>Passed</span>
        )}
        {row.speaker && <Sign name="speaker" label="Speaker" className="text-gold" />}
        {option?.reason && (
          <span className="min-w-0 truncate text-xs text-red" title={option.reason}>
            {option.reason}
          </span>
        )}
        {option?.note && !option.reason && (
          <span className="flex-none text-xs whitespace-nowrap text-text tabular-nums">
            {option.note}
          </span>
        )}
        {chosen && <span className="flex-none text-xs font-bold text-gold">Selected</span>}
      </div>
      {/* A fixed grid: every card has each value in the same place. */}
      <div className="grid grid-cols-4 items-center gap-x-3 gap-y-1 self-start text-sm whitespace-nowrap tabular-nums [grid-area:val] phone:grid-cols-3 phone:gap-x-1.5 phone:text-xs">
        <Stat icon="resources" label="Resources" title="Resources: ready / total">
          {row.resources.join("/")}
        </Stat>
        <Stat icon="influence" label="Influence" title="Influence: ready / total">
          {row.influence.join("/")}
        </Stat>
        <Stat icon="planet" label="Planets">
          {row.planets}
        </Stat>
        <Stat icon="coin" label="Trade goods">
          {row.tradeGoods}
        </Stat>
        <Stat icon="commodity" label="Commodities" title="Commodities / limit">
          {row.commodities.join("/")}
        </Stat>
        <span
          title="Command tokens: tactic · fleet · strategy"
          className="inline-flex flex-none items-center gap-1 phone:order-last phone:col-span-2"
        >
          {/* The dots keep the order of the command sheet in the text: "3 · 3 · 2". */}
          <Pool label="Tactic pool" count={tactic} />
          <span className="text-faint">{" · "}</span>
          <Pool label="Fleet pool" count={fleet} up />
          <span className="text-faint">{" · "}</span>
          <Pool label="Strategy pool" count={strategy} />
        </span>
        <Stat icon="card" label="Action cards" title="Action cards in hand">
          {row.actionCards}
        </Stat>
        <Stat icon="secret" label="Secret objectives" title="Secret objectives: scored / held">
          {row.secrets.join("/")}
        </Stat>
      </div>
      <div
        className={cx(
          "absolute top-0 z-20 hidden w-[min(340px,90vw)] cursor-default rounded-lg border border-line bg-raised px-3 py-2.5 text-xs leading-[1.45] font-normal whitespace-normal text-text shadow-[0_10px_30px_#0008]",
          // Left of the cards: over the board, never over another card.
          column === 0 ? "right-[calc(100%+12px)]" : "right-[calc(200%+18px)]",
          // While the cards are the picker, the sheet of a player does not cover the board.
          // A phone opens the sheet of the player with a tap; it has the same text.
          !pick && "group-focus-within:block group-hover:block phone:hidden!",
        )}
      >
        <strong>
          <SeatSymbol seat={row.seat} /> {seat.name} · {seat.faction}
        </strong>
        {/* The names only. The sheet of the player has the printed texts. */}
        <p className="mt-[5px]">{cardLine(row.abilities, (card) => card.name)}</p>
        <p className="mt-[5px] text-muted">
          <Sign
            name="tech"
            label="Technologies"
            className="mr-1 size-3.5 align-[-2px] text-faint"
          />
          {row.technologies.join(" · ")}
        </p>
        <p className="mt-[5px] text-muted">
          {cardLine(row.leaders, (card) => card.kind.split(" · ")[0] || card.name)}
        </p>
        {row.components.length > 0 && (
          <p className="mt-[5px] text-muted">{cardLine(row.components, (card) => card.name)}</p>
        )}
      </div>
    </li>
  );
}

const SORTS: [PlayerSort, string, string][] = [
  ["initiative", "Initiative", "By initiative, the lowest first"],
  ["vp", "VP", "By victory points, the most first"],
];

/** The order that the viewer chose is kept in the browser, for each seat. */
const sortKey = (seat: string) => `ti4.players.sort.${seat}`;
function readSort(seat: string): PlayerSort | null {
  try {
    const value = localStorage.getItem(sortKey(seat));
    return value === "initiative" || value === "vp" ? value : null;
  } catch {
    return null;
  }
}
function writeSort(seat: string, sort: PlayerSort) {
  try {
    localStorage.setItem(sortKey(seat), sort);
  } catch {
    // Without storage the choice holds until the page is left.
  }
}

/**
 * The rows in an order: by initiative (a seat with no strategy card last) or by victory points,
 * the most first. A tie keeps the seat order.
 */
function sorted(rows: PlayerRowView[], sort: PlayerSort, initiative: string[], seats: string[]) {
  const key =
    sort === "initiative"
      ? (row: PlayerRowView) => {
          const index = initiative.indexOf(row.seat);
          return index < 0 ? initiative.length : index;
        }
      : (row: PlayerRowView) => -row.victoryPoints;
  return [...rows].sort((a, b) => key(a) - key(b) || seats.indexOf(a.seat) - seats.indexOf(b.seat));
}

/**
 * One card for each player, two columns, in the order of the phase or the one that the viewer
 * chose. ▶ and the frame show the turn. Each value is an icon and a number; the name of the value
 * is its tooltip, and the legend of the header lists the icons.
 */
export function PlayerTable({
  view,
  open,
  onToggle,
  onOpenSeat,
}: {
  view: PlayerTableView;
  open: boolean;
  onToggle: () => void;
  onOpenSeat: (seat: string) => void;
}) {
  const seats = Object.keys(useSeats());
  const viewer = view.rows.find((row) => row.isMe)?.seat ?? "spectator";
  const [chosen, setChosen] = useState<{ seat: string; sort: PlayerSort | null }>(() => ({
    seat: viewer,
    sort: readSort(viewer),
  }));
  // Another seat in the same page (a hotseat game) has its own choice.
  const choice = chosen.seat === viewer ? chosen.sort : readSort(viewer);
  const sort = choice ?? view.sort;
  const rows = choice ? sorted(view.rows, sort, view.initiative, seats) : view.rows;
  // While the task asks for a player, every card is in view.
  const shown = open || view.pick ? rows : rows.filter((row) => row.isMe || row.turn === "now");
  const hint = open ? "Show only you and the active player" : "Show all players";
  return (
    <section aria-label="Players" className="relative flex-none border-b border-line bg-board">
      {/* Under a finger the tap area of "?" reaches 10px past the button: the row keeps the room. */}
      <div className="flex items-center gap-1.5 px-2 pt-1 text-xs text-faint touch:pr-3">
        <button
          type="button"
          className="w-3 text-muted phone:hidden"
          aria-expanded={open}
          aria-label={hint}
          title={hint}
          onClick={onToggle}
        >
          {open ? "▾" : "▸"}
        </button>
        <strong className="font-semibold text-muted">Players</strong>
        {view.turn > 0 && <span className="font-strong text-text">Turn {view.turn}</span>}
        {!open && !view.pick && (
          <span>
            {shown.length} of {view.rows.length}
          </span>
        )}
        <span className="ml-auto">Sort</span>
        <span role="group" aria-label="Sort players" className="flex rounded-sm border border-line">
          {SORTS.map(([id, label, title]) => (
            <button
              key={id}
              type="button"
              title={title}
              aria-pressed={sort === id}
              onClick={() => {
                writeSort(viewer, id);
                setChosen({ seat: viewer, sort: id });
              }}
              className={cx(
                "px-1.5 leading-5",
                sort === id ? "bg-raised font-strong text-text" : "text-muted hover:text-text",
              )}
            >
              {label}
            </button>
          ))}
        </span>
        <Hint label="Icons of the player cards" width={260}>
          <ul className="grid gap-1">
            {LEGEND.map(([icon, label]) => (
              <li key={icon} className="flex items-center gap-2">
                <Icon name={icon} className="size-4 text-icon" />
                {label}
              </li>
            ))}
          </ul>
        </Hint>
      </div>
      <ul className="grid grid-cols-2 gap-1.5 p-1.5 leading-[1.35]">
        {shown.map((row, index) => (
          <Card
            key={row.seat}
            row={row}
            column={index % 2}
            pick={view.pick}
            onOpen={() => onOpenSeat(row.seat)}
          />
        ))}
      </ul>
      {view.freeCards.length > 0 && (
        <div className="border-t border-line px-2 py-1 text-xs text-muted">
          Free strategy cards:{" "}
          {view.freeCards
            .map((card) => `${card.number} ${card.name} +${card.tradeGoods} TG`)
            .join(" · ")}
        </div>
      )}
    </section>
  );
}
