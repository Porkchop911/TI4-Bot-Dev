import { describe, expect, it } from "vitest";
import { relicEffectSummary, relicGainText, relicToasts, readRelicLog } from "./relicToasts.ts";

describe("relic gain toasts", () => {
  it("an own purge of Nano-Forge says it gives no victory point", () => {
    const text = relicGainText({ player: "p1", relic: "nanoforge" }, true);
    const lines = text.split("\n");
    expect(lines[0]).toBe("gained Nano-Forge");
    expect(lines[1]).toMatch(/^Attach this card to a non-legendary, non-home planet you control/);
    expect(lines[1]).not.toContain("ACTION");
    expect(lines.at(-1)).toBe("no victory point on gain");
  });

  it("the Shard of the Throne names its point", () => {
    const text = relicGainText({ player: "p1", relic: "shard", victory_points: 1 }, true);
    expect(text.split("\n")[0]).toBe("gained Shard of the Throne");
    expect(text).toContain("+1 victory point on gain");
    expect(text).not.toContain("no victory point");
  });

  it("another player's gain has no point note unless it scored", () => {
    expect(relicGainText({ player: "p2", relic: "nanoforge" }, false)).not.toContain("victory point");
    expect(relicGainText({ player: "p2", relic: "shard", victory_points: 1 }, false)).toContain("+1 victory point");
  });

  it("an unknown relic falls back to a readable name", () => {
    expect(relicGainText({ player: "p2", relic: "lost_idol" }, false)).toBe("gained Lost Idol");
  });

  it("summarises the first sentence only", () => {
    expect(relicEffectSummary("Your units do not roll.\n\nACTION: Exhaust this card. Then more.")).toBe("Your units do not roll.");
    expect(relicEffectSummary("> ACTION: Purge this card; gain 1 point. Later.")).toBe("Purge this card; gain 1 point.");
    expect(relicEffectSummary(undefined)).toBe("");
  });

  it("toasts only records past the cursor, flagging the viewer's own", () => {
    const log = [
      { round: 2, player: "p1", relic: "codex" },
      { round: 3, player: "p2", relic: "shard", victory_points: 1 },
    ];
    const first = relicToasts(log, 1, "p1");
    expect(first.toasts.map((t) => [t.id, t.own, t.actor])).toEqual([["relic:1:3:p2:shard", false, "p2"]]);
    expect(first.next).toBe(2);
    expect(relicToasts(log, first.next, "p1").toasts).toEqual([]);
    expect(relicToasts(log, 0, null).toasts.every((t) => !t.own)).toBe(true);
  });

  it("reads the log defensively", () => {
    expect(readRelicLog(null)).toEqual([]);
    expect(readRelicLog({ relic_log: [{ player: "p1", relic: "codex" }, 3, {}] })).toHaveLength(1);
  });
});
