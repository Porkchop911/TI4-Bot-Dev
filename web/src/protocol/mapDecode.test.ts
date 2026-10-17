import { describe, expect, it } from "vitest";
import { fixtureLobby, fixturePreview, fixtureTemplates } from "../dev/mapPickerFixtures.ts";
import { decodeLobby } from "./decode.ts";
import { decodeMapChoice, decodeMapList, decodeMapPreview } from "@ti4/shared/wire/decode.ts";

describe("map protocol decoding", () => {
  it("keeps only maps that build", () => {
    const list = [
      ...fixtureTemplates(4),
      { ...fixtureTemplates(4)[0], alias: "4pStaticEq", buildable: false },
    ];
    expect(decodeMapList(list).map((m) => m.alias)).not.toContain("4pStaticEq");
    expect(decodeMapList(list)).toHaveLength(4);
    expect(() => decodeMapList({})).toThrow(/map list/);
  });

  it("keeps a template's display name and rejects a blank one", () => {
    const entry = {
      ...fixtureTemplates(6)[0],
      alias: "6pBalancedSpiral",
      name: "Balanced Spiral",
    };
    expect(decodeMapList([entry])[0].name).toBe("Balanced Spiral");
    expect(decodeMapList([fixtureTemplates(6)[0]])[0]).not.toHaveProperty("name");
    expect(() => decodeMapList([{ ...entry, name: "" }])).toThrow(/map list entry/);
    const choice = {
      kind: "template",
      alias: "6pBalancedSpiral",
      name: "Balanced Spiral",
      systems: 31,
      hyperlanes: false,
      recommended: false,
    };
    expect(decodeMapChoice(choice).name).toBe("Balanced Spiral");
    expect(() => decodeMapChoice({ ...choice, name: 3 })).toThrow(/map choice/);
  });

  it("round-trips a preview and rejects a broken one", () => {
    const preview = fixturePreview(4);
    expect(decodeMapPreview(JSON.parse(JSON.stringify(preview)))).toEqual(preview);
    expect(() => decodeMapPreview({ ...preview, tiles: "nope" })).toThrow(/tiles/);
    expect(() => decodeMapPreview({ ...preview, choice: { kind: "x" } })).toThrow(/choice/);
  });

  it("requires a template choice to name its alias", () => {
    expect(() =>
      decodeMapChoice({
        kind: "template",
        systems: 3,
        hyperlanes: false,
        recommended: false,
      }),
    ).toThrow();
    expect(
      decodeMapChoice({
        kind: "random",
        systems: 3,
        hyperlanes: false,
        recommended: false,
      }).kind,
    ).toBe("random");
  });

  it("decodes the lobby's map and tolerates a lobby without one", () => {
    const lobby = fixtureLobby(3, "3pHyperlanes", 4);
    const decoded = decodeLobby(JSON.parse(JSON.stringify(lobby)), "gallery-map");
    expect(decoded.map?.alias).toBe("3pHyperlanes");
    expect(decoded.map_revision).toBe(4);
    const { map: _map, map_revision: _rev, ...old } = lobby;
    void _map;
    void _rev;
    const legacy = decodeLobby(old, "gallery-map");
    expect(legacy.map).toBeUndefined();
    expect(legacy.map_revision).toBeUndefined();
  });

  it("reads the strategy card set and ignores one it does not know or none at all", () => {
    const raw = (set: unknown) =>
      JSON.parse(JSON.stringify({ ...fixtureLobby(3), strategy_card_set: set }));
    expect(decodeLobby(raw("te"), "gallery-map").strategy_card_set).toBe("te");
    expect(decodeLobby(raw("pok"), "gallery-map").strategy_card_set).toBe("pok");
    expect(decodeLobby(raw("base_game"), "gallery-map").strategy_card_set).toBeUndefined();
    expect(decodeLobby(raw(undefined), "gallery-map").strategy_card_set).toBeUndefined();
  });

  it("reads the faction pool and a bot seat's chosen faction, and tolerates their absence", () => {
    const base = JSON.parse(JSON.stringify(fixtureLobby(3)));
    base.slots[1] = { ...base.slots[1], occupant: base.slots[1].occupant ?? "player_x", bot: "smart", faction: "sol" };
    const decoded = decodeLobby({ ...base, factions: ["sol", "hacan", 3] }, "gallery-map");
    expect(decoded.factions).toEqual(["sol", "hacan"]);
    expect(decoded.slots[1].faction).toBe("sol");
    const old = decodeLobby(JSON.parse(JSON.stringify(fixtureLobby(3))), "gallery-map");
    expect(old.factions).toBeUndefined();
    expect(old.slots[0]).not.toHaveProperty("faction");
  });
});
