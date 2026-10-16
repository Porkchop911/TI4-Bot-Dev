import {
  PendingChoiceDto,
  ChoiceOptionDto,
  OutstandingConstraintDto,
  DecisionTargetDto,
} from "../protocol/types.ts";
import { isReactionStepSubtype } from "./reactionModel.ts";

export type SelectionMode =
  | { mode: "single" }
  | { mode: "multi"; min: number; max: number }
  | { mode: "quantity"; target: number; paid: number; unit: "resources" | "influence" | "votes" }
  | { mode: "transaction"; partnerSeat: string }
  | { mode: "tactical_move"; activeSystem: string }
  | { mode: "tactical_cargo"; activeSystem: string }
  | { mode: "tactical_invasion"; activeSystem: string }
  | { mode: "production"; capacity: number; systemId: string }
  | { mode: "sustain"; hitsRemaining: number }
  | { mode: "casualty"; hitsToAssign: number };

export type ChoiceWorkflowKind =
  | "system_activation"
  | "planet_selection"
  | "ready_planets"
  | "tactical_movement"
  | "tactical_cargo"
  | "tactical_invasion"
  | "payment"
  | "production"
  | "combat_sustain"
  | "combat_casualty"
  | "combat_retreat"
  | "agenda_vote_outcome"
  | "agenda_vote_planets"
  | "transaction_propose"
  | "transaction_answer"
  | "action_card_reaction"
  | "objective_scoring"
  | "strategy_card_draft"
  | "technology_research"
  | "generic_selection";

export interface ChoiceRendererModel {
  readonly workflow: ChoiceWorkflowKind;
  readonly selectionMode: SelectionMode;
  readonly prompt: string;
  readonly actor: string;
  readonly nonce: string;
  readonly isOptional: boolean;
  readonly contextTarget: DecisionTargetDto | null;
  readonly options: readonly ChoiceOptionDto[];
  readonly outstanding: readonly OutstandingConstraintDto[];
  readonly declineOption: ChoiceOptionDto | null;
  readonly optionsByKind: ReadonlyMap<string, ChoiceOptionDto[]>;
  readonly optionsByTarget: ReadonlyMap<string, ChoiceOptionDto[]>;
}

export function getPaymentPayload(opt: ChoiceOptionDto): {
  worth: number;
  owed: number;
  kind: "resources" | "influence";
  source?: string;
  planetName?: string;
} {
  const p = opt.payload ?? {};
  return {
    worth: Number(p.worth ?? 0),
    owed: Number(p.owed ?? 0),
    kind: String(p.kind ?? "resources").toLowerCase() === "influence" ? "influence" : "resources",
    source: typeof p.source === "string" ? p.source : undefined,
    planetName: typeof p.planet_name === "string" ? p.planet_name : undefined,
  };
}

export function getMovementPayload(opt: ChoiceOptionDto): {
  origin?: string;
  unit?: string;
  damaged?: boolean;
  capacity?: number;
  gravity_drive?: boolean;
} {
  const p = opt.payload ?? {};
  return {
    origin: typeof p.origin === "string" ? p.origin : undefined,
    unit: typeof p.unit === "string" ? p.unit : undefined,
    damaged: Boolean(p.damaged),
    capacity: typeof p.capacity === "number" ? p.capacity : undefined,
    gravity_drive: Boolean(p.gravity_drive),
  };
}

export function getTradePayload(opt: ChoiceOptionDto): {
  net?: number;
  their_net?: number;
} {
  const p = opt.payload ?? {};
  return {
    net: typeof p.net === "number" ? p.net : undefined,
    their_net: typeof p.their_net === "number" ? p.their_net : undefined,
  };
}

export function getCombatPayload(opt: ChoiceOptionDto): {
  unit?: string;
  damaged?: boolean;
} {
  const p = opt.payload ?? {};
  return {
    unit: typeof p.unit === "string" ? p.unit : undefined,
    damaged: Boolean(p.damaged),
  };
}

