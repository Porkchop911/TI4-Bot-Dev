import { useState } from "react";
import type { OriginView } from "../../../model";
import { plural } from "../../../model";
import { Button, Icon, cx } from "../../../ui";
import { SystemDetails } from "../../board/SystemDetails";
import { useDispatch } from "../../context";
import { Stays } from "./parts";
import { CargoRow, OriginTokens, loadSource } from "./Tokens";

/**
 * The fleet of one system with its controls, on the board, at the system that the player chose
 * there. On a phone it is a sheet over the lower part of the map.
 */
export function OriginSheet({ view }: { view: OriginView }) {
  const dispatch = useDispatch();
  // The kind of unit that a slot loads. It is a choice of the hand, not of the game.
  const [chosen, setChosen] = useState<string | null>(null);
  const source = loadSource(view, chosen);
  return (
    <div
      role="group"
      aria-label={`Move from ${view.label}`}
      className="absolute bottom-2.5 left-2.5 z-2 max-h-[62%] w-[430px] overflow-auto rounded-lg border border-line bg-surface/97 text-sm phone:inset-x-0 phone:bottom-0 phone:z-3 phone:max-h-[70%] phone:w-auto phone:rounded-none phone:border-x-0 phone:border-b-0 phone:bg-surface"
    >
      {/* The heading and the cargo row stay in view: only the ships scroll under them. */}
      <div className="sticky top-0 z-1 bg-surface">
        <div className="flex items-center gap-2 border-b border-line py-0.5 pr-1 pl-3 phone:pl-2.5">
          <h3 className="truncate text-base font-semibold" title={view.label}>
            {view.label}
          </h3>
          <span className="min-w-0 flex-1 truncate text-xs text-muted">
            {plural(view.away, "system")} away
            {view.leaves && <strong className="font-mid text-text"> · {view.leaves}</strong>}
          </span>
          {/* Takes back what leaves this system, and with it the mark of the system on the map. */}
          <Button
            size="sm"
            tone="quiet"
            disabled={!view.handled}
            onClick={() => dispatch({ type: "resetOrigin", system: view.system })}
          >
            Reset
          </Button>
          {/* Closes the fleet and marks the system on the map, also when nothing leaves it. */}
          <button
            type="button"
            aria-label="Done with this system"
            title="Done with this system · closes the fleet and marks the system on the map"
            aria-pressed={view.handled}
            className={cx(
              "grid size-7 flex-none place-items-center rounded-md border touch:size-9",
              view.handled
                ? "border-accent bg-accent/15 text-accent"
                : "border-line-strong text-icon hover:bg-raised",
            )}
            onClick={() => dispatch({ type: "markOrigin", system: view.system })}
          >
            <Icon name="check" />
          </button>
        </div>
        <CargoRow origin={view} source={source} onSelect={setChosen} />
      </div>
      <OriginTokens origin={view} source={source} editing />
      <Stays origin={view} />
      {/* What is in the system, as the inspector has it. It scrolls with the ships. */}
      {view.details && (
        <SystemDetails
          view={view.details}
          className="border-t border-line px-3 pb-2.5 pt-1.5 text-xs phone:px-2.5"
        />
      )}
    </div>
  );
}

/** In the panel: what leaves each system. The controls are on the board. */
export function MovementSummary({ origins }: { origins: OriginView[] }) {
  const dispatch = useDispatch();
  return (
    <div className="rounded-lg border border-line">
      <div className="border-b border-line px-3.5 py-1.5 text-xs text-muted phone:px-2.5">
        Choose a system on the map to move its ships.
      </div>
      {origins.map((origin) => (
        <button
          key={origin.system}
          type="button"
          className="row flex min-h-8 w-full flex-wrap items-baseline gap-x-2.5 px-3.5 py-1 text-left hover:bg-raised [.row+&]:border-t [.row+&]:border-line/50 phone:px-2.5"
          onClick={() => dispatch({ type: "inspectSystem", system: origin.system })}
        >
          <strong className="font-mid">{origin.label}</strong>
          <span className="text-xs text-muted">
            {origin.leaves ? (
              <>
                <strong className="font-mid text-text">{origin.leaves}</strong>
                {" · "}
                {origin.ships
                  .filter((ship) => ship.count)
                  .map((ship) => `${ship.count} ${ship.name}`)
                  .join(", ")}
              </>
            ) : origin.handled ? (
              "✓ Nothing leaves"
            ) : (
              "Nothing staged"
            )}
          </span>
          {origin.warning && <span className="text-xs text-orange">⚠ {origin.warning}</span>}
        </button>
      ))}
    </div>
  );
}
