import type { SpaceOddsView } from "../../model";
import { unitName } from "../../model";
import { Hint, Icon, cx } from "../../ui";
import { SeatSymbol, useSeat } from "../context";

const percent = (value: number | null) => (value === null ? "…" : `${value}%`);

/**
 * The odds of the space combat that the staged movement starts, as `ti4-calc` computes them in the
 * page: Win, Draw and Loss of the moving seat, then the ships of each side that are expected to
 * survive, against the ships that fight. While the answer for this movement is on its way, the
 * numbers of the last one are dimmed.
 */
export function SpaceOdds({ view, className }: { view: SpaceOddsView; className?: string }) {
  const defender = useSeat(view.defender);
  return (
    <section
      aria-label="Space combat"
      aria-busy={view.pending}
      className={cx("flex min-w-0 flex-col gap-0.5 text-sm", className)}
    >
      <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5">
        <span className="flex min-w-0 items-baseline gap-1.5">
          <strong className="font-mid whitespace-nowrap">Space combat</strong>{" "}
          <span className="text-muted">·</span> <SeatSymbol seat={view.defender} />{" "}
          <span className="truncate text-muted" title={defender?.name}>
            {defender?.faction ?? view.defender}
          </span>
        </span>{" "}
        {/* The numbers and their hint go to the next line together when the name needs the room. */}
        <span className="ml-auto flex items-baseline gap-x-3">
          <span className={cx("whitespace-nowrap tabular-nums", view.pending && "opacity-50")}>
            Win <strong className="text-md font-strong">{percent(view.win)}</strong>
            <span className="text-muted"> · </span>Draw {percent(view.draw)}
            <span className="text-muted"> · </span>Loss {percent(view.loss)}
          </span>{" "}
          {view.pending && <span title="Computing">…</span>}
          <Hint label="About the odds">
            Simulated{view.rollouts > 0 ? ` · ${view.rollouts} rollouts` : ""}. Draw: both fleets
            are destroyed, or the combat does not end. Survivors: the ships that are expected to be
            left, of the ships that fight. Counted: the ships of both sides, the damaged ones,
            ANTI-FIGHTER BARRAGE and the SPACE CANNON of both sides before the combat. Left out:
            action cards, promissory notes, leaders and retreats. Each side takes a hit with SUSTAIN
            DAMAGE first, then loses its cheapest ship.
          </Hint>
        </span>
      </div>
      {view.sides.map((side) => (
        <SideLine key={side.seat} side={side} pending={view.pending} />
      ))}
    </section>
  );
}

function SideLine({ side, pending }: { side: SpaceOddsView["sides"][number]; pending: boolean }) {
  const seat = useSeat(side.seat);
  return (
    <div
      role="group"
      className="flex min-w-0 items-center gap-x-3 gap-y-0.5"
      aria-label={`Survivors of ${seat?.faction ?? side.seat}`}
    >
      <SeatSymbol seat={side.seat} />{" "}
      <span className={cx("flex flex-wrap gap-x-3 gap-y-0.5", pending && "opacity-50")}>
        {side.ships.map((ship) => (
          <span
            key={ship.unit}
            className="inline-flex items-center gap-1 whitespace-nowrap tabular-nums"
            title={unitName(ship.unit, ship.fielded)}
          >
            <Icon name={ship.unit} className="size-4 text-icon" />
            {ship.expected ?? "…"} / {ship.fielded}{" "}
          </span>
        ))}
      </span>
    </div>
  );
}