export function getAgendaPlanetVotes(opt: ChoiceOptionDto): number | null {
  const p = opt.payload ?? {};
  if (typeof p.votes === "number") return p.votes;
  if (typeof p.influence === "number") return p.influence;
  if (typeof p.worth === "number") return p.worth;
  if (typeof p.amount === "number") return p.amount;

  // Structured fallback for engine label format "exhaust {planet} for {N} votes"
  const match = opt.label.match(/for (\d+) votes/i);
  if (match) {
    const parsed = parseInt(match[1], 10);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return null;
}

/** The planet an option is about, from the structured `payload.planet` only (never the option id). */
export function optionPlanetId(opt: ChoiceOptionDto): string | null {
  const planet = opt.payload?.planet;
  return typeof planet === "string" && planet.length > 0 ? planet : null;
}

export function isDeclineOption(opt: ChoiceOptionDto): boolean {
  return opt.id === "decline" || opt.kind === "decline";
}

/**
 * Subtypes that carry planet payloads but already have a dedicated workflow (payment, voting
 * baskets, production, invasion and ground combat). They must never become a planet selection.
 */
const PLANET_SELECTION_EXCLUDED_SUBTYPES = new Set([
  "pay_resources",
  "pay_influence",
  "leadership_spend_influence",
  "vote_exhaust_planet",
  "vote_tiebreak",
  "activate_system",
  "movement_step",
  "load_cargo",
  "commit_ground_forces",
  "produce_unit",
  "place_unit",
  "bombardment",
  "bombardment_target",
  "assign_ground_casualty",
  "fight_ground_combat_round",
  "start_next_ground_combat",
]);

/** Option kinds that belong to another workflow and therefore can't be an "extra" option. */
const NON_EXTRA_OPTION_KINDS = new Set([
  "activate",
  "move",
  "load",
  "land",
  "produce",
  "pay",
  "casualty",
  "sustain",
  "retreat",
  "offer",
  "research",
  "vote_planet",
  "score",
]);

/**
 * A non-planet option offered alongside planet picks (Bio-Stims technologies, The Acropolis
 * relics/leaders). It must not point somewhere on the board itself.
 */
export function isExtraPlanetSelectionOption(opt: ChoiceOptionDto): boolean {
  if (isDeclineOption(opt) || optionPlanetId(opt)) return false;
  if (opt.kind && NON_EXTRA_OPTION_KINDS.has(opt.kind)) return false;
  const p = opt.payload ?? {};
  return (
    p.system === undefined && p.to === undefined && p.origin === undefined && p.unit === undefined
  );
}

/**
 * A single-pick decision answered by choosing a planet on the map: at least one option carries
 * `payload.planet`, and every other non-decline option is either a planet pick too or a
 * board-less extra (technology, relic, …).
 */
export function isPlanetSelectionChoice(choice: PendingChoiceDto | null): boolean {
  if (!choice) return false;
  const subtype = choice.context?.subtype ?? "";
  if (PLANET_SELECTION_EXCLUDED_SUBTYPES.has(subtype)) return false;
  if (choice.context?.invasion_seq !== undefined && choice.context.invasion_seq !== null)
    return false;
  const offered = choice.options.filter((o) => !isDeclineOption(o));
  if (!offered.some((o) => optionPlanetId(o))) return false;
  // Movement payloads (a destination or an origin) mean the system is part of the answer.
  if (offered.some((o) => o.payload?.to !== undefined || o.payload?.origin !== undefined))
    return false;
  return offered.every((o) => optionPlanetId(o) !== null || isExtraPlanetSelectionOption(o));
}

/** Diplomacy's "ready which planet": every option readies one exhausted planet. */
export function isReadyPlanetChoice(choice: PendingChoiceDto | null | undefined): boolean {
  return Boolean(
    choice &&
      choice.context?.subtype === "ready_planet" &&
      choice.options.length > 0 &&
      choice.options.every((option) => option.kind === "ready"),
  );
}

export function deriveChoiceRendererModel(
  choice: PendingChoiceDto | null,
  viewerSeat: string | null,
): ChoiceRendererModel | null {
  if (!choice || !viewerSeat || choice.actor !== viewerSeat) {
    return null;
  }

  const subtype = choice.context?.subtype ?? "";
  const isOptional = Boolean(
    choice.context?.optional ||
    choice.options.some((o) => o.id === "decline" || o.kind === "decline"),
  );
  const declineOption =
    choice.options.find((o) => o.id === "decline" || o.kind === "decline") ?? null;
  const constraints = choice.context?.outstanding?.[0];

  // Grouping options by kind
  const optionsByKind = new Map<string, ChoiceOptionDto[]>();
  const optionsByTarget = new Map<string, ChoiceOptionDto[]>();

  for (const opt of choice.options) {
    const kindKey = opt.kind ?? "default";
    const kindList = optionsByKind.get(kindKey) || [];
    kindList.push(opt);
    optionsByKind.set(kindKey, kindList);

    if (opt.payload) {
      const targetSys = opt.payload.system ?? opt.payload.to ?? opt.payload.origin;
      if (typeof targetSys === "string" || typeof targetSys === "number") {
        const key = String(targetSys);
        const targetList = optionsByTarget.get(key) || [];
        targetList.push(opt);
        optionsByTarget.set(key, targetList);
      }
    }
  }

  // 1. Payment & Economy
  if (
    subtype === "pay_resources" ||
    subtype === "pay_influence" ||
    (subtype === "leadership_spend_influence" &&
      choice.options.some((o) => o.id === "trade_good" || o.id.startsWith("exhaust|"))) ||
    (optionsByKind.has("pay") &&
      choice.options.some((o) => o.id === "trade_good" || o.id.startsWith("exhaust|")))
  ) {
    const owed = constraints?.amount ?? 0;
    const paid = constraints?.paid ?? 0;
    const kind =
      subtype === "pay_influence" || constraints?.kind?.toLowerCase() === "influence"
        ? "influence"
        : "resources";
    return {
      workflow: "payment",
      selectionMode: { mode: "quantity", target: owed, paid, unit: kind },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 2. System Activation
  if (
    subtype === "activate_system" ||
    (choice.options.length > 0 && choice.options.every((o) => o.kind === "activate"))
  ) {
    return {
      workflow: "system_activation",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: false,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption: null,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 3. Tactical Movement, Cargo, and Ground Commitment
  if (
    subtype === "movement_step" ||
    (choice.options.some((o) => o.kind === "move") &&
      choice.prompt.toLowerCase().includes("movement"))
  ) {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_movement",
      selectionMode: { mode: "tactical_move", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  if (subtype === "load_cargo") {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_cargo",
      selectionMode: { mode: "tactical_cargo", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  if (subtype === "commit_ground_forces") {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_invasion",
      selectionMode: { mode: "tactical_invasion", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // Production Builder
  if (subtype === "produce_unit" || subtype === "place_unit") {
    const systemId =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    const capacity = constraints?.amount ?? 0;
    return {
      workflow: "production",
      selectionMode: { mode: "production", capacity, systemId },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: Boolean(declineOption),
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // Objective scoring uses the generic selection UI but remains distinct for
  // workflow-level presentation and future scoring-specific enhancements.
  if (
    subtype === "score_objective" ||
    subtype === "score_secret_objective" ||
    subtype === "imperial_score_objective"
  ) {
    return {
      workflow: "objective_scoring",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  if (subtype === "draft_strategy_card") {
    return {
      workflow: "strategy_card_draft",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // Technology Research
  if (
    subtype === "research_technology" ||
    // An engine-built offer card (Deepwrought's "reduce by 1") is not a technology pick.
    (choice.details?.kind !== "offer" &&
      choice.options.length > 0 &&
      choice.options.some((o) => o.kind === "research"))
  ) {
    const isPrimary =
      choice.context?.source &&
      typeof choice.context.source === "object" &&
      "StrategyCard" in choice.context.source &&
      (choice.context.source as { StrategyCard: { card: string; secondary: boolean } }).StrategyCard
        ?.card === "Technology" &&
      !(choice.context.source as { StrategyCard: { card: string; secondary: boolean } })
        .StrategyCard?.secondary;

    const min = constraints?.min_selection ?? (isOptional ? 0 : 1);
    const max = constraints?.max_selection ?? (isPrimary ? 2 : 1);

    return {
      workflow: "technology_research",
      selectionMode: max > 1 ? { mode: "multi", min, max } : { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 4. Combat Phasing: Sustain vs Casualties vs Retreat
  if (subtype === "sustain_damage") {
    return {
      workflow: "combat_sustain",
      selectionMode: { mode: "sustain", hitsRemaining: constraints?.amount ?? 0 },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  if (subtype === "assign_casualty" || optionsByKind.has("casualty")) {
    const hits = constraints?.amount ?? 0;
    return {
      workflow: "combat_casualty",
      selectionMode: { mode: "casualty", hitsToAssign: hits },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: false,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption: null,
      optionsByKind,
      optionsByTarget,
    };
  }

  if (subtype === "announce_retreat" || subtype === "retreat_to" || optionsByKind.has("retreat")) {
    return {
      workflow: "combat_retreat",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 5. Bilateral Transactions
  if (
    subtype === "propose_transaction" ||
    subtype === "answer_transaction" ||
    optionsByKind.has("offer")
  ) {
    const partnerSeat =
      choice.context?.target && "Player" in choice.context.target
        ? choice.context.target.Player
        : "";
    return {
      workflow: subtype === "answer_transaction" ? "transaction_answer" : "transaction_propose",
      selectionMode: { mode: "transaction", partnerSeat },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // Diplomacy readies its planets together: one list and one Confirm for the whole question.
  if (isReadyPlanetChoice(choice)) {
    const required = Math.min(2, choice.options.length);
    return {
      workflow: "ready_planets",
      selectionMode: { mode: "multi", min: required, max: required },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: false,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption: null,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 6. Planet selection on the map (action cards, abilities, relics, Elect Planet votes, …)
  if (isPlanetSelectionChoice(choice)) {
    return {
      workflow: "planet_selection",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: Boolean(declineOption) || Boolean(choice.context?.optional),
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 7. Agenda Voting
  if (subtype === "cast_vote" || subtype === "vote_exhaust_planet" || subtype === "vote_tiebreak") {
    return {
      workflow: subtype === "vote_exhaust_planet" ? "agenda_vote_planets" : "agenda_vote_outcome",
      selectionMode:
        subtype === "vote_exhaust_planet"
          ? { mode: "quantity", target: 0, paid: 0, unit: "votes" }
          : { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 8. Reaction Windows
  if (
    isReactionStepSubtype(subtype) ||
    (isOptional && choice.context?.source && "Reaction" in choice.context.source)
  ) {
    return {
      workflow: "action_card_reaction",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget,
    };
  }

  // 9. Bounded Multi-Selection Fallback
  const min = constraints?.min_selection ?? 1;
  const max =
    constraints?.max_selection ?? (constraints?.min_selection ? constraints.min_selection : 1);
  return {
    workflow: "generic_selection",
    selectionMode: max > 1 ? { mode: "multi", min, max } : { mode: "single" },
    prompt: choice.prompt,
    actor: choice.actor,
    nonce: choice.nonce,
    isOptional,
    contextTarget: choice.context?.target ?? null,
    options: choice.options,
    outstanding: choice.context?.outstanding ?? [],
    declineOption,
    optionsByKind,
    optionsByTarget,
  };
}

/**
 * Classifies a pending choice into a workflow kind.
 * Delegates to `deriveChoiceRendererModel` to avoid duplicating the
 * classification logic — any new workflow registered in that function is
 * automatically reflected here.
 *
 * Passing `choice.actor` as the viewer means spectators should call
 * `deriveChoiceRendererModel` directly when they need a null result.
 */
export function classifyChoiceWorkflow(choice: PendingChoiceDto | null): ChoiceWorkflowKind {
  if (!choice) return "generic_selection";
  return deriveChoiceRendererModel(choice, choice.actor)?.workflow ?? "generic_selection";
}
