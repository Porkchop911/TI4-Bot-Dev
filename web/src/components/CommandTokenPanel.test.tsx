import { act } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { PlanetResourcesContext } from "../presentation/PlanetResourcesContext.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const pool = (id: string) => ({ id, kind: "pool", label: id });
const gainChoice = (toPlace = 2): PendingChoiceDto => ({
  actor: "seat_1",
  nonce: "gain-1",
  prompt: "gain a command token into which pool",
  options: ["tactic_tokens", "fleet_tokens", "strategic_tokens"].map(pool),
  context: { subtype: "gain_command_token" } as PendingChoiceDto["context"],
  details: {
    kind: "command_tokens",
    mode: "gain",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 7,
    tokens_to_place: toPlace,
  },
});
const redistributeChoice = (): PendingChoiceDto => ({
  actor: "seat_1",
  nonce: "redis-1",
  prompt: "redistribute your command tokens",
  options: ["3|4|2", "2|5|2", "2|4|3"].map((id) => ({ id, kind: "redistribute", label: id })),
  context: { subtype: "status_redistribute_tokens" } as PendingChoiceDto["context"],
  details: {
    kind: "command_tokens",
    mode: "redistribute",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 7,
    total: 9,
  },
});
const click = async (id: string) =>
  act(async () => {
    fireEvent.click(screen.getByTestId(id));
  });

describe("command token panel (gain)", () => {
  it("shows pools, counts, the total, the remaining tokens and pips", async () => {
    render(<PendingChoiceModal choice={gainChoice()} onSubmit={vi.fn()} onSubmitBatch={vi.fn()} />);
    expect(screen.getByTestId("token-total")).toHaveTextContent("2");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("2");
    expect(screen.getByTestId("token-reinforcements")).toHaveTextContent("7");
    expect(screen.getByTestId("token-count-fleet")).toHaveTextContent("4");
    await click("token-plus-fleet");
    expect(screen.getByTestId("token-count-fleet")).toHaveTextContent("5");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("1");
    const pips = screen.getByTestId("token-pips-fleet");
    expect(pips).toHaveAttribute("data-kept", "4");
    expect(pips).toHaveAttribute("data-added", "1");
    expect(screen.queryByTestId("choice-option")).not.toBeInTheDocument();
  });

  it("keeps Confirm disabled until every token is assigned, then sends one tokens batch", async () => {
    const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
    render(
      <PendingChoiceModal choice={gainChoice()} onSubmit={vi.fn()} onSubmitBatch={onSubmitBatch} />,
    );
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    await click("token-plus-strategic");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    await click("token-plus-tactic");
    expect(screen.getByTestId("token-confirm")).toBeEnabled();
    expect(screen.getByTestId("token-plus-fleet")).toBeDisabled();
    await click("token-confirm");
    expect(onSubmitBatch).toHaveBeenCalledTimes(1);
    expect(onSubmitBatch).toHaveBeenCalledWith({
      kind: "tokens",
      steps: [
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "pool", pool: "strategic_tokens" },
      ],
    });
  });

  it("minus and Reset take staged tokens back", async () => {
    render(<PendingChoiceModal choice={gainChoice()} onSubmit={vi.fn()} onSubmitBatch={vi.fn()} />);
    expect(screen.getByTestId("token-minus-tactic")).toBeDisabled();
    await click("token-plus-tactic");
    await click("token-minus-tactic");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("2");
    await click("token-plus-tactic");
    await click("token-plus-fleet");
    await click("token-reset");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("2");
    expect(screen.getByTestId("token-count-tactic")).toHaveTextContent("3");
  });

  it("re-syncs after a rejected or interrupted plan: error shown, staging cleared", async () => {
    const onSubmitBatch = vi.fn().mockRejectedValue(new Error("Batch rejected: moved on"));
    render(
      <PendingChoiceModal choice={gainChoice()} onSubmit={vi.fn()} onSubmitBatch={onSubmitBatch} />,
    );
    await click("token-plus-tactic");
    await click("token-plus-tactic");
    await click("token-confirm");
    expect(screen.getByTestId("token-error")).toHaveTextContent("moved on");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("2");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
  });

  it("falls back to the plain list when no batch submitter is supplied", () => {
    render(<PendingChoiceModal choice={gainChoice()} onSubmit={vi.fn()} />);
    expect(screen.queryByTestId("command-token-panel")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("choice-option")).toHaveLength(3);
  });
});

