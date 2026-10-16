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
  ability(id: string): string;
  /** A vote: "For", "Against", or what is elected. */
  outcome(id: string): string;
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
    ability: (id) => byId(id, components?.abilities, records?.abilities) ?? humanizeId(id),
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
  | { kind: "text"; seat: string | null; text: string; dice?: LogLineView["dice"] }
  | { kind: "units"; seat: string | null; head: string; units: Map<string, number> }
  | { kind: "list"; seat: string | null; head: string; items: string[]; goods: number }
  | { kind: "tokens"; seat: string | null; pools: Map<TokenPool, number> };

type UnitsLine = Extract<Line, { kind: "units" }>;
type ListLine = Extract<Line, { kind: "list" }>;
type TokensLine = Extract<Line, { kind: "tokens" }>;

/** The pools of the command sheet, in the order of the sheet. */
const POOLS: readonly TokenPool[] = ["tactic", "fleet", "strategy"];

/** The groups in the order that an entry shows them. A group of another title comes last. */
const GROUPS = [
  "Activation",
  "Movement",
  "Space cannon",
  "Space combat",
  "Invasion",
  "Production",
  "Primary",
  "Secondaries",
  "Picks",
  "Scored",
  "Command tokens",
  "Agenda",
  "Effects",
  "Cards played",
  "Transactions",
  "Rolls",
];

class Groups {
  private readonly groups = new Map<string, Map<string, Line>>();
  private count = 0;

  constructor(private readonly names: Names) {}

  /** The line of `key` in `group`, made when there is none. A null key is always a new line. */
  line<T extends Line>(group: string, key: string | null, make: () => T): T {
    let lines = this.groups.get(group);
    if (!lines) {
      lines = new Map();
      this.groups.set(group, lines);
    }
    const at = key ?? `#${this.count++}`;
    const found = lines.get(at);
    if (found) {
      return found as T;
    }
    const made = make();
    lines.set(at, made);
    return made;
  }

  text(group: string, seat: string | null, text: string, dice?: LogLineView["dice"]) {
    this.line(group, null, () => ({ kind: "text", seat, text, ...(dice ? { dice } : {}) }));
  }

