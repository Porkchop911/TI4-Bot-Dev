/** Display-only facts the engine attaches to Diplomacy's ready decisions (`details.ready_hints`). */
export interface ReadyHintPlanet {
  id: string;
  name: string;
  resources: number | null;
  influence: number | null;
}

export type SpenderStatus = "played" | "to_come" | "not_in_play";

export interface SpenderHint {
  card: string;
  /** What the card spends: Leadership influence, Technology and Warfare resources. */
  spends: "influence" | "resources";
  status: SpenderStatus;
}

export interface ReadyHints {
  planets: ReadyHintPlanet[];
  spenders: SpenderHint[];
}

const STATUSES: readonly SpenderStatus[] = ["played", "to_come", "not_in_play"];

/** Parse `details.ready_hints`; null when absent or malformed (the UI then shows nothing extra). */
export function readReadyHints(details: Record<string, unknown> | undefined): ReadyHints | null {
  const raw = details?.ready_hints;
  if (!raw || typeof raw !== "object") return null;
  const { planets, spenders } = raw as Record<string, unknown>;
  if (!Array.isArray(planets) || !Array.isArray(spenders)) return null;
  const outPlanets: ReadyHintPlanet[] = [];
  for (const entry of planets) {
    if (!entry || typeof entry !== "object") return null;
    const e = entry as Record<string, unknown>;
    if (typeof e.id !== "string") return null;
    outPlanets.push({
      id: e.id,
      name: typeof e.name === "string" ? e.name : e.id,
      resources: typeof e.resources === "number" ? e.resources : null,
      influence: typeof e.influence === "number" ? e.influence : null,
    });
  }
  const outSpenders: SpenderHint[] = [];
  for (const entry of spenders) {
    if (!entry || typeof entry !== "object") return null;
    const e = entry as Record<string, unknown>;
    if (typeof e.card !== "string" || !STATUSES.includes(e.status as SpenderStatus)) return null;
    outSpenders.push({
      card: e.card,
      spends: e.spends === "influence" ? "influence" : "resources",
      status: e.status as SpenderStatus,
    });
  }
  return { planets: outPlanets, spenders: outSpenders };
}

/** "Leadership played this round: yes" style line text. */
export function spenderAnswer(status: SpenderStatus): { text: string; played: boolean } {
  if (status === "played") return { text: "yes", played: true };
  if (status === "to_come") return { text: "no, still to come", played: false };
  return { text: "no, not in play", played: false };
}