describe("command token panel (redistribute)", () => {
  it("starts on the current arrangement and submits the matching option id", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={redistributeChoice()} onSubmit={onSubmit} />);
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("0");
    expect(screen.getByTestId("token-confirm")).toBeEnabled();
    await click("token-minus-tactic");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("1");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    expect(screen.getByTestId("token-pips-tactic")).toHaveAttribute("data-removed", "1");
    await click("token-plus-fleet");
    await click("token-confirm");
    expect(onSubmit).toHaveBeenCalledWith("2|5|2");
  });

  it("blocks an arrangement the engine does not offer", async () => {
    render(<PendingChoiceModal choice={redistributeChoice()} onSubmit={vi.fn()} />);
    await click("token-minus-tactic");
    await click("token-plus-tactic");
    await click("token-minus-strategic");
    await click("token-plus-strategic");
    await click("token-minus-fleet");
    await click("token-plus-tactic");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    expect(screen.getByTestId("token-blocker")).toBeInTheDocument();
  });
});

const purchase = {
  cost: 3,
  influence_available: 9,
  max: 3,
  trade_goods: 7,
  trade_good_worth: 1,
  planets: [{ id: "jord", worth: 2 }],
};
const gainBuyChoice = (): PendingChoiceDto => {
  const choice = gainChoice(3);
  return { ...choice, nonce: "gain-buy-1", details: { ...choice.details, purchase } };
};

const resourcesOf = new Map([
  ["jord", 2],
  ["arcturus", 0],
  ["lodor", 3],
]);
const renderModal = (choice: PendingChoiceDto, onSubmitBatch = vi.fn().mockResolvedValue(undefined)) => {
  render(
    <PlanetResourcesContext.Provider value={resourcesOf}>
      <PendingChoiceModal choice={choice} onSubmit={vi.fn()} onSubmitBatch={onSubmitBatch} />
    </PlanetResourcesContext.Provider>,
  );
  return onSubmitBatch;
};
const assign = async (pools: string[]) => {
  for (const pool of pools) await click(`token-plus-${pool}`);
};

