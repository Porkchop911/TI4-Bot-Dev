// The Log: what the seats did since the last turn of the viewer, read from the public events of
// the game (`events.ts`). A turn is the events of one `action_id`; a phase that is no action phase
// is one entry with what the seats did in it. Only the typed facts of an event are read (`fact`,
// `roll`), never its `detail` text, and every id is named as the rest of the page names it.
// Presentation only: no rule is decided here.
import {
  type LogAction,
  type LogGroupView,
  type LogItemView,
  type LogLineView,
  type LogPhaseView,
  type LogTurnView,
  type LogView,
  plural,
} from "../../model";
import type { EventLog } from "../events";
import type {
  Catalog,
  ComponentSource,
  GameEvent,
  Phase,
  PublicFact,
  PublicRoll,
  SessionUpdate,
  TokenPool,
  TurnAction,
} from "../wire";
import { type ExploredView, exploredCard } from "./exploration";
import { systemLabel, tilesOf } from "./movement";
import {
  byUnitOrder,
  humanizeId,
  isShip,
  phaseName,
  seatLabel,
  strategyCard,
  unitLabel,
  unitType,
} from "./names";
import { planetNames } from "./payment";

// --- the names of the ids ---------------------------------------------------------------------

/** How the Log names what an event names by id. */
interface Names {
  seat(id: string): string;
  system(id: string): string;
  planet(id: string): string;
  /** "2 Carrier I", "Cruiser I". */
  units(id: string, count: number): string;
  tech(id: string): string;
  /** A strategy card: "Warfare". */
  strategy(id: string): string;
  objective(id: string): string;
  /** A card that is played: an action card, or a promissory note. */
  card(id: string): string;
  leader(id: string): string;
  relic(id: string): string;
  explore(id: string): string;
  /** An exploration of the events: its card as printed and what came of it. */
  explored(fact: Extract<PublicFact, { kind: "explored" }>): ExploredView;
  ability(id: string): string;
  /** A vote: "For", "Against", or what is elected. */
  outcome(id: string): string;
  agenda(id: string): string;
}

const byId = (
  id: string,
  ...lists: (readonly { id: string; name: string }[] | undefined)[]
): string | null => {
  for (const list of lists) {
    const found = list?.find((item) => item.id === id);
    if (found) {
      return found.name.trim();
    }
  }
  return null;
};

function namesOf(update: SessionUpdate, printed: Catalog | null): Names {
  const tiles = tilesOf(update);
  const planets = planetNames(update);
  const components = printed?.components;
  const records = printed?.records;
  const seats = new Set(update.view.players.map((player) => player.id));
  const planet = (id: string) => planets.get(id) ?? byId(id, records?.planets) ?? humanizeId(id);
  const objective = (id: string) =>
    byId(
      id,
      records?.public_objectives,
      records?.secret_objectives,
      components?.public_objectives,
      components?.secret_objectives,
    ) ?? humanizeId(id);
  return {
    seat: (id) => seatLabel(update, id, printed),
    system: (id) => systemLabel(tiles, id),
    planet,
    units: (id, count) =>
      count === 1 ? unitLabel(id, printed) : `${count} ${unitLabel(id, printed, count)}`,
    strategy: (id) => strategyCard(id, printed).name,
    tech: (id) => byId(id, components?.technologies, records?.technologies) ?? humanizeId(id),
    objective,
    card: (id) =>
      components?.action_cards.find((card) => card.id === id || card.name === id)?.name ??
      records?.action_cards.find((card) => card.id === id || card.automation_id === id)?.name ??
      byId(id, records?.promissory_notes) ??
      humanizeId(id),
    leader: (id) => byId(id, components?.leaders, records?.leaders) ?? humanizeId(id),
    relic: (id) => byId(id, components?.relics, records?.relics) ?? humanizeId(id),
    explore: (id) => byId(id, printed?.exploration.cards, records?.explores) ?? humanizeId(id),
    explored: (fact) => exploredCard(update, printed?.exploration ?? null, fact),
    ability: (id) => byId(id, components?.abilities, records?.abilities) ?? humanizeId(id),
    agenda: (id) => byId(id, components?.agendas) ?? humanizeId(id),
    outcome: (id) =>
      id === "for"
        ? "For"
        : id === "against"
          ? "Against"
          : seats.has(id)
            ? seatLabel(update, id, printed)
            : planets.has(id) || byId(id, records?.planets)
              ? planet(id)
              : objective(id),
  };
}

// --- the details of an entry, in groups -------------------------------------------------------

/** A line that is being made: plain, or a count of units or of items that grows. */
type Line =
  | { kind: "text"; seat: string | null; text: string }
  | { kind: "units"; seat: string | null; head: string; units: Map<string, number> }
  | { kind: "list"; seat: string | null; head: string; items: string[]; goods: number }
  | { kind: "tokens"; seat: string | null; pools: Map<TokenPool, number> }
  | {
      kind: "vote";
      seat: string | null;
      /** The outcome in words; null: abstained. */
      outcome: string | null;
      planets: string[];
      votes: number;
    };

type UnitsLine = Extract<Line, { kind: "units" }>;
type ListLine = Extract<Line, { kind: "list" }>;
type TokensLine = Extract<Line, { kind: "tokens" }>;
type VoteLine = Extract<Line, { kind: "vote" }>;

/** The pools of the command sheet, in the order of the sheet. */
const POOLS: readonly TokenPool[] = ["tactic", "fleet", "strategy"];

/**
 * The groups in the order that an entry shows them, by the start of their key. A group of
 * another key comes last.
 */
const GROUPS = [
  "Activation",
  "Movement",
  "Space cannon",
  "Space combat",
  "Invasion",
  "Ground combat",
  "Exploration",
  "Production",
  "Primary",
  "Secondaries",
  "Picks",
  "Scored",
  "Revealed",
  "Command tokens",
  "Agenda",
  "Effects",
  "Cards played",
  "Transactions",
  "Rolls",
];

/** The groups of an entry by key; the title of a group is its key unless it is given one. */
class Groups {
  private readonly groups = new Map<string, { title: string; lines: Map<string, Line> }>();
  private count = 0;

  constructor(private readonly names: Names) {}

  private group(key: string) {
    let found = this.groups.get(key);
    if (!found) {
      found = { title: key, lines: new Map() };
      this.groups.set(key, found);
    }
    return found;
  }

  /** The title of a group whose key is no title: a combat, an agenda. */
  title(key: string, title: string) {
    this.group(key).title = title;
  }

  /** The line of `key` in `group`, made when there is none. A null key is always a new line. */
  line<T extends Line>(group: string, key: string | null, make: () => T): T {
    const lines = this.group(group).lines;
    const at = key ?? `#${this.count++}`;
    const found = lines.get(at);
    if (found) {
      return found as T;
    }
    const made = make();
    lines.set(at, made);
    return made;
  }

