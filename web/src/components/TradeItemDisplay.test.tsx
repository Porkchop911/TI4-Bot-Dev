import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { TradeItemDisplay } from "./TradeItemDisplay.tsx";

// Run 01-2322: the trade desk showed "Promissory Note: support_for_throne" instead of the card name.
describe("TradeItemDisplay card names", () => {
  const name = () => screen.getByTestId("item").querySelector(".trade-item__name")?.textContent;

  it("shows Support for the Throne by name, not by its id", () => {
    render(<TradeItemDisplay testId="item" label="Promissory Note: support_for_throne" type="offer" isCard />);
    expect(name()).toBe("Support for the Throne");
  });

  it("resolves engine note aliases (cf:generic) to the printed name and card text", () => {
    render(<TradeItemDisplay testId="item" label="Promissory Note: cf:generic" type="receive" isCard />);
    expect(name()).toBe("Ceasefire");
    expect(screen.getByTestId("item")).toHaveTextContent("cannot move units to the active system");
  });

  it("keeps the owning faction of a faction-bound note", () => {
    render(<TradeItemDisplay testId="item" label="Promissory Note: ceasefire:sol" type="receive" isCard />);
    expect(name()).toBe("Ceasefire (sol)");
  });

  it("names action cards from the catalog", () => {
    render(<TradeItemDisplay testId="item" label="Action Card: arch_expedition" type="offer" isCard />);
    expect(name()).toBe("Archaeological Expedition");
  });

  it("names faction notes", () => {
    render(<TradeItemDisplay testId="item" label="Promissory Note: ra:jolnar" type="offer" isCard />);
    expect(name()).toBe("Research Agreement (jolnar)");
  });

  it("gives unknown cards a readable name and keeps the id in the title", () => {
    render(<TradeItemDisplay testId="item" label="Action Card: nope_card" type="offer" isCard />);
    expect(name()).toBe("Nope Card");
    expect(screen.getByTestId("item").querySelector(".trade-item__name")).toHaveAttribute(
      "title",
      "Action Card: nope_card",
    );
  });

  it("leaves non-card items as they are", () => {
    render(<TradeItemDisplay testId="item" label="2 trade goods" type="offer" />);
    expect(name()).toBe("2 trade goods");
  });
});
