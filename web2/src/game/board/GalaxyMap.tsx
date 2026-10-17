import { useMemo, type KeyboardEvent, type RefObject } from "react";
import type {
  BoardView,
  PlanetMarkView,
  BoardTaskView,
  RouteView,
  SystemId,
  TileView,
} from "../../model";
import { plural } from "../../model";
import { cx } from "../../ui";
import { SeatShape, TECH_COLOR, useDispatch, useSeats } from "../context";
import { useLink } from "../link";
import { TYPE_COLOR, TYPE_NAME } from "./PlanetTraits";
import { HEX_R, hexCenter, hexPoints, routePath } from "./hex";

export type MapViewId = "standard" | "owner" | "space" | "ground" | "tech" | "explore";

const TYPE_LETTER = { cultural: "C", hazardous: "H", industrial: "I" } as const;

const ANOMALY_LABEL = {
  asteroid: "Asteroid field",
  supernova: "Supernova",
  nebula: "Nebula",
  rift: "Gravity rift",
};

function AnomalyGlyph({ kind }: { kind: keyof typeof ANOMALY_LABEL }) {
  if (kind === "rift") {
    return (
      <>
        <circle r="12" fill="none" stroke="#b79cf0" strokeWidth="1.2" strokeDasharray="3 3" />
        <circle r="6.5" fill="none" stroke="#b79cf0" strokeWidth="1.2" />
        <circle r="2" fill="#b79cf0" />
      </>
    );
  }
  if (kind === "asteroid") {
    return (
      <path
        d="m-11-3 5-5 6 2 1 6-5 4-6-2Zm14 1 5-2 4 4-3 5-5-1Zm-6 9 4 1 1 4-4 2-3-3Z"
        fill="#59677a"
        stroke="#93a2b6"
        strokeWidth=".8"
      />
    );
  }
  if (kind === "supernova") {
    return (
      <>
        <circle r="7" fill="#f2a65a" fillOpacity=".85" />
        <path
          d="M0-14v5m0 18v5m-14-14h5m18 0h5m-20-10 3.5 3.5m13 13 3.5 3.5m0-20-3.5 3.5m-13 13-3.5 3.5"
          stroke="#f2a65a"
          strokeWidth="1.3"
          fill="none"
        />
      </>
    );
  }
  return (
    <>
      <circle cx="-6" cy="1" r="8" fill="#c583c9" fillOpacity=".28" />
      <circle cx="5" cy="-3" r="9" fill="#c583c9" fillOpacity=".28" />
      <circle cx="4" cy="6" r="6" fill="#c583c9" fillOpacity=".28" />
    </>
  );
}

