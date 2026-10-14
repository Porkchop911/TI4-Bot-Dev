import { describe, expect, it } from "vitest";
import { decodeReactionModes, decodeServerMessage } from "./decode.ts";
import { PROTOCOL_VERSION, StateUpdateMsg, InitialSnapshotMsg } from "./types.ts";

const base = {
  protocol_version: PROTOCOL_VERSION,
  game_id: "g1",
  game_version: 3,
  viewer: { role: "player", seat: "p1" },
  view: {},
  state: {},
  galaxy_layout: { version: 1, active_sources: [], placements: [] },
  turn_status: { status: "waiting" },
};

describe("reaction_modes on snapshots and updates", () => {
  it("keeps known modes on both message kinds", () => {
    const update = decodeServerMessage(
      { ...base, type: "state_update", reaction_modes: { Sabotage: "never" } },
      "g1",
    ) as StateUpdateMsg;
    expect(update.reaction_modes).toEqual({ Sabotage: "never" });
    const snapshot = decodeServerMessage(
      { ...base, type: "initial_snapshot", reaction_modes: { Sabotage: "never", Hack: "always" } },
      "g1",
    ) as InitialSnapshotMsg;
    expect(snapshot.reaction_modes).toEqual({ Sabotage: "never", Hack: "always" });
  });

  it("loads messages without modes, as every older server sends them", () => {
    const update = decodeServerMessage({ ...base, type: "state_update" }, "g1") as StateUpdateMsg;
    expect(update.reaction_modes).toBeUndefined();
  });

  it("drops unknown values and malformed shapes instead of failing", () => {
    expect(decodeReactionModes({ A: "never", B: "maybe", C: 3 })).toEqual({ A: "never" });
    expect(decodeReactionModes("nope")).toEqual({});
    const update = decodeServerMessage(
      { ...base, type: "state_update", reaction_modes: ["never"] },
      "g1",
    ) as StateUpdateMsg;
    expect(update.reaction_modes).toBeUndefined();
  });
});
