import { describe, expect, it } from "vitest";
import { describeLegendaryOption, legendaryWindow } from "./legendaryMenu.ts";

const endOfTurn = { context: { subtype: "legendary_end_of_turn" } as never };
const option = (id: string, label: string) => ({ id, kind: "legendary", label });

describe("legendary menu", () => {
  it("names the window the abilities are offered in", () => {
    expect(legendaryWindow(endOfTurn)).toBe("at the end of your turn");
    expect(legendaryWindow({ context: { subtype: "legendary_pass" } as never })).toBe("as you pass");
    expect(legendaryWindow({ context: { subtype: "pick_player" } as never })).toBeNull();
  });

  it("shows the planet, its stats and the printed ability text for an option", () => {
    const view = describeLegendaryOption(endOfTurn, option("primor", "The Atrament"));
    expect(view).toMatchObject({ ability: "The Atrament", planet: "Primor", stats: "2R/1I" });
    expect(view?.text).toMatch(/exhaust/i);
  });

  it("only describes legendary options of a legendary window", () => {
    expect(describeLegendaryOption(endOfTurn, { id: "decline", kind: "decline", label: "decline" })).toBeNull();
    expect(
      describeLegendaryOption({ context: { subtype: "other" } as never }, option("primor", "x")),
    ).toBeNull();
  });
});
