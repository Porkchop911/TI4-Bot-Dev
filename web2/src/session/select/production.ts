// The production of a tactical action from the facts of the engine and what is staged: the
// units that can be bought, the places of the active system, the payment on the board.
import type {
  ActionButtonView,
  BoardTaskView,
  BuildSheetView,
  BuildUnitView,
  PaymentView,
  ProductionView,
  TileView,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import {
  GOODS,
  type ProductionDraft,
  UNAVAILABLE,
  blocked,
  buildKey,
  purchases,
  sizes,
  totals,
} from "../productionDraft";
import type {
  BuildFact,
  Catalog,
  FleetStandingFact,
  ProductionFacts,
  SessionUpdate,
} from "../wire";
import { systemLabel, tilesOf } from "./movement";
import { COMMODITY, facesOf, planetPays } from "./payment";
import { isShip, unitLabel, unitType } from "./names";

export type Stage = "build" | "pay";

/** The icon of a unit. A unit that the table of names does not have is shown as infantry. */
const typeOf = (unit: string): UnitType => unitType(unit) ?? "infantry";

/** The name of a planet, wherever it is on the map. */
export const planetNames = (update: SessionUpdate) =>
  new Map(
    (update.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet.label] as const),
    ),
  );

/** The reason in the row of a unit, where the counter would be. */
const UNAVAILABLE_WORD: Record<NonNullable<BuildFact["unavailable"]>, string> = {
  technology: "No technology",
  supply: "None left",
  blockade: "Blockaded",
  no_place: "No place",
  only_another: "Other unit only",
  too_costly: "Too costly",
  banned: "Barred",
  barred: "Barred here",
  not_offered: "Not offered",
};

const placeName = (names: Map<string, string>, place: string) =>
  place === "space" ? "Space area" : `Planet ${names.get(place) ?? place}`;

/** The build of the active system: what the sheet on the board and the panel show. */
export function selectBuild(
  update: SessionUpdate,
  facts: ProductionFacts,
  draft: ProductionDraft,
  stage: Stage,
  /** The printed names of the units. */
  printed: Catalog | null = null,
  /**
   * The fleet supply and the capacity after the staged build, as the engine reads them. Null
   * before the engine answered: the gauges show the system before the build.
   */
  standing: FleetStandingFact | null = null,
): BuildSheetView {
  const tiles = tilesOf(update);
  const names = planetNames(update);
  const total = totals(facts, draft);
  const size = sizes(facts, draft);
  /** The units that the staged purchases of a unit make in a place. */
  const madeAt = (unit: number, place: string) =>
    draft.builds.reduce(
      (sum, build, index) =>
        build.unit === unit && build.place === place ? sum + size[index] : sum,
      0,
    );
  // The places in the order of the system: the space area, then its planets.
  const order = ["space", ...(tiles.get(facts.system)?.planets ?? []).map((planet) => planet.id)];
  const used = [...new Set(facts.units.flatMap((unit) => unit.places))].sort(
    (a, b) => order.indexOf(a) - order.indexOf(b),
  );
  const row = (fact: BuildFact, index: number): BuildUnitView => {
    const unit = typeOf(fact.unit);
    const name = unitLabel(fact.unit, printed);
    const reason = blocked(facts, draft, index);
    return {
      id: `build-unit:${index}`,
      unit,
      name,
      price: fact.per > 1 ? `${fact.cost} for ${fact.per}` : `${fact.cost}`,
      combat: fact.combat
        ? `${fact.combat.hits_on}+${fact.combat.dice > 1 ? ` (x${fact.combat.dice})` : ""}`
        : "–",
      move: fact.move_value ? `${fact.move_value}` : "–",
      capacity: fact.capacity ? `${fact.capacity}` : "–",
      left: fact.supply === undefined ? null : fact.supply - total.made[index],
      abilities: fact.abilities ?? [],
      rules: fact.rules ?? "",
      unavailable: fact.unavailable
        ? { word: UNAVAILABLE_WORD[fact.unavailable], reason: UNAVAILABLE[fact.unavailable] }
        : null,
      counts: fact.unavailable
        ? []
        : fact.places.map((place) => {
            const value = purchases(draft, index, place);
            return {
              place: placeName(names, place),
              counter: {
                id: buildKey(index, place),
                value,
                max: reason === null ? value + 1 : value,
                label: isShip(unit) ? name : `${name}, ${placeName(names, place)}`,
              },
              made: madeAt(index, place),
              reason,
            };
          }),
    };
  };
  const rows = facts.units.map(row);
  const ships = rows.filter((unit) => isShip(unit.unit));
  const forces = rows.filter((unit) => !isShip(unit.unit));
  return {
    system: facts.system,
    label: systemLabel(tiles, facts.system),
    groups: [
      { title: "Ships · Space area", byPlace: false, units: ships },
      { title: "Ground forces", byPlace: true, units: forces },
    ].filter((group) => group.units.length > 0),
    places: used.map((place) => ({
      id: place,
      name: placeName(names, place),
      builds: facts.units
        .flatMap((unit, index) => {
          const count = madeAt(index, place);
          return count ? [`${count} ${unitLabel(unit.unit, printed, count)}`] : [];
        })
        .join(", "),
    })),
    made: total.units,
    cost: total.cost,
    discount: facts.free ? 0 : facts.discount,
    short: Math.max(0, total.cost - total.available),
    gauges: [
      { label: "Production capacity", used: total.units, total: total.capacity },
      { label: "Resources", used: total.cost, total: total.available },
      {
        label: "Fleet supply after",
        used: standing?.fleet_charged ?? facts.fleet_supply.charged,
        total: facts.fleet_supply.limit,
      },
      {
        label: "Fighters and cargo in space",
        used: standing?.consumed ?? facts.transport.used,
        total: standing?.transport ?? facts.transport.limit,
      },
    ],
    stage,
  };
}

