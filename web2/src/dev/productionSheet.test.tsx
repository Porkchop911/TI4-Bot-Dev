import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { ProductionSheet } from "../game/steps/production/ProductionSheet";
import productionUpdate from "../session/fixtures/update-production.json";
import { EMPTY } from "../session/productionDraft";
import { selectBuild } from "../session/select/production";
import type { ProductionFacts, SessionUpdate } from "../session/wire";

// The sheet of a production over an update of the real engine (see `production.test.ts`). The
// test is here because `dev` is the layer that may import both the session and the components.
const update = productionUpdate as unknown as SessionUpdate;
const sheet = selectBuild(update, update.tactical as ProductionFacts, EMPTY, "build");
const units = sheet.groups.flatMap((group) => group.units);

const open = (name: string) => {
  fireEvent.click(screen.getByRole("button", { name: `${name}, printed values` }));
  return within(screen.getByRole("note"));
};

it("has a card behind the name of every unit, with its values", () => {
  render(<ProductionSheet view={sheet} />);
  expect(units.length).toBeGreaterThan(6);
  for (const unit of units) {
    const card = open(unit.name);
    expect(card.getByText(unit.name)).toBeInTheDocument();
    for (const word of ["Cost", "Combat", "Move", "Capacity"]) {
      expect(card.getByText(word)).toBeInTheDocument();
    }
    for (const ability of unit.abilities) {
      expect(card.getByText(ability)).toBeInTheDocument();
    }
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("note")).toBeNull();
  }
});

it("prints a war sun: the values, the keywords with their dice, and its special rule", () => {
  render(<ProductionSheet view={sheet} />);
  const card = open("War sun");
  const value = (word: string) => card.getByText(word).nextElementSibling;
  expect(value("Cost")).toHaveTextContent("12");
  expect(value("Combat")).toHaveTextContent("3+ (x3)");
  expect(value("Move")).toHaveTextContent("2");
  expect(value("Capacity")).toHaveTextContent("6");
  expect(card.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "SUSTAIN DAMAGE",
    "BOMBARDMENT 3+ (x3)",
  ]);
  expect(
    card.getByText("Other player's units in this system lose PLANETARY SHIELD."),
  ).toBeInTheDocument();
  // The card has no control: a keyword is not explained.
  expect(card.queryByRole("button")).toBeNull();
});

it("leaves out the section that a unit does not have", () => {
  render(<ProductionSheet view={sheet} />);
  const cruiser = open("Cruiser");
  expect(cruiser.queryByRole("list")).toBeNull();
  expect(cruiser.queryByRole("paragraph")).toBeNull();
});

it("opens the card of a unit that is not produced here", () => {
  render(<ProductionSheet view={sheet} />);
  const off = units.find((unit) => unit.unavailable)!;
  expect(open(off.name).getByText("Combat")).toBeInTheDocument();
});