  text(group: string, seat: string | null, text: string) {
    this.line(group, null, () => ({ kind: "text", seat, text }));
  }

  /**
   * Adds units to the line of the seat and `head` ("Lost: 2 Fighter I, Carrier I"), or of `key`
   * when it is given (the losses of one round).
   */
  units(group: string, seat: string | null, head: string, unit: string, count = 1, key?: string) {
    const line = this.line<UnitsLine>(group, key ?? `${seat}|${head}`, () => ({
      kind: "units",
      seat,
      head,
      units: new Map<string, number>(),
    }));
    line.units.set(unit, (line.units.get(unit) ?? 0) + count);
  }

  /** Adds command tokens to the line of the seat: "Gained 2 tactic tokens, 1 fleet token". */
  tokens(group: string, seat: string | null, pool: TokenPool, count: number) {
    const line = this.line<TokensLine>(group, `${seat}|tokens`, () => ({
      kind: "tokens",
      seat,
      pools: new Map<TokenPool, number>(),
    }));
    line.pools.set(pool, (line.pools.get(pool) ?? 0) + count);
  }

  /** Adds an item to the line of `key`: "Spent resources: Jord, Arretze, 2 trade goods". */
  item(group: string, seat: string | null, head: string, item: string | null) {
    const line = this.line<ListLine>(group, `${seat}|${head}`, () => ({
      kind: "list",
      seat,
      head,
      items: [],
      goods: 0,
    }));
    if (item === null) {
      line.goods += 1;
    } else {
      line.items.push(item);
    }
  }

  view(): LogGroupView[] {
    const rank = (key: string) => {
      const at = GROUPS.findIndex((item) => key.startsWith(item));
      return at < 0 ? GROUPS.length : at;
    };
    return [...this.groups]
      .filter(([, group]) => group.lines.size > 0)
      .sort(([a], [b]) => rank(a) - rank(b))
      .map(([key, group]) => {
        const views = [...group.lines.values()].map((line) => this.lineView(line));
        return {
          title: group.title,
          lines: key === "Secondaries" ? bySeat(views) : views,
          ...(key.startsWith("Agenda|") ? { agenda: true as const } : {}),
        };
      });
  }

  private lineView(line: Line): LogLineView {
    switch (line.kind) {
      case "text":
        return { seat: line.seat, text: line.text };
      case "units": {
        const units = [...line.units]
          .sort(([a], [b]) => byUnitOrder(a, b))
          .map(([unit, count]) => this.names.units(unit, count));
        return { seat: line.seat, text: `${line.head}${units.join(", ")}` };
      }
      case "list": {
        const items = [...line.items, ...(line.goods ? [plural(line.goods, "trade good")] : [])];
        return { seat: line.seat, text: `${line.head}${items.join(", ")}` };
      }
      case "tokens": {
        const pools = POOLS.filter((pool) => line.pools.get(pool)).map((pool) =>
          plural(line.pools.get(pool) ?? 0, `${pool} token`),
        );
        return { seat: line.seat, text: `Gained ${pools.join(", ")}` };
      }
      case "vote": {
        const vote = { outcome: line.outcome, votes: line.votes, planets: line.planets };
        if (line.outcome === null) {
          return { seat: line.seat, text: "Abstained", vote };
        }
        const votes = line.votes ? ` · ${plural(line.votes, "vote")}` : "";
        const planets = line.planets.length ? `: ${line.planets.join(", ")}` : "";
        // "Voted" tells a vote from a card played in the same agenda, and an elected planet
        // from the planets that voted.
        return { seat: line.seat, text: `Voted ${line.outcome}${votes}${planets}`, vote };
      }
    }
  }
}

/** The answer of a seat to a secondary: the first line of its part. */
const FOLLOWED = "Followed";
const DECLINED = "Declined";

/**
 * The lines of the secondaries, by seat: the seats in the order that they answered, each with
 * its answer first and then what it did; the seats that declined last.
 */
function bySeat(lines: LogLineView[]): LogLineView[] {
  const order: (string | null)[] = [];
  for (const line of lines) {
    if (!order.includes(line.seat)) {
      order.push(line.seat);
    }
  }
  const declined = (seat: string | null) =>
    lines.some((line) => line.seat === seat && line.text === DECLINED);
  const rank = (seat: string | null) => order.indexOf(seat) + (declined(seat) ? order.length : 0);
  const answer = (line: LogLineView) => (line.text === FOLLOWED || line.text === DECLINED ? 0 : 1);
  return lines
    .map((line, at) => ({ line, at }))
    .sort(
      (a, b) =>
        rank(a.line.seat) - rank(b.line.seat) || answer(a.line) - answer(b.line) || a.at - b.at,
    )
    .map(({ line }) => line);
}

/** The step of a roll, by its reason. */
function rollGroup(roll: PublicRoll): string {
  switch (roll.reason.split(":")[0]) {
    case "gravity rift":
      return "Movement";
    case "space cannon":
      return "Space cannon";
    case "space combat":
    case "anti-fighter barrage":
      return "Space combat";
    case "bombardment":
    case "space cannon defense":
      return "Invasion";
    case "ground combat":
      return "Ground combat";
    default:
      return "Rolls";
  }
}

/** What a component action uses, by name. */
function sourceName(source: ComponentSource, names: Names, played: string | null): string {
  switch (source.type) {
    case "technology":
      return names.tech(source.tech);
    case "leader":
      return names.leader(source.leader);
    case "action_card":
      return played ?? "Action card";
    case "relic":
      return names.relic(source.relic);
    case "relic_fragments":
      return `${humanizeId(source.fragment.toLowerCase())} relic fragments`;
    case "exploration":
      return names.explore(source.card);
    case "faction":
      return names.ability(source.ability);
    case "expedition":
      return "Expedition";
    case "deal_payment":
      return "Payment of a deal";
    default:
      return "Component";
  }
}

/**
 * A combat of an entry: a group of its own, from its first roll or loss to the fact of its end
 * (`combat_ended`). A space combat is in a system, a ground combat on a planet.
 */
interface Combat {
  key: string;
  kind: "space" | "ground";
  /** The system of a space combat, the planet of a ground combat. */
  place: string;
  attacker: string | null;
  defender: string | null;
  /** The round so far; 0 before the first (the anti-fighter barrage). */
  round: number;
  /** The seats and values that rolled in the round: a second roll of one starts the next round. */
  rolled: Set<string>;
  /** No decision since the last roll: the round goes on. */
  open: boolean;
  /** The host told its end. */
  ended: boolean;
  /** A decision told the retreat ("Retreated to …"). */
  retreatTold: boolean;
}

/** An agenda of the agenda phase: its group, who voted in it, and whether the host resolved it. */
interface Agenda {
  key: string;
  number: number;
  voted: Set<string>;
  resolved: boolean;
  /** The speaker broke a tie: the end of the agenda need not say that the speaker decided. */
  tieBroken: boolean;
}