describe("command token panel (planet first: gain and buy)", () => {
  it("starts from the planets: nothing staged, the suggestion offered as one tap", async () => {
    const onSubmitBatch = renderModal(gainBuyChoice());
    expect(screen.getByTestId("token-total")).toHaveTextContent("3");
    expect(screen.getByTestId("token-buy")).toHaveTextContent("per command token");
    expect(within(screen.getByTestId("token-buy")).getByLabelText("3 influence")).toBeInTheDocument();
    expect(screen.getByTestId("token-payment-planet-jord")).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByTestId("token-purchase-summary")).toHaveTextContent("Influence selected 0");
    expect(screen.getByTestId("token-purchase-summary")).toHaveTextContent("buys 0 tokens");
    expect(screen.getByTestId("token-buy-minus")).toBeDisabled();
    expect(screen.getByTestId("token-payment-auto")).toHaveTextContent("Use suggested planets (buys 3)");
    // The planet block comes before the pool assignment in the document.
    const buy = screen.getByTestId("token-buy");
    const pools = screen.getByTestId("token-pool-tactic");
    expect(buy.compareDocumentPosition(pools) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(onSubmitBatch).not.toHaveBeenCalled();
  });

  it("the suggestion tap scrolls down to the assignment part", async () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    renderModal(gainBuyChoice());
    expect(scroll).not.toHaveBeenCalled();
    await click("token-payment-auto");
    expect(scroll).toHaveBeenCalledTimes(1);
    expect(scroll.mock.instances[0]).toBe(screen.getByTestId("token-assign-heading"));
  });

  it("one tap on the suggestion stages the planets and trade goods for the most that can be bought", async () => {
    const onSubmitBatch = renderModal(gainBuyChoice());
    await click("token-payment-auto");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("3");
    expect(screen.getByTestId("token-total")).toHaveTextContent("6");
    expect(screen.getByTestId("token-payment-planet-jord")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("token-payment-goods-count")).toHaveTextContent("7");
    expect(screen.getByTestId("token-purchase-summary")).toHaveTextContent("Influence selected 9");
    await assign(["tactic", "tactic", "fleet", "fleet", "strategic", "strategic"]);
    await click("token-confirm");
    const steps = onSubmitBatch.mock.calls[0][0].steps;
    expect(steps.filter((s: { kind: string }) => s.kind === "purchase")).toHaveLength(3);
    expect(steps.filter((s: { kind: string }) => s.kind === "trade_good")).toHaveLength(7);
  });

  it("the token count follows the selection: selecting planets then assigning pools sends the old wire format", async () => {
    const onSubmitBatch = renderModal(gainBuyChoice());
    await click("token-payment-planet-jord");
    expect(screen.getByTestId("token-purchase-summary")).toHaveAttribute("data-bought", "0");
    expect(screen.getByTestId("token-payment-problem")).toHaveTextContent("buys no token");
    await click("token-payment-goods-plus");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("1");
    expect(screen.getByTestId("token-total")).toHaveTextContent("4");
    expect(screen.getByTestId("token-split")).toHaveTextContent("3 free + 1 bought");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    await assign(["tactic", "tactic", "fleet", "strategic"]);
    await click("token-confirm");
    expect(onSubmitBatch).toHaveBeenCalledWith({
      kind: "tokens",
      steps: [
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "pool", pool: "fleet_tokens" },
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "trade_good" },
        { kind: "pool", pool: "strategic_tokens" },
        { kind: "purchase", buy: false },
      ],
    });
  });

  it("'Buy N tokens' still works first: it pre-stages the cheapest payment and the selection stays editable", async () => {
    renderModal(gainBuyChoice());
    await click("token-buy-plus");
    await click("token-buy-plus");
    expect(screen.getByTestId("token-split")).toHaveTextContent("3 free + 2 bought");
    expect(screen.getByTestId("token-payment-planet-jord")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("token-influence-spent")).toHaveTextContent("6");
    // Taking jord out leaves 4 trade goods = 1 token; the count follows.
    await click("token-payment-planet-jord");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("1");
    await click("token-payment-clear");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("0");
    expect(screen.getByTestId("token-total")).toHaveTextContent("3");
  });

  it("stops the quick selector at what is affordable and drops staged tokens when buying fewer", async () => {
    renderModal(gainBuyChoice());
    for (let i = 0; i < 5; i += 1) await click("token-buy-plus");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("3");
    expect(screen.getByTestId("token-buy-plus")).toBeDisabled();
    for (let i = 0; i < 6; i += 1) await click("token-plus-fleet");
    await click("token-buy-minus");
    expect(screen.getByTestId("token-remaining")).toHaveTextContent("0");
    expect(screen.getByTestId("token-pips-fleet")).toHaveAttribute("data-added", "5");
    await click("token-reset");
    expect(screen.getByTestId("token-buy-count")).toHaveTextContent("0");
    expect(screen.getByTestId("token-total")).toHaveTextContent("3");
  });

  it("is a plain list again without a batch submitter", () => {
    render(<PendingChoiceModal choice={gainBuyChoice()} onSubmit={vi.fn()} />);
    expect(screen.queryByTestId("command-token-panel")).not.toBeInTheDocument();
  });

  it("shows the secondary window's purchase on the same panel and answers it as one plan", async () => {
    const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
    const window: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "secondary-1",
      prompt: "spend 3 influence for a command token",
      options: [
        { id: "no", kind: "strategy", label: "spend nothing further" },
        { id: "yes", kind: "strategy", label: "spend 3 influence" },
      ],
      context: { subtype: "buy_token_with_influence" } as PendingChoiceDto["context"],
      details: {
        kind: "strategy_secondary",
        card: "pok1leadership",
        played_by: "seat_2",
        mode: "buy",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 7,
        tokens_to_place: 0,
        purchase,
      },
    };
    render(<PendingChoiceModal choice={window} onSubmit={vi.fn()} onSubmitBatch={onSubmitBatch} />);
    expect(screen.getByTestId("token-confirm")).toHaveTextContent("No purchase");
    await click("token-payment-planet-jord");
    await click("token-payment-goods-plus");
    await click("token-plus-strategic");
    expect(screen.getByTestId("token-confirm")).toHaveTextContent("Confirm tokens and purchase");
    await click("token-confirm");
    expect(onSubmitBatch).toHaveBeenCalledWith({
      kind: "tokens",
      steps: [
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "trade_good" },
        { kind: "pool", pool: "strategic_tokens" },
        { kind: "purchase", buy: false },
      ],
    });
  });

  it("a selection that buys no token cannot be confirmed as 'no purchase'", async () => {
    renderModal(gainBuyChoice());
    await assign(["tactic", "tactic", "fleet"]);
    expect(screen.getByTestId("token-confirm")).toBeEnabled();
    await click("token-payment-planet-jord");
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    await click("token-payment-clear");
    expect(screen.getByTestId("token-confirm")).toBeEnabled();
  });
});

