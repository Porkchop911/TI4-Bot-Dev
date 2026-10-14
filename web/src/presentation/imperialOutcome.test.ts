import { describe, expect, it } from "vitest";
import { describeImperialOutcome } from "./imperialOutcome.ts";

const choice = (details: Record<string, unknown> | undefined, subtype = "imperial_score_objective") =>
  ({ context: { subtype }, details }) as never;

describe("describeImperialOutcome", () => {
  it("says +1 VP when the player holds Mecatol Rex", () => {
    const view = describeImperialOutcome(
      choice({ kind: "imperial", controls_mecatol: true, secrets_held: 1, secrets_max: 3 }),
    );
    expect(view?.headline).toBe("+1 VP (you hold Mecatol Rex)");
    expect(view?.summary).toBe("+1 VP for Mecatol Rex");
  });

  it("says draw a secret, with the hand, when the player does not", () => {
    const view = describeImperialOutcome(
      choice({ kind: "imperial", controls_mecatol: false, secrets_held: 2, secrets_max: 3 }),
    );
    expect(view?.headline).toBe("Draw a secret objective (2/3 held)");
    expect(view?.controlsMecatol).toBe(false);
  });

  it("leaves out the hand when the engine did not send it", () => {
    expect(describeImperialOutcome(choice({ kind: "imperial", controls_mecatol: false }))?.headline).toBe(
      "Draw a secret objective",
    );
  });

  it("is null for other decisions or without details", () => {
    expect(describeImperialOutcome(choice(undefined))).toBeNull();
    expect(describeImperialOutcome(choice({ kind: "imperial", controls_mecatol: true }, "score_objective"))).toBeNull();
  });
});