/** The details of the events of one entry. `actor` is the seat of a turn; `strategic` sorts by seat. */
function detailsOf(
  events: readonly GameEvent[],
  names: Names,
  scope: {
    actor: string | null;
    strategic: boolean;
    phase: Phase | null;
    /** The action of the turn (`actionOf`): another action in it is a line. */
    action?: TurnAction | null;
  },
): { groups: LogGroupView[]; facts: { seat: string | null; fact: PublicFact }[] } {
  const groups = new Groups(names);
  const facts: { seat: string | null; fact: PublicFact }[] = [];
  /** The group of what a seat did that has no group of its own. */
  const effects = (seat: string | null) =>
    scope.strategic ? (seat === scope.actor ? "Primary" : "Secondaries") : "Effects";
  // In a strategic action what a seat did is in the part of its seat; else in its step.
  const own = (step: string, seat: string | null) => (scope.strategic ? effects(seat) : step);

  // The combats, each a group. The rounds of a space combat are not told by the host: the
  // engine rolls the units of one seat and one value together, so a second roll of the same seat
  // and value, or a roll after a decision, is the next round. A round of ground combat is a fact.
  const combats: Combat[] = [];
  let activated: string | null = null;
  const combat = (kind: Combat["kind"], place: string, make = true): Combat | null => {
    const found = combats.findLast((item) => item.kind === kind && item.place === place);
    if (found && !found.ended) {
      return found;
    }
    if (!make) {
      return found ?? null;
    }
    const made: Combat = {
      key: `${kind === "space" ? "Space combat" : "Ground combat"}|${place}|${combats.length}`,
      kind,
      place,
      attacker: null,
      defender: null,
      round: 0,
      rolled: new Set(),
      open: false,
      ended: false,
      retreatTold: false,
    };
    combats.push(made);
    return made;
  };
  /** The space combat that is under way, else the last one, else a new one in `system`. */
  const space = (system?: string | null): Combat => {
    const place = system ?? activated ?? "";
    const open = combats.findLast((item) => item.kind === "space" && !item.ended);
    return open ?? (combat("space", place) as Combat);
  };
  /** A seat that fights: the attacker is the seat of the turn, the other is the defender. */
  const fights = (fight: Combat, seat: string | null) => {
    if (!seat) {
      return;
    }
    fight.attacker ??= scope.actor ?? seat;
    if (seat !== fight.attacker) {
      fight.defender ??= seat;
    }
  };
  /** The planet of the round of ground combat that is under way: its rolls need no guess. */
  let groundPlanet: string | null = null;
  /** The group of the last roll: a loss that no decision chose is of the fire before it. */
  let fire: string | null = null;
  /**
   * The group and the line of a loss: in its combat, by round; else in the step of the fire
   * before it (space cannon, bombardment); else in the step of the place.
   */
  const lossAt = (seat: string, system?: string, planet?: string): [string, string] => {
    if (planet) {
      const fight = combat("ground", planet, false);
      if (fight) {
        return [fight.key, `${fight.round}`];
      }
      return [own("Invasion", seat), ""];
    }
    const fight = combats.findLast((item) => item.kind === "space");
    if (fight && (!fight.ended || fire === fight.key)) {
      return [fight.key, `${fight.round}`];
    }
    if (fire === "Space cannon") {
      return [own("Space cannon", seat), ""];
    }
    if (fight) {
      return [fight.key, `${fight.round}`];
    }
    const made = space(system);
    fights(made, seat);
    return [made.key, `${made.round}`];
  };

  // The agendas of the agenda phase, each a group: a vote of a seat that voted, or after the
  // host resolved the agenda, is of the next agenda.
  const agendas: Agenda[] = [];
  const agenda = (next: boolean): Agenda => {
    const last = agendas.at(-1);
    if (last && !last.resolved && !next) {
      return last;
    }
    const number = agendas.length + 1;
    const made: Agenda = {
      key: `Agenda|${number}`,
      number,
      voted: new Set(),
      resolved: false,
      tieBroken: false,
    };
    groups.title(made.key, `Agenda ${number}`);
    agendas.push(made);
    return made;
  };
  /** The vote line of each seat in the agenda, for the planets that it exhausts. */
  const votes = new Map<string, VoteLine>();

  for (const entry of events) {
    const roll = entry.event.kind === "dice_rolled" ? entry.roll : undefined;
    if (roll) {
      const reason = roll.reason.split(":")[0];
      const seat = roll.roller ?? null;
      const step = rollGroup(roll);
      let group = step;
      let text = roll.label;
      if (reason === "space combat") {
        const fight = space(roll.system);
        fights(fight, seat);
        const key = `${seat}|${roll.hits_on}`;
        if (!fight.open || fight.rolled.has(key)) {
          fight.round += 1;
          fight.rolled = new Set();
          fight.open = true;
        }
        fight.rolled.add(key);
        group = fight.key;
        text = `Round ${fight.round}`;
      } else if (reason === "anti-fighter barrage") {
        const fight = space(roll.system);
        fights(fight, seat);
        group = fight.key;
      } else if (step === "Rolls" && combats.some((item) => item.kind === "space" && !item.ended)) {
        // A roll in the middle of a space combat (an ability, a card) is a part of it.
        group = space(roll.system).key;
      } else if (reason === "ground combat") {
        const fight = groundPlanet ? combat("ground", groundPlanet, false) : null;
        if (fight) {
          fights(fight, seat);
          group = fight.key;
          text = `Round ${fight.round}`;
        } else {
          group = "Invasion";
        }
      }
      fire = group;
      const unit = roll.subject?.unit;
      groups.text(group, seat, unit ? `${text} · ${names.units(unit, 1)}` : text);
      continue;
    }
    // A decision, or a change of the state, between the rolls ends a round of space combat.
    for (const fight of combats) {
      fight.open = false;
    }
    const fact = entry.fact;
    if (
      (entry.event.kind !== "decision_resolved" && entry.event.kind !== "state_changed") ||
      !fact
    ) {
      continue;
    }
    const seat = entry.actor ?? null;
    facts.push({ seat, fact });
    switch (fact.kind) {
      case "action_started":
        // One is the action of the turn; another is an action within it (an agent).
        if (fact.action !== scope.action && fact.action.type === "component") {
          groups.text(
            effects(seat),
            seat,
            `Component action · ${sourceName(fact.action.source, names, null)}`,
          );
        }
        break;
      case "turn_ended":
        break;
      case "transaction_opened":
        groups.text("Transactions", seat, `Transaction with ${names.seat(fact.with)}`);
        break;
      case "transaction_proposed":
        groups.text("Transactions", seat, `Offer to ${names.seat(fact.to)}`);
        break;
      case "negotiation_ended":
        groups.text("Transactions", seat, `Ended the negotiation with ${names.seat(fact.with)}`);
        break;
      case "transaction_answered": {
        const verb =
          fact.answer === "accept"
            ? "Accepted"
            : fact.answer === "refuse"
              ? "Refused"
              : "Countered";
        groups.text("Transactions", seat, `${verb} the offer of ${names.seat(fact.proposer)}`);
        break;
      }
      case "activated":
        activated = fact.system;
        groups.text(own("Activation", seat), seat, `Activated ${names.system(fact.system)}`);
        break;
      case "moved":
        groups.units(
          own("Movement", seat),
          seat,
          `${names.system(fact.from)} → ${names.system(fact.to)}: `,
          fact.unit,
        );
        break;
      case "loaded": {
        const place = fact.planet
          ? names.planet(fact.planet)
          : fact.system
            ? names.system(fact.system)
            : null;
        groups.units(
          own("Movement", seat),
          seat,
          place ? `Loaded on ${place}: ` : "Loaded: ",
          fact.unit,
        );
        break;
      }
      case "produced":
      case "unit_placed": {
        const place = fact.planet
          ? names.planet(fact.planet)
          : fact.system
            ? names.system(fact.system)
            : null;
        const group =
          fact.kind === "produced" && entry.stage === "production"
            ? own("Production", seat)
            : effects(seat);
        const verb = fact.kind === "produced" ? "Produced" : "Placed";
        groups.units(
          group,
          seat,
          place ? `${verb} on ${place}: ` : `${verb}: `,
          fact.unit,
          fact.kind === "produced" ? fact.count : 1,
        );
        break;
      }
      case "paid": {
        const group = entry.stage === "production" ? own("Production", seat) : effects(seat);
        groups.item(
          group,
          seat,
          `Spent ${fact.spend}: `,
          fact.with.type === "planet" ? names.planet(fact.with.planet) : null,
        );
        break;
      }
      case "structure_placed":
        groups.text(
          effects(seat),
          seat,
          `Placed ${names.units(fact.unit, 1)} on ${names.planet(fact.planet)}`,
        );
        break;
      case "casualty":
      case "destroyed": {
        // A loss that a seat chose, or that the game took: the host tells each once.
        const [group, round] = lossAt(fact.seat, fact.system, fact.planet);
        groups.units(
          group,
          fact.seat,
          "Lost: ",
          fact.unit,
          fact.kind === "destroyed" ? fact.count : 1,
          `${fact.seat}|lost|${round}`,
        );
        break;
      }
      case "sustained": {
        const [group, round] = lossAt(fact.seat, fact.system, fact.planet);
        groups.units(
          group,
          fact.seat,
          "Sustained damage: ",
          fact.unit,
          1,
          `${fact.seat}|sustained|${round}`,
        );
        break;
      }
      case "sustain_declined": {
        const [group, round] = lossAt(fact.seat, fact.system, fact.planet);
        groups.line(group, `${fact.seat}|no sustain|${round}`, () => ({
          kind: "text",
          seat: fact.seat,
          text: "Did not sustain damage",
        }));
        break;
      }
      case "retreat_announced":
        if (fact.retreat) {
          const fight = space(fact.system);
          fights(fight, fact.seat);
          groups.text(fight.key, fact.seat, "Announced a retreat");
        }
        break;
      case "retreated": {
        const fight = space(fact.from);
        fights(fight, fact.seat);
        fight.retreatTold = true;
        groups.text(fight.key, fact.seat, `Retreated to ${names.system(fact.to)}`);
        break;
      }
      case "combat_ended": {
        const fight =
          fact.combat === "space"
            ? space(fact.system)
            : (combat("ground", fact.planet ?? "") as Combat);
        fight.attacker = fact.attacker;
        fight.defender = fact.defender;
        if (fact.retreated && !fight.retreatTold) {
          groups.text(fight.key, fact.retreated, "Retreated");
        }
        if (fact.winner) {
          groups.text(fight.key, fact.winner, `Won by ${names.seat(fact.winner)}`);
        } else if (!fact.retreated) {
          groups.text(fight.key, null, "Both sides destroyed");
        }
        fight.ended = true;
        break;
      }
      case "control_gained": {
        const fight = combat("ground", fact.planet, false);
        const group = fight
          ? fight.key
          : scope.phase === "action"
            ? own("Invasion", fact.seat)
            : effects(fact.seat);
        groups.text(
          group,
          fact.seat,
          fact.from
            ? `Took ${names.planet(fact.planet)} from ${names.seat(fact.from)}`
            : `Took ${names.planet(fact.planet)}`,
        );
        break;
      }
      case "explored": {
        const place = fact.planet
          ? names.planet(fact.planet)
          : fact.system
            ? names.system(fact.system)
            : null;
        const { effect } = names.explored(fact);
        groups.text(
          own("Exploration", fact.seat),
          fact.seat,
          `Explored${place ? ` ${place}` : ""}: ${names.explore(fact.card)}${effect ? ` · ${effect}` : ""}`,
        );
        break;
      }
      case "landed":
        groups.units(
          own("Invasion", seat),
          seat,
          `Landed on ${names.planet(fact.planet)}: `,
          fact.unit,
        );
        break;
      case "researched":
        groups.text(effects(seat), seat, `Researched ${names.tech(fact.tech)}`);
        break;
      case "strategy_picked":
        // The group says what the line is: "Picks", "Scored".
        groups.text("Picks", seat, names.strategy(fact.card));
        break;
      case "voted": {
        if (seat === null) {
          break;
        }
        const last = agendas.at(-1);
        const at = agenda(!last || last.resolved || last.voted.has(seat));
        if (at !== last) {
          votes.clear();
        }
        at.voted.add(seat);
        const line = groups.line<VoteLine>(at.key, `vote|${seat}`, () => ({
          kind: "vote",
          seat,
          outcome: fact.outcome ? names.outcome(fact.outcome) : null,
          planets: [],
          votes: 0,
        }));
        votes.set(seat, line);
        break;
      }
      case "planet_exhausted_for_vote": {
        const line = seat === null ? undefined : votes.get(seat);
        if (line) {
          line.planets.push(names.planet(fact.planet));
          line.votes += fact.votes ?? 0;
        }
        break;
      }
      case "tie_broken": {
        const at = agenda(false);
        at.tieBroken = true;
        groups.text(at.key, seat, `Broke the tie: ${names.outcome(fact.outcome)}`);
        break;
      }
      case "agenda_resolved": {
        const at = agenda(false);
        at.resolved = true;
        const tally = Object.entries(fact.votes)
          .filter(([, count]) => count > 0)
          .sort(([, a], [, b]) => b - a)
          .map(([outcome, count]) => `${names.outcome(outcome)} ${count}`);
        groups.title(
          at.key,
          `${names.agenda(fact.agenda)} · ${fact.outcome ? names.outcome(fact.outcome) : "no outcome"}${tally.length ? ` (${tally.join(" · ")})` : ""}`,
        );
        if (fact.speaker_decided && !at.tieBroken) {
          groups.text(at.key, null, "Decided by the speaker");
        }
        if (fact.discarded) {
          groups.text(at.key, null, "Discarded with no effect");
        } else if (fact.law) {
          groups.text(at.key, null, fact.enacted ? "Enacted as a law" : "Not enacted");
        }
        break;
      }
      case "card_played": {
        // In the agenda phase a card is of the agenda that it was played for.
        const last = agendas.at(-1);
        const group =
          scope.phase === "agenda" && (!last?.resolved || agendas.length < 2)
            ? agenda(!!last?.resolved).key
            : "Cards played";
        groups.text(group, seat, `Played ${names.card(fact.card)}`);
        break;
      }
      case "objective_scored":
        groups.text(
          scope.phase === "status" ? "Scored" : effects(seat),
          seat,
          `${scope.phase === "status" ? "" : "Scored "}${names.objective(fact.objective)}${fact.secret ? " · secret objective" : ""}`,
        );
        break;
      case "objective_revealed":
        if (scope.phase === "status") {
          groups.text("Revealed", null, names.objective(fact.objective));
        } else {
          groups.text(effects(seat), seat, `Revealed ${names.objective(fact.objective)}`);
        }
        break;
      case "tokens_gained":
        groups.tokens(effects(seat), seat, fact.pool, fact.count);
        break;
      case "secondary_followed":
        groups.text(
          scope.strategic ? "Secondaries" : effects(seat),
          seat,
          fact.followed ? FOLLOWED : DECLINED,
        );
        break;
      case "token_bought_with_influence":
        // The purchase is its payment ("Spent influence") and the token it gains.
        break;
      case "diplomacy_chosen":
        groups.text(effects(seat), seat, `Chose ${names.system(fact.system)}`);
        break;
      case "speaker_chosen":
        groups.text(effects(seat), seat, `Chose ${names.seat(fact.seat)} as speaker`);
        break;
      case "commodities_replenished":
        groups.item(effects(seat), seat, "Replenished commodities: ", names.seat(fact.seat));
        break;
      case "planet_readied":
        groups.item(effects(seat), seat, "Readied ", names.planet(fact.planet));
        break;
      case "tokens_redistributed":
        {
          const sheet = `${fact.tactic} tactic · ${fact.fleet} fleet · ${fact.strategy} strategy`;
          // The group of the status phase says what the line is.
          if (scope.phase === "status") {
            groups.text("Command tokens", seat, sheet);
          } else {
            groups.text(effects(seat), seat, `Redistributed command tokens: ${sheet}`);
          }
        }
        break;
      case "token_recalled":
        groups.text(
          effects(seat),
          seat,
          `Took the command token back from ${names.system(fact.system)}`,
        );
        break;
      case "free_tactical":
        groups.text(
          effects(seat),
          seat,
          `Activated ${names.system(fact.system)} without a command token`,
        );
        break;
      case "ground_combat_started": {
        // A further ground combat on the planet (coexistence): a combat of its own.
        const before = combat("ground", fact.planet, false);
        if (before) {
          before.ended = true;
        }
        const fight = combat("ground", fact.planet) as Combat;
        fight.attacker = fact.seat;
        groups.text(fight.key, fact.seat, "Started a further ground combat");
        groundPlanet = fact.planet;
        break;
      }
      case "ground_combat_round": {
        const fight = combat("ground", fact.planet) as Combat;
        fight.attacker ??= fact.seat;
        fight.round += 1;
        groundPlanet = fact.planet;
        break;
      }
      case "bombardment_targeted":
        groups.text(
          own("Invasion", seat),
          seat,
          `Bombarded ${names.seat(fact.target)} on ${names.planet(fact.planet)}`,
        );
        break;
      case "custodians_removed":
        groups.text(
          own("Invasion", seat),
          seat,
          `Removed the custodians from ${names.system(fact.system)}`,
        );
        break;
      case "unit_removed":
        groups.units(effects(seat), seat, `Removed in ${names.system(fact.system)}: `, fact.unit);
        break;
      default:
        // A fact of a newer host: not shown until the Log knows it.
        break;
    }
  }

  // The title of a combat: its kind, its place and its sides.
  for (const fight of combats) {
    const place =
      fight.kind === "space"
        ? fight.place
          ? names.system(fight.place)
          : ""
        : names.planet(fight.place);
    const sides =
      fight.attacker && fight.defender
        ? `${names.seat(fight.attacker)} vs ${names.seat(fight.defender)}`
        : "";
    groups.title(
      fight.key,
      [fight.kind === "space" ? "Space combat" : "Ground combat", place, sides]
        .filter(Boolean)
        .join(" · "),
    );
  }
  return { groups: groups.view(), facts };
}

