import type { LogPhaseView, LogTurnView, LogView } from "../../model";
import { cx } from "../../ui";
import { SeatSymbol, useSeats } from "../context";

/**
 * The turns since the viewer's last turn, oldest first: one large row for each turn and each
 * phase that is no action phase, the turn that is played now last. A row says who did what and
 * how it ended; what happened in it is the sheet of its action, read-only.
 */
export function LogHistory({ view }: { view: LogView }) {
  // The start of an action phase has nothing to say: the turns that follow say their round.
  const entries = view.entries.filter((entry) => entry.kind === "turn" || entry.phase !== "action");
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
          {entry.kind === "turn" ? <TurnRow view={entry} /> : <PhaseRow view={entry} />}
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
 * turn of the viewer and the round and turn; then the action, then its outcomes.
 */
function TurnRow({ view }: { view: LogTurnView }) {
  const seat = useSeats()[view.seat];
  const name = seat ? seat.name || seat.faction : view.seat;
  // The player table says the name of the player and the faction; so does a row.
  const faction = seat?.name && seat.name !== seat.faction ? seat.faction : null;
  const full = faction ? `${name} · ${faction}` : name;
  return (
    <article
      aria-label={`Turn ${view.turn} · ${name} · ${view.label}`}
      className={cx(ROW, view.live ? "border-accent/60 bg-accent/[.06]" : "bg-surface/40")}
    >
      <div className="flex min-w-0 items-baseline gap-2">
        {view.live && <Live />}
        <SeatSymbol seat={view.seat} />
        <span className="min-w-0 truncate font-strong" title={full}>
          {name}
          {faction && <span className="font-normal text-muted"> {faction}</span>}
        </span>
        {view.mine && <span className="shrink-0 text-xs text-accent">You</span>}
        <span className="ml-auto shrink-0 text-xs text-muted tabular-nums">
          Round {view.round} · turn {view.turn}
        </span>
      </div>
      <div className="mt-0.5 truncate font-strong" title={view.label}>
        {view.label}
      </div>
      {view.summary && (
        <div className="mt-0.5 line-clamp-2 text-sm text-muted" title={view.summary}>
          {view.summary}
        </div>
      )}
    </article>
  );
}

/** A strategy, status or agenda phase: its name and what happened in it. */
function PhaseRow({ view }: { view: LogPhaseView }) {
  return (
    <section
      aria-label={view.label}
      className={cx(ROW, view.live ? "border-accent/60 bg-accent/[.06]" : "bg-well/60")}
    >
      <div className="flex min-w-0 items-baseline gap-2">
        {view.live && <Live />}
        <span className="shrink-0 font-strong">{view.label}</span>
      </div>
      {view.summary && (
        <div className="mt-0.5 line-clamp-2 text-sm text-muted" title={view.summary}>
          {view.summary}
        </div>
      )}
    </section>
  );
}
