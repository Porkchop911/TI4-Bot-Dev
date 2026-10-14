import { describe, expect, it } from "vitest";
import { describeTechnologyPickOption, isTechnologyPick, technologyPickStage } from "./technologyPick.ts";

const divert = {
  context: { subtype: "divert_funding_pick_technology" } as never,
  prompt: "Divert Funding: which technology to return",
};

describe("technology picks (Divert Funding, Plagiarize, ...)", () => {
  it("recognises a card asking which technology", () => {
    expect(isTechnologyPick(divert)).toBe(true);
    expect(isTechnologyPick({ context: { subtype: "spy_pick_player" } as never })).toBe(false);
  });

  it("shows the colour, the prerequisites and the printed text of a technology", () => {
    const view = describeTechnologyPickOption(divert, { id: "pa", kind: "technology" });
    expect(view?.name).toBe("Psychoarchaeology");
    expect(view?.track).toBe("Biotic");
    expect(view?.text[0]).toMatch(/technology specialties/);
  });

  it("has nothing for options that are not technologies", () => {
    expect(describeTechnologyPickOption(divert, { id: "done", kind: "decline" })).toBeNull();
    expect(
      describeTechnologyPickOption({ context: { subtype: "other" } as never }, { id: "pa", kind: "technology" }),
    ).toBeNull();
  });

  it("tells the two halves of Divert Funding apart by their prompt", () => {
    expect(technologyPickStage(divert)?.title).toBe("Return a technology");
    expect(
      technologyPickStage({ ...divert, prompt: "Divert Funding: what to research with the funding" })?.title,
    ).toBe("Research another technology");
    expect(technologyPickStage({ ...divert, prompt: "Plagiarize: which technology to steal" })).toBeNull();
  });
});
