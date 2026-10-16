import { describe, it, expect } from "vitest";
import { MAX_TOAST_HISTORY, recordToasts } from "./toastHistory.ts";

describe("recordToasts", () => {
  it("keeps every kind of toast with the wording it showed", () => {
    const list = recordToasts(
      [],
      [
        { id: "a", kind: "auto-resolve", decisionType: "pay 1 more resources", selectedValue: "trade goods" },
        { id: "b", kind: "action", decisionType: "", selectedValue: "", actor: "p2", text: "played Sabotage" },
        { id: "c", kind: "invasion", decisionType: "", selectedValue: "", text: "Mecatol fell", tag: "Invasion" },
        { id: "d", kind: "recap", decisionType: "", selectedValue: "", actor: "p3", text: "passed" },
      ],
    );
    expect(list.map((h) => h.text)).toEqual([
      "Only one way left to pay: trade goods",
      "played Sabotage",
      "Mecatol fell",
      "passed",
    ]);
    expect(list[1]).toMatchObject({ actor: "p2", kind: "action" });
    expect(list[2].tag).toBe("Invasion");
    expect(list[3].tag).toBe("Turn");
  });

  it("updates a grown toast in place and ignores an unchanged one", () => {
    const first = recordToasts([], [{ id: "b", kind: "action", decisionType: "", selectedValue: "", actor: "p2", text: "played A" }]);
    expect(recordToasts(first, [{ id: "b", kind: "action", decisionType: "", selectedValue: "", actor: "p2", text: "played A" }])).toBe(first);
    const grown = recordToasts(first, [{ id: "b", kind: "action", decisionType: "", selectedValue: "", actor: "p2", text: "played A; played B" }]);
    expect(grown).toHaveLength(1);
    expect(grown[0].text).toBe("played A; played B");
  });

  it("keeps only the newest entries", () => {
    const many = Array.from({ length: MAX_TOAST_HISTORY + 5 }, (_, i) => ({
      id: `n${i}`,
      kind: "notice" as const,
      decisionType: "",
      selectedValue: "",
      text: `t${i}`,
    }));
    const list = recordToasts([], many);
    expect(list).toHaveLength(MAX_TOAST_HISTORY);
    expect(list[list.length - 1].id).toBe(`n${MAX_TOAST_HISTORY + 4}`);
  });
});
