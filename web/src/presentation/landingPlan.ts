import type { BoardView, PlayerView } from "../protocol/types.ts";
import { findAttachmentMeta, findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { getUnitBaseType, getUnitDisplayName } from "../components/UnitIcon.tsx";
import { resolveUnit } from "./factionInfo.ts";
import { describeUnitStats, hitChancePercent } from "./unitStats.ts";

/**
 * The default landing plan and the per-planet "what happens when it is taken" lines of the
 * invasion landing tray. Pure functions; the tray only renders their results.
 *
 * Facts, from crates/ti4-engine/src/invasion.rs:
 *  - `commit_ground_forces` offers one landing per distinguishable (unit, damaged, planet) for every
 *    landable planet (Mecatol only once the custodians token is off). Units come from the system's
 *    space area; there is no capacity limit on landing.
 *  - A planet is contested when another player has ground forces on it. A planet holding only
 *    rival structures falls without resistance (structures are destroyed on the change of control).
 *  - Exploration happens only when the invader gains control of a planet nobody controlled (35.1),
 *    into the deck of one of the planet's traits (the invader chooses with two).
 */

export interface Landing {
  planet: string;
  unit: string;
  damaged: boolean;
}

export interface PlanPlanet {
  id: string;
  /** resources + influence including attachment modifiers. */
  value: number;
  /** Secondary tie-break: legendary and attachment count. Higher is more valuable. */
  extra: number;
  /** Another player has units (any kind) on it. */
  defended: boolean;
  /** The invader already controls it, or has forces on it with no rival present. */
  held: boolean;
}

export interface PlanStock {
  unit: string;
  damaged: boolean;
  count: number;
}

export interface LandingPlan {
  landings: Landing[];
  /** Forces were spread over more than one planet. */
  split: boolean;
  /** Uninhabited planets that need a landing (value order). */
  uninhabited: string[];
}

/** Mechs are the strongest, other ground forces next, infantry the weakest. */
const rank = (unit: string) => (unit === "mech" ? 2 : unit === "infantry" ? 0 : 1);

const byValue = (a: PlanPlanet, b: PlanPlanet) =>
  b.value - a.value || b.extra - a.extra || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);

/**
 * Default landing plan.
 *
 * Uninhabited planets (no rival unit of any kind) that the invader does not already hold are
 * "targets". Defended planets keep the old behaviour: the main force goes to the most valuable
 * defended planet.
 *  1. No targets: every available force goes to the best defended planet, else the best planet
 *     (the previous default).
 *  2. Targets but nothing defended: every target gets one force in descending value order while
 *     units last (the best target gets the strongest unit, the others the weakest, so mechs land
 *     where it matters); the rest is dealt round-robin over the targets by value, strongest first.
 *  3. Both: the contested planet keeps at least half of the forces (rounded up). Up to
 *     min(targets, floor(N/2)) of the weakest units (infantry first) go one each to the targets in
 *     value order, everything else to the best defended planet.
 * Never plans more than the stock, and only (planet, unit, damaged) combinations that are offered.
 */
export function planDefaultLanding(
  planets: PlanPlanet[],
  stock: PlanStock[],
  offered: (planet: string, unit: string, damaged: boolean) => boolean,
): LandingPlan {
  const empty: LandingPlan = { landings: [], split: false, uninhabited: [] };
  if (planets.length === 0) return empty;
  const pool: { unit: string; damaged: boolean }[] = [];
  for (const s of stock) for (let i = 0; i < s.count; i++) pool.push({ unit: s.unit, damaged: s.damaged });
  // strongest first; undamaged before damaged; name for determinism
  pool.sort(
    (a, b) =>
      rank(b.unit) - rank(a.unit) ||
      Number(a.damaged) - Number(b.damaged) ||
      (a.unit < b.unit ? -1 : a.unit > b.unit ? 1 : 0),
  );
  const total = pool.length;
  const landings: Landing[] = [];
  const give = (planet: string, fromEnd: boolean): boolean => {
    const order = fromEnd ? [...pool.keys()].reverse() : [...pool.keys()];
    for (const i of order) {
      const u = pool[i]!;
      if (offered(planet, u.unit, u.damaged)) {
        pool.splice(i, 1);
        landings.push({ planet, unit: u.unit, damaged: u.damaged });
        return true;
      }
    }
    return false;
  };
  const sorted = [...planets].sort(byValue);
  const targets = sorted.filter((p) => !p.defended && !p.held);
  const defended = sorted.filter((p) => p.defended);
  const uninhabited = targets.map((p) => p.id);
  const dump = (planet: string) => {
    while (give(planet, false));
  };

  if (targets.length === 0) {
    dump((defended[0] ?? sorted[0]!).id);
  } else if (defended.length === 0) {
    if (targets.length === 1) dump(targets[0]!.id);
    else {
      targets.forEach((p, i) => {
        if (pool.length > 0) give(p.id, i > 0);
      });
      let progressed = true;
      while (pool.length > 0 && progressed) {
        progressed = false;
        for (const p of targets) if (pool.length > 0 && give(p.id, false)) progressed = true;
      }
    }
  } else {
    const spread = Math.min(targets.length, Math.floor(total / 2));
    for (let i = 0; i < spread; i++) give(targets[i]!.id, true);
    dump(defended[0]!.id);
  }
  const planetsUsed = new Set(landings.map((l) => l.planet));
  return { landings: sortLandings(landings, sorted), split: planetsUsed.size > 1, uninhabited };
}

