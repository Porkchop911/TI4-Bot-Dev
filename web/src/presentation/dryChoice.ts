import type {
  BoardView,
  ChoiceOptionDto,
  PendingChoiceDto,
  PlayerView,
} from "../protocol/types.ts";
import { findPlanetMeta, findStrategyCardMeta } from "../protocol/contentCatalog.ts";
import type { StrategicAction } from "./strategicAction.ts";
import { ownPlanets, researchableEstimate, techName, STRUCTURE_UNITS } from "./secondaryPlan.ts";
import { pendingFromEngine, type ExactResult } from "./secondaryPreview.ts";

/**
 * A "dry choice": the engine's PendingChoice for a strategy-card secondary, synthesized on the
 * client from what it already knows (the card in progress, the viewer's own technologies, planets
 * and tokens). The real components render it in preparation mode; answering it records a plan
 * instead of submitting. Shapes follow `ti4-engine` `strategy.rs::secondary_choice` /
 * `secondary_question` and `strategy_cards.rs` (`ready_planets`, `structure_options`,
 * `research_option`, `purchase_details`), read, not run.
 */

/** Which question of the card a dry choice stands for. */
export type DryStep = "secondary" | "tech" | "planet1" | "planet2" | "site" | "produce" | "pay";

export interface DryChoice {
  choice: PendingChoiceDto;
  step: DryStep;
  /** `null` when the dry choice is faithful; otherwise what is approximate until the real question opens. */
  approximate: string | null;
  /**
   * True when the engine itself built the question (the server's read-only preview): the options,
   * payloads and context are the real ones, as of the position the preview was taken at.
   */
  exact: boolean;
  /** How the engine would pay the 4 resources of a Technology research (exact previews only). */
  payment?: import("../protocol/types.ts").PreviewPayment;
}

export const DRY_NONCE_PREFIX = "prepare:";
export const isDryNonce = (nonce: string | undefined | null): boolean =>
  Boolean(nonce?.startsWith(DRY_NONCE_PREFIX));

const STRATEGY_KIND = "strategy";
const INFLUENCE_PER_TOKEN = 3;

/** The follow prompt and "yes" caption per card, exactly the engine's strings. */
const SECONDARY_PROMPT: Record<string, { prompt: string; yes: string; no: string }> = {
  Trade: { prompt: "spend a strategy token to replenish commodities", yes: "replenish", no: "decline" },
  Construction: { prompt: "spend a strategy token to build a structure", yes: "build", no: "decline" },
  Warfare: { prompt: "spend a strategy token to produce at home", yes: "produce", no: "decline" },
  Technology: { prompt: "spend a strategy token and 4 resources to research", yes: "spend", no: "decline" },
  Imperial: { prompt: "spend a strategy token to draw a secret objective", yes: "draw", no: "decline" },
  Diplomacy: { prompt: "spend a strategy token to ready two planets", yes: "ready", no: "decline" },
  Politics: { prompt: "spend a strategy token to draw two action cards", yes: "draw", no: "decline" },
};

const baseChoice = (
  viewerSeat: string,
  step: DryStep,
  key: string,
  prompt: string,
  options: ChoiceOptionDto[],
  subtype: string,
  details?: Record<string, unknown>,
): PendingChoiceDto => ({
  actor: viewerSeat,
  nonce: `${DRY_NONCE_PREFIX}${key}:${step}`,
  prompt,
  options,
  context: { subtype } as PendingChoiceDto["context"],
  ...(details ? { details } : {}),
});

/** Planets that can pay influence now: ready, controlled, with influence on the card. */
function influencePlanets(board: BoardView | undefined, seat: string) {
  return ownPlanets(board, seat)
    .filter((planet) => !planet.exhausted)
    .map((planet) => ({ id: planet.planet, worth: findPlanetMeta(planet.planet)?.influence ?? 0 }))
    .filter((planet) => planet.worth > 0);
}

export interface DryInput {
  action: StrategicAction;
  viewer: PlayerView;
  board: BoardView | undefined;
  step: DryStep;
  /** Diplomacy second pick: the planet already prepared first. */
  taken?: string[];
  /** The engine's own answer for this step, when the server could give one. */
  exact?: ExactResult | null;
}

