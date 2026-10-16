import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { decodeMapChoice, decodeServerMessage, isStaleServerMessage } from "./decode.ts";
import { PROTOCOL_VERSION, type SessionUpdate } from "./index.ts";

// The protocol fixtures of both apps, written by the Rust tests (`cargo wire-web`).
const dir = fileURLToPath(new URL("../../../fixtures/protocol/", import.meta.url));
const fixture = (name: string): unknown => JSON.parse(readFileSync(`${dir}${name}`, "utf8"));
const names = readdirSync(dir).filter((name) => name.endsWith(".json"));
const messages = names.filter(
  (name) => !name.startsWith("update-") && name !== "event_visibility.json",
);

describe("the protocol fixtures", () => {
  it.each(messages)("decodes %s", (name) => {
    const message = decodeServerMessage(fixture(name), "game_12345");
    expect(message.protocol_version).toBe(PROTOCOL_VERSION);
  });

  it.each(names.filter((name) => name.startsWith("update-")))("reads %s as an update", (name) => {
    const update = fixture(name) as SessionUpdate;
    expect(update.viewer).toEqual({ role: "player", seat: "a" });
    expect(update.pending_choice?.choice.options.length).toBeGreaterThan(0);
  });
});

describe("decodeServerMessage", () => {
  const snapshot = fixture("actor_snapshot.json") as Record<string, unknown>;

  it("refuses another version, another game and a message of no type", () => {
    expect(() => decodeServerMessage({ ...snapshot, protocol_version: 2 }, "game_12345")).toThrow(
      /unsupported protocol version/,
    );
    expect(() => decodeServerMessage(snapshot, "other")).toThrow(/unexpected game id/);
    expect(() => decodeServerMessage({ protocol_version: PROTOCOL_VERSION }, "g")).toThrow(
      /missing message type/,
    );
  });

  it("checks the messages of drafts and staged work", () => {
    const base = { protocol_version: PROTOCOL_VERSION, game_id: "g" };
    expect(
      decodeServerMessage({ ...base, type: "seat_staged", revision: 2, slots: {} }, "g").type,
    ).toBe("seat_staged");
    expect(() => decodeServerMessage({ ...base, type: "seat_staged", slots: {} }, "g")).toThrow(
      /invalid staged work/,
    );
    expect(decodeServerMessage({ ...base, type: "draft_applied", nonce: "n" }, "g").type).toBe(
      "draft_applied",
    );
    expect(() =>
      decodeServerMessage({ ...base, type: "draft_outcome", request_id: "r" }, "g"),
    ).toThrow(/invalid draft outcome/);
  });

  it("keeps every reaction mode of the protocol and drops what is not one", () => {
    const update = decodeServerMessage(
      {
        ...snapshot,
        type: "state_update",
        reaction_modes: { Sabotage: "never", Scuttle: "ownturn", Rally: "maybe" },
      },
      "game_12345",
    );
    expect(update.type === "state_update" && update.reaction_modes).toEqual({
      Sabotage: "never",
      Scuttle: "ownturn",
    });
  });

  it("drops an update older than the one shown", () => {
    const update = decodeServerMessage(
      { ...snapshot, type: "state_update", game_version: 41 },
      "game_12345",
    );
    expect(isStaleServerMessage(update, 42)).toBe(true);
    expect(isStaleServerMessage(update, 41)).toBe(false);
  });
});

describe("decodeMapChoice", () => {
  it("needs an alias for a template and none for the random map", () => {
    const view = { systems: 30, hyperlanes: false, recommended: true };
    expect(decodeMapChoice({ ...view, kind: "random" }).kind).toBe("random");
    expect(() => decodeMapChoice({ ...view, kind: "template" })).toThrow(/map choice alias/);
  });
});
