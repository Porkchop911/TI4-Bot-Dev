// A tactical turn of the Log, opened: the steps of its action as Live showed them, read-only.
// Everything is of the public events of the game: the board of each step is the board of now
// taken back by the changes of the board (`history.ts`), the dice are the rounds of its fights
// (`BoardView.space_rounds`, `ground_steps`) and its public rolls, and what the seat did is the
// facts of its decisions. What only explains the options of a step (the range of a ship, the
// capacity, the limits of a production) is not there: the choice is made.
import type {
  ActivationView,
  BattleRecordView,
  BlockView,
  BattleRowView,
  BattleTableView,
  CombatView,
  FlowActionView,
  InvasionView,
  LandingPlanetView,
  MoveShipView,
  MovementView,
  OriginView,
  ProductionView,
  StepContentView,
  StepTabView,
  TacticalActionView,
} from "../../model";
import { plural } from "../../model";
import type { EventLog } from "../events";
import { boardAt, withBoard } from "../history";
import { reasonOf } from "../rolls";
import type {
  Catalog,
  CombatDieRoll,
  CombatView as CombatState,
  ExplorationCatalog,
  GameEvent,
  GroundStepView,
  PlacedUnitView,
  PublicFact,
  PublicRoll,
  SessionUpdate,
  SpaceRoundView,
} from "../wire";
import { roundTable, sideRows } from "./combat";
import { NO_TRAITS, exploredCard, planetTraits } from "./exploration";
import { turnEvents } from "./log";
import { selectPastFlow } from "./pastFlow";
import { systemLabel, tilesOf } from "./movement";
import { cannonTable, removedUnits, riftRows } from "./movementRolls";
import { byUnitOrder, factionName, forceOf, unitKey, unitLabel, unitType } from "./names";

const STEPS = ["Activation", "Movement", "Space combat", "Invasion", "Production"] as const;

/** A turn of the Log that is open: its action, and the game as its open step left it. */
export interface PastTurn {
  /** The game with the board of the open step. */
  update: SessionUpdate;
  /** The active system of the action. */
  system: string | null;
  /** The active system by its name: "Ixth - Mahact · #52". */
  label?: string;
  /** A tactical action; a strategic or a component action that has one, with its card first. */
  action: TacticalActionView | FlowActionView;
}

type Fact<K extends PublicFact["kind"]> = Extract<PublicFact, { kind: K }>;

/** The tactical action of a turn, opened. */
type PastTactical = PastTurn & { action: TacticalActionView; label: string };

/** The opened turn as a tactical action: it has steps. Throws for another action. */
export function pastTactical(past: PastTurn | null): PastTurn & { action: TacticalActionView } {
  if (past?.action.kind !== "tactical") {
    throw new Error("the turn is no tactical action");
  }
  return past as PastTurn & { action: TacticalActionView };
}

/** An event of the turn, with its place in the events of the game. */
interface At {
  at: number;
  entry: GameEvent;
}

/** Units by their printed unit: one of their ids, and how many. */
function counted(ids: Iterable<string>, printed: Catalog | null) {
  const units = new Map<string, { id: string; count: number }>();
  for (const id of ids) {
    const key = unitKey(id, printed);
    units.set(key, { id, count: (units.get(key)?.count ?? 0) + 1 });
  }
  return [...units.values()].sort((a, b) => byUnitOrder(a.id, b.id));
}

/** "2 Fighter II, 1 Cruiser II". */
const unitsText = (ids: Iterable<string>, printed: Catalog | null) =>
  counted(ids, printed)
    .map(({ id, count }) => `${count} ${unitLabel(id, printed, count)}`)
    .join(", ");

/** A finished round as the battle table of a round shows it. */
const stateOf = (
  fight: Pick<SpaceRoundView, "system_id" | "attacker" | "defender" | "battle_seq" | "round">,
  ships: PlacedUnitView[],
  dice: CombatDieRoll[],
  hits: Record<string, number>,
): CombatState => ({
  system_id: fight.system_id,
  round: fight.round,
  battle_seq: fight.battle_seq,
  phase: "complete",
  round_start: ships,
  barrage_start: [],
  barrage_hits: {},
  barrage_dice: [],
  remaining_hits: {},
  attacker: fight.attacker,
  defender: fight.defender,
  dice_rolls: dice,
  attacker_hits: hits[fight.attacker] ?? 0,
  defender_hits: hits[fight.defender] ?? 0,
});