const pressOnKey = (event: KeyboardEvent<SVGGElement>) => {
  if (event.key !== "Enter" && event.key !== " ") {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  event.currentTarget.dispatchEvent(new MouseEvent("click", { bubbles: true }));
};

function Planet({
  planet,
  x,
  y,
  r,
  view,
  task,
}: {
  planet: PlanetMarkView;
  x: number;
  y: number;
  r: number;
  view: MapViewId;
  task: BoardTaskView | null;
}) {
  const dispatch = useDispatch();
  const seats = useSeats();
  const { linked, props } = useLink([`pl:${planet.id}`]);
  const onPlanets = task?.target === "planet" ? task : null;
  const payable = !!onPlanets?.interactive && planet.id in onPlanets.values;
  const chosen = !!onPlanets?.chosen[planet.id];
  const color = planet.owner ? seats[planet.owner].color : undefined;
  const text = (body: React.ReactNode, size = 10, fill?: string) => (
    <text x={x} y={y + size * 0.35} fontSize={size} style={fill ? { fill } : undefined}>
      {body}
    </text>
  );
  // A planet that the task can use shows resources and influence, so the player sees what the
  // payment gives up. The value that pays is large. A ring and a check mark show the choice.
  const face = onPlanets?.faces?.[planet.id] ?? task?.unit;
  const pays = face === "influence" ? "influence" : "resources";
  const value = (kind: "resources" | "influence") => (
    <tspan className={kind === pays ? "v-on" : "v-off"}>{planet[kind]}</tspan>
  );
  const inside = payable
    ? text(
        <>
          {value("resources")}
          <tspan className="v-off">/</tspan>
          {value("influence")}
        </>,
        r > 11 ? 12.5 : 11,
      )
    : view === "standard"
      ? text(
          <>
            <tspan style={{ fill: "var(--color-gold)" }}>{planet.resources}</tspan>
            <tspan style={{ fill: "var(--color-faint)" }}>/</tspan>
            <tspan style={{ fill: "var(--color-cyan)" }}>{planet.influence}</tspan>
          </>,
          8.5,
        )
      : view === "ground"
        ? text(`${planet.groundForces}${planet.planetaryShield ? "◈" : ""}`, 9.5)
        : view === "tech"
          ? planet.tech && text(planet.tech, 11, TECH_COLOR[planet.tech])
          : view === "explore"
            ? // The letter is the type, "?" says that the planet is still explored, "+" that
              // something is attached.
              text(
                <>
                  {planet.traits.types.map((type) => (
                    <tspan key={type} style={{ fill: TYPE_COLOR[type] }}>
                      {TYPE_LETTER[type]}
                    </tspan>
                  ))}
                  {planet.traits.unexplored && "?"}
                  {planet.traits.attachments.length > 0 && "+"}
                </>,
                planet.traits.types.length > 1 ? 8.5 : 10,
              )
            : planet.owner && (
                <SeatShape
                  symbol={seats[planet.owner].symbol}
                  x={x}
                  y={y}
                  size={11}
                  color={color}
                />
              );
  const button =
    onPlanets && payable
      ? {
          role: "button",
          tabIndex: 0,
          "aria-pressed": chosen,
          "aria-label": `${onPlanets.verb} ${planet.name}, ${onPlanets.values[planet.id]} ${face}, ${planet.resources} resources / ${planet.influence} influence${planet.exhausted ? ", exhausted" : ""}`,
          onKeyDown: pressOnKey,
          onClick: (event: React.MouseEvent) => {
            event.stopPropagation();
            dispatch({ type: "togglePlanet", planet: planet.id });
          },
        }
      : {};
  // A planet that the task can use is larger than the others.
  const radius = payable ? r + 1.5 : r;
  return (
    <g
      className={cx(
        "planet",
        !planet.owner && "free",
        payable && "payable",
        chosen && payable && (onPlanets?.kind === "pay" ? "chosen" : "chosen picked"),
        // A planet of a recorded payment is exhausted too.
        (planet.exhausted || (chosen && !payable)) && "exhausted",
        linked && "linked",
      )}
      style={color ? ({ "--seat": color } as React.CSSProperties) : undefined}
      {...props}
      {...button}
    >
      <title>
        {planet.name} · {planet.resources} resources / {planet.influence} influence
        {planet.owner ? " · " + seats[planet.owner].faction : ""}
        {planet.exhausted ? " · exhausted" : ""}
        {planet.traits.types.map((type) => " · " + TYPE_NAME[type]).join("")}
        {planet.traits.unexplored ? " · unexplored" : ""}
        {planet.traits.attachments.map((item) => " · " + item.name).join("")}
      </title>
      <circle cx={x} cy={y} r={radius} />
      {inside}
      {chosen && payable && (
        <g className="chosen-badge" transform={`translate(${x + radius - 2} ${y - radius + 2})`}>
          <circle r="4.6" />
          <path d="m-2.2 0 1.6 1.7 2.8-3.2" />
        </g>
      )}
    </g>
  );
}

interface TileProps {
  tile: TileView;
  active: boolean;
  /** The open window, or the turn of the Log that is open, is about this system. */
  marked: boolean;
  inspected: boolean;
  targeting: boolean;
  view: MapViewId;
  task: BoardTaskView | null;
}

/** Vertical centre of the planets, the wormhole and the anomaly in a tile. */
const BODY_Y = -5;

/**
 * One system. It shows what matters for play: planets, wormhole, anomaly, command token and fleets.
 * The name is flavor: it is in the tooltip and in the inspector, not on the tile.
 */
function Tile({ tile, active, marked, inspected, targeting, view, task }: TileProps) {
  const dispatch = useDispatch();
  const seats = useSeats();
  const { linked, props } = useLink([`sys:${tile.id}`]);
  const { x, y } = hexCenter(tile);
  const strength = view === "space";
  const label = `${tile.name}, system ${tile.id}${tile.home ? `, ${seats[tile.home].faction} home system` : ""}${tile.anomaly ? ", " + ANOMALY_LABEL[tile.anomaly] : ""}${tile.wormhole ? `, ${tile.wormhole} wormhole` : ""}${tile.commandToken ? ", your command token is here" : ""}${tile.handled ? ", handled" : ""}${marked ? ", the open window or turn is about this system" : ""}${tile.fleets.map((fleet) => `, ${plural(fleet.ships, seats[fleet.seat].faction + " ship")}`).join("")}`;
  // Planets and the wormhole are in one row. A system has at most three planets.
  const slots = tile.planets.length + (tile.wormhole ? 1 : 0);
  const gap = slots > 3 ? 19 : 25;
  const slotX = (index: number) => x + (index - (slots - 1) / 2) * gap;
  const planetR = slots > 2 ? 10 : 11.5;
  // A task that asks for a system: a click on a system that can be chosen chooses it. So does a
  // system of a task on planets that chooses systems too.
  const target =
    !!task?.interactive &&
    (task.target === "system" ? tile.id in task.values : !!task.systems?.includes(tile.id));
  const chosen = target && !!task.chosen[tile.id];
  // It can be chosen, and the task has nothing to do there: it does not stand out.
  const plain = target && !!task.highlight && !task.highlight.includes(tile.id);
  return (
    <g
      className={cx(
        "system",
        tile.home && "home",
        inspected && "inspected",
        active && "active",
        marked && "marked",
        (tile.anomaly === "asteroid" || tile.anomaly === "supernova") && "blocked",
        linked && "linked",
        target && "target",
        plain && "plain",
        chosen && "chosen",
        task?.context?.includes(tile.id) && "context",
      )}
      style={tile.home ? ({ "--seat": seats[tile.home].color } as React.CSSProperties) : undefined}
      role="button"
      tabIndex={0}
      aria-pressed={target ? chosen : undefined}
      aria-label={`${target ? task.verb : targeting && !tile.commandToken ? "Activate" : "Inspect"} ${label}`}
      onKeyDown={pressOnKey}
      onClick={() =>
        dispatch(
          target
            ? { type: "toggleSystem", system: tile.id }
            : { type: "inspectSystem", system: tile.id },
        )
      }
      {...props}
    >
      <title>
        {tile.name} · #{tile.id}
      </title>
      <polygon className="hex" points={hexPoints({ x, y }, HEX_R - 2)} />
      {marked && (
        // The system of the open window or turn: a wide ring, which no other state of a tile has.
        <polygon className="mark" points={hexPoints({ x, y }, HEX_R - 2)} />
      )}
      {tile.control && (
        // Inner ring: the player with ships here. Red and dashed when two or more players have ships.
        <polygon
          className={cx("control", tile.control === "contested" && "contested")}
          points={hexPoints({ x, y }, HEX_R - 5.5)}
          style={tile.control === "contested" ? undefined : { stroke: seats[tile.control].color }}
        />
      )}
      <text className="t-id" x={x} y={y - 31}>
        #{tile.id}
      </text>
      {tile.commandToken && (
        <path d={`m${x + 9} ${y - 33} 9 0-4.5 8Z`} fill="var(--color-cyan)">
          <title>Your command token</title>
        </path>
      )}
      {tile.anomaly && (
        <g className="anomaly" transform={`translate(${x} ${y + BODY_Y})`}>
          <AnomalyGlyph kind={tile.anomaly} />
        </g>
      )}
      {tile.planets.map((planet, index) => (
        <Planet
          key={planet.id}
          planet={planet}
          x={slotX(index)}
          y={y + BODY_Y}
          r={planetR}
          view={view}
          task={task}
        />
      ))}
      {tile.wormhole && (
        <g className="wormhole" transform={`translate(${slotX(slots - 1)} ${y + BODY_Y})`}>
          <circle r="8" />
          <text y="3.2">{tile.wormhole}</text>
        </g>
      )}
      {tile.fleets.map((fleet, index) => {
        const wide = strength || tile.fleets.some((mark) => mark.was !== undefined);
        const markX = x + (index - (tile.fleets.length - 1) / 2) * (wide ? 38 : 28);
        const shift = wide ? 13 : 6;
        return (
          <g key={fleet.seat} className="t-mark">
            <SeatShape
              symbol={seats[fleet.seat].symbol}
              x={markX - shift}
              y={y + 23}
              size={11}
              color={seats[fleet.seat].color}
            />
            <text x={markX - shift + 8} y={y + 27} style={{ fill: seats[fleet.seat].color }}>
              {/* A staged movement changes the count: the mark shows now → after. The combat
                  view has no room for both: it shows the fleet after the move, with its hits. */}
              {fleet.was !== undefined && !strength && `${fleet.was}→`}
              {fleet.ships}
              {strength ? `·${fleet.strength.toFixed(1)}` : ""}
            </text>
          </g>
        );
      })}
      {tile.handled && (
        // A check mark, not only a colour: the player is done with this system in the movement.
        <g className="t-staged" transform={`translate(${x - 25} ${y - 19})`}>
          <title>Handled in this movement</title>
          <circle r="6" />
          <path d="m-2.9 0 2.1 2.2 3.7-4.2" />
        </g>
      )}
      {tile.pickedUp > 0 && (
        <text className="t-pick" x={x - 9} y={y - 16}>
          −{tile.pickedUp}
          <title>{plural(tile.pickedUp, "unit")} picked up here</title>
        </text>
      )}
      {tile.note && (
        // A sign for what has no number: ships that cannot leave, cargo that no ship takes.
        <g className="t-note" transform={`translate(${x + 25} ${y - 19})`}>
          <title>{tile.note.text}</title>
          <circle r="6" />
          {tile.note.sign === "stay" ? (
            <path d="m-2.6-2.6 5.2 5.2m0-5.2-5.2 5.2" />
          ) : (
            <path d="M-2.8-2.2h5.6v4.4h-5.6zm-1.4 4.8 8.4-5.2" />
          )}
        </g>
      )}
    </g>
  );
}

function Route({ route, tiles }: { route: RouteView; tiles: Record<SystemId, TileView> }) {
  const { linked } = useLink(route.links ?? [`rt:${route.id}`]);
  return (
    <g className={cx("route-group", linked && "linked")}>
      <path
        className={cx("route", route.staged ? "staged" : "idle", linked && "linked")}
        d={routePath(route.path, tiles)}
        markerEnd="url(#route-end)"
      />
      {route.marks?.map((mark) => {
        const { x, y } = hexCenter(tiles[mark.system]);
        return mark.sign === "die" ? (
          <g key={mark.system} className="route-die" transform={`translate(${x + 27} ${y + 3})`}>
            <title>{mark.text}</title>
            <rect x="-6" y="-6" width="12" height="12" rx="2.5" />
            <circle cx="-2.6" cy="-2.6" r="1.1" />
            <circle r="1.1" />
            <circle cx="2.6" cy="2.6" r="1.1" />
          </g>
        ) : (
          <g key={mark.system} className="route-plus" transform={`translate(${x + 27} ${y + 3})`}>
            <title>{mark.text}</title>
            <rect x="-8" y="-6" width="16" height="12" rx="2.5" />
            <text y="3">+1</text>
          </g>
        );
      })}
    </g>
  );
}

/** The galaxy: every system, the wormhole pairs, and the routes of the ships in the open task. */
export function GalaxyMap({
  view,
  mapView,
  svgRef,
}: {
  view: BoardView;
  mapView: MapViewId;
  svgRef: RefObject<SVGSVGElement | null>;
}) {
  const byId = useMemo(
    () => Object.fromEntries(view.tiles.map((tile) => [tile.id, tile])),
    [view.tiles],
  );
  return (
    <svg
      ref={svgRef}
      role="group"
      aria-label="Galaxy board"
      // While the player chooses planets, everything else on the board is dimmed.
      data-tasking={view.task?.interactive && view.task.target === "planet" ? "" : undefined}
      // While the player chooses a system, the systems that cannot be chosen are dimmed.
      data-targeting={view.task?.interactive && view.task.target === "system" ? "" : undefined}
    >
      <defs>
        <marker
          id="route-end"
          viewBox="0 0 8 8"
          refX="6"
          refY="4"
          markerWidth="5"
          markerHeight="5"
          orient="auto"
        >
          <path d="M1 1 7 4 1 7" fill="none" stroke="var(--color-cyan)" strokeWidth="1.5" />
        </marker>
      </defs>
      {view.tiles.map((tile) => (
        <Tile
          key={tile.id}
          tile={tile}
          active={tile.id === view.activeSystem}
          marked={tile.id === view.marked}
          inspected={tile.id === view.inspected}
          targeting={view.targeting}
          view={mapView}
          task={view.task}
        />
      ))}
      {view.wormholes.map((pair) => (
        <path key={pair.join()} className="wormhole-arc" d={routePath(pair, byId)}>
          <title>{byId[pair[0]].wormhole} wormholes: these systems are adjacent</title>
        </path>
      ))}
      {view.routes.map((route) => (
        <Route key={route.id} route={route} tiles={byId} />
      ))}
    </svg>
  );
}
