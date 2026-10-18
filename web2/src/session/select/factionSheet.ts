// The sheet of a faction as printed: what a player chooses a faction by. Everything is the
// catalogue of the engine; the page words the lines and writes no text of a card.
import type { ComponentCardView, FactionSheetView } from "../../model";
import type { Catalog } from "../wire";
import { plain, timing } from "./components";
import { humanizeId, unitLabel, unitRecord } from "./names";

const LEADER_KINDS = ["agent", "commander", "hero"];

/**
 * "2 Advanced Carrier I", "Destroyer I": a unit of the starting fleet with its count when there
 * are several. The sheet names the base unit (`carrier`); the unit card of the faction that has
 * that base type is the one it starts with.
 */
function units(faction: string, id: string, count: number, catalog: Catalog | null): string {
  const own = catalog?.records.factions
    .find((item) => item.id === faction)
    ?.units.find((unit) => unitRecord(unit, catalog)?.base_type === id);
  const name = unitLabel(own ?? id, catalog, count);
  return count === 1 ? name : `${count} ${name}`;
}

/** The sheet of the faction `id`. Null without the catalogue, or for a faction that it does not have. */
export function factionSheet(id: string, catalog: Catalog | null): FactionSheetView | null {
  const printed = catalog?.components;
  const sheet = printed?.faction_sheets[id];
  if (!printed || !sheet) {
    return null;
  }
  const technologies = new Map(printed.technologies.map((card) => [card.id, card]));
  const technology = (tech: string) => technologies.get(tech)?.name ?? humanizeId(tech);
  const planets = sheet.home_planets ?? [];
  const fleet = sheet.starting_fleet ?? [];
  const at = (planet: string | undefined) =>
    fleet
      .filter((entry) => entry.planet === planet)
      .map((entry) => units(id, entry.unit, entry.count, catalog))
      .join(" · ");

  const choice = sheet.starting_technology_choice;
  const starting = choice
    ? `Choose ${choice.amount} of ${choice.options.map(technology).join(", ")}`
    : (sheet.starting_technologies ?? []).map(technology).join(", ") || "None";

  const facts = [
    {
      label: "Home system",
      value: planets
        .map((planet) => `${planet.name} ${planet.resources}/${planet.influence}`)
        .join(" · "),
    },
    { label: "Commodities", value: String(sheet.commodities) },
    { label: "Starting technologies", value: starting },
    { label: "Starting fleet", value: at(undefined) },
    ...planets.map((planet) => ({ label: `On ${planet.name}`, value: at(planet.id) })),
  ].filter((fact) => fact.value);

  const abilities = new Map(printed.abilities.map((card) => [card.id, card]));
  const leaders = new Map(printed.leaders.map((card) => [card.id, card]));
  const card = (name: string, kind: string, text: string | undefined): ComponentCardView => ({
    name,
    kind,
    window: "",
    text: plain(text),
    state: null,
    unlock: "",
  });
  return {
    title: sheet.title,
    facts,
    abilities: (printed.factions[id] ?? []).map((ability) => {
      const found = abilities.get(ability);
      return {
        ...card(found?.name ?? humanizeId(ability), "", found?.text),
        window: timing(found?.window),
      };
    }),
    technologies: (sheet.faction_technologies ?? []).map((tech) =>
      card(technology(tech), "", technologies.get(tech)?.text),
    ),
    leaders: (sheet.leaders ?? [])
      .flatMap((leader) => leaders.get(leader) ?? [])
      .sort((a, b) => LEADER_KINDS.indexOf(a.kind) - LEADER_KINDS.indexOf(b.kind))
      .map((leader) => ({
        ...card(
          leader.name,
          [humanizeId(leader.kind), leader.title].filter(Boolean).join(" · "),
          leader.text,
        ),
        window: timing(leader.window),
        unlock: leader.unlock ?? "",
      })),
  };
}
