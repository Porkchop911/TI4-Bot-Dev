import type {
  BoardView,
  ChoiceOptionDto,
  GameEvent,
  PendingChoiceDto,
  PlayerView,
} from "../protocol/types.ts";

/** Small builders for the strategy-card secondary preparation tests. */

export const player = (id: string, overrides: Partial<PlayerView> = {}): PlayerView => ({
  id,
  faction: "sol",
  victory_points: 0,
  trade_goods: 0,
  commodities: 0,
  tactic_tokens: 3,
  fleet_tokens: 3,
  strategic_tokens: 2,
  passed: false,
  strategy_cards: [],
  exhausted_strategy_cards: [],
  technologies: [],
  exhausted_technologies: [],
  relics: [],
  exhausted_relics: [],
  action_cards_count: 0,
  secret_objectives_count: 0,
  leaders: {},
  ...overrides,
});

let counter = 0;
/** A published decision event of an action; `detail` carries "<seat> played <Card>". */
export function actionEvent(
  actionId: string,
  actor: string,
  overrides: Partial<GameEvent> = {},
): GameEvent {
  counter += 1;
  return {
    id: `e${counter}`,
    timestamp: "2026-10-07T00:00:00Z",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    action_id: actionId,
    action_type: "strategic",
    action_actor: "a",
    actor,
    round: 2,
    phase: "action",
    ...overrides,
  } as GameEvent;
}

/** The log of seat `a` playing `card` (by printed name when `named`) as action_5. */
export const playedLog = (named: string | null = "Technology"): GameEvent[] => [
  actionEvent("action_5", "a", named ? { detail: `a played ${named}` } : { detail: "a began a component action" }),
];

export const option = (id: string, kind = "strategy", label = id): ChoiceOptionDto => ({ id, kind, label });

const yesNo = (): ChoiceOptionDto[] => [option("no", "strategy", "decline"), option("yes", "strategy", "go")];

/** The follower's own secondary question for `card`, as the engine sends it. */
export function secondaryChoice(
  card: string,
  overrides: Partial<PendingChoiceDto> = {},
  options: ChoiceOptionDto[] = yesNo(),
): PendingChoiceDto {
  return {
    actor: "b",
    nonce: "n-secondary",
    prompt: "spend a strategy token to research",
    options,
    context: { subtype: "strategy_secondary" } as PendingChoiceDto["context"],
    details: { kind: "strategy_secondary", card, played_by: "a", tokens_left: 2, costs_token: true },
    ...overrides,
  };
}

export const stepChoice = (
  subtype: string,
  options: ChoiceOptionDto[],
  nonce = `n-${subtype}`,
): PendingChoiceDto => ({
  actor: "b",
  nonce,
  prompt: subtype,
  options,
  context: { subtype } as PendingChoiceDto["context"],
});

/** The server's answer to a preview request, as the client hands it to the preparation hook. */
export const previewReply = (
  body: import("../protocol/types.ts").SecondaryPreviewBody,
): import("../protocol/client.ts").SecondaryPreviewReply => ({
  kind: "preview",
  body,
  asOfVersion: 9,
  asOfDecisions: 40,
});

/** The engine's own window question for `card`, exactly as `strategy.rs::secondary_choice` builds it. */
export function engineWindow(
  card: string,
  prompt: string,
  extra: Record<string, unknown> = {},
): import("../protocol/types.ts").EngineChoice {
  return {
    player: "b",
    prompt,
    options: [option("no", "strategy", "decline"), option("yes", "strategy", "go")],
    details: {
      kind: "strategy_secondary",
      card,
      played_by: "a",
      tokens_left: 2,
      costs_token: true,
      ...extra,
    },
  };
}

/** The engine's "research a technology" question for the Technology secondary (cost 4, decline offered). */
export function engineResearch(ids: string[]): import("../protocol/types.ts").EngineChoice {
  return {
    player: "b",
    prompt: "research a technology",
    options: [
      ...ids.map((id) => ({
        id,
        kind: "research",
        label: id,
        payload: { cost: 4, cost_tokens: 0 },
      })),
      option("decline", "decline", "decline"),
    ],
    context: {
      subtype: "research_technology",
      source: { StrategyCard: { card: "Technology", secondary: true } },
    } as import("../protocol/types.ts").EngineChoice["context"],
  };
}

/** The home production's build list as `production.rs` offers it: capacity 3, 5 resources to spend. */
export function engineProduction(
  builds: { unit: string; cost: number; count: number }[],
  over: { capacity?: number; resources?: number } = {},
): import("../protocol/types.ts").EngineChoice {
  return {
    player: "b",
    prompt: "produce in 18 (3 left)",
    options: [
      ...builds.map(({ unit, cost, count }) => ({
        id: `build|${unit}|${count}`,
        kind: "produce",
        label: `produce ${count}x ${unit} for ${cost}`,
        payload: {
          unit,
          cost,
          printed_cost: cost,
          discount: 0,
          count,
          placed: count,
          yield: count,
          credit: 0,
          available_resources: over.resources ?? 5,
          free_this_use: false,
          credit_used: 0,
          owed: cost,
          production_spent: count,
          system: "18",
        },
      })),
      option("done_producing", "decline", "produce nothing further"),
    ],
    context: {
      subtype: "produce_unit",
      target: { System: "18" },
      outstanding: [{ kind: "ProductionCapacity", amount: over.capacity ?? 3, paid: 0 }],
    } as import("../protocol/types.ts").EngineChoice["context"],
    details: { fleet_supply: { used: 1, limit: 4 } },
  };
}

/** The payment question the first build opens: two planets and trade goods. */
export function enginePayment(owed: number): import("../protocol/types.ts").EngineChoice {
  return {
    player: "b",
    prompt: `pay ${owed} more resources`,
    options: [
      {
        id: "exhaust|jord",
        kind: "pay",
        label: "exhaust jord",
        payload: { worth: 4, owed, kind: "resources", source: "planet", planet_name: "Jord" },
      },
      {
        id: "exhaust|arinam",
        kind: "pay",
        label: "exhaust arinam",
        payload: { worth: 1, owed, kind: "resources", source: "planet", planet_name: "Arinam" },
      },
      { id: "trade_good", kind: "pay", label: "spend a trade good", payload: { worth: 1, owed, kind: "resources" } },
    ],
    context: {
      subtype: "pay_resources",
      outstanding: [{ kind: "resources", amount: owed, paid: 0 }],
    } as import("../protocol/types.ts").EngineChoice["context"],
  };
}

export const boardWith = (
  planets: { system: string; planet: string; owner: string; exhausted?: boolean }[],
): BoardView => {
  const systems: BoardView["systems"] = {};
  for (const entry of planets) {
    systems[entry.system] ??= { system_id: entry.system, command_tokens: [], planets: {}, units: [] };
    systems[entry.system].planets[entry.planet] = {
      planet_id: entry.planet,
      controlled_by: entry.owner,
      exhausted: Boolean(entry.exhausted),
    };
  }
  return { systems };
};