/** The window's first question: follow or skip (Leadership: the influence purchase itself). */
function secondaryStep({ action, viewer, board }: DryInput): DryChoice {
  const meta = findStrategyCardMeta(action.card);
  const name = meta?.name ?? action.cardName;
  const costsToken = action.family !== "leadership";
  const details: Record<string, unknown> = {
    kind: "strategy_secondary",
    card: action.card,
    played_by: action.primary,
    tokens_left: viewer.strategic_tokens,
    costs_token: costsToken,
  };
  if (action.family === "leadership") {
    const planets = influencePlanets(board, viewer.id);
    const influence =
      planets.reduce((sum, planet) => sum + planet.worth, 0) + viewer.trade_goods;
    details.kind = "strategy_secondary";
    details.mode = "buy";
    details.pools = {
      tactic: viewer.tactic_tokens,
      fleet: viewer.fleet_tokens,
      strategic: viewer.strategic_tokens,
    };
    details.tokens_to_place = 0;
    details.purchase = {
      cost: INFLUENCE_PER_TOKEN,
      influence_available: influence,
      max: Math.floor(influence / INFLUENCE_PER_TOKEN),
      trade_goods: viewer.trade_goods,
      trade_good_worth: 1,
      planets,
    };
    const choice = baseChoice(
      viewer.id,
      "secondary",
      action.key,
      `spend ${INFLUENCE_PER_TOKEN} influence for a command token`,
      [
        { id: "no", kind: STRATEGY_KIND, label: "spend nothing further" },
        { id: "yes", kind: STRATEGY_KIND, label: `spend ${INFLUENCE_PER_TOKEN} influence` },
      ],
      "buy_token_with_influence",
      details,
    );
    return {
      choice,
      step: "secondary",
      exact: false,
      approximate:
        "Your influence is counted from your planets and trade goods. Faction abilities that change what you can spend, and the exact reinforcement room, are only known when the real question opens; the payment is planned then.",
    };
  }
  const contract = SECONDARY_PROMPT[name];
  const choice = baseChoice(
    viewer.id,
    "secondary",
    action.key,
    contract?.prompt ?? `${action.card} secondary`,
    [
      { id: "no", kind: STRATEGY_KIND, label: contract?.no ?? "decline" },
      { id: "yes", kind: STRATEGY_KIND, label: contract?.yes ?? "follow" },
    ],
    "strategy_secondary",
    details,
  );
  return {
    choice,
    step: "secondary",
    exact: false,
    approximate:
      action.family === "trade"
        ? "If the Trade player replenishes you, no secondary is offered to you at all and this plan is dropped."
        : null,
  };
}

function techStep({ action, viewer }: DryInput): DryChoice {
  // As the engine's secondary research offers it (`paid_research`): each option carries the
  // card's 4-resource cost, the strategy token was taken by the window, and decline closes it.
  const options: ChoiceOptionDto[] = researchableEstimate(viewer).map((id) => ({
    id,
    kind: "research",
    label: techName(id),
    payload: { cost: 4, cost_tokens: 0 },
  }));
  options.push({ id: "decline", kind: "decline", label: "decline" });
  const choice = baseChoice(viewer.id, "tech", action.key, "research a technology", options, "research_technology");
  choice.context = {
    ...choice.context!,
    source: { StrategyCard: { card: "Technology", secondary: true } },
  };
  return {
    choice,
    step: "tech",
    exact: false,
    approximate:
      "This list is worked out from your technologies. Prerequisite skips, faction waivers and discounts are only known when the real question opens, and the engine chooses how the 4 resources are paid.",
  };
}

function planetStep(input: DryInput): DryChoice {
  const { action, viewer, board, step, taken = [] } = input;
  const options: ChoiceOptionDto[] = ownPlanets(board, viewer.id)
    .filter((planet) => planet.exhausted && !taken.includes(planet.planet))
    .map((planet) => ({
      id: planet.planet,
      kind: "ready",
      label: `ready ${planet.planet}`,
      payload: { planet: planet.planet, system: planet.system },
    }));
  const choice = baseChoice(viewer.id, step, action.key, "ready which planet", options, "ready_planet");
  return {
    choice,
    step,
    exact: false,
    approximate:
      "Planets are read from the board as it is now; the real question offers the planets that are exhausted when it opens.",
  };
}

