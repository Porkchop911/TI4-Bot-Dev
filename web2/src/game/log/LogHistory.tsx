import type { LogPhaseView, LogTurnView, LogView } from "../../model";
import { cx } from "../../ui";
import { SeatSymbol, useDispatch, useSeats } from "../context";

/**
 * The turns since the viewer's last turn, newest first: one large row for each turn and each
 * phase that is no action phase, the turn that is played now on top. A row says who did what and
 * how it ended; what happened in it is the sheet of its action, read-only: a tactical, strategic
 * or component turn, and a phase, open it over the board.
 */
export function LogHistory({ view, open = null }: { view: LogView; open?: string | null }) {
  // The start of an action phase has nothing to say: the turns that follow say their round.
  const entries = view.entries
    .filter((entry) => entry.kind === "turn" || entry.phase !== "action")
    .reverse();
  if (entries.length === 0) {
    return (
      <p className="px-3 py-2 text-sm text-muted">
        {view.scope === "round" ? "No turns in this round yet" : "No turns since your last turn"}
      </p>
    );
  }
  return (
    <ol aria-label="Turns" className="space-y-1.5 px-2.5">
      {entries.map((entry) => (
        <li key={entry.id}>
          {entry.kind === "turn" ? (
            <TurnRow view={entry} open={entry.id === open} />
          ) : (
            <PhaseRow view={entry} open={entry.id === open} />
          )}
        </li>
      ))}
    </ol>
  );
}

const ROW = "rounded-md border border-line px-3 py-2";

/** "▶" before the turn or the phase that is played now, so that the tint is not its only sign. */
const Live = () => (
  <span className="shrink-0 text-accent" title="Played now">
    ▶
  </span>
);

/**
 * A turn: the seat (symbol, the name of the player and the faction, cut with "…"), "You" on a
 * turn of the viewer and the round and turn; then the action, then its outcomes. A tactical,
 * strategic or component turn is a control: it opens its action over the board, read-only
 * ("Open" while it is open). A pass opens nothing.
 */
function TurnRow({ view, open }: { view: LogTurnView; open: boolean }) {
  const dispatch = useDispatch();
  const seat = useSeats()[view.seat];
  const name = seat ? seat.name || seat.faction : view.seat;
  // The player table says the name of the player and the faction; so does a row.
  const faction = seat?.name && seat.name !== seat.faction ? seat.faction : null;
  const full = faction ? `${name} · ${faction}` : name;
  const opens =
    view.action === "tactical" || view.action === "strategic" || view.action === "component";
  const label = `Turn ${view.turn} · ${name} · ${view.label}`;
  const className = cx(
    ROW,
    "block w-full text-left",
    open
      ? "border-accent bg-accent/[.1]"
      : view.live
        ? "border-accent/60 bg-accent/[.06]"
        : "bg-surface/40",
    opens && "hover:bg-raised",
  );
  const content = (
    <>
      <span className="flex min-w-0 items-baseline gap-2">
        {view.live && <Live />}
        <SeatSymbol seat={view.seat} />
        <span className="min-w-0 truncate font-strong" title={full}>
          {name}
          {faction && <span className="font-normal text-muted"> {faction}</span>}
        </span>
        {view.mine && <span className="shrink-0 text-xs text-accent">You</span>}
        <span className="ml-auto shrink-0 text-xs text-muted tabular-nums">
          {open && <span className="mr-2 font-strong text-accent">Open</span>}
          Round {view.round} · turn {view.turn}
        </span>
      </span>
      <span className="mt-0.5 block truncate font-strong" title={view.label}>
        {view.label}
      </span>
      {view.summary && (
        <span className="mt-0.5 line-clamp-2 block text-sm text-muted" title={view.summary}>
          {view.summary}
        </span>
      )}
    </>
  );
  return opens ? (
    <button
      type="button"
      aria-label={label}
      aria-pressed={open}
      className={className}
      onClick={() =>
        dispatch(open ? { type: "closeTurn" } : { type: "inspectTurn", turn: view.id })
      }
    >
      {content}
    </button>
  ) : (
    <article aria-label={label} className={className}>
      {content}
    </article>
  );
}

/**
 * A strategy, status or agenda phase: its name and what happened in it. It is a control: it opens
 * what the seats did in it over the board, read-only ("Open" while it is open).
 */
function PhaseRow({ view, open }: { view: LogPhaseView; open: boolean }) {
  const dispatch = useDispatch();
  return (
    <button
      type="button"
      aria-label={view.label}
      aria-pressed={open}
      className={cx(
        ROW,
        "block w-full text-left hover:bg-raised",
        open
          ? "border-accent bg-accent/[.1]"
          : view.live
            ? "border-accent/60 bg-accent/[.06]"
            : "bg-well/60",
      )}
      onClick={() =>
        dispatch(open ? { type: "closeTurn" } : { type: "inspectTurn", turn: view.id })
      }
    >
      <span className="flex min-w-0 items-baseline gap-2">
        {view.live && <Live />}
        <span className="shrink-0 font-strong">{view.label}</span>
        {open && <span className="ml-auto shrink-0 text-xs font-strong text-accent">Open</span>}
      </span>
      {view.summary && (
        <span className="mt-0.5 line-clamp-2 block text-sm text-muted" title={view.summary}>
          {view.summary}
        </span>
      )}
    </button>
  );
}
