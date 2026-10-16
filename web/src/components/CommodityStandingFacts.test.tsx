import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ExploreRewardPanel } from "./ExploreRewardParts.tsx";
import { CommodityStandingPanel } from "./CommodityStandingFacts.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const merchantStation = (details?: Record<string, unknown>): PendingChoiceDto => ({
  actor: "p1",
  nonce: "n1",
  prompt: "Merchant Station",
  context: {
    subtype: "merchant_station_choose_reward",
    source: { Content: "merchant_station" },
  } as never,
  options: [
    { id: "replenish", label: "replenish commodities", kind: "explore", payload: {} },
    { id: "convert", label: "convert commodities to trade goods", kind: "explore", payload: {} },
  ],
  details,
});

describe("commodity standing on commodity offers", () => {
  it("shows the seat's commodities of its limit on the exploration card", () => {
    render(
      <ExploreRewardPanel
        choice={merchantStation({ commodities: { have: 1, max: 4, trade_goods: 3 } })}
      />,
    );
    expect(screen.getByTestId("explore-card-name")).toHaveTextContent("Merchant Station");
    expect(screen.getByTestId("commodity-standing-have")).toHaveTextContent("1 / 4");
    expect(screen.getByTestId("commodity-standing-goods")).toHaveTextContent("3");
  });

  it("says when the commodities are already full", () => {
    render(<ExploreRewardPanel choice={merchantStation({ commodities: { have: 4, max: 4 } })} />);
    expect(screen.getByTestId("commodity-standing-have")).toHaveTextContent("4 / 4 full");
    expect(screen.queryByTestId("commodity-standing-goods")).toBeNull();
  });

  it("shows only the printed card when the engine sent no figures", () => {
    render(<ExploreRewardPanel choice={merchantStation()} />);
    expect(screen.getByTestId("explore-card")).toBeInTheDocument();
    expect(screen.queryByTestId("commodity-standing")).toBeNull();
  });

  it("stands alone above a plain option list", () => {
    render(
      <CommodityStandingPanel choice={{ details: { commodities: { have: 0, max: 3, trade_goods: 1 } } }} />,
    );
    expect(screen.getByTestId("commodity-standing-panel")).toHaveTextContent("Your commodities");
    expect(screen.getByTestId("commodity-standing-have")).toHaveTextContent("0 / 3");
  });
});
