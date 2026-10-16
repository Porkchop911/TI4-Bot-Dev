import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { BoardView, InspectorView } from "../../model";
import { Button, CloseButton, Hint, Icon, Segmented, cx, usePhone, type IconName } from "../../ui";
import { useDispatch } from "../context";
import { SpaceOdds } from "../combat/SpaceOdds";
import { GalaxyMap, type MapViewId } from "./GalaxyMap";
import { LandingSheet } from "../steps/invasion/LandingSheet";
import { ProductionSheet } from "../steps/production/ProductionSheet";
import { OriginSheet } from "../steps/movement/OriginSheet";
import { OriginTokens } from "../steps/movement/Tokens";
import { PaymentBar } from "./PaymentBar";
import { SystemDetails } from "./SystemDetails";
import { useCamera } from "./useCamera";

const MAP_VIEWS: { id: MapViewId; label: string; icon: IconName; legend: string }[] = [
  { id: "standard", label: "Standard", icon: "coin", legend: "resources / influence per planet" },
  {
    id: "owner",
    label: "Owner",
    icon: "flag",
    legend: "symbol in a planet: its owner · symbol + number: ships of that player",
  },
  { id: "space", label: "Space combat", icon: "cruiser", legend: "ships · average hits per round" },
  {
    id: "ground",
    label: "Ground combat",
    icon: "infantry",
    legend: "ground forces per planet · ◈ Planetary Shield",
  },
  { id: "tech", label: "Tech skips", icon: "tech", legend: "B G Y R technology specialty" },
  {
    id: "explore",
    label: "Exploration",
    icon: "target",
    legend: "C H I planet type: cultural, hazardous, industrial · ? unexplored · + attachment",
  },
];

