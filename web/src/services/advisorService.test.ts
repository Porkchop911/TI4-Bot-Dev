import { describe, it, expect, vi, afterEach } from "vitest";
import { normalizeFaction, buildBattleRequest, fetchBattleOdds } from "./advisorService.ts";
import { BattleOddsResponse } from "../protocol/advisorTypes.ts";

describe("advisorService", () => {
  const originalFetch = global.fetch;

  afterEach(() => {
    global.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  describe("normalizeFaction", () => {
    it("maps recognized faction strings to base canonical IDs", () => {
      expect(normalizeFaction("Federation of Sol")).toBe("sol");
      expect(normalizeFaction("sol")).toBe("sol");
      expect(normalizeFaction("Barony of Letnev")).toBe("letnev");
      expect(normalizeFaction("Emirates of Hacan")).toBe("hacan");
      expect(normalizeFaction("Universities of Jol-Nar")).toBe("jolnar");
      expect(normalizeFaction("Sardakk N'orr")).toBe("sardakk");
      expect(normalizeFaction("Xxcha Kingdom")).toBe("xxcha");
      expect(normalizeFaction("Yssaril Tribes")).toBe("yssaril");
      expect(normalizeFaction("Clan of Saar")).toBe("saar");
      expect(normalizeFaction("Embers of Muaat")).toBe("muaat");
      expect(normalizeFaction("Winnu")).toBe("winnu");
      expect(normalizeFaction("Yin Brotherhood")).toBe("yin");
      expect(normalizeFaction("L1Z1X Mindnet")).toBe("l1z1x");
      expect(normalizeFaction("Mentak Coalition")).toBe("mentak");
      expect(normalizeFaction("Naalu Collective")).toBe("naalu");
      expect(normalizeFaction("Ghosts of Creuss")).toBe("creuss");
      expect(normalizeFaction("Arborec")).toBe("arborec");
      expect(normalizeFaction("Nekro Virus")).toBe("nekro");
      expect(normalizeFaction("Argent Flight")).toBe("argent");
      expect(normalizeFaction("Empyrean")).toBe("empyrean");
      expect(normalizeFaction("Mahact Gene-Sorcerers")).toBe("mahact");
      expect(normalizeFaction("Naaz-Rokha Alliance")).toBe("naazrokha");
      expect(normalizeFaction("Nomad")).toBe("nomad");
      expect(normalizeFaction("Titans of Ul")).toBe("titans");
      expect(normalizeFaction("Vuil'Raith Cabal")).toBe("vuilraith");
      expect(normalizeFaction("Council Keleres")).toBe("keleres");
    });

    it("falls back to generic for empty or unrecognized values", () => {
      expect(normalizeFaction(null)).toBe("generic");
      expect(normalizeFaction(undefined)).toBe("generic");
      expect(normalizeFaction("")).toBe("generic");
      expect(normalizeFaction("Unknown")).toBe("unknown");
    });
  });

  describe("buildBattleRequest", () => {
    it("builds a valid BattleOddsRequest with in_progress: true and normalized factions", () => {
      const request = buildBattleRequest({
        attackerFaction: "Federation of Sol",
        attackerUnits: { carrier: 1, fighter: 4 },
        attackerDamaged: { carrier: 1 },
        defenderFaction: "Barony of Letnev",
        defenderUnits: { dreadnought: 2 },
        defenderDamaged: { dreadnought: 1 },
        simulations: 1000,
      });

      expect(request).toEqual({
        attacker: {
          faction: "sol",
          units: { carrier: 1, fighter: 4 },
          damaged: { carrier: 1 },
        },
        defender: {
          faction: "letnev",
          units: { dreadnought: 2 },
          damaged: { dreadnought: 1 },
        },
        in_progress: true,
        simulations: 1000,
      });
    });

    it("defaults simulations to 2000 and omits empty damaged maps", () => {
      const request = buildBattleRequest({
        attackerUnits: { cruiser: 2 },
        defenderUnits: { destroyer: 1 },
      });

      expect(request.simulations).toBe(2000);
      expect((request.attacker as { faction: string }).faction).toBe("generic");
      expect((request.attacker as { damaged?: Record<string, number> }).damaged).toBeUndefined();
    });
  });

  describe("fetchBattleOdds", () => {
    it("posts request to /advisor/battle and returns parsed BattleOddsResponse", async () => {
      const mockResponse: BattleOddsResponse = {
        simulations: 2000,
        attacker_win_rate: 0.65,
        defender_win_rate: 0.3,
        mutual_destruction_rate: 0.05,
        unresolved_rate: 0,
        average_rounds: 2.1,
        attacker_expected_survivors: { carrier: 0.7 },
        defender_expected_survivors: { dreadnought: 0.3 },
        attacker_fielded: { carrier: 1 },
        defender_fielded: { dreadnought: 1 },
      };

      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: async () => mockResponse,
      } as Response);

      const request = buildBattleRequest({
        attackerUnits: { carrier: 1 },
        defenderUnits: { dreadnought: 1 },
      });

      const result = await fetchBattleOdds(request);
      expect(result).toEqual(mockResponse);
      expect(global.fetch).toHaveBeenCalledWith("/advisor/battle", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
        signal: undefined,
      });
    });

    it("throws an error when server returns non-ok response", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 400,
        statusText: "Bad Request",
        text: async () => "Invalid unit id",
      } as Response);

      const request = buildBattleRequest({
        attackerUnits: { carrier: 1 },
        defenderUnits: { dreadnought: 1 },
      });

      await expect(fetchBattleOdds(request)).rejects.toThrow(
        /Advisor battle request failed with status 400: Invalid unit id/,
      );
    });
  });
});
