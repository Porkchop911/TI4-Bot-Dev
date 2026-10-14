import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { describeVoteGoods, voteGoodsLabel } from "../presentation/voteGoods.ts";
import { offerCases } from "../dev/offerGalleryCases.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice = (): PendingChoiceDto =>
  offerCases().find((c) => c.title === "Hacan commander: spend trade goods for votes")!.choice;

describe("vote trade goods", () => {
  it("reads the commander, the outcome, the votes and the goods", () => {
    const view = describeVoteGoods(choice())!;
    expect(view).toMatchObject({
      title: "Gila the Silvertongue",
      outcome: "For",
      votes: 7,
      goods: 4,
      votesPerGood: 2,
    });
    expect([...view.spend.keys()]).toEqual([1, 2, 3, 4]);
    expect(voteGoodsLabel(view, 3)).toBe("Spend 3 trade goods for 6 more votes (7 → 13)");
    expect(voteGoodsLabel(view, 1)).toBe("Spend 1 trade good for 2 more votes (7 → 9)");
  });

  it("is not a vote-goods picker without the details or the spend options", () => {
    expect(describeVoteGoods({ ...choice(), details: undefined })).toBeNull();
    const onlyDecline = { ...choice(), options: choice().options.filter((o) => o.id === "decline") };
    expect(describeVoteGoods(onlyDecline)).toBeNull();
  });

  it("steps the amount and answers with the matching spend option", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice()} onSubmit={onSubmit} />);
    expect(screen.getByTestId("vote-goods-spend")).toHaveTextContent("Spend 1 trade good for 2 more votes (7 → 9)");
    fireEvent.click(screen.getByTestId("vote-goods-more"));
    fireEvent.click(screen.getByTestId("vote-goods-more"));
    expect(screen.getByTestId("vote-goods-amount")).toHaveTextContent("3 of 4");
    fireEvent.click(screen.getByTestId("vote-goods-spend"));
    expect(onSubmit).toHaveBeenCalledWith("spend|3");
  });

  it("can decline", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice()} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByTestId("vote-goods-skip"));
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });
});
