import { describe, expect, it } from "vitest";
import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";
import { findTechnologyMeta } from "../protocol/contentCatalog.ts";
import {
  IN_SCOPE_FACTION_IDS,
  describeFaction,
  describeUnit,
  findFactionRecord,
  leaderStatus,
  resolveUnit,
} from "./factionInfo.ts";

const unitDescription = (key: string, faction?: string | null, techs: string[] = []) => {
  const resolved = resolveUnit(key, faction, techs);
  return resolved ? describeUnit(resolved) : undefined;
};

describe("faction lookup", () => {
  it("resolves by id and by printed name", () => {
    expect(findFactionRecord("sol")?.id).toBe("sol");
    expect(findFactionRecord("Federation of Sol")?.id).toBe("sol");
    expect(findFactionRecord("Barony of Letnev")?.id).toBe("letnev");
    expect(findFactionRecord("The Barony of Letnev")?.id).toBe("letnev");
  });

  it("returns undefined for an unknown or empty faction", () => {
    expect(findFactionRecord("no_such_faction")).toBeUndefined();
    expect(findFactionRecord("")).toBeUndefined();
    expect(findFactionRecord(null)).toBeUndefined();
    expect(describeFaction("no_such_faction")).toBeUndefined();
  });
});

describe("faction descriptions", () => {
  it.each(IN_SCOPE_FACTION_IDS)("%s resolves every section from the catalog", (id) => {
    const info = describeFaction(id);
    expect(info).toBeDefined();
    const faction = info!;
    expect(faction.abilities.length).toBeGreaterThan(0);
    for (const ability of faction.abilities) {
      expect(ability.permanentEffect || ability.windowEffect).toBeTruthy();
    }
    expect(faction.promissoryNotes.length).toBeGreaterThan(0);
    expect(faction.flagship?.text.length).toBeGreaterThanOrEqual(0);
    expect(faction.flagship?.factionSpecific).toBe(true);
    expect(faction.mech?.factionSpecific).toBe(true);
    expect(faction.startingTech.length).toBeGreaterThan(0);
    expect(faction.leaders.map((l) => l.type)).toEqual(["agent", "commander", "hero"]);
    for (const leader of faction.leaders) expect(leader.text).toBeTruthy();
  });

  // Factions the engine plays that are not seated by the in-scope rotation (reachable only through
  // the soak harness or, for the Obsidian, the Firmament's flip): the card must still resolve.
  it.each(["bastion", "deepwrought", "crimson", "ralnel", "nekro", "firmament", "obsidian"])(
    "%s (outside the seated scope) resolves a card without throwing",
    (id) => {
      const info = describeFaction(id);
      expect(info).toBeDefined();
      expect(info!.abilities.length).toBeGreaterThan(0);
      expect(info!.promissoryNotes.length).toBeGreaterThan(0);
      expect(info!.leaders.length).toBeGreaterThan(0);
    },
  );

  it("covers the whole catalog without throwing", () => {
    for (const id of Object.keys(GENERATED_CONTENT_CATALOG.factions)) {
      expect(() => describeFaction(id)).not.toThrow();
    }
  });

  it("every id a faction record points at exists in the catalog", () => {
    const { factions, units, leaders, factionAbilities, promissoryNotes, technologies } =
      GENERATED_CONTENT_CATALOG as unknown as Record<string, Record<string, any>>;
    const missing: string[] = [];
    for (const faction of Object.values(factions)) {
      if (faction.id === "neutral") continue;
      for (const id of faction.units) if (!units[id]) missing.push(`${faction.id} unit ${id}`);
      for (const id of faction.leaders) if (!leaders[id]) missing.push(`${faction.id} leader ${id}`);
      for (const id of faction.abilities)
        if (!factionAbilities[id]) missing.push(`${faction.id} ability ${id}`);
      for (const id of faction.promissoryNotes)
        if (!promissoryNotes[id]) missing.push(`${faction.id} note ${id}`);
      for (const id of [...faction.startingTech, ...faction.factionTech])
        if (!technologies[id]) missing.push(`${faction.id} tech ${id}`);
    }
    expect(missing).toEqual([]);
  });

  it("reads Sol's text straight from the catalog", () => {
    const sol = describeFaction("sol")!;
    expect(sol.mech?.name).toBe("ZS Thunderbolt M2");
    expect(sol.mech?.text.join(" ")).toContain("Orbital Drop");
    expect(sol.flagship?.name).toBe("Genesis");
    expect(sol.leaders.find((l) => l.type === "hero")?.name).toContain("Jace X");
    expect(sol.startingTech.map((t) => t.name).length).toBeGreaterThan(0);
    for (const tech of sol.factionTech) expect(findTechnologyMeta(tech.id)?.name).toBe(tech.name);
  });
});