  /** Adds units to the line of `key`: "Lost 2 Fighter I, Carrier I". */
  units(group: string, seat: string | null, head: string, unit: string, count = 1) {
    const line = this.line<UnitsLine>(group, `${seat}|${head}`, () => ({
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
    const rank = (title: string) => {
      const at = GROUPS.findIndex((item) => title.startsWith(item));
      return at < 0 ? GROUPS.length : at;
    };
    return [...this.groups]
      .filter(([, lines]) => lines.size > 0)
      .sort(([a], [b]) => rank(a) - rank(b))
      .map(([title, lines]) => {
        const views = [...lines.values()].map((line) => this.lineView(line));
        return { title, lines: title === "Secondaries" ? bySeat(views) : views };
      });
  }

  private lineView(line: Line): LogLineView {
    switch (line.kind) {
      case "text":
        return { seat: line.seat, text: line.text, ...(line.dice ? { dice: line.dice } : {}) };
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

/** The group of a roll, by its reason. */
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
    case "ground combat":
    case "space cannon defense":
      return "Invasion";
    default:
      return "Rolls";
  }
}

/** The reasons of the rolls of a combat, which has rounds. */
const COMBAT = new Set(["space combat", "ground combat"]);

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
  /**
   * The combat round so far, by group. The engine rolls the units of one seat and one value
   * together: a second roll of the same seat and value, or a roll after a decision, is the next
   * round.
   */
  const rounds = new Map<string, { number: number; rolled: Set<string>; open: boolean }>();
  /**
   * The round of ground combat that a fact started (`ground_combat_round`), and the rounds so far
   * on each planet. With it the rolls of ground combat need no guess.
   */
  let ground: { planet: string; number: number } | null = null;
  const groundRounds = new Map<string, number>();
  /** The agenda of the votes so far: who voted in it, and its number. */
  let agenda = { number: 0, voted: new Set<string>() };
  /** The vote line of each seat in the agenda, for the planets that it exhausts. */
  const votes = new Map<string, ListLine>();

  for (const entry of events) {
    const roll = entry.event.kind === "dice_rolled" ? entry.roll : undefined;
    if (roll) {
      const group = rollGroup(roll);
      const seat = roll.roller ?? null;
      const reason = roll.reason.split(":")[0];
      let text = roll.label;
      if (reason === "ground combat" && ground) {
        text = `${roll.label} · ${names.planet(ground.planet)} · round ${ground.number}`;
      } else if (COMBAT.has(reason)) {
        const round = rounds.get(group) ?? { number: 0, rolled: new Set(), open: false };
        const key = `${seat}|${roll.hits_on}`;
        if (!round.open || round.rolled.has(key)) {
          round.number += 1;
          round.rolled = new Set();
          round.open = true;
        }
        round.rolled.add(key);
        rounds.set(group, round);
        text = `${roll.label} · round ${round.number}`;
      }
      const unit = roll.subject?.unit;
      groups.text(group, seat, unit ? `${text} · ${names.units(unit, 1)}` : text, {
        faces: roll.faces,
        hitsOn: roll.hits_on ?? null,
        hits: roll.hits ?? 0,
      });
      continue;
    }
    // A decision between the rolls ends a round of combat.
    for (const round of rounds.values()) {
      round.open = false;
    }
    const fact = entry.fact;
    if (entry.event.kind !== "decision_resolved" || !fact) {
      continue;
    }
    const seat = entry.actor ?? null;
    facts.push({ seat, fact });
    // In a strategic action what a seat did is in the part of its seat; else in its step.
    const own = (step: string) => (scope.strategic ? effects(seat) : step);
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
        groups.text(own("Activation"), seat, `Activated ${names.system(fact.system)}`);
        break;
      case "moved":
        groups.units(
          own("Movement"),
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
        groups.units(own("Movement"), seat, place ? `Loaded on ${place}: ` : "Loaded: ", fact.unit);
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
            ? own("Production")
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
        const group = entry.stage === "production" ? own("Production") : effects(seat);
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
        groups.units(
          fact.planet ? own("Invasion") : own("Space combat"),
          fact.seat,
          fact.planet ? `Lost on ${names.planet(fact.planet)}: ` : "Lost: ",
          fact.unit,
        );
        break;
      case "sustained":
        groups.units(
          fact.planet ? own("Invasion") : own("Space combat"),
          fact.seat,
          "Sustained damage: ",
          fact.unit,
        );
        break;
      case "retreat_announced":
        if (fact.retreat) {
          groups.text(own("Space combat"), fact.seat, "Announced a retreat");
        }
        break;
      case "retreated":
        groups.text(own("Space combat"), fact.seat, `Retreated to ${names.system(fact.to)}`);
        break;
      case "landed":
        groups.units(own("Invasion"), seat, `Landed on ${names.planet(fact.planet)}: `, fact.unit);
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
        if (agenda.voted.has(seat) || agenda.number === 0) {
          agenda = { number: agenda.number + 1, voted: new Set() };
          votes.clear();
        }
        agenda.voted.add(seat);
        const line = groups.line<ListLine>(
          `Agenda ${agenda.number} · Votes`,
          `vote|${seat}`,
          () => ({
            kind: "list",
            seat,
            head: fact.outcome ? `${names.outcome(fact.outcome)}` : "Abstained",
            items: [],
            goods: 0,
          }),
        );
        votes.set(seat, line);
        break;
      }
      case "planet_exhausted_for_vote": {
        const line = seat === null ? undefined : votes.get(seat);
        if (line) {
          line.head = line.items.length ? line.head : `${line.head} · `;
          line.items.push(names.planet(fact.planet));
        }
        break;
      }
      case "tie_broken":
        groups.text(
          `Agenda ${Math.max(agenda.number, 1)} · Votes`,
          seat,
          `Broke the tie: ${names.outcome(fact.outcome)}`,
        );
        break;
      case "card_played":
        groups.text("Cards played", seat, `Played ${names.card(fact.card)}`);
        break;
      case "objective_scored":
        groups.text(
          scope.phase === "status" ? "Scored" : effects(seat),
          seat,
          `${scope.phase === "status" ? "" : "Scored "}${names.objective(fact.objective)}${fact.secret ? " · secret objective" : ""}`,
        );
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
      case "ground_combat_started":
        groundRounds.set(fact.planet, 0);
        groups.text(
          own("Invasion"),
          fact.seat,
          `Started a ground combat on ${names.planet(fact.planet)}`,
        );
        break;
      case "ground_combat_round": {
        const number = (groundRounds.get(fact.planet) ?? 0) + 1;
        groundRounds.set(fact.planet, number);
        ground = { planet: fact.planet, number };
        break;
      }
      case "sustain_declined":
        groups.line(
          fact.planet ? own("Invasion") : own("Space combat"),
          `${fact.seat}|no sustain|${fact.planet ?? ""}`,
          () => ({ kind: "text", seat: fact.seat, text: "Did not sustain damage" }),
        );
        break;
      case "bombardment_targeted":
        groups.text(
          own("Invasion"),
          seat,
          `Bombarded ${names.seat(fact.target)} on ${names.planet(fact.planet)}`,
        );
        break;
      case "custodians_removed":
        groups.text(
          own("Invasion"),
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
): string {
  const parts: string[] = [];
  let ships = 0;
  const landed: string[] = [];
  let produced = 0;
  let tokens = 0;
  let lost = 0;
  let destroyed = 0;
  let combat = false;
  let retreated = false;
  let answered = 0;
  let followed = 0;
  let replenished = 0;
  const other: string[] = [];
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
      case "produced":
        produced += own ? fact.count : 0;
        break;
      case "tokens_gained":
        tokens += own ? fact.count : 0;
        break;
      case "casualty":
        combat = true;
        if (fact.seat === turn.seat) {
          lost += 1;
        } else {
          destroyed += 1;
        }
        break;
      case "retreated":
        combat = true;
        retreated ||= fact.seat === turn.seat;
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
      default:
        break;
    }
  }
  combat ||= turn.events.some((entry) => entry.roll && rollGroup(entry.roll) === "Space combat");
  if (ships) {
    parts.push(`moved ${plural(ships, "ship")}`);
  }
  if (combat) {
    const losses = [lost && `lost ${lost}`, destroyed && `destroyed ${destroyed}`].filter(Boolean);
    parts.push(
      `${retreated ? "retreated from combat" : "combat"}${losses.length ? ` (${losses.join(", ")})` : ""}`,
    );
  }
  if (landed.length) {
    parts.push(
      `landed on ${landed.slice(0, 2).join(", ")}${landed.length > 2 ? ` +${landed.length - 2}` : ""}`,
    );
  }
  parts.push(...new Set(other));
  if (produced) {
    parts.push(`produced ${plural(produced, "unit")}`);
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

function phaseView(phase: RawPhase, names: Names): LogPhaseView {
  const { groups, facts } = detailsOf(phase.events, names, {
    actor: null,
    strategic: false,
    phase: phase.phase,
  });
  const count = (kind: PublicFact["kind"]) =>
    facts.filter((item) => item.fact.kind === kind).length;
  const agendas = groups.filter((group) => group.title.startsWith("Agenda")).length;
  const summary =
    phase.phase === "strategy" && count("strategy_picked")
      ? `${plural(count("strategy_picked"), "strategy card")} picked`
      : phase.phase === "status" && count("objective_scored")
        ? `${plural(count("objective_scored"), "objective")} scored`
        : phase.phase === "agenda" && agendas
          ? `${plural(agendas, "agenda")} voted`
          : "";
  return {
    kind: "phase",
    id: `${phase.phase}-${phase.round}`,
    phase: phase.phase,
    round: phase.round,
    label: `${phaseName(phase.phase)} · Round ${phase.round}`,
    summary,
    groups,
  };
}

/**
 * The Log of the viewer: the entries from its last turn on, that turn first, so that the player
 * sees what it did and what the others did after it; the turn that is played now is last. A
 * turn of the viewer that is played now is not its last turn. A spectator, and a seat before its
 * first turn, see the round in play.
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
      return phaseView(item, names);
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
  const last =
    viewer === null
      ? -1
      : entries.findLastIndex(
          (entry) =>
            entry.kind === "turn" &&
            entry.seat === viewer &&
            !entry.live &&
            entry.action !== "other",
        );
  if (last >= 0) {
    return { scope: "since-last-turn", entries: entries.slice(last) };
  }
  return {
    scope: "round",
    entries: entries.filter((entry) => entry.round === update.view.round),
  };
}
