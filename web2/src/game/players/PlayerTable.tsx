import type {
  ComponentCardView,
  PlayerPickView,
  PlayerRowView,
  PlayerTableView,
} from "../../model";
import { cx } from "../../ui";
import { SeatSymbol, useDispatch, useSeat, useSeats } from "../context";

// Full names: the table is wide at the design size (1920×1080). The title explains the format.
const COLUMNS = [
  ["VP", "Victory points of 10"],
  ["Strategy cards", "Strategy cards · ✓ used · two for each player in a game of four or fewer"],
  ["Resources", "Resources: ready / total"],
  ["Influence", "Influence: ready / total"],
  ["Trade goods", "Trade goods"],
  ["Commodities", "Commodities / limit"],
  ["Tactic · Fleet · Strategy", "Command tokens: tactic · fleet · strategy"],
  ["Action cards", "Action cards in hand"],
  ["Secrets", "Secret objectives: scored / held"],
  ["Planets", "Planets"],
] as const;

const CELL = "px-1.5 py-0.5 text-right whitespace-nowrap tabular-nums phone:px-2.5 phone:py-2.5";
// On a phone the values scroll sideways and the name stays in view.
const NAME =
  "w-[99%] max-w-0 pr-[5px] text-left phone:sticky phone:left-[18px] phone:z-1 phone:w-auto phone:max-w-none phone:min-w-[132px] phone:bg-board";

/** "Agent exhausted · Commander locked": the cards by name, with the state of one that is not ready. */
const cardLine = (cards: ComponentCardView[], name: (card: ComponentCardView) => string) =>
  cards
    .map((card) => `${name(card)}${card.state && card.state !== "ready" ? ` ${card.state}` : ""}`)
    .join(" · ");

/** The sign of a seat that no person plays, as the lobby has it. */
const PLAYED = { rule: "⚙", random: "⚄" };

