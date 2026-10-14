import { describe, expect, it } from "vitest";
import { decodeAutoResolved, decodeServerMessage } from "./decode.ts";
import { PROTOCOL_VERSION, StateUpdateMsg } from "./types.ts";

const update = {
  type: "state_update",
  protocol_version: PROTOCOL_VERSION,
  game_id: "g1",
  game_version: 3,
  viewer: { role: "player", seat: "p1" },
  view: {},
  state: {},
  galaxy_layout: { version: 1, active_sources: [], placements: [] },
  turn_status: { status: "waiting" },
};

describe("auto_resolved on a state update", () => {
  it("keeps well-formed notes and their count", () => {
    const msg = decodeServerMessage(
      {
        ...update,
        auto_resolved: [
          { id: "auto-1", prompt: "pay 1 more resources", selected: "trade goods", reason: "only way", count: 2 },
        ],
      },
      "g1",
    ) as StateUpdateMsg;
    expect(msg.auto_resolved).toEqual([
      { id: "auto-1", prompt: "pay 1 more resources", selected: "trade goods", reason: "only way", count: 2 },
    ]);
  });

  it("loads old messages and fixtures that carry no notes", () => {
    const msg = decodeServerMessage(update, "g1") as StateUpdateMsg;
    expect(msg.auto_resolved).toBeUndefined();
  });

  it("drops malformed notes instead of failing the update", () => {
    const msg = decodeServerMessage(
      { ...update, auto_resolved: [{ id: 5 }, "x", { id: "ok", prompt: "p", selected: "s" }] },
      "g1",
    ) as StateUpdateMsg;
    expect(msg.auto_resolved).toEqual([{ id: "ok", prompt: "p", selected: "s", reason: "" }]);
    const none = decodeServerMessage({ ...update, auto_resolved: "nope" }, "g1") as StateUpdateMsg;
    expect(none.auto_resolved).toBeUndefined();
  });

  it("ignores counts that are not whole numbers above one", () => {
    expect(decodeAutoResolved([{ id: "a", prompt: "p", selected: "s", reason: "r", count: 1.5 }])[0].count).toBeUndefined();
  });
});