// --- the turns and the phases -----------------------------------------------------------------

interface RawTurn {
  kind: "turn";
  id: string;
  seat: string;
  round: number | null;
  events: GameEvent[];
  ended: boolean;
  /** The seat ended it: a fact of the end of the turn, or a pass. */
  finished: boolean;
}

interface RawPhase {
  kind: "phase";
  phase: Phase;
  round: number;
  events: GameEvent[];
}

type Raw = RawTurn | RawPhase;

/** The actions that a turn started. */
const actionsOf = (turn: RawTurn): TurnAction[] =>
  turn.events.flatMap((entry) =>
    entry.fact?.kind === "action_started" ? [entry.fact.action] : [],
  );

/**
 * The action of a turn: its tactical or strategic action or its pass, else its first component
 * action. A component action before it (an agent at the turn menu) is a part of the turn.
 */
const actionOf = (turn: RawTurn): TurnAction | null => {
  const actions = actionsOf(turn);
  return actions.findLast((action) => action.type !== "component") ?? actions[0] ?? null;
};

/** The entries of the game: its turns and its phases, in order. */
function entriesOf(events: EventLog): { raw: Raw[]; finished: boolean } {
  const raw: Raw[] = [];
  const turns = new Map<string, RawTurn>();
  let finished = false;
  let round: number | null = null;
  const phaseEntry = (phase: Phase, at: number) => {
    const last = raw.findLast((item) => item.kind === "phase");
    if (last && last.phase === phase && last.round === at) {
      return last;
    }
    const made: RawPhase = { kind: "phase", phase, round: at, events: [] };
    raw.push(made);
    return made;
  };
  const closeTurns = () => {
    for (const turn of turns.values()) {
      turn.ended = true;
    }
  };
  for (const entry of events) {
    if (entry.visibility.visibility !== "public") {
      continue;
    }
    const event = entry.event;
    if (event.kind === "phase_transition") {
      closeTurns();
      round = event.round;
      phaseEntry(event.phase, event.round);
      continue;
    }
    if (event.kind === "game_finished") {
      closeTurns();
      finished = true;
      continue;
    }
    if (event.kind === "game_initialized") {
      round = event.round;
      continue;
    }
    round = entry.round ?? round;
    if (entry.action_id) {
      let turn = turns.get(entry.action_id);
      if (!turn) {
        // A new action ends the one before.
        closeTurns();
        const seat = entry.action_actor ?? entry.actor;
        if (!seat) {
          continue;
        }
        turn = {
          kind: "turn",
          id: entry.action_id,
          seat,
          round,
          events: [],
          ended: false,
          finished: false,
        };
        turns.set(entry.action_id, turn);
        raw.push(turn);
      }
      turn.events.push(entry);
      if (
        entry.fact?.kind === "turn_ended" ||
        (entry.fact?.kind === "action_started" && entry.fact.action.type === "pass")
      ) {
        turn.ended = true;
        turn.finished = true;
      }
      continue;
    }
    if (entry.phase && round !== null) {
      phaseEntry(entry.phase, entry.round ?? round).events.push(entry);
    }
  }
  return { raw: mergePreludes(raw), finished };
}