/**
 * Of the turn `inspect.turn` of the Log: its tactical action, with the step `inspect.step` open
 * (by default the last step that it came to), and the game with the board of that step. Null when
 * the turn is no tactical action, or the events no longer have it.
 */
export function selectPastTurn(
  update: SessionUpdate,
  events: EventLog,
  inspect: { turn: string; step: number | null },
  catalog: ExplorationCatalog | null = null,
  printed: Catalog | null = null,
): PastTurn | null {
  const turn = turnEvents(events, inspect.turn);
  if (!turn) {
    return null;
  }
  const { seat } = turn;
  const entries: At[] = turn.at.map((at) => ({ at, entry: events[at] }));
  const started = entries.flatMap(({ entry }) =>
    entry.fact?.kind === "action_started" && (entry.actor ?? seat) === seat
      ? [entry.fact.action.type]
      : [],
  );
  const live =
    !entries.some(
      ({ entry }) => entry.fact?.kind === "turn_ended" && (entry.actor ?? seat) === seat,
    ) && !started.includes("pass");
  if (started.includes("tactical")) {
    return tacticalOf(update, events, inspect, entries, seat, live, catalog, printed);
  }
  return withTactical(update, events, inspect, entries, seat, live, catalog, printed);
}

/**
 * The facts of a tactical action that its steps show: what moves, fights, lands, is produced and
 * explored, and what pays for it. Another action in a turn (the primary of a strategy card) shows
 * the rest.
 */
const STEP_FACTS = new Set<PublicFact["kind"]>([
  "activated",
  "free_tactical",
  "moved",
  "loaded",
  "unit_removed",
  "retreat_announced",
  "retreated",
  "casualty",
  "destroyed",
  "sustained",
  "sustain_declined",
  "combat_ended",
  "bombardment_targeted",
  "ground_combat_started",
  "ground_combat_round",
  "landed",
  "control_gained",
  "explored",
  "produced",
  "paid",
]);

/**
 * A strategic or a component turn whose action has a tactical action in it (the primary of
 * Warfare of Thunder's Edge, a card that activates a system): the action as its seats did it, the
 * first tab named after its card, and the steps of its tactical action after it, as an opened
 * tactical turn has them. The events of the tactical action are from its activation to its last
 * step, before the secondaries; the lines of the card leave them out. Null for any other turn.
 */
function withTactical(
  update: SessionUpdate,
  events: EventLog,
  inspect: { turn: string; step: number | null },
  entries: At[],
  seat: string,
  live: boolean,
  catalog: ExplorationCatalog | null,
  printed: Catalog | null,
): PastTurn | null {
  const begins = entries.findIndex(
    ({ entry }) =>
      (entry.fact?.kind === "activated" || entry.fact?.kind === "free_tactical") &&
      (entry.actor ?? seat) === seat,
  );
  if (begins < 0) {
    return selectPastFlow(update, events, inspect.turn, printed);
  }
  // The tactical action ends with its last step: the secondaries of the card come after it.
  const secondaries = entries.findIndex(
    ({ entry }, index) => index > begins && entry.fact?.kind === "secondary_followed",
  );
  const before = secondaries < 0 ? entries.length : secondaries;
  let ends = begins;
  for (let index = begins; index < before; index += 1) {
    const { entry } = entries[index];
    if (entry.roll || (entry.fact && STEP_FACTS.has(entry.fact.kind))) {
      ends = index;
    }
  }
  // The change of the board of its last step is in it too: the last event of that step.
  const change = entries.findIndex(
    ({ entry }, index) => index > ends && index < before && entry.event.kind === "board_changed",
  );
  if (change >= 0) {
    ends = change;
  }
  const span = entries.slice(begins, ends + 1);
  const shown = new Set(
    span
      .filter(({ entry }) => entry.roll || (entry.fact && STEP_FACTS.has(entry.fact.kind)))
      .map(({ entry }) => entry),
  );
  const flow = selectPastFlow(update, events, inspect.turn, printed, shown);
  if (!flow) {
    return null;
  }
  // The steps of the tactical action are the tabs after the card: tab 0 is the card.
  const step = inspect.step ?? 0;
  const tactical = tacticalOf(
    update,
    events,
    { turn: inspect.turn, step: step > 0 ? step - 1 : null },
    span,
    seat,
    live && ends === entries.length - 1,
    catalog,
    printed,
  );
  if (!tactical) {
    return flow;
  }
  const context = `${flow.action.context} · ${tactical.label}`;
  const tabs: StepTabView[] = [
    { name: flow.action.heading, status: "done", caption: "" },
    ...tactical.action.tabs,
  ];
  if (step === 0) {
    return {
      update: flow.update,
      system: tactical.system,
      label: tactical.label,
      action: { ...flow.action, context, tabs, selected: 0 },
    };
  }
  const selected = tactical.action.selected + 1;
  return {
    ...tactical,
    action: {
      ...tactical.action,
      context,
      tabs,
      selected,
      task: { ...tactical.action.task, step: selected, help: flow.action.help },
    },
  };
}

