import type {
  LogDiceView,
  LogGroupView,
  LogLineView,
  LogPhaseView,
  LogTurnView,
  LogView,
} from "../../model";
import { Die, cx } from "../../ui";
import { SeatSymbol, useSeats } from "../context";

/**
 * The turns since the viewer's last turn, oldest first: one dense row for each turn and each
 * phase, the overview. A row opens to the details of its entry, in groups. `open` is what the
 * viewer opened, by the id of an entry.
 */
export function LogHistory({
  view,
  open,
  onToggle,
}: {
  view: LogView;
  open: ReadonlySet<string>;
  onToggle: (id: string) => void;
}) {
  if (view.entries.length === 0) {
    return (
      <p className="px-3 py-2 text-sm text-muted">
        {view.scope === "round" ? "No turns in this round yet" : "No turns since your last turn"}
      </p>
    );
  }
  return (
    <ol aria-label="Turns" className="border-t border-line">
      {view.entries.map((entry) => (
        <li key={entry.id}>
          {entry.kind === "turn" ? (
            <TurnRow view={entry} open={open.has(entry.id)} onToggle={() => onToggle(entry.id)} />
          ) : (
            <PhaseRow view={entry} open={open.has(entry.id)} onToggle={() => onToggle(entry.id)} />
          )}
        </li>
      ))}
    </ol>
  );
}

/** "▸" closed, "▾" open: the sign of every group of the Log and the sheets. */
const Toggle = ({ open }: { open: boolean }) => <span aria-hidden="true">{open ? "▾" : "▸"}</span>;

/**
 * A turn in one line: the toggle, its number in the round, the seat (symbol and name, cut with
 * "…"), the action and its outcome, muted and cut with "…". A phone has the outcome in a second
 * line.
 */
function TurnRow({
  view,
  open,
  onToggle,
}: {
  view: LogTurnView;
  open: boolean;
  onToggle: () => void;
}) {
  const seat = useSeats()[view.seat];
  const name = seat ? seat.name || seat.faction : view.seat;
  // The player table says the name of the player and the faction; so does a row.
  const faction = seat?.name && seat.name !== seat.faction ? seat.faction : null;
  const full = faction ? `${name} · ${faction}` : name;
  // A pass has nothing to open.
  const details = view.groups.length > 0;
  const row = (
    <>
      {details ? <Toggle open={open} /> : <span aria-hidden="true" />}
      <span
        className="text-right text-xs text-muted tabular-nums"
        title={`Round ${view.round} · turn ${view.turn}`}
      >
        {view.turn}
      </span>
      <span className="flex min-w-0 items-baseline gap-1.5">
        {view.live && (
          <span className="text-accent" title="Played now">
            ▶
          </span>
        )}
        <SeatSymbol seat={view.seat} />
        <span className="min-w-0 truncate phone:whitespace-normal" title={full}>
          {name}
          {faction && <span className="text-muted"> {faction}</span>}
        </span>
        {view.mine && <span className="shrink-0 text-xs font-normal text-accent">You</span>}
      </span>
      <span className="min-w-0 truncate font-strong phone:whitespace-normal" title={view.label}>
        {view.label}
      </span>
      <span
        className="min-w-0 truncate text-muted phone:col-span-2 phone:col-start-3 phone:text-xs phone:whitespace-normal"
        title={view.summary || undefined}
      >
        {view.summary}
      </span>
    </>
  );
  const grid = cx(
    "grid w-full min-w-0 items-baseline gap-x-2 border-b border-line px-3 py-1 text-left text-sm",
    "grid-cols-[0.75rem_1.5rem_minmax(0,9rem)_auto_minmax(0,1fr)]",
    "phone:grid-cols-[0.75rem_1.25rem_minmax(0,7.5rem)_minmax(0,1fr)]",
    view.live && "bg-accent/[.06]",
  );
  return (
    <article aria-label={`Turn ${view.turn} · ${name} · ${view.label}`}>
      {details ? (
        <button
          type="button"
          aria-expanded={open}
          onClick={onToggle}
          className={cx(grid, "hover:bg-surface/60")}
        >
          {row}
        </button>
      ) : (
        <div className={grid}>{row}</div>
      )}
      {open && details && <Groups groups={view.groups} />}
    </article>
  );
}