/**
 * The events of a turn of the Log (`LogTurnView.id`) by their places in `events`, in order: also
 * the parts before its action (an agent at the turn menu). Null when the events have no such turn
 * (an undo took it back).
 */
export function turnEvents(events: EventLog, id: string): { seat: string; at: number[] } | null {
  const turn = entriesOf(events).raw.find((item) => item.kind === "turn" && item.id === id);
  if (turn?.kind !== "turn") {
    return null;
  }
  const places = new Map(events.map((entry, at) => [entry, at]));
  return { seat: turn.seat, at: turn.events.map((entry) => places.get(entry)!) };
}

/** A phase of the Log by its id, with its facts and the places of its events in `events`. */
export interface PhaseParts {
  view: LogPhaseView;
  facts: { seat: string | null; fact: PublicFact }[];
  at: number[];
}

/**
 * A strategy, status or agenda phase of the Log (`LogPhaseView.id`), as the Log has it, with its
 * facts in order and its events by their places in `events`. Null when the events have no such
 * phase.
 */
export function phaseParts(
  update: SessionUpdate,
  events: EventLog,
  id: string,
  printed: Catalog | null = null,
): PhaseParts | null {
  const names = namesOf(update, printed);
  const { raw, finished } = entriesOf(events);
  const phase = raw.find(
    (item): item is RawPhase => item.kind === "phase" && `${item.phase}-${item.round}` === id,
  );
  if (!phase || phase.phase === "action") {
    return null;
  }
  const live = phaseLive(raw, finished, phase, update);
  const { facts } = detailsOf(phase.events, names, {
    actor: null,
    strategic: false,
    phase: phase.phase,
  });
  const places = new Map(events.map((entry, at) => [entry, at]));
  return {
    view: withGivenCards(phaseView(phase, names, live), update),
    facts,
    at: phase.events.map((entry) => places.get(entry)!),
  };
}