/** Group by planet (value order), strongest unit first, so the draft is stable and readable. */
function sortLandings(landings: Landing[], sorted: PlanPlanet[]): Landing[] {
  const order = new Map(sorted.map((p, i) => [p.id, i]));
  return landings
    .map((l, i) => ({ l, i }))
    .sort(
      (a, b) =>
        (order.get(a.l.planet) ?? 0) - (order.get(b.l.planet) ?? 0) ||
        rank(b.l.unit) - rank(a.l.unit) ||
        Number(a.l.damaged) - Number(b.l.damaged) ||
        a.i - b.i,
    )
    .map(({ l }) => l);
}

/** Whether two drafts hold the same landings per planet/unit (order ignored). */
export function sameDraft(a: Landing[], b: Landing[]): boolean {
  if (a.length !== b.length) return false;
  const key = (l: Landing) => `${l.planet}|${l.unit}|${l.damaged}`;
  const counts = new Map<string, number>();
  for (const l of a) counts.set(key(l), (counts.get(key(l)) ?? 0) + 1);
  for (const l of b) {
    const n = counts.get(key(l));
    if (!n) return false;
    counts.set(key(l), n - 1);
  }
  return true;
}

// ---------------------------------------------------------------------------------------------
// Invasion effects per planet

export interface PlanetEffect {
  kind: "state" | "explore" | "legendary" | "attachment" | "specialty" | "mecatol";
  /** One compact line. */
  text: string;
  /** Longer text for an info popover (printed ability text, explanation). */
  detail?: string;
}

export type PlanetStanding = "defended" | "uninhabited" | "held";

export interface PlanetInvasionInfo {
  standing: PlanetStanding;
  /** Display name of the defending player(s), when defended. */
  defenders: string[];
  effects: PlanetEffect[];
}

const titleCase = (s: string) => s.replace(/(^|[\s_-])(\w)/g, (_, a, b) => a + b.toUpperCase());

/** Units of other players on the planet make it defended; controlled-by-me or my forces only make it held. */
export function planetStanding(
  board: BoardView | undefined,
  system: string,
  planetId: string,
  actor: string,
): { standing: PlanetStanding; defenders: string[]; controller: string | null } {
  const sys = board?.systems[system];
  const view = Object.values(sys?.planets ?? {}).find((p) => p.planet_id === planetId);
  const controller = view?.controlled_by ?? null;
  const here = (sys?.units ?? []).filter((u) => u.planet === planetId);
  const defenders = [...new Set(here.filter((u) => u.owner !== actor).map((u) => u.owner))];
  if (defenders.length > 0) return { standing: "defended", defenders, controller };
  if (controller === actor || here.some((u) => u.owner === actor))
    return { standing: "held", defenders, controller };
  return { standing: "uninhabited", defenders, controller };
}

/**
 * What taking this planet does, from data the client has: board planet/unit views, the map tile's
 * traits and the generated content catalog. `assigned` is the number of forces staged on it.
 *
 * Not knowable client-side (so not shown): traits an attachment adds (the catalog export carries no
 * planet types), Fracture first-claim relics (tile back is not exported), Thunder's Edge's
 * breakthrough, faction/technology abilities that trigger on gaining control, and what the
 * exploration card turns out to be.
 */
