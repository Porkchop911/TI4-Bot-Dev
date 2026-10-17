import { describe, expect, it } from "vitest";
import { describeAgendaPeek } from "./agendaPeek.ts";
import type { PlayerView } from "../protocol/types.ts";

const card = (id: string, name: string) => ({ id, name, type: "Law", target: "For/Against", text1: "For: x.", text2: "Against: y." });
const players = [
  { id: "me", agenda_peek: [card("a", "Mutiny"), card("b", "Sanctions")] },
  { id: "rival", agenda_peek: [card("c", "Secret")] },
  { id: "idle" },
] as unknown as PlayerView[];

describe("Insider Information's peek", () => {
  it("lists the viewer's own cards in deck order with their printed text", () => {
    const peek = describeAgendaPeek(players, "me")!;
    expect(peek.entries.map((entry) => [entry.place, entry.name])).toEqual([[1, "Mutiny"], [2, "Sanctions"]]);
    expect(peek.entries[0].tags).toEqual(["Law", "For/Against"]);
    expect(peek.entries[0].text).toEqual(["For: x.", "Against: y."]);
    expect(peek.key).toBe("a,b");
  });

  it("shows nothing to a seat that did not look, to spectators or without a viewer", () => {
    expect(describeAgendaPeek(players, "idle")).toBeNull();
    expect(describeAgendaPeek(players, "nobody")).toBeNull();
    expect(describeAgendaPeek(players, null)).toBeNull();
  });
});
