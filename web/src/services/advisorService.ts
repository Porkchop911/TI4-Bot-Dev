import {
  BattleOddsRequest,
  BattleOddsResponse,
  BattleSideInput,
} from "../protocol/advisorTypes.ts";

/**
 * Normalizes faction name or id to a canonical id accepted by ti4-content / ti4-advisor.
 */
export function normalizeFaction(faction?: string | null): string {
  if (!faction) return "generic";
  const f = faction.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (f.includes("sol")) return "sol";
  if (f.includes("letnev")) return "letnev";
  if (f.includes("hacan")) return "hacan";
  if (f.includes("jol") || f.includes("nar")) return "jolnar";
  if (f.includes("sardakk") || f.includes("norr")) return "sardakk";
  if (f.includes("xxcha")) return "xxcha";
  if (f.includes("yssaril")) return "yssaril";
  if (f.includes("saar")) return "saar";
  if (f.includes("muaat")) return "muaat";
  if (f.includes("winnu")) return "winnu";
  if (f.includes("yin")) return "yin";
  if (f.includes("l1z1x") || f.includes("l1z1")) return "l1z1x";
  if (f.includes("mentak")) return "mentak";
  if (f.includes("naalu")) return "naalu";
  if (f.includes("creuss")) return "creuss";
  if (f.includes("arborec")) return "arborec";
  if (f.includes("nekro")) return "nekro";
  if (f.includes("argent")) return "argent";
  if (f.includes("empyrean")) return "empyrean";
  if (f.includes("mahact")) return "mahact";
  if (f.includes("naaz") || f.includes("rokha")) return "naazrokha";
  if (f.includes("nomad")) return "nomad";
  if (f.includes("titan")) return "titans";
  if (f.includes("cabal") || f.includes("vuil")) return "vuilraith";
  if (f.includes("keleres")) return "keleres";
  return f || "generic";
}

export interface FetchBattleOddsOptions {
  signal?: AbortSignal;
  baseUrl?: string;
}

/**
 * Calls the ti4-advisor /battle endpoint to simulate combat rollouts.
 * Defaults to baseUrl "/advisor" (proxied to port 8081).
 */
export async function fetchBattleOdds(
  request: BattleOddsRequest,
  options?: FetchBattleOddsOptions,
): Promise<BattleOddsResponse> {
  const baseUrl = options?.baseUrl ?? "/advisor";
  const url = `${baseUrl.replace(/\/$/, "")}/battle`;

  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
    signal: options?.signal,
  });

  if (!response.ok) {
    const errorText = await response.text().catch(() => "");
    throw new Error(
      `Advisor battle request failed with status ${response.status}: ${errorText || response.statusText}`,
    );
  }

  return (await response.json()) as BattleOddsResponse;
}

export interface GroundOddsSide {
  faction: string;
  units: Record<string, number>;
  damaged: Record<string, number>;
  guns?: Record<string, number>;
}

export interface GroundOddsRequest {
  attacker: GroundOddsSide;
  defender: GroundOddsSide;
  harrow?: Record<string, number>;
  simulations?: number;
}

export async function fetchGroundOdds(
  request: GroundOddsRequest,
  signal?: AbortSignal,
): Promise<{ attacker_win_rate: number; simulations: number }> {
  const response = await fetch("/advisor/ground_odds", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
    signal,
  });
  if (!response.ok) throw new Error(`Ground odds unavailable: ${response.status}`);
  return response.json();
}

/**
 * Constructs a BattleOddsRequest for combat in progress.
 */
export function buildBattleRequest(params: {
  attackerFaction?: string | null;
  attackerUnits: Record<string, number>;
  attackerDamaged?: Record<string, number>;
  defenderFaction?: string | null;
  defenderUnits: Record<string, number>;
  defenderDamaged?: Record<string, number>;
  simulations?: number;
}): BattleOddsRequest {
  const attackerSide: BattleSideInput = {
    faction: normalizeFaction(params.attackerFaction),
    units: params.attackerUnits,
  };
  if (params.attackerDamaged && Object.keys(params.attackerDamaged).length > 0) {
    attackerSide.damaged = params.attackerDamaged;
  }

  const defenderSide: BattleSideInput = {
    faction: normalizeFaction(params.defenderFaction),
    units: params.defenderUnits,
  };
  if (params.defenderDamaged && Object.keys(params.defenderDamaged).length > 0) {
    defenderSide.damaged = params.defenderDamaged;
  }

  return {
    attacker: attackerSide,
    defender: defenderSide,
    in_progress: true,
    simulations: params.simulations ?? 2000,
  };
}
