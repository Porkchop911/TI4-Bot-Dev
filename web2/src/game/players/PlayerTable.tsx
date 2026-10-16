import type { ReactNode } from "react";
import type {
  ComponentCardView,
  PlayerPickView,
  PlayerRowView,
  PlayerTableView,
} from "../../model";
import { Icon, cx, type IconName } from "../../ui";
import { SeatSymbol, useDispatch, useSeat, useSeats } from "../context";

/** "Agent exhausted · Commander locked": the cards by name, with the state of one that is not ready. */
const cardLine = (cards: ComponentCardView[], name: (card: ComponentCardView) => string) =>
  cards
    .map((card) => `${name(card)}${card.state && card.state !== "ready" ? ` ${card.state}` : ""}`)
    .join(" · ");

/** The sign of a seat that no person plays, as the lobby has it. */
const PLAYED = { rule: "⚙", random: "⚄", model: "✦" };

/** A symbol that stands for a word: the word is its accessible name and its tooltip. */
function Sign({ name, label, className }: { name: IconName; label: string; className?: string }) {
  return (
    <span role="img" aria-label={label} title={label} className="inline-flex flex-none">
      <Icon name={name} className={cx("size-3.5", className)} />
    </span>
  );
}

/**
 * One value of a seat: its icon and its value. The full name is the tooltip and the accessible
 * name of the icon. A fixed width keeps the values of the cards under each other.
 */
function Stat({
  icon,
  label,
  title = label,
  width,
  children,
}: {
  icon: IconName;
  label: string;
  /** The tooltip, when it says more than the name: the format of the value. */
  title?: string;
  width: string;
  children: ReactNode;
}) {
  return (
    <span
      title={title}
      className={cx("inline-flex flex-none items-center gap-1 phone:w-auto", width)}
    >
      <span role="img" aria-label={label} className="inline-flex">
        <Icon name={icon} className="size-3.5 text-faint" />
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
        <Icon name={up ? "token" : "tokenDown"} className="size-3 text-faint" />
      </span>
      {count}
    </span>
  );
}