function siteStep({ action, viewer, board }: DryInput): DryChoice {
  const units = new Set<string>();
  for (const [systemId, system] of Object.entries(board?.systems ?? {})) {
    for (const unit of system.units ?? []) {
      if (unit.owner === viewer.id && unit.planet) units.add(`${unit.unit_type}|${systemId}|${unit.planet}`);
    }
  }
  const has = (unit: string, system: string, planet: string) =>
    [...units].some((entry) => {
      const [type, sys, pl] = entry.split("|");
      return sys === system && pl === planet && type.toLowerCase().includes(unit);
    });
  const options: ChoiceOptionDto[] = [];
  for (const planet of ownPlanets(board, viewer.id)) {
    for (const unit of STRUCTURE_UNITS) {
      if (has(unit, planet.system, planet.planet)) continue;
      options.push({
        id: `${unit}|${planet.system}|${planet.planet}`,
        kind: "build",
        label: `place ${unit} on ${planet.planet}`,
        payload: { planet: planet.planet, system: planet.system, unit },
      });
    }
  }
  options.push({ id: "decline", kind: "decline", label: "decline" });
  return {
    choice: baseChoice(viewer.id, "site", action.key, "place a structure", options, "place_structure"),
    step: "site",
    exact: false,
    approximate:
      "Every planet you control is listed with a PDS or a space dock unless you already have that structure there. The engine also checks space dock rules, your supply and unit upgrades, so the real question may offer fewer sites.",
  };
}

/** The step's question exactly as the engine would ask it, under a client-made nonce. */
function exactStep(input: DryInput, exact: Extract<ExactResult, { kind: "question" }>): DryChoice {
  const { action, viewer, step } = input;
  const choice = pendingFromEngine(exact.choice, `${DRY_NONCE_PREFIX}${action.key}:${step}`);
  choice.actor = viewer.id;
  return { choice, step, approximate: null, exact: true, payment: exact.payment };
}

/** Whether `step` exists for the card at all (the exact question is only used where it does). */
function stepExists(family: StrategicAction["family"], step: DryStep): boolean {
  switch (step) {
    case "secondary":
      return family !== null;
    case "tech":
      return family === "technology";
    case "planet1":
    case "planet2":
      return family === "diplomacy";
    case "site":
      return family === "construction";
    case "produce":
    case "pay":
      return family === "warfare";
  }
}

/**
 * Builds the dry choice for one step; `null` when the card has no such step (or, for Warfare's
 * production, when the engine's exact question is not available: its build list cannot be
 * estimated). With the engine's exact question the real options are used as they are.
 */
export function buildDryChoice(input: DryInput): DryChoice | null {
  const family = input.action.family;
  if (input.exact?.kind === "question" && stepExists(family, input.step)) {
    return exactStep(input, input.exact);
  }
  return estimateDryChoice(input);
}

/** The client-side estimate of one step (the fallback when no exact answer is available). */
function estimateDryChoice(input: DryInput): DryChoice | null {
  const family = input.action.family;
  switch (input.step) {
    case "secondary":
      return family ? secondaryStep(input) : null;
    case "tech":
      return family === "technology" ? techStep(input) : null;
    case "planet1":
    case "planet2": {
      if (family !== "diplomacy") return null;
      const dry = planetStep(input);
      return dry.choice.options.length ? dry : null;
    }
    case "site":
      return family === "construction" ? siteStep(input) : null;
    case "produce":
    case "pay":
      // Warfare's build list (the home system's PRODUCTION) is only known to the engine.
      return null;
  }
}

/** The step that follows `step` for the card after a "follow", or `null` when the plan is complete. */
export function nextDryStep(
  family: StrategicAction["family"],
  step: DryStep,
  picked: { planets: number },
): DryStep | null {
  if (step === "secondary") {
    if (family === "technology") return "tech";
    if (family === "diplomacy") return "planet1";
    if (family === "construction") return "site";
    if (family === "warfare") return "produce";
    return null;
  }
  if (step === "planet1" && picked.planets < 2) return "planet2";
  if (step === "produce" && family === "warfare") return "pay";
  return null;
}
