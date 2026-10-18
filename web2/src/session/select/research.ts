// A research: the technology tree of the viewer, with the technologies that the game offers as
// the cells that can be chosen. The game says what can be researched (the options of the
// question); the tree around them is printed (`research` of the update). A cell stages its
// option, as a row of a list does. A paid research is paid on the map, staged with the chosen
// technology (`details.payment` of the question), and sent with it at once.
import type { BlockView, BoardTaskView, PaymentView, TechCellView, TechColor } from "../../model";
import type { PlanStep } from "../movementPlan";
import type {
  Catalog,
  Choice,
  ResearchPaymentHints,
  ResearchTechnology,
  SessionUpdate,
} from "../wire";
import { hintOf } from "./editor";
import { factionName, humanizeId } from "./names";
import {
  NO_PAYMENT,
  type PayOffer,
  type Paying,
  autoPaying,
  goodsPaying,
  paidBy,
  payOfferOf,
  paySteps,
  paymentOf,
  planetNames,
  togglePaying,
} from "./payment";

const COLUMNS: [TechColor, string][] = [
  ["B", "Blue"],
  ["G", "Green"],
  ["Y", "Yellow"],
  ["R", "Red"],
];

/** The kind of an option that researches a technology (`RESEARCH_KIND` of the engine). */
const RESEARCH = "research";

export interface ResearchView {
  block: BlockView & { kind: "techs" };
  /** The option that researches nothing, when the question has one. */
  pass: string | null;
  /** How many technologies can be researched. */
  open: number;
  /** The name of a technology of the tree, by its option. */
  name: (id: string) => string;
  /** The payment of a paid research on the map, once a technology is chosen. */
  payment: PaymentView | null;
  task: BoardTaskView | null;
  /** Why the research cannot be sent yet: the payment is short. */
  problem: string | null;
  /** What the research changes, in game values: "Technologies 5 → 6 · Trade goods 4 → 2". */
  change: string | null;
}

interface Bill {
  cost: number;
  offer: PayOffer;
  /** The specialty planets that a technology keeps ready for its prerequisites. */
  kept: Record<string, string[]>;
}

/** The payment of a paid research, as the question lists it. Null for a free research. */
function billOf(choice: Choice): Bill | null {
  const hints = hintOf<ResearchPaymentHints>(choice, "payment");
  return hints && hints.cost > 0
    ? { cost: hints.cost, offer: payOfferOf(hints), kept: hints.kept }
    : null;
}

/** What can pay for `technology`: every planet of the bill but those that it keeps ready. */
function offerFor(bill: Bill, technology: string): PayOffer {
  const kept = new Set(bill.kept[technology] ?? []);
  return { ...bill.offer, planets: bill.offer.planets.filter((planet) => !kept.has(planet.id)) };
}

/** The staged payment, without a planet that the chosen technology keeps ready. */
const payingFor = (offer: PayOffer, paying: Paying): Paying => ({
  ...paying,
  planets: paying.planets.filter((id) => offer.planets.some((planet) => planet.id === id)),
});

/** The resources that a research costs, as the option says. */
const costOf = (choice: Choice, id: string): number => {
  const cost = choice.options.find((option) => option.id === id)?.payload.cost;
  return typeof cost === "number" ? cost : 0;
};
const price = (cost: number) => (cost ? `${cost} resources` : "free");

/**
 * The tree of a question that researches: every option but a decline is a technology. Null for
 * any other question.
 */
