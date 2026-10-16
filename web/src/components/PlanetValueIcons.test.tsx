import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import {
  CostValue,
  PlanetValue,
  PlanetValuePair,
  ValueText,
  ValueUnit,
  pairLabel,
  valueKind,
  valueLabel,
} from "./PlanetValueIcons.tsx";

describe("value labels", () => {
  it("uses singular and plural words, and zero", () => {
    expect(valueLabel("resources", 1)).toBe("1 resource");
    expect(valueLabel("resources", 0)).toBe("0 resources");
    expect(valueLabel("resources", 4)).toBe("4 resources");
    expect(valueLabel("influence", 1)).toBe("1 influence");
    expect(valueLabel("influence", 0)).toBe("0 influence");
  });

  it("says ready of total when they differ", () => {
    expect(valueLabel("resources", 2, 5)).toBe("2 of 5 resources");
    expect(valueLabel("influence", 3, 3)).toBe("3 influence");
    expect(pairLabel(1, 6)).toBe("1 resource, 6 influence");
  });

  it("maps currencies and units to a kind", () => {
    expect(valueKind("Influence")).toBe("influence");
    expect(valueKind("I")).toBe("influence");
    expect(valueKind("Resources")).toBe("resources");
    expect(valueKind("R")).toBe("resources");
    expect(valueKind(undefined)).toBe("resources");
  });
});

describe("PlanetValue", () => {
  it("shows icon and number, with the word for screen readers and the tooltip", () => {
    render(<PlanetValue kind="resources" value={3} />);
    const el = screen.getByLabelText("3 resources");
    expect(el).toHaveAttribute("role", "img");
    expect(el).toHaveAttribute("title", "3 resources");
    expect(el).toHaveTextContent("3");
    expect(el.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(el).not.toHaveTextContent(/resources/i);
  });

  it("dims zero and spent values and keeps the word", () => {
    render(
      <>
        <PlanetValue kind="influence" value={0} />
        <PlanetValue kind="influence" value={2} state="spent" />
      </>,
    );
    expect(screen.getByLabelText("0 influence")).toHaveAttribute("data-state", "muted");
    expect(screen.getByLabelText("2 influence")).toHaveAttribute("data-state", "spent");
  });

  it("shows ready over total, a sign, and size variants", () => {
    render(
      <>
        <PlanetValue kind="resources" value={2} total={5} size="bar" />
        <PlanetValue kind="influence" value={3} sign="+" size="tooltip" />
      </>,
    );
    const ready = screen.getByLabelText("2 of 5 resources");
    expect(ready).toHaveTextContent("2/5");
    expect(ready.className).toContain("planet-value--bar");
    const plus = screen.getByLabelText("plus 3 influence");
    expect(plus).toHaveTextContent("+3");
    expect(plus.className).toContain("planet-value--tooltip");
  });

  it("can always show the total, and takes an explicit label", () => {
    render(<PlanetValue kind="resources" value={4} total={4} alwaysTotal label="Resources: 4 ready / 4 total" />);
    const el = screen.getByLabelText("Resources: 4 ready / 4 total");
    expect(el).toHaveTextContent("4/4");
  });
});

describe("ValueUnit, PlanetValuePair and ValueText", () => {
  it("renders a bare unit icon that still names the word", () => {
    render(<ValueUnit kind="influence" />);
    expect(screen.getByLabelText("influence")).toHaveAttribute("title", "influence");
  });

  it("renders a pair, resources first", () => {
    render(<PlanetValuePair resources={1} influence={6} />);
    const imgs = screen.getAllByRole("img");
    expect(imgs.map((i) => i.getAttribute("aria-label"))).toEqual(["1 resource", "6 influence"]);
  });

  it("swaps value words in our own text for icons and leaves the rest", () => {
    render(
      <p data-testid="t">
        <ValueText text="Cost: 3 resources, then Lodor (3R/1I) and 1 influence left" />
      </p>,
    );
    const t = screen.getByTestId("t");
    expect(t).toHaveTextContent("Cost: 3, then Lodor (31) and 1 left");
    expect(screen.getAllByRole("img").map((i) => i.getAttribute("aria-label"))).toEqual([
      "3 resources",
      "3 resources",
      "1 influence",
      "1 influence",
    ]);
  });

  it("leaves text without values alone", () => {
    render(<ValueText text="No planets" />);
    expect(screen.getByText("No planets")).toBeInTheDocument();
  });
});

describe("CostValue", () => {
  it("shows a plain cost as icon + number", () => {
    render(<CostValue text="4" />);
    expect(screen.getByRole("img", { name: "4 resources" })).toBeInTheDocument();
  });
  it("keeps the batch size as text and puts the icon on the price", () => {
    const { container } = render(<CostValue text="2 for 1" />);
    expect(container).toHaveTextContent("2 for 1");
    expect(screen.getByRole("img", { name: "1 resource" })).toBeInTheDocument();
  });
  it("leaves Free as text", () => {
    render(<CostValue text="Free" />);
    expect(screen.getByText("Free")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });
});
