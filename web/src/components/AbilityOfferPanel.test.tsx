import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";
import { abilityOfferLabel, describeAbilityOffer } from "../presentation/abilityOffer.ts";

const munitions = (have = 5): PendingChoiceDto => ({
  actor: "a",
  nonce: "m1",
  prompt: "spend 2 trade goods for Munitions Reserves",
  context: { subtype: "munitions_reserves_reroll" } as never,
  options: [
    { id: "munitions", kind: "ability", label: "reroll this round's misses" },
    { id: "decline", kind: "decline", label: "decline" },
  ],
  details: {
    kind: "ability_offer",
    ability: {
      name: "Munitions Reserves",
      window: "At the start of each round of space combat",
      effect: "You may spend 2 trade goods to re-roll any number of your dice during that combat round.",
    },
    cost: { trade_goods: 2, have: have },
  },
});

describe("ability offer", () => {
  it("reads the printed ability and the price against the goods held", () => {
    const view = describeAbilityOffer(munitions())!;
    expect(view).toMatchObject({ name: "Munitions Reserves", tradeGoods: 2, held: 5 });
    expect(abilityOfferLabel(view)).toBe("Spend 2 trade goods (5 → 3) to use Munitions Reserves");
  });

  it("is not an offer without the details or without a yes/no pair", () => {
    expect(describeAbilityOffer({ ...munitions(), details: undefined })).toBeNull();
    expect(describeAbilityOffer({ ...munitions(), options: [munitions().options[0]] })).toBeNull();
  });

  it("shows the card and answers with the chosen option", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={munitions()} onSubmit={onSubmit} />);
    expect(screen.getByTestId("ability-offer-window")).toHaveTextContent("start of each round");
    expect(screen.getByTestId("ability-offer-goods")).toHaveTextContent("5 (3 after)");
    fireEvent.click(screen.getByTestId("ability-offer-yes-btn"));
    expect(onSubmit).toHaveBeenCalledWith("munitions");
  });

  it("Skip declines", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={munitions()} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByTestId("ability-offer-skip-btn"));
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("cannot be bought without the goods", () => {
    render(<PendingChoiceModal choice={munitions(1)} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("ability-offer-yes-btn")).toBeDisabled();
  });
});