/** What pays now: "Jord 4 + Lisis 2 + 1 trade good", "Mecatol Rex 6 influence". */
function paySummary(update: SessionUpdate, facts: ProductionFacts, draft: ProductionDraft) {
  const names = planetNames(update);
  const paying = new Map(facts.planets.map((planet) => [planet.planet, planet]));
  return [
    ...draft.planets.map((planet) =>
      planetPays(
        names.get(planet) ?? planet,
        paying.get(planet)?.worth ?? 0,
        paying.get(planet)?.face,
      ),
    ),
    ...(draft.goods ? [plural(draft.goods, "trade good")] : []),
    ...(draft.commodities ? [plural(draft.commodities, "commodity", "commodities")] : []),
  ].join(" + ");
}

/** The payment of the staged build: how it stands, and the controls that the board shows. */
export function selectPayment(
  update: SessionUpdate,
  facts: ProductionFacts,
  draft: ProductionDraft,
  editable: boolean,
): PaymentView {
  const { cost, paid } = totals(facts, draft);
  const worth = Math.max(1, facts.trade_good_worth);
  // The trade goods that pay exactly what the planets leave.
  const short = cost - (paid - draft.goods * facts.trade_good_worth);
  const rest = short >= 0 && short % worth === 0 ? short / worth : null;
  const action = (
    label: string,
    intent: ActionButtonView["intent"],
    disabled: boolean,
  ): ActionButtonView => ({ label, tone: "quiet", intent, disabled });
  return {
    title: "Payment",
    cost,
    paid,
    unit: "resources",
    editable,
    summary: paySummary(update, facts, draft),
    goods: facts.trade_goods
      ? {
          id: GOODS,
          value: draft.goods,
          max: facts.trade_goods,
          rest: rest !== null && rest !== draft.goods && rest <= facts.trade_goods ? rest : null,
        }
      : null,
    commodities: facts.commodities
      ? { id: COMMODITY, value: draft.commodities ?? 0, max: facts.commodities }
      : null,
    actions: [
      action("Auto-pay", { type: "payment", action: "auto" }, cost <= 0),
      action(
        "Clear payment",
        { type: "payment", action: "reset" },
        draft.planets.length === 0 && draft.goods === 0 && !draft.commodities,
      ),
    ],
  };
}

/** The production step while it is staged. The controls are on the board. */
export const selectProduction = (
  update: SessionUpdate,
  facts: ProductionFacts,
  draft: ProductionDraft,
  stage: Stage,
  printed: Catalog | null = null,
  standing: FleetStandingFact | null = null,
): ProductionView => ({
  kind: "production",
  sheet: selectBuild(update, facts, draft, stage, printed, standing),
  skipped: null,
  title: "",
  subtitle: "",
  rows: [],
  gauges: [],
  payment: selectPayment(update, facts, draft, stage === "pay"),
  done: null,
});

/**
 * What the board asks for. While the build is chosen, the active system opens its sheet. Then
 * the planets that pay are the choice: a tap exhausts one.
 */
export function productionTask(
  update: SessionUpdate,
  facts: ProductionFacts,
  draft: ProductionDraft,
  stage: Stage,
  open: boolean,
): BoardTaskView {
  if (stage === "build") {
    return {
      target: "system",
      kind: "pick",
      interactive: true,
      values: { [facts.system]: 0 },
      chosen: open ? { [facts.system]: true } : {},
      verb: "Produce in",
      unit: "system",
    };
  }
  return {
    target: "planet",
    kind: "pay",
    interactive: true,
    values: Object.fromEntries(facts.planets.map((planet) => [planet.planet, planet.worth])),
    chosen: Object.fromEntries(draft.planets.map((planet) => [planet, true])),
    verb: "Exhaust",
    unit: "resources",
    payment: selectPayment(update, facts, draft, true),
    ...facesOf(facts.planets),
  };
}

/** The systems of the planets that can pay, and the system that produces: what the board frames. */
export function paymentSystems(update: SessionUpdate, facts: ProductionFacts): string[] {
  const pays = new Set(facts.planets.map((planet) => planet.planet));
  return (update.view.board.map_tiles ?? [])
    .filter(
      (tile) =>
        tile.system_id === facts.system ||
        (tile.planets ?? []).some((planet) => pays.has(planet.id)),
    )
    .map((tile) => tile.system_id);
}

/** The tile of the system that produces, with the staged units on it. */
export function withProduction(
  tile: TileView,
  facts: ProductionFacts,
  draft: ProductionDraft,
  viewer: string | null,
): TileView {
  if (tile.id !== facts.system) {
    return tile;
  }
  const size = sizes(facts, draft);
  const at = (place: string, ships: boolean) =>
    draft.builds.reduce(
      (sum, build, index) =>
        build.place === place && isShip(typeOf(facts.units[build.unit].unit)) === ships
          ? sum + size[index]
          : sum,
      0,
    );
  const ships = at("space", true);
  const mine = tile.fleets.some((fleet) => fleet.seat === viewer);
  return {
    ...tile,
    fleets:
      ships === 0 || viewer === null
        ? tile.fleets
        : mine
          ? tile.fleets.map((fleet) =>
              fleet.seat === viewer ? { ...fleet, ships: fleet.ships + ships } : fleet,
            )
          : [...tile.fleets, { seat: viewer, ships, strength: 0 }],
    planets: tile.planets.map((planet) => ({
      ...planet,
      groundForces: planet.groundForces + at(planet.id, false),
    })),
  };
}