/**
 * The tactical action of a turn: its steps (`entries`, the events of the action) with the step
 * `inspect.step` open, by default the last that it came to.
 */
function tacticalOf(
  update: SessionUpdate,
  events: EventLog,
  inspect: { turn: string; step: number | null },
  entries: At[],
  seat: string,
  live: boolean,
  catalog: ExplorationCatalog | null,
  printed: Catalog | null,
): PastTactical | null {
  if (!entries.length) {
    return null;
  }
  const facts = <K extends PublicFact["kind"]>(kind: K, of: string | null = seat) =>
    entries.flatMap(({ at, entry }) =>
      entry.fact?.kind === kind && (of === null || (entry.actor ?? seat) === of)
        ? [{ at, fact: entry.fact as Fact<K> }]
        : [],
    );
  const ids = new Set(entries.map(({ entry }) => entry.action_id));
  // The board after the step of the event at `at`: its change is the last event of the step.
  const stepEnd = (at: number) => {
    for (let next = at; next < events.length; next += 1) {
      const entry = events[next];
      if (next > at && entry.action_id && !ids.has(entry.action_id)) {
        return next;
      }
      if (entry.event.kind === "board_changed") {
        return next + 1;
      }
    }
    return events.length;
  };
  const start = entries[0].at;
  const last = entries[entries.length - 1].at;
  const view = update.view;
  const board = (cursor: number) => boardAt(view, events, cursor);
  const tiles = tilesOf(update);
  const viewer = update.viewer.role === "player" ? update.viewer.seat : "";
  const name = (id: string) =>
    factionName(view.players.find((player) => player.id === id)?.faction ?? id, printed);

  // A system activated, or one that the action takes for its tactical action (Warfare).
  const activated = [...facts("activated"), ...facts("free_tactical")].sort(
    (a, b) => a.at - b.at,
  )[0];
  const system = activated?.fact.system ?? null;
  const label = system ? systemLabel(tiles, system) : "No system";

  // The fights of the turn: the board keeps them round by round until the turn ends.
  let rounds: SpaceRoundView[] = [];
  let ground: GroundStepView[] = [];
  for (const { entry } of entries) {
    const delta = entry.event.kind === "board_changed" ? entry.board : undefined;
    if (delta?.space_rounds) {
      rounds = [...rounds.slice(0, delta.space_rounds.kept), ...delta.space_rounds.after];
    }
    if (delta?.ground_steps) {
      ground = [...ground.slice(0, delta.ground_steps.kept), ...delta.ground_steps.after];
    }
  }
  const rolls: { at: number; roll: PublicRoll }[] = entries.flatMap(({ at, entry }) =>
    entry.roll ? [{ at, roll: entry.roll }] : [],
  );

  const moved = facts("moved");
  const loaded = facts("loaded");
  const landed = facts("landed");
  const produced = facts("produced");
  const ended = facts("combat_ended", null);
  const spaceEnd = ended.find(({ fact }) => fact.combat === "space");
  const groundFacts = [
    ...landed,
    ...facts("control_gained"),
    ...ended.filter(({ fact }) => fact.combat !== "space"),
  ].sort((a, b) => a.at - b.at);
  const lastOf = (list: { at: number }[]) =>
    list.reduce((most, item) => Math.max(most, item.at), -1);

  // The cursor of the board of each step, and whether the action came to it.
  const activationAt = activated ? stepEnd(activated.at) : stepEnd(start);
  const movementAt = moved.length ? stepEnd(lastOf([...moved, ...loaded])) : activationAt;
  const combatAt = spaceEnd ? stepEnd(spaceEnd.at) : movementAt;
  // A bombardment with no landing has ground steps and no fact of the ground: its change of the
  // board ends the step.
  const groundChanges = entries.filter(
    ({ entry }) => entry.event.kind === "board_changed" && entry.board?.ground_steps,
  );
  const invasionAt =
    groundFacts.length || ground.length
      ? stepEnd(lastOf([...groundFacts, ...groundChanges]))
      : combatAt;
  const productionAt = produced.length ? stepEnd(lastOf(produced)) : stepEnd(last);
  // A space combat of the turn that is under way (the turn that is played now): its open round.
  const fighting = board(stepEnd(last)).board.combat;
  const underWay = fighting && fighting.system_id === system ? fighting : null;
  const happened = [
    true,
    moved.length > 0,
    rounds.length > 0 || !!spaceEnd || !!underWay,
    landed.length > 0 || ground.length > 0,
    produced.length > 0,
  ];
  const cursors = [activationAt, movementAt, combatAt, invasionAt, productionAt];
  const steps = STEPS.map((_, step) => step).filter((step) => happened[step]);
  const shown = Math.min(inspect.step ?? steps.length - 1, steps.length - 1);
  const step = steps[shown];
  const then = {
    ...update,
    view: withBoard(view, board(cursors[step])),
    pending_choice: undefined,
  };

  /** The units of `owner` in the space area of the active system after the first `cursor` events. */
  const fleet = (cursor: number, owner: string) =>
    system
      ? (board(cursor).board.systems[system]?.units ?? []).filter(
          (unit) => unit.owner === owner && unit.planet === null,
        )
      : [];

  const activation = (): ActivationView => {
    const pool = (cursor: number) => board(cursor).pools[seat]?.tactic_tokens ?? 0;
    const [now, after] = [pool(start), pool(activationAt)];
    return {
      kind: "activation",
      editing: false,
      draft: false,
      chosen: null,
      systems: [],
      active: system ? { system, name: tiles.get(system)?.label ?? "System" } : null,
      tacticPool: [now, after],
      costsToken: after < now,
    };
  };

  const movement = (): MovementView => {
    const from = new Map<string, { ships: string[]; cargo: string[] }>();
    const origin = (id: string) => {
      const known = from.get(id) ?? { ships: [], cargo: [] };
      from.set(id, known);
      return known;
    };
    let lastFrom: string | null = null;
    for (const item of [...moved, ...loaded].sort((a, b) => a.at - b.at)) {
      if (item.fact.kind === "moved") {
        lastFrom = item.fact.from;
        origin(item.fact.from).ships.push(item.fact.unit);
      } else {
        const site = item.fact.system ?? lastFrom;
        if (site) {
          origin(site).cargo.push(item.fact.unit);
        }
      }
    }
    const origins = [...from.entries()].map(([id, { ships, cargo }]): OriginView => ({
      system: id,
      label: systemLabel(tiles, id),
      away: 0,
      ships: counted(ships, printed).map(({ id: unit, count }): MoveShipView => ({
        key: `${id}:${unit}`,
        unit: unitType(unit) ?? "cruiser",
        name: unitLabel(unit, printed, count),
        damaged: false,
        count,
        total: count,
        facts: [],
        reason: null,
        invalid: false,
        link: [],
        units: [],
      })),
      cargo: [],
      leaves: [plural(ships.length, "ship"), cargo.length ? `${cargo.length} cargo` : null]
        .filter((part) => part !== null)
        .join(" · "),
      loaded: cargo.length,
      capacity: 0,
      stays: "",
      warning: null,
      handled: true,
    }));
    const before = counted(
      fleet(activationAt, seat).map((unit) => unit.unit_type),
      printed,
    );
    const after = counted(
      fleet(movementAt, seat).map((unit) => unit.unit_type),
      printed,
    );
    const keys = new Set([...before, ...after].map(({ id }) => unitKey(id, printed)));
    const arrival = [...keys].map((key) => {
      const was = before.find(({ id }) => unitKey(id, printed) === key);
      const is = after.find(({ id }) => unitKey(id, printed) === key);
      const id = (is ?? was)!.id;
      return {
        unit: unitType(id) ?? "cruiser",
        name: unitLabel(id, printed, is?.count ?? 0),
        now: was?.count ?? 0,
        after: is?.count ?? 0,
      };
    });
    // The rolls of the movement: a gravity rift on the way, the space cannon offense after it.
    const first = (list: { at: number }[]) =>
      list.reduce((most, item) => Math.min(most, item.at), Infinity);
    const until = Math.min(first(groundFacts), spaceEnd?.at ?? Infinity);
    const moving = rolls
      .filter(
        ({ at, roll }) => at < until && ["gravity rift", "space cannon"].includes(reasonOf(roll)),
      )
      .map(({ roll }) => roll)
      .sort((a, b) => a.index - b.index);
    const ships = (owner: string) =>
      counted(
        fleet(movementAt, owner).map((unit) => unit.unit_type),
        printed,
      ).map(({ id, count }) => ({
        unit: unitType(id) ?? "cruiser",
        name: unitLabel(id, printed, count),
        count,
      }));
    return {
      kind: "movement",
      target: { system: system ?? "", label },
      origins,
      unreachable: null,
      arrival,
      gauges: [],
      excessShips: 0,
      riftRolls: 0,
      rift: riftRows(moving, (id) => systemLabel(tiles, id), printed),
      removed: removedUnits(moving, printed),
      cannon: cannonTable(
        moving.filter((roll) => reasonOf(roll) === "space cannon"),
        seat,
        viewer,
        ships,
        printed,
      ),
      odds: null,
    };
  };

  /** How a fight ended, for the table of its last roll. */
  const outcome = (fact: Fact<"combat_ended"> | undefined) =>
    !fact
      ? ""
      : fact.winner
        ? `Won by ${name(fact.winner)}`
        : fact.retreated
          ? `${name(fact.retreated)} retreated`
          : "Both sides destroyed";

  const combat = (): CombatView => {
    const battles = new Set(rounds.map((round) => round.battle_seq)).size;
    const records = rounds.flatMap((round): BattleRecordView[] => {
      const prefix = battles > 1 ? `${systemLabel(tiles, round.system_id)} · ` : "";
      const barrage: BattleRecordView[] = round.barrage_dice?.length
        ? [
            {
              key: `${round.battle_seq}:${round.round}:barrage`,
              label: `${prefix}Anti-fighter barrage`,
              table: {
                ...roundTable(
                  then,
                  stateOf(
                    round,
                    round.barrage_start ?? round.round_start,
                    round.barrage_dice,
                    round.barrage_hits ?? {},
                  ),
                  printed,
                ),
                label: "Anti-fighter barrage",
                score: "",
              },
            },
          ]
        : [];
      return [
        ...barrage,
        {
          key: `${round.battle_seq}:${round.round}`,
          label: `${prefix}Round ${round.round}`,
          table: {
            ...roundTable(
              then,
              stateOf(round, round.round_start, round.dice_rolls, round.hits),
              printed,
            ),
            score: "",
          },
        },
      ];
    });
    // The round under way, as the step of a combat that the viewer watches has it.
    if (
      underWay &&
      !rounds.some(
        (round) => round.battle_seq === underWay.battle_seq && round.round === underWay.round,
      )
    ) {
      records.push({
        key: `${underWay.battle_seq}:${underWay.round}`,
        label: `Round ${underWay.round}`,
        table: roundTable(then, underWay, printed),
      });
    }
    const final = spaceEnd && records.at(-1);
    if (final) {
      final.table = { ...final.table, score: outcome(spaceEnd?.fact) };
    }
    return { kind: "combat", decision: null, records, revision: `past:${inspect.turn}` };
  };

  const invasion = (): InvasionView => {
    const before = board(combatAt).board.systems[system ?? ""];
    const traits = planetTraits(then, catalog);
    const meta = (system && tiles.get(system)?.planets) || [];
    const planets = meta.map((info): LandingPlanetView => {
      const units = (before?.units ?? []).filter((unit) => unit.planet === info.id);
      const lands = landed
        .filter(({ fact }) => fact.planet === info.id)
        .map(({ fact }) => fact.unit);
      return {
        id: info.id,
        name: info.label,
        resources: info.resources ?? 0,
        influence: info.influence ?? 0,
        owner: before?.planets[info.id]?.controlled_by ?? null,
        traits: traits(info) ?? NO_TRAITS,
        there: [...new Set(units.map((unit) => unit.owner))].map((owner) => ({
          seat: owner,
          force: forceOf(
            units.filter((unit) => unit.owner === owner),
            printed,
          ),
        })),
        tokens: lands.map((unit, index) => ({
          key: `${info.id}:${index}`,
          unit: unitType(unit) ?? "infantry",
          name: unitLabel(unit, printed),
          value: 0,
        })),
        accepts: {},
        lands: unitsText(lands, printed),
        reason: null,
        odds: null,
      };
    });
    const planetName = (id: string) => meta.find((item) => item.id === id)?.label ?? id;
    const fought = new Set(ground.map((item) => item.step.planet)).size;
    const records = ground.map((item, index): BattleRecordView => {
      const { step } = item;
      const prefix = fought > 1 ? `${planetName(step.planet)} · ` : "";
      const name =
        step.kind === "bombardment"
          ? "Bombardment"
          : step.kind === "space_cannon_defense"
            ? "Space cannon defense"
            : `Round ${step.round}`;
      const table = groundTable(
        item,
        `${planetName(step.planet)} · ${step.kind === "ground_round" ? `Ground combat · ${name}` : name}`,
        viewer,
        printed,
      );
      // The last roll on a planet says how its ground combat ended.
      const lastHere = !ground.slice(index + 1).some((other) => other.step.planet === step.planet);
      const end = ended.find(({ fact }) => fact.combat !== "space" && fact.planet === step.planet);
      return {
        key: `${item.invasion_seq}:${step.planet}:${step.kind}:${step.round}:${index}`,
        label: `${prefix}${name}`,
        table: lastHere && end ? { ...table, score: outcome(end.fact) } : table,
      };
    });
    return {
      kind: "invasion",
      landing: {
        system: system ?? "",
        label,
        forces: [],
        chosen: null,
        planets,
        landed: landed.length,
        stays: "",
      },
      records,
    };
  };

  const production = (): ProductionView => {
    const places = new Map<string, { name: string; units: string[] }>();
    for (const { fact } of produced) {
      const id = fact.planet ?? "space";
      const place = places.get(id) ?? {
        name: fact.planet
          ? `Planet ${tiles.get(fact.system ?? system ?? "")?.planets?.find((item) => item.id === fact.planet)?.label ?? fact.planet}`
          : "Space area",
        units: [],
      };
      place.units.push(...Array.from({ length: fact.count }, () => fact.unit));
      places.set(id, place);
    }
    const made = produced.reduce((sum, { fact }) => sum + fact.count, 0);
    return {
      kind: "production",
      sheet: {
        system: system ?? "",
        label,
        groups: [],
        places: [...places.entries()].map(([id, place]) => ({
          id,
          name: place.name,
          builds: unitsText(place.units, printed),
        })),
        made,
        cost: 0,
        discount: 0,
        short: 0,
        gauges: [],
        stage: "build",
      },
      payment: {
        title: "Payment",
        cost: 0,
        paid: 0,
        unit: "resources",
        editable: false,
        summary: "",
        goods: null,
        actions: [],
      },
    };
  };

  /** What paid the production, in one line: "Paid with Jord, Lodor · 1 trade good". */
  const paid = () => {
    const payments = facts("paid").filter(({ at }) => at <= productionAt);
    const planets = payments.flatMap(({ fact }) =>
      fact.with.type === "planet" ? [fact.with.planet] : [],
    );
    const goods = payments.filter(({ fact }) => fact.with.type === "trade_good").length;
    const planetLabel = (id: string) =>
      [...tiles.values()].flatMap((tile) => tile.planets ?? []).find((item) => item.id === id)
        ?.label ?? id;
    const parts = [
      planets.length ? planets.map(planetLabel).join(", ") : null,
      goods ? plural(goods, "trade good") : null,
    ].filter((part) => part !== null);
    return parts.length ? `Paid with ${parts.join(" · ")}` : "Nothing paid";
  };

  const content: StepContentView =
    step === 0
      ? activation()
      : step === 1
        ? movement()
        : step === 2
          ? combat()
          : step === 3
            ? invasion()
            : production();
  const shipsMoved = moved.length;
  const caption = (index: number) =>
    index === 0
      ? system
        ? `#${system}`
        : ""
      : index === 1
        ? plural(shipsMoved, "ship")
        : index === 2
          ? rounds.length || underWay
            ? plural(Math.max(underWay?.round ?? 0, ...rounds.map((round) => round.round)), "round")
            : ""
          : index === 3
            ? landed.length
              ? plural(landed.length, "unit")
              : ""
            : plural(
                produced.reduce((sum, { fact }) => sum + fact.count, 0),
                "unit",
              );
  const tabs = steps.map((index): StepTabView => ({
    name: STEPS[index],
    status: "done",
    caption: caption(index),
  }));
  // An exploration is shown with the step that it came in (the frontier token after the
  // movement, a planet after the invasion), as Live showed its card: printed, and what came of it.
  const explored = facts("explored")
    .filter(({ at }) => (steps.find((index) => cursors[index] > at) ?? steps.at(-1)) === step)
    .flatMap(({ fact }): BlockView[] => {
      const card = exploredCard(update, catalog, fact);
      return [
        { kind: "source", name: card.name, type: card.type, text: card.text },
        ...(card.effect
          ? [{ kind: "note" as const, tone: "quiet" as const, text: card.effect }]
          : []),
      ];
    });
  const playedNow = live && shown === steps.length - 1;
  const note =
    step === 4
      ? paid()
      : step === 2 && !rounds.length
        ? outcome(spaceEnd?.fact) || "No round was rolled"
        : "";
  return {
    update: then,
    system,
    label,
    action: {
      kind: "tactical",
      context: `Tactical action · ${label}`,
      badge: null,
      tabs,
      selected: shown,
      current: null,
      contentKey: `past:${inspect.turn}:${step}`,
      interrupt: null,
      closing: explored,
      task: {
        step: shown,
        title: STEPS[step],
        pill: playedNow
          ? { tone: "live", label: "Played now" }
          : { tone: "done", label: "Resolved" },
        trail: [],
        edit: null,
        help: [],
        role: "decide",
        content,
        footer: { note, error: false, actions: [] },
      },
    },
  };
}

