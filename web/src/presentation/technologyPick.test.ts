import { describe, expect, it } from "vitest";
import {
  describeTechnologyPickOption,
  isTechnologyPick,
  splitTechnologyOptionId,
  technologyPickStage,
} from "./technologyPick.ts";

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
    expect(technologyPickStage({ ...divert, prompt: "Plagiarize: which technology to steal" })?.title).toBe(
      "Steal a technology",
    );
  });

  it("reads Plagiarize's seat|technology ids as the technology and its owner", () => {
    const plagiarize = {
      context: { subtype: "plagiarize_pick_technology" } as never,
      prompt: "Plagiarize: which technology to steal",
    };
    const view = describeTechnologyPickOption(plagiarize, { id: "player_ab12|det", kind: "technology" });
    expect(view?.name).toBe("Dark Energy Tap");
    expect(view?.owner).toBe("player_ab12");
    expect(view?.track).toBe("Propulsion");
    expect(splitTechnologyOptionId("player_ab12|md_base")).toEqual({ owner: "player_ab12", technology: "md_base" });
    expect(splitTechnologyOptionId("pa")).toEqual({ owner: null, technology: "pa" });
  });
});

describe("starting technology", () => {
  const start = (details: Record<string, unknown>, options = 8) =>
    ({
      context: { subtype: "starting_technology" } as never,
      prompt: "choose a starting technology",
      details,
      options: Array.from({ length: options }, (_, i) => ({ id: `t${i}`, kind: "starting_technology", label: "" })) as never,
    }) as const;

  it("is a technology pick that reads the engine's options", () => {
    const winnu = start({ pick: 1, of: 1, rule: "listed", held: [] });
    expect(isTechnologyPick(winnu)).toBe(true);
    const view = describeTechnologyPickOption(winnu, { id: "det", kind: "starting_technology" });
    expect(view?.name).toBe("Dark Energy Tap");
    expect(view?.prereqs).toEqual([]);
    const keleres = describeTechnologyPickOption(
      winnu,
      { id: "st", kind: "starting_technology", payload: { owners: ["a", "b"] } },
    );
    expect(keleres?.holders).toEqual(["a", "b"]);
  });

  it("says what a Winnu, an Argent and a Keleres pick is", () => {
    expect(technologyPickStage(start({ pick: 1, of: 1, rule: "listed", held: [] }))?.text).toMatch(/Choose one technology/);
    const argent = technologyPickStage(start({ pick: 2, of: 2, rule: "listed", held: ["st"] }, 2));
    expect(argent?.title).toBe("Starting technology · 2 of 2");
    expect(argent?.text).toMatch(/stays in the technology deck/);
    expect(argent?.text).toMatch(/Sarween Tools/);
    expect(technologyPickStage(start({ pick: 1, of: 2, rule: "owned_by_others", held: [] }))?.text).toMatch(
      /another player starts with/,
    );
  });
});
