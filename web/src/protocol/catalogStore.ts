import type { Catalog, ContentRecords } from "./types.ts";
import type {
  AttachmentStaticMeta,
  BreakthroughMeta,
  CardMeta,
  ExplorationCardMeta,
  ObjectiveMeta,
  PlanetStaticMeta,
  StrategyCardMeta,
  TechnologyMeta,
} from "./contentCatalog.ts";
import type {
  FactionAbilityRecord,
  FactionRecord,
  LeaderRecord,
  PromissoryNoteRecord,
  UnitRecord,
} from "../presentation/factionInfo.ts";

/**
 * The printed cards as web's content wrappers read them: the records of the catalog
 * (`Catalog.records`, from the engine or `GET /api/catalog`), each kind by id in the order of the
 * ids, with web's field names. `CatalogProvider` fills them once, before the app renders; the tests
 * fill them from `fixtures/catalog/catalog.json` (`src/test/setup.ts`). The tables are the same
 * objects for the life of the page, so a module may keep a reference to one.
 */
export interface ContentTables {
  strategyCards: Record<string, StrategyCardMeta>;
  secretObjectives: Record<string, ObjectiveMeta>;
  publicObjectives: Record<string, ObjectiveMeta>;
  /** By id, and by the id the engine may name a card by (`automation_id`). */
  actionCards: Record<string, CardMeta>;
  technologies: Record<string, TechnologyMeta & { types: readonly string[] }>;
  explorationCards: Record<string, ExplorationCardMeta>;
  planets: Record<string, PlanetStaticMeta & { techSpecialties: readonly string[] }>;
  attachments: Record<string, AttachmentStaticMeta>;
  factions: Record<string, FactionRecord>;
  units: Record<string, UnitRecord>;
  leaders: Record<string, LeaderRecord & { source?: string }>;
  factionAbilities: Record<string, FactionAbilityRecord>;
  promissoryNotes: Record<string, PromissoryNoteRecord>;
  relics: Record<string, { id: string; name: string; text?: string; source?: string }>;
  breakthroughs: Record<string, BreakthroughMeta>;
}

/** The tables of the page: empty until `setCatalog` fills them. */
export const CONTENT: ContentTables = {
  strategyCards: {},
  secretObjectives: {},
  publicObjectives: {},
  actionCards: {},
  technologies: {},
  explorationCards: {},
  planets: {},
  attachments: {},
  factions: {},
  units: {},
  leaders: {},
  factionAbilities: {},
  promissoryNotes: {},
  relics: {},
  breakthroughs: {},
};

let loaded: string | null = null;

/** The digest of the corpus of the catalog in the tables (`Catalog.content`); null before one. */
export function catalogContent(): string | null {
  return loaded;
}

/** Puts a catalog into the tables. A second catalog replaces the first. */
export function setCatalog(catalog: Catalog): void {
  const tables = contentTables(catalog.records);
  for (const key of Object.keys(CONTENT) as (keyof ContentTables)[]) {
    const table = CONTENT[key] as Record<string, unknown>;
    for (const id of Object.keys(table)) delete table[id];
    Object.assign(table, tables[key]);
  }
  loaded = catalog.content;
}

/** Leaves out what a record does not print (`undefined`), as the corpus does. */
function defined<T extends object>(record: T): T {
  return Object.fromEntries(Object.entries(record).filter(([, value]) => value !== undefined)) as T;
}

const text = (value: string | undefined): string | undefined => value || undefined;

