import { expect, it } from "vitest";
import type { FlowActionView, MenuRowView } from "./action";
import { shortcuts } from "./shortcuts";

const row = (key: string, option: string): MenuRowView => ({
  key,
  title: option,
  disabled: false,
  intent: { type: "chooseOption", option },
});

const view = (interrupt: boolean): FlowActionView => ({
  kind: "flow",
  title: "",
  subtitle: "",
  badge: null,
  past: null,
  tabs: [],
  selected: 0,
  current: null,
  contentKey: "",
  heading: "",
  pill: null,
  trail: [],
  help: [],
  blocks: [{ kind: "menu", rows: [row("1", "below"), row("2", "below")] }],
  interrupt: interrupt
    ? {
        eyebrow: "Reaction",
        link: [],
        blocks: [{ kind: "menu", rows: [row("1", "window")] }],
        actions: [],
      }
    : null,
  closing: [],
  footer: {
    note: "",
    error: false,
    actions: [{ label: "Pass", key: "Enter", intent: { type: "reaction", action: "pass" } }],
  },
});

it("gives the keys to a reaction window while it is open", () => {
  expect(shortcuts(view(false)).get("1")).toEqual({ type: "chooseOption", option: "below" });
  const keys = shortcuts(view(true));
  expect(keys.get("1")).toEqual({ type: "chooseOption", option: "window" });
  // The action below has no keys while the window is open; the footer keeps Enter.
  expect(keys.has("2")).toBe(false);
  expect(keys.get("enter")).toEqual({ type: "reaction", action: "pass" });
});