/**
 * The battle table of one roll of an invasion (a bombardment, the space cannon defense, a round of
 * a ground combat): the invader, then the seat that it fights, with their ground forces on the
 * planet before the roll. A seat whose dice are of units that are not on the planet (the ships of a
 * bombardment, a PDS) has a row for each kind of them.
 */
function groundTable(
  item: GroundStepView,
  label: string,
  viewer: string,
  printed: Catalog | null,
): BattleTableView {
  const { step } = item;
  const units = step.before.filter((unit) => unit.planet === step.planet);
  const dice: CombatDieRoll[] = step.dice.map((die) => ({
    player: die.player,
    unit: die.group,
    roll: die.face,
    target: die.target,
    hit: die.hit,
  }));
  // The seats of the fight: the invader, and the seats that rolled or took hits. A seat that only
  // shares the planet (coexistence) does not fight.
  const seats = [
    ...new Set([
      item.invader,
      ...dice.flatMap((die) => (die.player ? [die.player] : [])),
      ...Object.keys(step.hits),
    ]),
  ];
  return {
    label,
    score: "",
    owed: null,
    sides: seats.map((seat) => {
      const own = units.filter((unit) => unit.owner === seat);
      const rows = sideRows(own, { dice_rolls: dice }, seat, printed);
      const kinds = new Set(own.map((unit) => unitKey(unit.unit_type, printed)));
      // The dice of units that are not on the planet: one row for each roll.
      const away = new Map<string, CombatDieRoll[]>();
      for (const die of dice) {
        if (
          die.player === seat &&
          !die.unit.split("+").some((unit) => kinds.has(unitKey(unit, printed)))
        ) {
          away.set(die.unit, [...(away.get(die.unit) ?? []), die]);
        }
      }
      const extra = [...away.entries()].map(([unit, rolled]): BattleRowView => ({
        unit: unitType(unit.split("+")[0]),
        label: unit
          .split("+")
          .map((part) => unitLabel(part, printed))
          .join(", "),
        count: null,
        target: rolled[0].target,
        targetTitle: `Hits on ${rolled[0].target}+`,
        modified: false,
        dice: rolled.map((die) => ({ roll: die.roll, hit: die.hit })),
        hits: "",
        scored: rolled.some((die) => die.hit),
        pills: [],
      }));
      return {
        seat,
        role: seat === item.invader ? "Attacker" : "Defender",
        isViewer: seat === viewer,
        hits: step.hits[seat] ?? (dice.some((die) => die.player === seat) ? 0 : null),
        rows: [...rows, ...extra],
      };
    }),
  };
}
