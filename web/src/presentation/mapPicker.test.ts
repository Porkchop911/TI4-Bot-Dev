import { describe, expect, it } from "vitest";
import { fixtureMapChoice, fixturePreview, fixtureTemplates } from "../dev/mapPickerFixtures.ts";
import {
  choiceOf,
  homeIdSet,
  mainTiles,
  mapCards,
  mapChangeNotice,
  mapName,
  mapSummaryLine,
  templateBadges,
  templateTitle,
  tileKind,
} from "./mapPicker.ts";

describe("map picker model", () => {
  it("lists exactly the templates the server gave, then Random, with one selected", () => {
    const templates = fixtureTemplates(6);
    const cards = mapCards(templates, fixtureMapChoice("6pBeMyNeighbor"));
    expect(cards.map((c) => c.key)).toEqual([...templates.map((t) => t.alias), "random"]);
    expect(cards.filter((c) => c.selected).map((c) => c.key)).toEqual(["6pBeMyNeighbor"]);
    expect(cards.find((c) => c.recommended)?.key).toBe("6pStandard");
  });

  it("offers Random even when no template fits, and selects it when it is the choice", () => {
    const cards = mapCards([], fixtureMapChoice(null));
    expect(cards).toHaveLength(1);
    expect(cards[0]).toMatchObject({
      key: "random",
      selected: true,
      choice: { kind: "random" },
    });
  });

  it("names layouts plainly and labels Nucleus and in-person ones honestly", () => {
    expect(templateTitle("6pStandard")).toBe("Standard");
    expect(templateTitle("6pBeMyNeighbor")).toBe("Be my neighbor");
    expect(templateTitle("3pInPersonHyperlanes")).toBe("Hyperlanes");
    expect(templateTitle("6pStandardNucleus")).toBe("Standard + Nucleus");
    expect(templateBadges("6pStandardNucleus")).toEqual(["Nucleus (no special rules yet)"]);
    expect(templateBadges("3pInPersonHyperlanes", { hyperlanes: true })).toEqual([
      "Hyperlanes",
      "Meant for in-person play",
    ]);
    expect(templateBadges("6pStandard", { hyperlanes: false })).toEqual([]);
  });

  it("describes the choice in one line and never mentions a seed", () => {
    const line = mapSummaryLine({
      ...fixtureMapChoice("6pStandard"),
      author: "Someone",
    });
    expect(line).toBe("6 players · Standard by Someone · 37 systems · no hyperlanes");
    expect(mapSummaryLine(fixtureMapChoice(null))).toBe("Random map · 36 systems · no hyperlanes");
    expect(`${line}${mapName(fixtureMapChoice(null))}`.toLowerCase()).not.toContain("seed");
    expect(choiceOf(fixtureMapChoice("6pStandard"))).toEqual({
      kind: "template",
      alias: "6pStandard",
    });
  });

  it("titles a community layout by the name the server sends", () => {
    const community = {
      ...fixtureTemplates(6)[0],
      alias: "6pWontYouBeMyNeighbor",
      name: "Won't You Be My Neighbor?",
      author: "Chart the Galaxy entry",
      recommended: false,
    };
    const card = mapCards([community], undefined)[0];
    expect(card.title).toBe("Won't You Be My Neighbor?");
    expect(card.detail).toContain("Chart the Galaxy entry");
    const choice = {
      ...fixtureMapChoice("6pScpt2021Final"),
      name: "SCPT 2021 Final",
      author: "SCPT",
    };
    expect(mapSummaryLine(choice)).toBe(
      "6 players · SCPT 2021 Final by SCPT · 37 systems · no hyperlanes",
    );
    expect(mapName(choice)).toBe("SCPT 2021 Final");
    expect(mapName(fixtureMapChoice("6pStandard"))).toBe("6pStandard");
  });

  it("classifies tiles for the pictures", () => {
    const preview = fixturePreview(6);
    const homes = homeIdSet(preview.seats);
    const kinds = mainTiles(preview.tiles).map((t) => tileKind(t, homes));
    expect(kinds.filter((k) => k === "home")).toHaveLength(6);
    expect(kinds.filter((k) => k === "mecatol")).toHaveLength(1);
    expect(kinds).toContain("anomaly");
    expect(kinds).toContain("wormhole");
    expect(kinds).toContain("planets");
    expect(tileKind({ system_id: "83a", label: "", q: 0, r: 0, hyperlane: true }, homes)).toBe(
      "hyperlane",
    );
  });

  it("announces a change only when the revision moves", () => {
    const map = fixtureMapChoice("6pStandard");
    expect(mapChangeNotice(null, { revision: 1, map })).toBeNull();
    expect(mapChangeNotice({ revision: 1, map }, { revision: 1, map })).toBeNull();
    expect(mapChangeNotice({ revision: 1, map }, { revision: 2, map })).toBe(
      "The host changed the map.",
    );
    expect(mapChangeNotice({ revision: undefined, map }, { revision: 2, map })).toBeNull();
  });
});