describe("unit resolution", () => {
  it("gives the generic printed numbers without a faction", () => {
    const dreadnought = unitDescription("dreadnought")!;
    expect(dreadnought.name).toBe("Dreadnought I");
    expect(dreadnought.facts).toContainEqual({ label: "Cost", value: "4" });
    expect(dreadnought.facts).toContainEqual({ label: "Combat", value: "5" });
    expect(dreadnought.facts).toContainEqual({ label: "Move", value: "1" });
    expect(dreadnought.facts).toContainEqual({ label: "Capacity", value: "1" });
    expect(dreadnought.keywords).toEqual(["Sustain damage", "Bombardment 5"]);
  });

  it("describes anti-fighter barrage, dice counts, space cannon and planetary shield", () => {
    expect(unitDescription("destroyer")!.keywords).toContain("Anti-fighter barrage 9 (x2)");
    expect(unitDescription("pds")!.keywords).toEqual(["Space cannon 6", "Planetary shield"]);
    expect(unitDescription("flagship", "xxcha")!.keywords).toContain("Space cannon 5 (x3) (deep)");
    expect(unitDescription("flagship", "letnev")!.facts).toContainEqual({
      label: "Combat",
      value: "5 (x2)",
    });
  });

  it("shows production and fighter costs", () => {
    expect(unitDescription("spacedock")!.facts).toContainEqual({ label: "Production", value: "+2" });
    expect(unitDescription("fighter")!.facts).toContainEqual({ label: "Cost", value: "2 for 1" });
  });

  it("swaps in the upgrade the seat owns", () => {
    expect(unitDescription("dreadnought", null, ["dn2"])!.name).toBe("Dreadnought II");
    expect(unitDescription("dreadnought", null, ["dn2"])!.upgraded).toBe(true);
    expect(unitDescription("dreadnought", "hacan", [])!.upgraded).toBe(false);
  });

  it("uses the faction's own variant, flagship and mech", () => {
    expect(unitDescription("carrier", "sol")!.name).toBe("Advanced Carrier I");
    expect(unitDescription("carrier", "sol", ["ac2"])!.name).toBe("Advanced Carrier II");
    expect(unitDescription("infantry", "sol")!.factionSpecific).toBe(true);
    const flagship = unitDescription("flagship", "hacan")!;
    expect(flagship.name).toBe("Wrath of Kenara");
    expect(flagship.text[0]).toContain("trade good");
    expect(unitDescription("mech", "sol")!.name).toBe("ZS Thunderbolt M2");
    expect(unitDescription("mech", "jolnar")!.text.length).toBeGreaterThan(0);
  });

  it("names a specific card when there is no seat", () => {
    expect(unitDescription("sol_carrier2")!.name).toBe("Advanced Carrier II");
  });

  it("returns undefined for a unit it does not know", () => {
    expect(resolveUnit("not_a_unit", "sol")).toBeUndefined();
    expect(resolveUnit("not_a_unit", null)).toBeUndefined();
  });

  it("answers a faction it does not know with the generic card, never a faction one", () => {
    const resolved = resolveUnit("mech", "no_such_faction")!;
    expect(resolved.factionSpecific).toBe(false);
    expect(resolved.unit.name).toBe("Factionless Mech");
  });

  it("every unit's required technology exists in the catalog", () => {
    const units = Object.values(GENERATED_CONTENT_CATALOG.units) as Array<{
      id: string;
      requiredTechId?: string;
    }>;
    // ghemina is a homebrew faction whose upgrade tech is not in the technology corpus.
    const missing = units
      .filter((u) => u.requiredTechId && !findTechnologyMeta(u.requiredTechId))
      .filter((u) => u.id !== "ghemina_carrier2")
      .map((u) => `${u.id}:${u.requiredTechId}`);
    expect(missing).toEqual([]);
  });
});

describe("leader status", () => {
  it("lowercases the reported status and ignores leaders it was not told about", () => {
    expect(leaderStatus({ solhero: "Locked" }, "solhero")).toBe("locked");
    expect(leaderStatus({}, "solhero")).toBeUndefined();
    expect(leaderStatus(undefined, "solhero")).toBeUndefined();
  });
});