/** What one seat did in a turn: its answer to a secondary, its lines, and their summary. */
export interface SeatPart {
  seat: string;
  /** Its answer to the secondary of a strategic action: followed or declined. Null: no answer. */
  followed: boolean | null;
  /** Its lines, in the order of the game, with no answer: "Produced on Jord: 2 Infantry II". */
  lines: string[];
  /** At most three outcomes in one line: "produced 3 units at Jord · #01". Empty: none. */
  summary: string;
}

/** A turn of the Log by its id, with what each seat did in it. */
export interface TurnParts {
  view: LogTurnView;
  action: TurnAction | null;
  /** The card that a component action played, when a decision or the state told it. */
  card: string | null;
  /** The seats of the turn, the seat of the turn first, the others in the order that they answered. */
  seats: SeatPart[];
}

/**
 * A turn of the Log (`LogTurnView.id`) and what each seat did in it, as the Log words it: the
 * parts of the seats of a strategic action, the lines of a component action. Null when the events
 * have no such turn.
 */
export function turnParts(
  update: SessionUpdate,
  events: EventLog,
  id: string,
  printed: Catalog | null = null,
  /**
   * Events whose lines another view of the turn shows (the tactical action of a strategic
   * action, in its own steps). They still count for the summaries.
   */
  shownElsewhere: ReadonlySet<GameEvent> | null = null,
): TurnParts | null {
  const names = namesOf(update, printed);
  const turn = entriesOf(events).raw.find((item) => item.kind === "turn" && item.id === id);
  if (turn?.kind !== "turn") {
    return null;
  }
  const action = actionOf(turn);
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const view = turnView(turn, 0, false, viewer, names, update.view.round);
  const scope = {
    actor: turn.seat,
    strategic: action?.type === "strategic",
    phase: "action" as const,
    action,
  };
  const { groups: all, facts } = detailsOf(turn.events, names, scope);
  const groups = shownElsewhere
    ? detailsOf(
        turn.events.filter((entry) => !shownElsewhere.has(entry)),
        names,
        scope,
      ).groups
    : all;
  const tiles = update.view.board.map_tiles ?? [];
  const systemOf = (planet: string) =>
    tiles.find((tile) => (tile.planets ?? []).some((item) => item.id === planet))?.system_id ??
    null;
  const order = [turn.seat];
  const lines = new Map<string, string[]>();
  for (const group of groups) {
    // A transaction is no part of a strategic action: it is a deal of the turn. A component
    // action may be a deal (Trade Convoys).
    if (group.title === "Transactions" && action?.type === "strategic") {
      continue;
    }
    for (const line of group.lines) {
      const seat = line.seat ?? turn.seat;
      if (!order.includes(seat)) {
        order.push(seat);
      }
      const said = lines.get(seat) ?? [];
      // A line that says again what the line before it said tells nothing new (a redistribution
      // before and after the tactical action of Warfare that changed nothing).
      if (line.text !== FOLLOWED && line.text !== DECLINED && said.at(-1) !== line.text) {
        lines.set(seat, [...said, line.text]);
      }
    }
  }
  const answer = new Map<string, boolean>();
  for (const { seat, fact } of facts) {
    if (seat && fact.kind === "secondary_followed") {
      answer.set(seat, fact.followed);
    }
  }
  const played = facts.find(
    (item) => item.seat === turn.seat && item.fact.kind === "card_played",
  )?.fact;
  return {
    view,
    action,
    card: played?.kind === "card_played" ? played.card : null,
    seats: order.map((seat) => ({
      seat,
      followed: answer.get(seat) ?? null,
      lines: lines.get(seat) ?? [],
      summary: summaryOf(
        { ...turn, seat },
        facts.filter((item) => item.seat === seat && item.fact.kind !== "secondary_followed"),
        names,
        systemOf,
      ),
    })),
  };
}

/**
 * What a seat did at its turn menu before it chose its action (a transaction, a component action
 * that does not end the turn, such as an agent) is a part of that turn: the game counts it as an
 * action of its own. A part that the seat ended (a fact of the end of the turn) is a turn.
 */
