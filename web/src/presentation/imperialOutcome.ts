import type { PendingChoiceDto } from "../protocol/types.ts";

export interface ImperialOutcome {
  controlsMecatol: boolean;
  /** Secret objectives held now; `null` when the engine did not say. */
  secretsHeld: number | null;
  secretsMax: number | null;
  /** What happens after the scoring window, whichever way it is answered. */
  headline: string;
  /** Short form for the confirm summary, e.g. "+1 VP for Mecatol Rex". */
  summary: string;
}

/**
 * Imperial's other half: you gain 1 VP if you control Mecatol Rex, otherwise you draw a secret
 * objective. Exactly one of the two, and it happens whether or not a public objective is scored.
 * `null` for any other decision.
 */
export function describeImperialOutcome(
  choice: Pick<PendingChoiceDto, "context" | "details">,
): ImperialOutcome | null {
  if (choice.context?.subtype !== "imperial_score_objective") return null;
  const details = choice.details;
  if (!details || details.kind !== "imperial" || typeof details.controls_mecatol !== "boolean") {
    return null;
  }
  const num = (value: unknown) => (typeof value === "number" ? value : null);
  const secretsHeld = num(details.secrets_held);
  const secretsMax = num(details.secrets_max);
  if (details.controls_mecatol) {
    return {
      controlsMecatol: true,
      secretsHeld,
      secretsMax,
      headline: "+1 VP (you hold Mecatol Rex)",
      summary: "+1 VP for Mecatol Rex",
    };
  }
  const held = secretsHeld !== null && secretsMax !== null ? ` (${secretsHeld}/${secretsMax} held)` : "";
  return {
    controlsMecatol: false,
    secretsHeld,
    secretsMax,
    headline: `Draw a secret objective${held}`,
    summary: "draw a secret objective",
  };
}