export function planetInvasionInfo(
  board: BoardView | undefined,
  system: string,
  planetId: string,
  actor: string,
  assigned: number,
  nameOf: (player: string) => string = (p) => p,
): PlanetInvasionInfo {
  const { standing, defenders, controller } = planetStanding(board, system, planetId, actor);
  const sys = board?.systems[system];
  const view = Object.values(sys?.planets ?? {}).find((p) => p.planet_id === planetId);
  const tileMeta = board?.map_tiles?.flatMap((t) => t.planets ?? []).find((p) => p.id === planetId);
  const catalog = findPlanetMeta(planetId);
  const effects: PlanetEffect[] = [];
  const contested = standing === "defended";
  const when = contested ? "If you win the ground combat" : "On landing";

  if (contested) {
    effects.push({
      kind: "state",
      text: `Defended by ${defenders.map(nameOf).join(", ")}: ground combat decides control`,
    });
  } else if (standing === "held") {
    effects.push({ kind: "state", text: "You already hold this planet: no landing needed" });
  } else {
    effects.push({
      kind: "state",
      text: controller
        ? `Undefended (controlled by ${nameOf(controller)}): landing takes it, their structures are destroyed`
        : "Undefended: landing takes it",
    });
  }

  if (planetId === "mr") {
    effects.push({
      kind: "mecatol",
      text: "Custodians token already removed (its victory point was scored then)",
      detail:
        "Taking Mecatol Rex itself has no extra capture effect in the engine; the 1 VP came with lifting the Custodians token.",
    });
  }

  if (standing !== "held") {
    const traits = (tileMeta?.traits ?? []).map((t) => t.toLowerCase());
    if (!controller && traits.length > 0) {
      const deck = traits.length > 1 ? `${traits.join(" or ")} (your choice)` : traits[0]!;
      effects.push({
        kind: "explore",
        text:
          assigned === 0
            ? `No units assigned (would explore ${deck})`
            : contested
              ? `${when}: explore ${deck}`
              : `Explores on landing: ${deck}`,
        detail:
          "Gaining control of a planet nobody controlled explores it (LRR 35.1). Taking a planet from another player does not. Traits added by attachments are not shown.",
      });
    } else if (!controller && traits.length === 0) {
      effects.push({ kind: "explore", text: "No exploration (planet has no trait)" });
    } else if (controller) {
      effects.push({
        kind: "explore",
        text: "No exploration: planet is already controlled",
        detail: "Only a planet nobody controlled is explored when taken (LRR 35.1).",
      });
    }
    if (tileMeta?.legendary || catalog?.legendaryAbilityName) {
      effects.push({
        kind: "legendary",
        text: `${when === "On landing" ? "Gains" : "If you win: gain"} legendary ability: ${
          catalog?.legendaryAbilityName ?? "Legendary"
        }`,
        detail: catalog?.legendaryAbilityText?.trim() || undefined,
      });
    }
  }

  for (const id of view?.attachments ?? []) {
    const meta = findAttachmentMeta(id);
    const mods = [
      meta?.resourcesModifier ? `${meta.resourcesModifier > 0 ? "+" : ""}${meta.resourcesModifier} resources` : "",
      meta?.influenceModifier ? `${meta.influenceModifier > 0 ? "+" : ""}${meta.influenceModifier} influence` : "",
    ].filter(Boolean);
    effects.push({
      kind: "attachment",
      text: `Attached: ${meta?.name ?? titleCase(humanizeId(id))}${mods.length ? ` (${mods.join(", ")})` : ""}`,
      detail: "Attachments stay on the planet when it changes hands; modifiers are already in the values above.",
    });
  }

  const specialties = tileMeta?.tech_specialties ?? catalog?.techSpecialties ?? [];
  if (specialties.length > 0 && standing !== "held") {
    effects.push({ kind: "specialty", text: `Tech specialty: ${specialties.map((s) => s.toLowerCase()).join(", ")}` });
  }
  return { standing, defenders, effects };
}

// ---------------------------------------------------------------------------------------------
// Defenders per planet

export interface DefenderUnitCount {
  /** The unit type as on the board (e.g. "infantry", "spec_ops", "pds2"). */
  unit: string;
  count: number;
  damaged: number;
}

export interface SpaceCannonGun {
  unit: string;
  count: number;
  hitsOn: number;
  /** Dice per unit. */
  dice: number;
}

export interface PlanetDefenderForce {
  owner: string;
  /** Infantry and mechs (and their faction variants). */
  ground: DefenderUnitCount[];
  groundTotal: number;
  pds: number;
  /** Space docks and other non-PDS structures. */
  otherStructures: DefenderUnitCount[];
  /** Units on the planet that roll Space Cannon Defense against forces landing there. */
  guns: SpaceCannonGun[];
}