function Row({
  row,
  first,
  pick,
  signs,
  onOpen,
}: {
  row: PlayerRowView;
  first: boolean;
  pick: PlayerPickView | null;
  /** A seat of the table has a sign: every row has its column. */
  signs: boolean;
  onOpen: () => void;
}) {
  const seat = useSeat(row.seat);
  const dispatch = useDispatch();
  // While the task asks for a player, a click on a row chooses that player.
  const option = pick?.seats[row.seat];
  const allowed = !!option && option.reason === null;
  const chosen = !!pick?.chosen.includes(row.seat);
  const act = pick ? () => allowed && dispatch({ type: "pickSeat", seat: row.seat }) : onOpen;
  const values = [
    <strong key="vp" className="text-text">
      {row.victoryPoints}
    </strong>,
    row.strategyCards.length
      ? row.strategyCards.map((card, index) => (
          <span
            key={card.number}
            className={cx(card.used && "opacity-60")}
            title={`${card.number} ${card.name}${card.used ? " · used" : ""}`}
          >
            {index > 0 && " · "}
            {card.number} {card.name}
            {card.used ? " ✓" : ""}
          </span>
        ))
      : "–",
    row.resources.join("/"),
    row.influence.join("/"),
    row.tradeGoods,
    row.commodities.join("/"),
    row.tokens.join(" · "),
    row.actionCards,
    row.secrets.join("/"),
    row.planets,
  ];
  const quiet = row.passed ? "text-faint" : undefined;
  return (
    <tr
      onClick={act}
      className={cx(
        "group relative focus-within:bg-white/5",
        (!pick || allowed) && "cursor-pointer hover:bg-white/5",
        pick && !allowed && "opacity-45",
        allowed && !chosen && "shadow-[inset_2px_0_0_var(--color-text)]",
        chosen && "shadow-[inset_2px_0_0_var(--color-gold)]",
        row.isMe && !pick && "bg-cyan/[.07]",
        !first && "[&>*]:border-t [&>*]:border-white/[.04]",
      )}
    >
      <td className="w-[18px] pl-1.5 text-center text-muted phone:sticky phone:left-0 phone:z-1 phone:bg-board">
        {row.turn === "now" ? (
          <span className="text-accent" title="Active player">
            ▶
          </span>
        ) : row.turn === "next" ? (
          <span title="Next player">›</span>
        ) : null}
      </td>
      <th scope="row" className={cx(NAME, "py-0.5 font-normal")}>
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
          className="flex w-full min-w-0 items-baseline gap-[5px] overflow-hidden text-left text-xs focus-visible:outline-offset-1"
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
          <span className={cx("w-[84px] flex-none truncate font-strong", quiet)} title={seat.name}>
            {seat.name}
          </span>
          {/* While the table is the picker, the row has what the choice gives in this place. */}
          {!pick && (
            <span className={cx("min-w-0 truncate phone:hidden", quiet ?? "text-muted")}>
              {seat.faction}
            </span>
          )}
          {row.speaker && (
            <span className="min-w-0 truncate text-2xs text-gold" title="Speaker">
              Speaker
            </span>
          )}
          {row.passed && <span className="text-2xs text-faint">passed</span>}
          {option?.reason && (
            <span className="flex-none text-2xs whitespace-nowrap text-red">{option.reason}</span>
          )}
          {option?.note && !option.reason && (
            <span className="flex-none text-2xs whitespace-nowrap text-text tabular-nums">
              {option.note}
            </span>
          )}
          {chosen && <span className="flex-none text-2xs font-bold text-gold">Selected</span>}
        </button>
        <div
          className={cx(
            "absolute top-0 right-[calc(100%+6px)] z-20 hidden w-[min(340px,90vw)] cursor-default rounded-lg border border-line bg-raised px-3 py-2.5 text-xs leading-[1.45] font-normal whitespace-normal text-text shadow-[0_10px_30px_#0008]",
            // While the table is the picker, the sheet of a player does not cover the board.
            // A phone opens the sheet of the player with a tap; it has the same text.
            !pick && "group-focus-within:block group-hover:block phone:hidden!",
          )}
        >
          <strong>
            <SeatSymbol seat={row.seat} /> {seat.name} · {seat.faction}
          </strong>
          {/* The names only. The sheet of the player has the printed texts. */}
          <p className="mt-[5px]">{cardLine(row.abilities, (card) => card.name)}</p>
          <p className="mt-[5px] text-muted">{row.technologies.join(" · ")}</p>
          <p className="mt-[5px] text-muted">
            {cardLine(row.leaders, (card) => card.kind.split(" · ")[0] || card.name)}
          </p>
          {row.components.length > 0 && (
            <p className="mt-[5px] text-muted">{cardLine(row.components, (card) => card.name)}</p>
          )}
        </div>
      </th>
      {values.map((value, column) => (
        <td
          key={column}
          className={cx(CELL, row.passed ? "text-faint" : row.isMe ? "text-text" : "text-muted")}
        >
          {value}
        </td>
      ))}
    </tr>
  );
}

/** One row for each player, one column for each value. Rows do not move; ▶ and › show the turn. */
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
  // While the task asks for a player, every row is in view.
  const shown =
    open || view.pick ? view.rows : view.rows.filter((row) => row.isMe || row.turn === "now");
  const hint = open ? "Show only you and the active player" : "Show all players";
  const signs = Object.values(useSeats()).some((seat) => seat.played);
  const head = "border-b border-line py-[3px] text-2xs font-semibold text-faint";
  return (
    <section
      aria-label="Players"
      className="relative flex-none border-b border-line bg-board phone:w-max phone:min-w-full"
    >
      <table className="w-full border-collapse text-xs leading-[1.3]">
        <thead>
          <tr>
            <td
              className={cx(
                head,
                "w-[18px] pl-1.5 text-center phone:sticky phone:left-0 phone:z-1 phone:bg-board",
              )}
            >
              <button
                type="button"
                className="px-0.5 text-xs text-muted phone:invisible"
                aria-expanded={open}
                aria-label={hint}
                title={hint}
                onClick={onToggle}
              >
                {open ? "▾" : "▸"}
              </button>
            </td>
            <th scope="col" className={cx(head, NAME)}>
              {open ? "Player" : `Player · ${shown.length} of ${view.rows.length}`}
            </th>
            {COLUMNS.map(([label, title]) => (
              <th
                key={label}
                scope="col"
                className={cx(head, "px-1.5 text-right whitespace-nowrap")}
              >
                <abbr title={title} className="cursor-help no-underline">
                  {label}
                </abbr>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((row, index) => (
            <Row
              key={row.seat}
              row={row}
              first={index === 0}
              pick={view.pick}
              signs={signs}
              onOpen={() => onOpenSeat(row.seat)}
            />
          ))}
        </tbody>
      </table>
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