function Card({
  row,
  column,
  pick,
  signs,
  onOpen,
}: {
  row: PlayerRowView;
  /** The column of the grid: the hover sheet goes left of the cards, over the board. */
  column: number;
  pick: PlayerPickView | null;
  /** A seat of the table has a sign: every card has its place. */
  signs: boolean;
  onOpen: () => void;
}) {
  const seat = useSeat(row.seat);
  const dispatch = useDispatch();
  // While the task asks for a player, a click on a card chooses that player.
  const option = pick?.seats[row.seat];
  const allowed = !!option && option.reason === null;
  const chosen = !!pick?.chosen.includes(row.seat);
  const act = pick ? () => allowed && dispatch({ type: "pickSeat", seat: row.seat }) : onOpen;
  const quiet = row.passed ? "text-faint" : undefined;
  const [tactic, fleet, strategy] = row.tokens;
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: a click anywhere on the card is a larger target; the name button is the control for the keyboard
    <li
      onClick={act}
      className={cx(
        "group relative rounded-md border border-line px-1.5 py-1 focus-within:bg-white/5",
        (!pick || allowed) && "cursor-pointer hover:bg-white/5",
        pick && !allowed && "opacity-45",
        allowed && !chosen && "shadow-[inset_2px_0_0_var(--color-text)]",
        chosen && "shadow-[inset_2px_0_0_var(--color-gold)]",
        row.isMe && !pick && "bg-cyan/[.07]",
        row.passed ? "text-faint" : row.isMe ? "text-text" : "text-muted",
      )}
    >
      <div className="flex h-5 min-w-0 items-center gap-[5px]">
        <span className="w-3 flex-none text-center text-xs text-muted">
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
              : `${seat.name}, ${seat.faction}${row.turn === "now" ? ", active player" : ""}${row.speaker ? ", speaker" : ""}${row.passed ? ", passed" : ""}. Open player sheet.`
          }
          className="flex min-w-0 flex-1 items-center gap-[5px] overflow-hidden text-left text-xs focus-visible:outline-offset-1"
        >
          <SeatSymbol seat={row.seat} />
          {signs &&
            (seat.played ? (
              <span
                className="w-3 flex-none text-center text-muted"
                role="img"
                aria-label={seat.played.word}
                title={seat.played.word}
              >
                {PLAYED[seat.played.kind]}
              </span>
            ) : (
              <span className="w-3 flex-none" />
            ))}
          <span
            className={cx("max-w-[120px] flex-none truncate font-strong", quiet ?? "text-text")}
            title={seat.name}
          >
            {seat.name}
          </span>
          {/* While the cards are the picker, a card has what the choice gives in this place. */}
          {!pick && (
            <span className={cx("min-w-0 truncate", quiet ?? "text-muted")} title={seat.faction}>
              {seat.faction}
            </span>
          )}
          {option?.reason && (
            <span className="min-w-0 truncate text-2xs text-red" title={option.reason}>
              {option.reason}
            </span>
          )}
          {option?.note && !option.reason && (
            <span className="flex-none text-2xs whitespace-nowrap text-text tabular-nums">
              {option.note}
            </span>
          )}
          {chosen && <span className="flex-none text-2xs font-bold text-gold">Selected</span>}
        </button>
        {row.speaker && <Sign name="speaker" label="Speaker" className="text-gold" />}
        {row.passed && <Sign name="passed" label="Passed" className="text-faint" />}
        {row.strategyCards.length > 0 && (
          <span
            role="group"
            aria-label="Strategy cards"
            className="flex flex-none items-center gap-1.5 text-xs"
          >
            {row.strategyCards.map((card) => (
              <span
                key={card.number}
                className={cx("inline-flex items-center gap-1", card.used && "opacity-60")}
                title={`Strategy card ${card.number} ${card.name}${card.used ? " · used" : ""}`}
              >
                <span className="inline-flex h-4 min-w-4 items-center justify-center rounded-sm border border-line-strong text-2xs font-strong text-text tabular-nums">
                  {card.number}
                </span>{" "}
                {card.name}
                {card.used ? " ✓" : ""}
              </span>
            ))}
          </span>
        )}
        <span
          title="Victory points of 10"
          className="inline-flex flex-none items-center gap-1 rounded-full bg-raised px-1.5 text-xs font-strong text-text tabular-nums"
        >
          <span role="img" aria-label="Victory points" className="inline-flex">
            <Icon name="star" className="size-3 text-gold" />
          </span>
          <span>{row.victoryPoints}</span>
        </span>
      </div>
      <div className="mt-0.5 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 pl-[17px] phone:gap-x-2.5 phone:pl-0 text-xs whitespace-nowrap tabular-nums">
        <Stat icon="resources" label="Resources" title="Resources: ready / total" width="w-[56px]">
          {row.resources.join("/")}
        </Stat>
        <Stat icon="influence" label="Influence" title="Influence: ready / total" width="w-[56px]">
          {row.influence.join("/")}
        </Stat>
        <Stat icon="planet" label="Planets" width="w-[34px]">
          {row.planets}
        </Stat>
        <Stat icon="coin" label="Trade goods" width="w-[34px]">
          {row.tradeGoods}
        </Stat>
        <Stat icon="commodity" label="Commodities" title="Commodities / limit" width="w-[44px]">
          {row.commodities.join("/")}
        </Stat>
        <span
          title="Command tokens: tactic · fleet · strategy"
          className="inline-flex w-[92px] flex-none items-center gap-1 phone:w-auto"
        >
          {/* The dots keep the order of the command sheet in the text: "3 · 3 · 2". */}
          <Pool label="Tactic pool" count={tactic} />
          <span className="text-faint">{" · "}</span>
          <Pool label="Fleet pool" count={fleet} up />
          <span className="text-faint">{" · "}</span>
          <Pool label="Strategy pool" count={strategy} />
        </span>
        <Stat icon="card" label="Action cards" title="Action cards in hand" width="w-[34px]">
          {row.actionCards}
        </Stat>
        <Stat
          icon="secret"
          label="Secret objectives"
          title="Secret objectives: scored / held"
          width="w-[40px]"
        >
          {row.secrets.join("/")}
        </Stat>
      </div>
      <div
        className={cx(
          "absolute top-0 z-20 hidden w-[min(340px,90vw)] cursor-default rounded-lg border border-line bg-raised px-3 py-2.5 text-xs leading-[1.45] font-normal whitespace-normal text-text shadow-[0_10px_30px_#0008]",
          // Left of the cards: over the board, never over another card.
          column === 0 ? "right-[calc(100%+10px)]" : "right-[calc(200%+14px)]",
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
          <Sign name="tech" label="Technologies" className="mr-1 align-[-2px] text-faint" />
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

/**
 * One card for each player, two columns on desktop, in the order of the selector. ▶ shows the
 * turn. Each value is an icon and a number; the name of the value is its tooltip.
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
  // While the task asks for a player, every card is in view.
  const shown =
    open || view.pick ? view.rows : view.rows.filter((row) => row.isMe || row.turn === "now");
  const hint = open ? "Show only you and the active player" : "Show all players";
  const signs = Object.values(useSeats()).some((seat) => seat.played);
  return (
    <section aria-label="Players" className="relative flex-none border-b border-line bg-board">
      <div className="flex items-center gap-1 px-1.5 pt-[3px] text-2xs font-semibold text-faint">
        <button
          type="button"
          className="w-3 text-xs text-muted phone:hidden"
          aria-expanded={open}
          aria-label={hint}
          title={hint}
          onClick={onToggle}
        >
          {open ? "▾" : "▸"}
        </button>
        {open ? "Players" : `Players · ${shown.length} of ${view.rows.length}`}
      </div>
      <ul className="grid grid-cols-2 gap-1 p-1 leading-[1.3] phone:grid-cols-1">
        {shown.map((row, index) => (
          <Card
            key={row.seat}
            row={row}
            column={index % 2}
            pick={view.pick}
            signs={signs}
            onOpen={() => onOpenSeat(row.seat)}
          />
        ))}
      </ul>
      {view.freeCards.length > 0 && (
        <div className="border-t border-line px-2 py-[3px] text-xs text-muted">
          Free strategy cards:{" "}
          {view.freeCards
            .map((card) => `${card.number} ${card.name} +${card.tradeGoods} TG`)
            .join(" · ")}
        </div>
      )}
    </section>
  );
}