describe("command token panel (planet first: credit, overpay and limits)", () => {
  const rich = {
    cost: 3,
    influence_available: 14,
    max: 4,
    trade_goods: 2,
    trade_good_worth: 1,
    planets: [
      { id: "jord", worth: 2 },
      { id: "arcturus", worth: 4 },
      { id: "lodor", worth: 3 },
    ],
  };
  const richChoice = (reinforcements = 7): PendingChoiceDto => {
    const choice = gainChoice(1);
    return {
      ...choice,
      nonce: `gain-rich-${reinforcements}`,
      details: { ...choice.details, reinforcements, purchase: rich },
    };
  };
  const summary = () => screen.getByTestId("token-purchase-summary");

  it("3 influence buys 1 token with nothing wasted", async () => {
    renderModal(richChoice());
    await click("token-payment-planet-lodor");
    expect(summary()).toHaveAttribute("data-bought", "1");
    expect(summary()).toHaveAttribute("data-wasted", "0");
    expect(screen.queryByTestId("token-payment-problem")).not.toBeInTheDocument();
  });

  it("5 influence from two planets is one token with a spare planet: Confirm is blocked and says which to take out", async () => {
    renderModal(richChoice());
    await click("token-payment-planet-lodor");
    await click("token-payment-planet-jord");
    expect(summary()).toHaveAttribute("data-bought", "1");
    expect(summary()).toHaveAttribute("data-wasted", "2");
    await assign(["tactic", "tactic"]);
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    expect(screen.getByTestId("token-payment-problem")).toHaveTextContent("take out jord");
  });

  it("4 influence from one planet buys 1 token and wastes 1", async () => {
    const onSubmitBatch = renderModal(richChoice());
    await click("token-payment-planet-arcturus");
    expect(summary()).toHaveAttribute("data-bought", "1");
    expect(summary()).toHaveAttribute("data-wasted", "1");
    await assign(["tactic", "tactic"]);
    await click("token-confirm");
    expect(onSubmitBatch.mock.calls[0][0].steps).toEqual([
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "arcturus" },
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "purchase", buy: false },
    ]);
  });

  it("6 influence buys 2 tokens exactly; 7 buys 2 and wastes 1 (the credit is lost after the last token)", async () => {
    const onSubmitBatch = renderModal(richChoice());
    await click("token-payment-planet-arcturus");
    await click("token-payment-planet-jord");
    expect(summary()).toHaveAttribute("data-bought", "2");
    expect(summary()).toHaveAttribute("data-wasted", "0");
    await click("token-payment-planet-jord");
    await click("token-payment-planet-lodor");
    expect(summary()).toHaveAttribute("data-bought", "2");
    expect(summary()).toHaveAttribute("data-wasted", "1");
    await assign(["tactic", "tactic", "fleet"]);
    await click("token-confirm");
    // The first token is covered by arcturus (4, 1 carried); the second by the carried 1 plus lodor.
    expect(onSubmitBatch.mock.calls[0][0].steps).toEqual([
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "arcturus" },
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "lodor" },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "purchase", buy: false },
    ]);
  });

  it("trade goods count toward the influence and are spent last", async () => {
    renderModal(richChoice());
    await click("token-payment-planet-jord");
    await click("token-payment-goods-plus");
    expect(summary()).toHaveAttribute("data-bought", "1");
    expect(screen.getByTestId("token-payment-goods-count")).toHaveTextContent("1");
  });

  it("the token count is capped by what the reinforcements can hold", async () => {
    // 1 free token + reinforcements 3 leaves room for 2 bought tokens, but 9 influence is selected.
    renderModal(richChoice(3));
    await click("token-payment-planet-arcturus");
    await click("token-payment-planet-lodor");
    await click("token-payment-planet-jord");
    expect(summary()).toHaveAttribute("data-bought", "2");
    expect(screen.getByTestId("token-limit-note")).toHaveTextContent("most you can buy is 2");
    await assign(["tactic", "tactic", "fleet"]);
    expect(screen.getByTestId("token-confirm")).toBeDisabled();
    await click("token-payment-planet-jord");
    expect(screen.queryByTestId("token-limit-note")).not.toBeInTheDocument();
    expect(screen.getByTestId("token-confirm")).toBeEnabled();
  });
});