export interface PlanetDefense {
  forces: PlanetDefenderForce[];
  /** Space Cannon Defense against the forces landed here; null when nothing on the planet has SPACE CANNON. */
  spaceCannonDefense: {
    dice: number;
    /** Expected hits from one roll of all dice (rounded to 0.1). */
    expectedHits: number;
    /** Why it will not fire, when the client can tell (L4 Disruptors). */
    silencedBy?: string;
  } | null;
}

/** Fallback PDS card values when the catalog cannot resolve the unit (PDS I hits on 6, PDS II on 5). */
const fallbackPdsGun = (unit: string, technologies: readonly string[]): { hitsOn: number; dice: number } =>
  /2|ii/i.test(unit) || technologies.includes("pds2") ? { hitsOn: 5, dice: 1 } : { hitsOn: 6, dice: 1 };

/**
 * Other players' units on a planet of the invaded system, from the board view: ground forces by
 * type, PDS and other structures, and the Space Cannon Defense they roll against forces landed
 * there (crates/ti4-engine/src/invasion.rs `space_cannon_defense`: every non-invader unit on the
 * planet with SPACE CANNON fires; L4 Disruptors on the invader silences it). Unit values come from
 * the content catalog for the owner's faction and researched technologies (public).
 *
 * Not modelled: Plasma Scoring's extra die, Disable, Quietus and other card/faction modifiers.
 */
export function planetDefense(
  board: BoardView | undefined,
  system: string,
  planetId: string,
  actor: string,
  players?: Record<string, PlayerView>,
): PlanetDefense {
  const here = (board?.systems[system]?.units ?? []).filter(
    (u) => u.planet === planetId && u.owner !== actor,
  );
  const owners = [...new Set(here.map((u) => u.owner))];
  const bump = (list: DefenderUnitCount[], unit: string, damaged: boolean) => {
    const entry = list.find((e) => e.unit === unit);
    if (entry) {
      entry.count += 1;
      if (damaged) entry.damaged += 1;
    } else list.push({ unit, count: 1, damaged: damaged ? 1 : 0 });
  };
  let dice = 0;
  let expected = 0;
  const forces = owners.map((owner): PlanetDefenderForce => {
    const player = players?.[owner];
    const technologies = player?.technologies ?? [];
    const force: PlanetDefenderForce = { owner, ground: [], groundTotal: 0, pds: 0, otherStructures: [], guns: [] };
    for (const u of here.filter((unit) => unit.owner === owner)) {
      const base = getUnitBaseType(u.unit_type);
      if (base === "infantry" || base === "mech") {
        bump(force.ground, u.unit_type, u.damaged);
        force.groundTotal += 1;
      } else if (base === "pds") {
        force.pds += 1;
      } else {
        bump(force.otherStructures, u.unit_type, u.damaged);
      }
      const resolved = resolveUnit(u.unit_type, player?.faction, technologies);
      const cannon = resolved
        ? describeUnitStats(resolved).spaceCannon
        : base === "pds"
          ? fallbackPdsGun(u.unit_type, technologies)
          : undefined;
      if (cannon) {
        const gun = force.guns.find((g) => g.unit === u.unit_type);
        if (gun) gun.count += 1;
        else force.guns.push({ unit: u.unit_type, count: 1, hitsOn: cannon.hitsOn, dice: cannon.dice });
        dice += cannon.dice;
        expected += (cannon.dice * hitChancePercent(cannon.hitsOn)) / 100;
      }
    }
    // Mechs before infantry, so the stronger defenders read first.
    force.ground.sort((a, b) => (getUnitBaseType(b.unit) === "mech" ? 1 : 0) - (getUnitBaseType(a.unit) === "mech" ? 1 : 0));
    return force;
  });
  const silencedBy = players?.[actor]?.technologies.includes("l4") ? "L4 Disruptors" : undefined;
  return {
    forces,
    spaceCannonDefense:
      dice > 0 ? { dice, expectedHits: Math.round(expected * 10) / 10, ...(silencedBy ? { silencedBy } : {}) } : null,
  };
}

/** "3 Infantry, 1 Mech" (one damaged mech: "1 Mech (1 damaged)"). */
export function formatDefenderUnits(list: DefenderUnitCount[]): string {
  return list
    .map(
      (e) =>
        `${e.count} ${getUnitDisplayName(e.unit, e.count)}${e.damaged ? ` (${e.damaged} damaged)` : ""}`,
    )
    .join(", ");
}
