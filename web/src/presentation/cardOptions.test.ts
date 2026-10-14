import { describe, expect, it } from "vitest";
import {
  describeCardOption,
  handDecisionConfirmLabel,
  handDecisionNote,
  isHandCardDecision,
} from "./cardOptions.ts";

describe("describeCardOption", () => {
  it("shows a discarded action card's name and printed text", () => {
    const info = describeCardOption("discard_over_hand_limit", { id: "4", label: "Ancient Burial Sites" });
    expect(info?.title).toBe("Ancient Burial Sites");
    expect(info?.text).toContain("Exhaust each cultural planet");
    expect(info?.badge).toBe("Agenda phase");
  });

  it("replaces a secret objective alias with its name, condition and points", () => {
    const info = describeCardOption("return_over_secret_hand_limit", { id: "baf", label: "return baf" });
    expect(info?.title).toBe("Betray a Friend");
    expect(info?.text).toContain("Win a combat against a player");
    expect(info?.badge).toBe("1 VP · scored in the action phase");
  });

  it("falls back to the label or a readable alias when the catalog does not know the card", () => {
    expect(describeCardOption("discard_over_hand_limit", { id: "9", label: "Mystery Card" })?.title).toBe("Mystery Card");
    expect(describeCardOption("return_over_secret_hand_limit", { id: "zz_unknown", label: "return zz_unknown" })?.title).not.toContain("return");
  });

  it("ignores decisions that are not about cards in hand", () => {
    expect(describeCardOption("gain_command_token", { id: "tactic_tokens", label: "tactic pool" })).toBeNull();
    expect(isHandCardDecision("discard_over_hand_limit")).toBe(true);
    expect(isHandCardDecision("gain_command_token")).toBe(false);
  });
});

describe("take a revealed action card", () => {
  it("shows each shown card's name and printed text like a discard does", () => {
    const info = describeCardOption("take_revealed_action_card", { id: "4", label: "Ancient Burial Sites" });
    expect(info?.title).toBe("Ancient Burial Sites");
    expect(info?.text).toContain("Exhaust each cultural planet");
    expect(isHandCardDecision("take_revealed_action_card")).toBe(true);
    expect(handDecisionConfirmLabel("take_revealed_action_card")).toBe("Take card");
    expect(
      handDecisionNote({ prompt: "take 1 of x's action cards", options: [], context: { subtype: "take_revealed_action_card" } }),
    ).toContain("Another player has shown you these action cards");
  });
});

describe("handDecisionNote", () => {
  it("reads the hand size from the discard prompt", () => {
    const note = handDecisionNote({
      prompt: "over the hand limit — discard one of 8",
      options: [],
      context: { subtype: "discard_over_hand_limit" },
    });
    expect(note).toContain("You hold 8 action cards");
  });

  it("counts the secret objectives offered", () => {
    const note = handDecisionNote({
      prompt: "return a secret objective to the deck",
      options: [{ id: "a", label: "x" }, { id: "b", label: "y" }, { id: "c", label: "z" }],
      context: { subtype: "return_over_secret_hand_limit" },
    });
    expect(note).toContain("You hold 3 secret objectives");
    expect(handDecisionConfirmLabel("return_over_secret_hand_limit")).toBe("Return objective");
    expect(handDecisionConfirmLabel("discard_over_hand_limit")).toBe("Discard card");
  });
});

describe("promissory note picks (Diplomatic Pressure)", () => {
  it("shows the printed name, text and owner instead of the note id", () => {
    const info = describeCardOption("dp1_pick_promissory note", { id: "cf:Xxcha Kingdom", label: "cf:Xxcha Kingdom" });
    expect(info?.title).toBe("Ceasefire");
    expect(info?.text).toContain("the Xxcha Kingdom player activates a system");
    expect(info?.text).not.toContain("<color>");
    expect(info?.badge).toBe("Promissory note · Xxcha Kingdom");
  });
  it("names Support for the Throne from its support: id", () => {
    const info = describeCardOption("dp2_pick_promissory note", { id: "support:Hacan", label: "support:Hacan" });
    expect(info?.title).toBe("Support for the Throne");
  });
});