describe("command token panel (resource values)", () => {
  const rich = {
    cost: 3,
    influence_available: 9,
    max: 3,
    trade_goods: 0,
    trade_good_worth: 1,
    planets: [
      { id: "jord", worth: 2 },
      { id: "arcturus", worth: 2 },
      { id: "lodor", worth: 3 },
    ],
  };
  const choice = (): PendingChoiceDto => {
    const base = gainChoice(1);
    return { ...base, nonce: "gain-res", details: { ...base.details, purchase: rich } };
  };

  it("shows each planet's influence AND resources in the list, with the words as labels", () => {
    renderModal(choice());
    const lodor = screen.getByTestId("token-payment-planet-lodor");
    expect(within(lodor).getByLabelText("Pays 3 influence")).toBeInTheDocument();
    expect(within(lodor).getByLabelText("Exhausting it loses 3 resources")).toBeInTheDocument();
    const arcturus = screen.getByTestId("token-payment-planet-arcturus");
    expect(within(arcturus).getByLabelText("Exhausting it loses 0 resources")).toBeInTheDocument();
  });

  it("the suggestion prefers planets with no resources and shows their resources too", async () => {
    renderModal(choice());
    await click("token-buy-plus");
    // 3 influence: lodor (3, 3 resources) pays it exactly, but arcturus + jord (2 + 2, 0 + 2 resources) costs
    // fewer resources only if jord had none; here lodor 3R vs arcturus+jord 2R, so the two win.
    expect(screen.getByTestId("token-payment-planet-lodor")).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByTestId("token-payment-planet-arcturus")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("token-payment-planet-jord")).toHaveAttribute("aria-pressed", "true");
    const suggestion = screen.getByTestId("token-suggestion");
    expect(within(suggestion).getByLabelText("Exhausting it loses 0 resources")).toBeInTheDocument();
    expect(within(screen.getByTestId("token-resources-lost")).getByLabelText("2 resources lost this round")).toBeInTheDocument();
  });
});

describe("command token panel (Predictive Intelligence restack)", () => {
  const moves = ["tactic|fleet", "tactic|strategy", "fleet|tactic", "fleet|strategy", "strategy|tactic", "strategy|fleet"];
  const restackChoice = (nonce: string, pools: { tactic: number; fleet: number; strategic: number }): PendingChoiceDto => ({
    actor: "seat_1",
    nonce,
    prompt: "Predictive Intelligence: redistribute command tokens",
    options: [
      ...moves.map((id) => ({ id, kind: "redistribute", label: id })),
      { id: "done", kind: "decline", label: "finish redistribution" },
    ],
    context: { subtype: "predictive_intelligence_redistribute" } as PendingChoiceDto["context"],
    details: { kind: "command_tokens", mode: "restack", pools, reinforcements: 7, total: 9 },
  });

  it("answers one question per move, then done, as the engine asks them", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <PendingChoiceModal choice={restackChoice("pi-1", { tactic: 3, fleet: 4, strategic: 2 })} onSubmit={onSubmit} />,
    );
    expect(screen.getByTestId("command-token-panel")).toHaveAttribute("data-mode", "restack");
    expect(screen.getByTestId("token-confirm")).toHaveTextContent("Keep as is");
    await click("token-minus-tactic");
    await click("token-minus-tactic");
    await click("token-plus-strategic");
    await click("token-plus-strategic");
    expect(screen.getByTestId("token-confirm")).toHaveTextContent("Confirm 2 moves");
    await click("token-confirm");
    expect(onSubmit).toHaveBeenLastCalledWith("tactic|strategy");
    rerender(
      <PendingChoiceModal choice={restackChoice("pi-2", { tactic: 2, fleet: 4, strategic: 3 })} onSubmit={onSubmit} />,
    );
    await act(async () => {});
    expect(onSubmit).toHaveBeenLastCalledWith("tactic|strategy");
    rerender(
      <PendingChoiceModal choice={restackChoice("pi-3", { tactic: 1, fleet: 4, strategic: 4 })} onSubmit={onSubmit} />,
    );
    await act(async () => {});
    expect(onSubmit).toHaveBeenLastCalledWith("done");
    expect(onSubmit).toHaveBeenCalledTimes(3);
  });

  it("keeping the arrangement just answers done", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={restackChoice("pi-1", { tactic: 3, fleet: 4, strategic: 2 })} onSubmit={onSubmit} />);
    await click("token-confirm");
    expect(onSubmit).toHaveBeenCalledWith("done");
  });
});