export function selectResearch(
  update: SessionUpdate,
  choice: Choice,
  staged: string | null,
  printed: Catalog | null,
  /** The staged payment. Undefined: the research is recorded in a draft, and nothing is paid. */
  paying?: Paying | null,
): ResearchView | null {
  const facts = update.research;
  const offered = choice.options.filter((option) => option.kind !== "decline");
  if (!facts || !offered.length || offered.some((option) => option.kind !== RESEARCH)) {
    return null;
  }
  const open = new Set(offered.map((option) => option.id));
  const cards = new Map((printed?.components.technologies ?? []).map((card) => [card.id, card]));
  const name = (id: string) => cards.get(id)?.name ?? humanizeId(id);
  const planets = planetNames(update);
  const owned = (color: TechColor) =>
    facts.technologies.filter((tech) => tech.owned && tech.colour === color && !tech.unit_upgrade)
      .length;
  const skips = (color: TechColor) => facts.specialties.filter((item) => item.colour === color);
  /** What stands for the prerequisites of a colour: the technologies and the specialties. */
  const have = (color: TechColor) => owned(color) + skips(color).length;

  const cell = (tech: ResearchTechnology, mark: boolean): TechCellView => {
    const seen: Partial<Record<TechColor, number>> = {};
    return {
      id: tech.id,
      name: name(tech.id),
      color: mark ? tech.colour : undefined,
      needs: tech.prerequisites.map((color) => {
        seen[color] = (seen[color] ?? 0) + 1;
        return { color, missing: !tech.owned && seen[color] > have(color) };
      }),
      state: tech.owned ? "owned" : open.has(tech.id) ? "open" : "closed",
      selected: tech.id === staged,
      text: cards.get(tech.id)?.text ?? "",
      intent: { type: "chooseOption", option: tech.id },
    };
  };
  // The fewer prerequisites, the higher in its column, as on the cards of the game.
  const order = (a: ResearchTechnology, b: ResearchTechnology) =>
    a.prerequisites.length - b.prerequisites.length || name(a.id).localeCompare(name(b.id));
  const all = [...facts.technologies].sort(order);
  const upgrades = all.filter((tech) => tech.unit_upgrade);
  const own = all.filter((tech) => !tech.unit_upgrade && (tech.faction || !tech.colour));
  const faction = update.view.players.find((player) => player.id === choice.player)?.faction;

  // Every technology costs the same in one question: the price of the first that is offered.
  const cost = costOf(choice, staged ?? offered[0].id);
  const chosen = staged && open.has(staged) ? staged : null;
  const bill = paying !== undefined ? billOf(choice) : null;
  const offer = bill && chosen ? offerFor(bill, chosen) : null;
  const staging = offer ? payingFor(offer, paying ?? NO_PAYMENT) : NO_PAYMENT;
  const pay = offer && bill ? paymentOf(update, offer, staging, bill.cost, "resources") : null;
  const paid = offer ? paidBy(offer, staging) : 0;
  const techs = facts.technologies.filter((tech) => tech.owned).length;
  const change = chosen
    ? [
        `Technologies ${techs} → ${techs + 1}`,
        ...(staging.goods && offer
          ? [`Trade goods ${offer.goods} → ${offer.goods - staging.goods}`]
          : []),
      ].join(" · ")
    : null;
  return {
    block: {
      kind: "techs",
      title: "Research",
      chosen: staged && open.has(staged) ? [`${name(staged)} · ${price(cost)}`] : [],
      // The price is said once: with the chosen technology, or here until one is chosen.
      aside: staged && open.has(staged) ? undefined : cost ? `Costs ${cost} resources` : "Free",
      columns: COLUMNS.map(([color, label]) => ({
        color,
        label: [
          `${label} ${owned(color)}`,
          ...skips(color).map(
            (skip) => `+1 ${planets.get(skip.planet) ?? humanizeId(skip.planet)}`,
          ),
        ].join(" · "),
        cells: all
          .filter((tech) => tech.colour === color && !tech.faction && !tech.unit_upgrade)
          .map((tech) => cell(tech, false)),
      })),
      bands: [
        { label: "Unit upgrades", cells: upgrades.map((tech) => cell(tech, false)) },
        {
          label: `${faction ? factionName(faction, printed) : "Faction"} technologies`,
          cells: own.map((tech) => cell(tech, true)),
        },
      ].filter((band) => band.cells.length),
    },
    pass: choice.options.find((option) => option.kind === "decline")?.id ?? null,
    open: open.size,
    name,
    payment: pay?.payment ?? null,
    task: pay?.task ?? null,
    problem: bill && chosen && paid < bill.cost ? `Pay ${bill.cost - paid} more resources.` : null,
    change,
  };
}

/** The offer of a paid research for its chosen technology. Null when nothing is paid. */
function chosenOffer(choice: Choice, staged: string | null): PayOffer | null {
  const bill = billOf(choice);
  return bill && staged && choice.options.some((option) => option.id === staged)
    ? offerFor(bill, staged)
    : null;
}

/** A planet of the payment of a research is chosen, or given back. */
export function toggleResearchPlanet(
  choice: Choice,
  staged: string | null,
  paying: Paying | null,
  planet: string,
): Paying | null {
  const offer = chosenOffer(choice, staged);
  return offer ? togglePaying(offer, payingFor(offer, paying ?? NO_PAYMENT), planet) : null;
}

/** The trade goods of the payment of a research. */
export function setResearchGoods(
  choice: Choice,
  staged: string | null,
  paying: Paying | null,
  value: number,
  source?: string,
): Paying | null {
  const offer = chosenOffer(choice, staged);
  return offer ? goodsPaying(offer, paying ?? NO_PAYMENT, value, source) : null;
}

/** Auto-pay of a research, or "Clear payment" (`reset`). */
export function payResearch(
  choice: Choice,
  staged: string | null,
  paying: Paying | null,
  how: "auto" | "reset",
): Paying | null {
  const offer = chosenOffer(choice, staged);
  const bill = billOf(choice);
  if (!offer || !bill) {
    return null;
  }
  return how === "reset" ? NO_PAYMENT : (autoPaying(offer, bill.cost) ?? paying ?? NO_PAYMENT);
}

/**
 * The payment of a paid research, as `pay` steps that follow its answer. Null while the staged
 * payment does not cover the price, or when the research is free.
 */
export function researchPlan(
  choice: Choice,
  staged: string | null,
  paying: Paying | null,
): PlanStep[] | null {
  const offer = chosenOffer(choice, staged);
  const bill = billOf(choice);
  if (!offer || !bill) {
    return null;
  }
  const staging = payingFor(offer, paying ?? NO_PAYMENT);
  return paidBy(offer, staging) >= bill.cost ? paySteps(offer, staging) : null;
}
