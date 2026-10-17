import { describe, expect, it } from "vitest";
import { describeComponent } from "./componentInfo.ts";
import { deriveTurnBar } from "./turnBar.ts";

const component = (id: string, label: string) => ({ id, label, kind: "component", payload: {} });

describe("describeComponent", () => {
  it("names a technology and gives its printed text, timing and cost", () => {
    const d = describeComponent(component("component|tech|sr", "use Sling Relay"));
    expect(d).toMatchObject({ name: "Sling Relay", kind: "Technology", timing: "ACTION", cost: "Exhausts the card" });
    expect(d.text).toMatch(/produce 1 ship in any system that contains one of your space docks/);
  });

  it("gives a leader's owner, window and ability (the engine sends only its name)", () => {
    const d = describeComponent(component("component|leader|solhero", "Jace X, 4th Air Legion"));
    expect(d.name).toBe("Jace X, 4th Air Legion");
    expect(d.kind).toMatch(/^Hero/);
    expect(d.owner).toBe("The Federation of Sol");
    expect(d.cost).toBe("Purged after use");
    expect(d.text && d.text.length).toBeGreaterThan(40);
  });

  it("marks an agent borrowed through Ssruu and describes the borrowed agent", () => {
    const d = describeComponent(component("component|leader|yssarilagent|xxchaagent", "Ggrocuto Rinn"));
    expect(d.name).toBe("Ggrocuto Rinn");
    expect(d.owner).toMatch(/Xxcha/);
    expect(d.note).toMatch(/Ssruu/);
  });

  it("prints a relic's name and text instead of the raw id", () => {
    const d = describeComponent(component("relic|codex", "use codex"));
    expect(d).toMatchObject({ name: "The Codex", kind: "Relic", timing: "ACTION", cost: "Purged after use" });
    expect(d.text).toMatch(/action cards of your choice from the action card discard pile/);
  });

  it("explains purging relic fragments", () => {
    const d = describeComponent(component("purge|Cultural", "purge 3 cultural relic fragments for a relic"));
    expect(d.name).toBe("Cultural relic fragments");
    expect(d.text).toMatch(/Purge 3 cultural relic fragments .* gain 1 relic/);
  });

  it("names Enigmatic Device, its cost and the technology it researches", () => {
    const d = describeComponent(component("play_area:ed1:gd", "spend 6 resources to research Gravity Drive"));
    expect(d.name).toBe("Enigmatic Device");
    expect(d.cost).toBe("Cost: 6 resources");
    expect(d.note).toBe("Researches Gravity Drive");
  });

  it("finds faction abilities, Hacan's technology and Hacan's note behind faction ids", () => {
    const drop = describeComponent(component("faction|orbital_drop", "Orbital Drop: spend a strategy token to land 2 infantry"));
    expect(drop).toMatchObject({ name: "Orbital Drop", kind: "Faction ability", owner: "The Federation of Sol", timing: "ACTION" });
    expect(drop.cost).toBe("Cost: 1 strategy token");
    expect(drop.text).toMatch(/place 2 infantry/);
    const biomes = describeComponent(component("faction|production_biomes", "Production Biomes: exhaust and spend a strategy token for 4 trade goods"));
    expect(biomes).toMatchObject({ name: "Production Biomes", kind: "Faction technology" });
    expect(biomes.text).toMatch(/gain 4 trade goods/);
    const convoys = describeComponent(component("faction|trade_convoys", "Trade Convoys: place it faceup in your play area"));
    expect(convoys).toMatchObject({ name: "Trade Convoys", kind: "Promissory note" });
    expect(convoys.owner).toMatch(/Hacan/);
    expect(convoys.text).toMatch(/not your neighbor/);
  });

  it("falls back to the engine's label for an unknown component", () => {
    const d = describeComponent({ id: "mystery|1", label: "something new", kind: "component", payload: {} });
    expect(d).toMatchObject({ name: "something new", kind: "Component action", text: null });
  });
});

describe("turn bar component rows", () => {
  it("carry the printed name, facts and full rules text while keeping the option id", () => {
    const model = deriveTurnBar({
      prompt: "action phase",
      options: [
        { id: "pass", label: "pass", kind: "action", payload: {} },
        component("relic|stellarconverter", "use stellarconverter"),
        component("component|leader|solhero", "Jace X, 4th Air Legion"),
      ],
      details: { kind: "turn_menu", closing: false },
    });
    const relic = model!.components.groups.find((g) => g.title === "Relics")!.items[0];
    expect(relic.optionId).toBe("relic|stellarconverter");
    expect(relic.label).toBe("Stellar Converter");
    expect(relic.facts).toEqual(["Relic", "ACTION", "Purged after use"]);
    expect(relic.detail).toMatch(/destroy all units on that planet/);
    const hero = model!.components.groups.find((g) => g.title === "Leaders")!.items[0];
    expect(hero.facts?.[0]).toMatch(/Hero.* · The Federation of Sol/);
    expect(hero.info.text).toBe(hero.detail);
  });
});
