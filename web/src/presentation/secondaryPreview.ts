import type {
  EngineChoice,
  PendingChoiceDto,
  PreviewPayment,
  SecondaryBlocker,
} from "../protocol/types.ts";
import type { SecondaryPreviewReply } from "../protocol/client.ts";
import { decodeDecisionTrigger } from "../protocol/decode.ts";
import { planetName } from "./secondaryPlan.ts";
import type { TokenStep } from "./commandTokens.ts";

/**
 * The engine's exact answer to "what would my secondary ask if its window opened now", reduced to
 * what the preparation panel needs. Produced by the server's read-only `preview_secondary`
 * (`ti4_engine::secondary_preview`); everything here is "as of now" and is re-checked against the
 * real question when it opens (`resolveStep` stays authoritative).
 */
export type ExactResult =
  /** The question the engine would ask, ready to render with the real components. */
  | { kind: "question"; choice: EngineChoice; payment?: PreviewPayment; skipped: string[] }
  /** The window would not open for this seat right now. */
  | { kind: "not_asked"; blocker: SecondaryBlocker }
  /** The answers given end the secondary: nothing further is asked (`unused`: answers it never asked for). */
  | { kind: "complete"; unused: number }
  /** A scripted answer is not offered by the question the engine asks at that point. */
  | { kind: "rejected"; at: number; answer: string; choice: EngineChoice }
  /** No exact answer (old server, refusal, timeout, unexpected question): use the estimate. */
  | { kind: "none"; why: string };

export function toExactResult(reply: SecondaryPreviewReply): ExactResult {
  if (reply.kind === "refused") return { kind: "none", why: `refused: ${reply.reason}` };
  const body = reply.body;
  switch (body.status) {
    case "question":
      return {
        kind: "question",
        choice: body.choice,
        payment: body.payment,
        skipped: (body.skipped ?? []).map((prompt) => prompt.prompt),
      };
    case "would_not_be_asked":
      return { kind: "not_asked", blocker: body.blocker };
    case "complete":
      return { kind: "complete", unused: body.unused_answers ?? 0 };
    case "rejected":
      return { kind: "rejected", at: body.at, answer: body.answer, choice: body.choice };
    case "unavailable":
      return { kind: "none", why: body.detail };
  }
}

/** The engine's choice as the shell's pending decision, under a client-made (dry) nonce. */
export function pendingFromEngine(choice: EngineChoice, nonce: string): PendingChoiceDto {
  const context = choice.context;
  return {
    nonce,
    actor: choice.player,
    prompt: choice.prompt,
    options: choice.options,
    context:
      context && "trigger" in context
        ? { ...context, trigger: decodeDecisionTrigger(context.trigger) }
        : context,
    ...(choice.details ? { details: choice.details } : {}),
  };
}

const BLOCKERS: Record<SecondaryBlocker, string> = {
  no_strategy_token: "you have no strategy token left to spend",
  cannot_afford_influence: "you cannot afford a command token with your influence",
  cannot_pay_resources: "you cannot pay the 4 resources right now",
  nothing_to_research: "you have nothing to research right now",
  no_exhausted_planet: "none of your planets is exhausted right now",
  commodities_full: "your commodities are already full",
  no_production: "your home system has no production right now",
  other: "it is not available to you right now",
};

/** One sentence for the preparation banner when the engine says the window would not open. */
export const describeBlocker = (blocker: SecondaryBlocker): string =>
  `As of now you would not be asked this secondary: ${BLOCKERS[blocker]}. Your plan is kept and applies if that changes before your turn.`;

/** "Jord (2 resources), 1 trade good" for the plan the engine would pay a research with. */
export function describePayment(payment: PreviewPayment): string {
  const parts = [
    ...payment.planets.map((planet) => planetName(planet)),
    ...(payment.trade_goods
      ? [`${payment.trade_goods} trade good${payment.trade_goods === 1 ? "" : "s"}`]
      : []),
  ];
  return parts.join(", ");
}

/**
 * The answers that script the engine's preview of a whole Leadership purchase: the window's own
 * answer, then per token its payments (one planet or trade good each) and its pool, with the
 * "again?" question answered yes between tokens. The closing "no" is not scripted: the preview
 * stops at that question.
 */
export function leadershipScript(follow: string, steps: readonly TokenStep[]): string[] {
  const answers = [follow];
  let first = true;
  for (const step of steps) {
    if (step.kind === "purchase") {
      // The first "yes" is the window's own answer.
      if (step.buy && !first) answers.push("yes");
      if (step.buy) first = false;
    } else if (step.kind === "exhaust") answers.push(`exhaust|${step.planet}`);
    else if (step.kind === "trade_good") answers.push("trade_good");
    else answers.push(step.pool);
  }
  return answers;
}

/** What the engine says about a scripted Leadership purchase: fine, not fine (with why), or unchecked. */
export type LeadershipCheck =
  | { kind: "accepted" }
  | { kind: "rejected"; reason: string }
  | { kind: "unchecked"; why: string };

export function checkLeadershipReply(result: ExactResult): LeadershipCheck {
  switch (result.kind) {
    case "question":
      return { kind: "accepted" };
    case "complete":
      return result.unused === 0
        ? { kind: "accepted" }
        : {
            kind: "rejected",
            reason:
              "the game would stop asking before the end of this purchase: you cannot afford that many tokens now",
          };
    case "rejected": {
      if (result.choice.context?.subtype === "pay_influence") {
        const what = result.answer.startsWith("exhaust|")
          ? `${planetName(result.answer.slice("exhaust|".length))} cannot be exhausted at this point of the payment`
          : result.answer === "trade_good"
            ? "a trade good cannot be spent at this point of the payment"
            : "that payment is not offered";
        return { kind: "rejected", reason: `the game does not offer it: ${what}` };
      }
      return { kind: "rejected", reason: `the game does not offer "${result.answer}" there` };
    }
    case "not_asked":
      return { kind: "unchecked", why: "the game would not ask you this secondary as of now" };
    case "none":
      return { kind: "unchecked", why: result.why };
  }
}