function mergePreludes(raw: Raw[]): Raw[] {
  const out: Raw[] = [];
  for (const item of raw) {
    const before = out.at(-1);
    if (
      item.kind === "turn" &&
      before?.kind === "turn" &&
      before.seat === item.seat &&
      !before.finished &&
      actionsOf(before).every((action) => action.type === "component")
    ) {
      // The turn keeps the id of its first part, so that it is the same row while it grows.
      before.events = [...before.events, ...item.events];
      before.round ??= item.round;
      before.ended = item.ended;
      before.finished = item.finished;
      continue;
    }
    out.push(item);
  }
  return out;
}

function turnView(
  turn: RawTurn,
  number: number,
  live: boolean,
  viewer: string | null,
  names: Names,
  fallbackRound: number,
): LogTurnView {
  const action = actionOf(turn);
  const kind: LogAction = action?.type ?? "other";
  const { groups, facts } = detailsOf(turn.events, names, {
    actor: turn.seat,
    strategic: kind === "strategic",
    phase: "action",
    action,
  });
  const mine = facts.filter((item) => item.seat === turn.seat).map((item) => item.fact);
  const activated = mine.find((fact) => fact.kind === "activated");
  const played = mine.find((fact) => fact.kind === "card_played");
  const label =
    action?.type === "tactical"
      ? activated
        ? `Tactical · ${names.system(activated.system)}`
        : "Tactical"
      : action?.type === "strategic"
        ? action.card
          ? `Strategic · ${names.strategy(action.card)}`
          : "Strategic"
        : action?.type === "component"
          ? `Component · ${sourceName(action.source, names, played ? names.card(played.card) : null)}`
          : action?.type === "pass"
            ? "Pass"
            : facts.some((item) => item.fact.kind.startsWith("transaction"))
              ? "Transaction"
              : "Turn";
  return {
    kind: "turn",
    id: turn.id,
    seat: turn.seat,
    mine: turn.seat === viewer,
    action: kind,
    label,
    round: turn.round ?? fallbackRound,
    turn: number,
    summary: kind === "pass" ? "" : summaryOf(turn, facts, names),
    groups: kind === "pass" ? [] : groups,
    live,
  };
}

/** The most important outcomes of a turn, in one line: at most three. */
function summaryOf(
  turn: RawTurn,
  facts: { seat: string | null; fact: PublicFact }[],
  names: Names,
  /**
   * The system of a planet: a production says where it was (the part of one seat of a strategic
   * action). Without it a production is its count.
   */
  systemOf: ((planet: string) => string | null) | null = null,
): string {
  const parts: string[] = [];
  let ships = 0;
  /** Where the seat produced, by system. */
  const where = new Set<string>();
  const landed: string[] = [];
  let produced = 0;
  let tokens = 0;
  let lost = 0;
  let destroyed = 0;
  let combat = false;
  let retreated = false;
  /** What the host told of the end of each space combat, from the seat of the turn. */
  const space: string[] = [];
  /** The planets of the seat: taken, or a ground combat that it lost. */
  const planets: string[] = [];
  const told = new Set<string>();
  let answered = 0;
  let followed = 0;
  let replenished = 0;
  const other: string[] = [];
  /** The explorations of the seat: "Primor: Demilitarized Zone". */
  const explored: string[] = [];
  for (const { seat, fact } of facts) {
    const own = seat === turn.seat;
    switch (fact.kind) {
      case "moved": {
        const type = unitType(fact.unit);
        ships += own && type && isShip(type) ? 1 : 0;
        break;
      }
      case "landed": {
        const planet = names.planet(fact.planet);
        if (own && !landed.includes(planet)) {
          landed.push(planet);
        }
        break;
      }
      case "produced": {
        produced += own ? fact.count : 0;
        const system = fact.system ?? (fact.planet ? systemOf?.(fact.planet) : null);
        if (own && system) {
          where.add(system);
        }
        break;
      }
      case "tokens_gained":
        tokens += own ? fact.count : 0;
        break;
      case "casualty":
      case "destroyed": {
        const count = fact.kind === "destroyed" ? fact.count : 1;
        if (fact.planet) {
          break;
        }
        if (fact.seat === turn.seat) {
          lost += count;
        } else {
          destroyed += count;
        }
        break;
      }
      case "retreated":
        combat = true;
        retreated ||= fact.seat === turn.seat;
        break;
      case "combat_ended":
        if (fact.combat === "space") {
          space.push(
            fact.retreated === turn.seat
              ? "retreated from the space combat"
              : fact.winner === turn.seat
                ? "won the space combat"
                : fact.winner && (fact.attacker === turn.seat || fact.defender === turn.seat)
                  ? "lost the space combat"
                  : fact.winner
                    ? `${names.seat(fact.winner)} won the space combat`
                    : "space combat: both sides destroyed",
          );
        } else if (fact.planet && fact.attacker === turn.seat && fact.winner !== turn.seat) {
          told.add(fact.planet);
          planets.push(`lost the ground combat on ${names.planet(fact.planet)}`);
        }
        break;
      case "control_gained":
        if (fact.seat === turn.seat && fact.from) {
          told.add(fact.planet);
          planets.push(`took ${names.planet(fact.planet)} from ${names.seat(fact.from)}`);
        }
        break;
      case "researched":
        if (own) {
          other.push(`researched ${names.tech(fact.tech)}`);
        }
        break;
      case "objective_scored":
        if (own) {
          other.push(`scored ${names.objective(fact.objective)}`);
        }
        break;
      case "structure_placed":
        if (own) {
          other.push(`placed ${names.units(fact.unit, 1)} on ${names.planet(fact.planet)}`);
        }
        break;
      case "secondary_followed":
        answered += 1;
        followed += fact.followed ? 1 : 0;
        break;
      case "diplomacy_chosen":
        if (own) {
          other.push(`chose ${names.system(fact.system)}`);
        }
        break;
      case "speaker_chosen":
        if (own) {
          other.push(`chose ${names.seat(fact.seat)} as speaker`);
        }
        break;
      case "commodities_replenished":
        replenished += own ? 1 : 0;
        break;
      case "custodians_removed":
        if (own) {
          other.push("removed the custodians");
        }
        break;
      case "explored":
        if (own) {
          const place = fact.planet
            ? names.planet(fact.planet)
            : fact.system
              ? names.system(fact.system)
              : null;
          explored.push(`${place ? `${place}: ` : ""}${names.explore(fact.card)}`);
        }
        break;
      default:
        break;
    }
  }
  combat ||= turn.events.some((entry) => entry.roll && rollGroup(entry.roll) === "Space combat");
  if (ships) {
    parts.push(`moved ${plural(ships, "ship")}`);
  }
  if (space.length) {
    // The host told the end: the outcome says it all, the losses are in the details.
    parts.push(...new Set(space));
  } else if (combat) {
    const losses = [lost && `lost ${lost}`, destroyed && `destroyed ${destroyed}`].filter(Boolean);
    parts.push(
      `${retreated ? "retreated from combat" : "combat"}${losses.length ? ` (${losses.join(", ")})` : ""}`,
    );
  } else if (lost || destroyed) {
    // Ships lost with no combat: to space cannon, to a mine.
    parts.push(
      [
        lost && `lost ${plural(lost, "ship")}`,
        destroyed && `destroyed ${plural(destroyed, "ship")}`,
      ]
        .filter(Boolean)
        .join(", "),
    );
  }
  parts.push(...planets);
  if (explored.length) {
    parts.push(
      `explored ${explored.slice(0, 2).join(", ")}${explored.length > 2 ? ` +${explored.length - 2}` : ""}`,
    );
  }
  // A planet that the seat took, or lost the fight for, is said once.
  const rest = landed.filter((planet) => ![...told].some((id) => names.planet(id) === planet));
  if (rest.length) {
    parts.push(
      `landed on ${rest.slice(0, 2).join(", ")}${rest.length > 2 ? ` +${rest.length - 2}` : ""}`,
    );
  }
  parts.push(...new Set(other));
  if (produced) {
    const at = systemOf && where.size === 1 ? ` at ${names.system([...where][0])}` : "";
    parts.push(`produced ${plural(produced, "unit")}${at}`);
  }
  if (replenished) {
    parts.push(`replenished ${plural(replenished, "seat")}`);
  }
  if (tokens) {
    parts.push(`gained ${plural(tokens, "command token")}`);
  }
  // Who followed a secondary is always told: the others' part of a strategic action.
  if (answered) {
    return [...parts.slice(0, 2), followed ? `${followed} followed` : "none followed"].join(" · ");
  }
  return parts.slice(0, 3).join(" · ");
}

