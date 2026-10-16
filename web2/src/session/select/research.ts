// A research: the technology tree of the viewer, with the technologies that the game offers as
// the cells that can be chosen. The game says what can be researched (the options of the
// question); the tree around them is printed (`research` of the update). A cell stages its
// option, as a row of a list does.
import type { BlockView, TechCellView, TechColor } from "../../model";
import type { Catalog, Choice, ResearchTechnology, SessionUpdate } from "../wire";
import { factionName, humanizeId } from "./names";

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
}

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
): ResearchView | null {
  const facts = update.research;
  const offered = choice.options.filter((option) => option.kind !== "decline");
  if (!facts || !offered.length || offered.some((option) => option.kind !== RESEARCH)) {
    return null;
  }
  const open = new Set(offered.map((option) => option.id));
  const cards = new Map((printed?.components.technologies ?? []).map((card) => [card.id, card]));
  const name = (id: string) => cards.get(id)?.name ?? humanizeId(id);
  const planets = new Map(
    (update.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet.label] as const),
    ),
  );
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
  };
}
