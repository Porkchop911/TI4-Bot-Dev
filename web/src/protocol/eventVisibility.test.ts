import { describe, it, expect } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { isPublicEvent, visibilityKind } from "./eventVisibility.ts";
import type { GameEvent } from "./types.ts";
import { actionToastFromEvent } from "../presentation/actionToasts.ts";

// Produced by (and round-trip checked in) the server's serde tests.
const serverEvents = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "../../../crates/ti4-server/fixtures/event_visibility.json"), "utf-8"),
) as GameEvent[];

describe("event visibility decoding", () => {
  it("decodes the server-serialised object form", () => {
    expect(serverEvents.map((e) => visibilityKind(e.visibility))).toEqual(["public", "seat", "referee"]);
    expect(serverEvents.map(isPublicEvent)).toEqual([true, false, false]);
  });

  it("accepts a plain string for back-compat and fails closed on junk", () => {
    expect(visibilityKind("public")).toBe("public");
    expect(visibilityKind("seat")).toBe("seat");
    expect(visibilityKind(undefined)).toBe("referee");
    expect(visibilityKind({})).toBe("referee");
    expect(visibilityKind(42)).toBe("referee");
  });

  it("only the public server event can raise an action toast", () => {
    const base = { ...serverEvents[0], actor: "p2", stage: "action selection", action_type: "tactical" };
    expect(actionToastFromEvent(base as GameEvent, "p1")).not.toBeNull();
    expect(actionToastFromEvent({ ...base, visibility: serverEvents[1].visibility } as GameEvent, "p1")).toBeNull();
    expect(actionToastFromEvent({ ...base, visibility: serverEvents[2].visibility } as GameEvent, "p1")).toBeNull();
  });
});