/**
 * A strategy card that the game gave with no choice (the last card of the phase) is no fact: in
 * the round in play the seats hold the cards of the phase that ended, and the summary counts them.
 */
function withGivenCards(view: LogPhaseView, update: SessionUpdate): LogPhaseView {
  if (view.phase !== "strategy" || view.live || view.round !== update.view.round) {
    return view;
  }
  const held = update.view.players.reduce(
    (sum, player) => sum + (player.strategy_cards ?? []).length,
    0,
  );
  const picked = view.groups.find((group) => group.title === "Picks")?.lines.length ?? 0;
  return held > picked ? { ...view, summary: `${plural(held, "strategy card")} picked` } : view;
}

/** A phase that is no action phase is played now while it is the last entry of the game. */
const phaseLive = (raw: Raw[], finished: boolean, phase: RawPhase, update: SessionUpdate) =>
  raw.at(-1) === phase &&
  !finished &&
  phase.phase !== "action" &&
  phase.phase === update.view.phase &&
  phase.round === update.view.round;

function phaseView(phase: RawPhase, names: Names, live: boolean): LogPhaseView {
  const { groups, facts } = detailsOf(phase.events, names, {
    actor: null,
    strategic: false,
    phase: phase.phase,
  });
  const of = <K extends PublicFact["kind"]>(kind: K) =>
    facts.flatMap((item) =>
      item.fact.kind === kind ? [item.fact as Extract<PublicFact, { kind: K }>] : [],
    );
  const picked = of("strategy_picked").length;
  const scored = of("objective_scored").length;
  const revealed = of("objective_revealed").map(
    (fact) => `revealed ${names.objective(fact.objective)}`,
  );
  const resolved = of("agenda_resolved").map(
    (fact) =>
      `${names.agenda(fact.agenda)}: ${fact.outcome ? names.outcome(fact.outcome) : "no outcome"}`,
  );
  const voted = new Set(facts.filter((item) => item.fact.kind === "voted").map((item) => item.seat))
    .size;
  const summary =
    phase.phase === "strategy" && picked
      ? `${plural(picked, "strategy card")} picked`
      : phase.phase === "status"
        ? [scored && `${plural(scored, "objective")} scored`, ...revealed]
            .filter(Boolean)
            .join(" · ")
        : phase.phase === "agenda"
          ? resolved.length
            ? resolved.join(" · ")
            : voted
              ? `${plural(groups.filter((group) => group.title.startsWith("Agenda")).length, "agenda")} voted`
              : ""
          : "";
  return {
    kind: "phase",
    id: `${phase.phase}-${phase.round}`,
    phase: phase.phase,
    round: phase.round,
    label: `${phaseName(phase.phase)} · Round ${phase.round}`,
    summary,
    groups,
    live,
  };
}

/**
 * The Log of the viewer: the entries from its previous turn on, that turn first, so that the
 * player sees what it did and what the others did after it; the turn that is played now is last.
 * The current turn of the viewer (played now, or ended with no turn of another seat after it) is
 * not its previous turn. A spectator, and a seat before its previous turn, see the round in play.
 */
export function selectLog(
  update: SessionUpdate,
  events: EventLog,
  printed: Catalog | null = null,
): LogView {
  const names = namesOf(update, printed);
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const { raw, finished } = entriesOf(events);
  const lastTurn = raw.findLast((item) => item.kind === "turn");
  const counts = new Map<number, number>();
  const entries: LogItemView[] = raw.map((item) => {
    if (item.kind === "phase") {
      // A phase that is no action phase is played now while it is the last entry; the turns
      // of an action phase are its entries.
      return withGivenCards(phaseView(item, names, phaseLive(raw, finished, item, update)), update);
    }
    const round = item.round ?? update.view.round;
    const number = (counts.get(round) ?? 0) + 1;
    counts.set(round, number);
    const live =
      item === lastTurn &&
      raw.at(-1) === item &&
      !item.ended &&
      !finished &&
      update.view.phase === "action" &&
      // A turn that ended with no fact of its end: another seat has the turn now.
      (update.view.active_player ?? item.seat) === item.seat;
    return turnView(item, number, live, viewer, names, update.view.round);
  });
  const mine = (entry: LogItemView) =>
    entry.kind === "turn" && entry.seat === viewer && entry.action !== "other";
  const latest = viewer === null ? -1 : entries.findLastIndex(mine);
  // The latest turn of the viewer is its current turn while no other seat had a turn after it:
  // played now, or ended with nobody after it yet. The Log then starts at the turn before it.
  const current =
    latest >= 0 &&
    !entries.slice(latest + 1).some((entry) => entry.kind === "turn" && entry.seat !== viewer);
  const last = current ? entries.slice(0, latest).findLastIndex(mine) : latest;
  if (last >= 0) {
    return { scope: "since-last-turn", entries: entries.slice(last) };
  }
  return {
    scope: "round",
    entries: entries.filter((entry) => entry.round === update.view.round),
  };
}