/**
 * A phase that is no action phase: a band with its name and what happened in it, which opens to
 * its groups. The start of an action phase is a band with nothing to open: a divider.
 */
function PhaseRow({
  view,
  open,
  onToggle,
}: {
  view: LogPhaseView;
  open: boolean;
  onToggle: () => void;
}) {
  const details = view.groups.length > 0;
  const band = cx(
    "flex w-full min-w-0 items-baseline gap-2 border-b border-line bg-surface/60 px-3 py-1 text-left text-xs",
  );
  const content = (
    <>
      {details && <Toggle open={open} />}
      <span className="shrink-0 font-strong">{view.label}</span>
      {view.summary && <span className="min-w-0 truncate text-muted">{view.summary}</span>}
    </>
  );
  return (
    <section aria-label={view.label}>
      {details ? (
        <button
          type="button"
          aria-expanded={open}
          onClick={onToggle}
          className={cx(band, "hover:bg-surface")}
        >
          {content}
        </button>
      ) : (
        <div className={band}>{content}</div>
      )}
      {open && details && <Groups groups={view.groups} />}
    </section>
  );
}

/** The details of an entry: each group is its title and its lines, dense. */
function Groups({ groups }: { groups: LogGroupView[] }) {
  return (
    <div className="space-y-1.5 border-b border-line bg-well/40 py-1.5 pr-3 pl-8 phone:pl-6">
      {groups.map((group) => (
        <section key={group.title} aria-label={group.title}>
          <h4 className="text-xs font-strong text-muted">{group.title}</h4>
          <ul>
            {group.lines.map((line, index) => {
              const before = group.lines[index - 1];
              return (
                <Line
                  key={index}
                  line={line}
                  group={group.title}
                  again={!!line.dice && !!before?.dice && before.text === line.text}
                />
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}

/**
 * One line: the symbol of its seat, the text, and the dice of a roll. The title of the group is
 * not said again at the start of a line ("Space combat · round 2" in Space combat is "Round 2"),
 * and a roll of the same round as the line before it does not say its round again (`again`): the
 * text keeps its room, so that the dice of a round are under each other.
 */
function Line({ line, group, again }: { line: LogLineView; group: string; again: boolean }) {
  const prefix = `${group} · `;
  const text = line.text.startsWith(prefix)
    ? capitalize(line.text.slice(prefix.length))
    : line.text;
  return (
    <li className="grid grid-cols-[1em_minmax(0,1fr)] items-baseline gap-x-1.5 py-px text-sm">
      {line.seat ? <SeatSymbol seat={line.seat} /> : <span />}
      <span className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5">
        <span className={cx("min-w-0", again && "invisible")}>{text}</span>
        {line.dice && <Dice view={line.dice} />}
      </span>
    </li>
  );
}

const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** The faces of a roll: a face that hits is bold, in its frame; then what it needed and did. */
function Dice({ view }: { view: LogDiceView }) {
  return (
    <span className="flex flex-wrap items-center gap-[3px]">
      {view.faces.map((face, index) => (
        <Die key={index} roll={face} hit={view.hitsOn === null ? undefined : face >= view.hitsOn} />
      ))}
      <span className="ml-1 text-xs text-muted tabular-nums">
        {[view.hitsOn !== null && `hits on ${view.hitsOn}`, hitWord(view.hits)]
          .filter(Boolean)
          .join(" · ")}
      </span>
    </span>
  );
}

const hitWord = (hits: number) => (hits === 0 ? "no hit" : hits === 1 ? "1 hit" : `${hits} hits`);