/** Each record by id, in the order of the ids; the first of two records with one id wins. */
function byId<T>(entries: Iterable<[string, T]>): Record<string, T> {
  const table = new Map<string, T>();
  for (const [id, record] of entries) if (!table.has(id)) table.set(id, record);
  return Object.fromEntries([...table].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const each = <R extends { id: string }, T>(records: readonly R[], shape: (record: R) => T) =>
  byId(records.map((record) => [record.id, shape(record)] as [string, T]));

/** The records of a catalog as web's tables. Pure: the tests compare it with what web showed before. */
export function contentTables(records: ContentRecords): ContentTables {
  const objective = (record: ContentRecords["secret_objectives"][number]): ObjectiveMeta => ({
    id: record.id,
    name: record.name,
    phase: record.phase,
    points: record.points,
    description: record.text,
  });
  return {
    strategyCards: each(records.strategy_cards, (card) => ({
      id: card.id,
      name: card.name,
      initiative: card.initiative,
      primaryText: card.primary_text,
      secondaryText: card.secondary_text,
    })),
    secretObjectives: each(records.secret_objectives, objective),
    publicObjectives: each(records.public_objectives, objective),
    actionCards: byId(
      records.action_cards.flatMap((card) => {
        const meta: CardMeta = defined({
          id: card.id,
          name: card.name,
          phase: text(card.phase),
          // The printed timing of a reaction card; plain action-phase cards print "Action".
          window: card.window && card.window !== "Action" ? card.window : undefined,
          description: card.text,
        });
        // The engine may name a card by the automation id of the corpus.
        const alias: [string, CardMeta][] = card.automation_id
          ? [[card.automation_id, { ...meta, id: card.automation_id }]]
          : [];
        return [[card.id, meta] as [string, CardMeta], ...alias];
      }),
    ),
    technologies: each(records.technologies, (tech) =>
      defined({
        id: tech.id,
        name: tech.name,
        types: tech.types,
        requirements: text(tech.requirements),
        faction: text(tech.faction),
        source: text(tech.source),
        baseUpgrade: text(tech.base_upgrade),
        description: tech.text,
      }),
    ),
    explorationCards: each(records.explores, (card) =>
      defined({
        id: card.id,
        name: card.name,
        type: card.kind,
        resolution: card.resolution,
        description: card.text,
        flavorText: text(card.flavor_text),
      }),
    ),
    planets: each(records.planets, (planet) =>
      defined({
        id: planet.id,
        name: planet.name,
        resources: planet.resources,
        influence: planet.influence,
        techSpecialties: planet.tech_specialties ?? [],
        legendaryAbilityName: text(planet.legendary_ability_name),
        legendaryAbilityText: text(planet.legendary_ability_text),
      }),
    ),
    attachments: each(records.attachments, (attachment) => ({
      id: attachment.id,
      name: attachment.name,
      resourcesModifier: attachment.resources_modifier,
      influenceModifier: attachment.influence_modifier,
    })),
    factions: each(records.factions, (faction) =>
      defined({
        id: faction.id,
        name: faction.name,
        commodities: faction.commodities,
        startingTech: faction.starting_tech,
        factionTech: faction.faction_tech,
        abilities: faction.abilities,
        leaders: faction.leaders,
        promissoryNotes: faction.promissory_notes,
        units: faction.units,
        source: text(faction.source),
      }),
    ),
    units: each(records.units, (unit) =>
      defined({
        id: unit.id,
        baseType: unit.base_type,
        name: unit.name,
        subtitle: text(unit.subtitle),
        faction: text(unit.faction),
        source: text(unit.source),
        cost: unit.cost,
        moveValue: unit.move_value,
        capacityValue: unit.capacity_value,
        combatHitsOn: unit.combat_hits_on,
        combatDieCount: unit.combat_die_count,
        sustainDamage: unit.sustain_damage || undefined,
        afbHitsOn: unit.afb_hits_on,
        afbDieCount: unit.afb_die_count,
        bombardHitsOn: unit.bombard_hits_on,
        bombardDieCount: unit.bombard_die_count,
        spaceCannonHitsOn: unit.space_cannon_hits_on,
        spaceCannonDieCount: unit.space_cannon_die_count,
        deepSpaceCannon: unit.deep_space_cannon || undefined,
        planetaryShield: unit.planetary_shield || undefined,
        disablesPlanetaryShield: unit.disables_planetary_shield || undefined,
        productionValue: unit.production_value,
        requiredTechId: text(unit.required_tech_id),
        upgradesFromUnitId: text(unit.upgrades_from_unit_id),
        upgradesToUnitId: text(unit.upgrades_to_unit_id),
        ability: text(unit.ability),
      }),
    ),
    leaders: each(records.leaders, (leader) =>
      defined({
        id: leader.id,
        faction: leader.faction,
        type: leader.kind,
        name: leader.name,
        title: text(leader.title),
        abilityName: text(leader.ability_name),
        abilityWindow: text(leader.ability_window),
        abilityText: text(leader.ability_text),
        unlockCondition: text(leader.unlock_condition),
        source: text(leader.source),
      }),
    ),
    factionAbilities: each(records.abilities, (ability) =>
      defined({
        id: ability.id,
        name: ability.name,
        faction: text(ability.faction),
        permanentEffect: text(ability.permanent_effect),
        window: text(ability.window),
        windowEffect: text(ability.window_effect),
      }),
    ),
    promissoryNotes: each(records.promissory_notes, (note) =>
      defined({ id: note.id, name: note.name, faction: text(note.faction), text: text(note.text) }),
    ),
    relics: each(records.relics, (relic) =>
      defined({ id: relic.id, name: relic.name, text: text(relic.text), source: text(relic.source) }),
    ),
    breakthroughs: each(records.breakthroughs, (card) =>
      defined({
        id: card.id,
        name: card.name,
        faction: text(card.faction),
        synergy: card.synergy,
        text: text(card.text),
      }),
    ),
  };
}