/** What is in one system: fleets, planets, and what the open action plans there. */
function SystemInspector({ view }: { view: InspectorView }) {
  const dispatch = useDispatch();
  return (
    <div
      className={cx(
        // The ships that arrive need the width of a fleet.
        view.arriving
          ? "max-h-[62%] w-[430px] phone:max-h-[60%]"
          : "max-h-[40%] w-[280px] phone:max-h-[55%]",
        "absolute bottom-2.5 left-2.5 z-2 overflow-auto rounded border border-line bg-surface/95 px-3 py-2.5 text-xs phone:inset-x-0 phone:bottom-0 phone:z-3 phone:w-auto phone:rounded-none phone:border-x-0 phone:border-b-0 phone:bg-surface",
      )}
    >
      <CloseButton
        label="Close system details"
        className="float-right -mt-1 -mr-1.5 ml-1.5"
        onClick={() => dispatch({ type: "closeInspector" })}
      />
      <h3 className="text-base font-semibold">{view.title}</h3>
      <SystemDetails view={view} />
      {view.arriving && (
        <div className="-mx-3 mt-2 border-t border-line">
          <div className="px-3 pt-2 phone:px-2.5">
            <strong className="text-sm font-mid">Arrives</strong>{" "}
            <span className="text-muted">{view.arriving.summary}</span>
          </div>
          {view.arriving.odds && (
            <SpaceOdds view={view.arriving.odds} className="mt-1 px-3 phone:px-2.5" />
          )}
          {view.arriving.origins.map((origin) => (
            <section key={origin.system} aria-label={`From ${origin.label}`}>
              {/* The controls of these ships are at the system that they leave. */}
              <button
                type="button"
                className="mt-1 flex w-full items-baseline gap-2 px-3 text-left hover:bg-raised phone:px-2.5"
                title="Open this system to change what leaves it"
                onClick={() => dispatch({ type: "inspectSystem", system: origin.system })}
              >
                <strong className="text-sm font-mid">From {origin.label}</strong>
                <span className="text-muted">{origin.leaves}</span>
              </button>
              <OriginTokens origin={origin} />
            </section>
          ))}
        </div>
      )}
      {view.activate && (
        <div className="mt-2 border-t border-line pt-2">
          {view.activate === "token" ? (
            <span className="text-muted">
              Your command token is here. You cannot activate this system again.
            </span>
          ) : (
            <Button
              size="sm"
              iconAfter="arrow"
              onClick={() =>
                dispatch({ type: "pickAction", action: "tactical", system: view.system })
              }
            >
              Activate this system
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

/** The board fills the left of the shell. Everything else on it floats: views, tools, inspector. */
export function Board({ view, children }: { view: BoardView; children?: ReactNode }) {
  const [mapView, setMapView] = useState<MapViewId>("standard");
  const phone = usePhone();
  const svg = useRef<SVGSVGElement>(null);
  const galaxy = useRef<HTMLDivElement>(null);
  const camera = useCamera(svg, galaxy, view.tiles);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the board is framed again only when the data source asks for it
  useLayoutEffect(() => camera.fitFor(view.taskSystems), [view.fitKey]);
  const tool =
    "grid h-7 place-items-center rounded-sm border border-line bg-surface text-base text-muted hover:text-text touch:h-9";
  const payment = view.task?.interactive ? view.task.payment : undefined;
  // A phone has no room for the words: a view and a tool are an icon there, with the name on it.
  const fit = (label: string, title: string, icon: IconName, onClick: () => void) => (
    <button
      type="button"
      className={cx(tool, phone ? "w-9" : "px-2 text-xs! whitespace-nowrap")}
      aria-label={label}
      title={title}
      onClick={onClick}
    >
      {phone ? <Icon name={icon} /> : label}
    </button>
  );
  return (
    <section
      aria-label="Galaxy board"
      className="relative flex min-h-0 min-w-0 overflow-hidden bg-board"
    >
      <div className="pointer-events-none absolute inset-x-2.5 top-2 z-2 flex flex-wrap items-center gap-x-3 gap-y-1 phone:flex-nowrap phone:gap-x-2">
        <Segmented
          label="Map view"
          options={phone ? MAP_VIEWS : MAP_VIEWS.map(({ icon, ...option }) => option)}
          value={mapView}
          onChange={setMapView}
          className="pointer-events-auto bg-surface"
        />
        <span className="pointer-events-auto flex-none">
          <Hint label="Map legend">
            {MAP_VIEWS.find((item) => item.id === mapView)!.legend} · dashed planet: exhausted · ▼
            your command token · α β wormholes · inner ring: ships of one player · red dashed:
            contested · ✓ system handled in the movement
          </Hint>
        </span>
        <div
          role="group"
          aria-label="Map zoom"
          className="pointer-events-auto ml-auto flex flex-none gap-[3px]"
        >
          {/* Under a finger a pinch zooms: the two zoom buttons are not shown. */}
          <button
            type="button"
            className={`${tool} w-7 touch:hidden`}
            aria-label="Zoom out"
            onClick={() => camera.zoom(0.8)}
          >
            −
          </button>
          <button
            type="button"
            className={`${tool} w-7 touch:hidden`}
            aria-label="Zoom in"
            onClick={() => camera.zoom(1.25)}
          >
            +
          </button>
          {fit("Fit task", "Show the systems of this action", "target", () =>
            camera.fit(view.taskSystems),
          )}
          {fit("Fit board", "Show the full board", "frame", () => camera.fit())}
        </div>
      </div>
      {payment && <PaymentBar view={payment} />}
      <div
        ref={galaxy}
        className="galaxy relative min-h-0 min-w-0 flex-1"
        onClickCapture={(event) => {
          if (!camera.wasDrag()) {
            return;
          }
          event.stopPropagation();
          event.preventDefault();
        }}
      >
        <GalaxyMap view={view} mapView={mapView} svgRef={svg} />
      </div>
      {view.inspector && <SystemInspector view={view.inspector} />}
      {view.origin && <OriginSheet key={view.origin.system} view={view.origin} />}
      {view.landing && <LandingSheet view={view.landing} />}
      {view.production && <ProductionSheet view={view.production} />}
      {children}
    </section>
  );
}
