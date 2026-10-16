import { act } from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TradeDeskModal } from "./TradeDeskModal.tsx";
import type { ChoiceOptionDto, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { forgetNegotiation } from "../presentation/tradeNegotiation.ts";

const offer = (id: string, payload: Record<string, unknown> = {}): ChoiceOptionDto => ({
  id,
  label: id,
  kind: "offer",
  payload,
});

const proposeChoice = (nonce = "p1"): PendingChoiceDto => ({
  actor: "seat_1",
  nonce,
  prompt: "transaction with Sol",
  context: { subtype: "propose_transaction", target: { Player: "seat_2" } },
  options: [
    offer("cc2", { net: 2 }),
    offer("2:3", { net: 1 }),
    offer("1:1", { net: 0 }),
    offer("0:1", { net: 1 }),
    offer("pnra:sol:3", { note: "ra:sol", net: -2 }),
    offer("npcf:sol:2", { received_promissory: "cf:sol", net: 0 }),
    offer("cnsabo1>cf:sol", { action_card: "sabo1", received_promissory: "cf:sol" }),
    { id: "decline", label: "Offer nothing", kind: "decline" },
  ],
});

/** The same menu from a server that accepts deals built from any amounts. */
const freeFormChoice = (): PendingChoiceDto => ({
  ...proposeChoice("f1"),
  details: { free_form: "x;" },
});

const answerChoice = (): PendingChoiceDto => ({
  actor: "seat_2",
  nonce: "a1",
  prompt: "Sol gives 2 trade goods, ceasefire:sol for 3 commodities -- accept?",
  context: { subtype: "answer_transaction", target: { Player: "seat_1" } },
  options: [
    { id: "accept", label: "accept", kind: "answer", payload: { net: 4 } },
    { id: "refuse", label: "refuse", kind: "decline" },
    { id: "counter", label: "counter-offer", kind: "answer" },
  ],
});

const player = (id: string, over: Partial<PlayerView> = {}): PlayerView =>
  ({
    id,
    faction: id === "seat_1" ? "hacan" : "sol",
    trade_goods: id === "seat_1" ? 3 : 5,
    commodities: id === "seat_1" ? 4 : 2,
    ...over,
  }) as PlayerView;

const players = { seat_1: player("seat_1"), seat_2: player("seat_2") };

const renderDesk = (choice: PendingChoiceDto, viewerSeat: string, onSubmit = vi.fn().mockResolvedValue(undefined)) => {
  render(
    <TradeDeskModal
      isOpen
      choice={choice}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      onClose={vi.fn()}
      players={players}
    />,
  );
  return onSubmit;
};

const click = (testId: string) => fireEvent.click(screen.getByTestId(testId));

describe("TradeStagingDesk (propose)", () => {
  beforeEach(() => forgetNegotiation());

  it("starts empty with Propose disabled and both columns present", () => {
    renderDesk(proposeChoice(), "seat_1");
    expect(screen.getByTestId("stage-give")).toHaveTextContent("You give");
    expect(screen.getByTestId("stage-receive")).toHaveTextContent("You receive");
    expect(screen.getByTestId("stage-status-empty")).toBeInTheDocument();
    expect(screen.getByTestId("propose-trade-btn")).toBeDisabled();
    // holdings: own purse and the partner's public purse
    expect(screen.getByTestId("stage-give-held")).toHaveTextContent("3 trade goods, 4 commodities");
    expect(screen.getByTestId("stage-receive-held")).toHaveTextContent("5 trade goods, 2 commodities");
  });

  it("stages amounts with steppers, enables Propose on a listed combination and sends its id", async () => {
    const onSubmit = renderDesk(proposeChoice(), "seat_1");
    click("stage-give-tg-inc");
    click("stage-give-tg-inc");
    click("stage-receive-tg-inc");
    click("stage-receive-tg-inc");
    click("stage-receive-tg-inc");
    expect(screen.getByTestId("stage-status-valid")).toHaveTextContent("Ready to propose");
    expect(screen.getByTestId("propose-trade-btn")).not.toBeDisabled();
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("2:3");
  });

  it("caps the steppers at the list and keeps Decrease disabled at zero", () => {
    renderDesk(proposeChoice(), "seat_1");
    expect(screen.getByTestId("stage-give-tg-dec")).toBeDisabled();
    click("stage-give-tg-inc");
    click("stage-give-tg-inc");
    // the list gives at most 2 trade goods
    expect(screen.getByTestId("stage-give-tg-inc")).toBeDisabled();
    expect(screen.getByTestId("stage-give-tg-inc")).toHaveAttribute(
      "aria-label",
      "Increase Trade goods (give)",
    );
  });

  it("says a combination isn't available, why, and offers the nearest deals as one click", async () => {
    const onSubmit = renderDesk(proposeChoice(), "seat_1");
    click("stage-give-tg-inc");
    click("stage-give-tg-inc");
    click("stage-receive-tg-inc");
    click("stage-receive-tg-inc"); // 2 for 2 is not listed
    const invalid = screen.getByTestId("stage-status-invalid");
    expect(invalid).toHaveTextContent("This combination isn't available right now.");
    expect(screen.getByTestId("propose-trade-btn")).toBeDisabled();
    const suggestion = screen.getByTestId("stage-suggest-2:3");
    expect(suggestion).toBeInTheDocument();
    click("stage-suggest-2:3");
    expect(screen.getByTestId("stage-status-valid")).toBeInTheDocument();
    expect(screen.getByTestId("stage-receive-tg-value")).toHaveTextContent("3");
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledWith("2:3");
  });

  it("stages a promissory note with its text and an action card for the partner's note", async () => {
    const onSubmit = renderDesk(proposeChoice(), "seat_1");
    // own note sold for trade goods
    click("stage-give-note-ra:sol");
    expect(screen.getByTestId("stage-give-note-ra:sol")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("stage-status-invalid")).toBeInTheDocument(); // price not staged yet
    click("stage-receive-tg-inc");
    click("stage-receive-tg-inc");
    click("stage-receive-tg-inc");
    expect(screen.getByTestId("stage-status-valid")).toHaveTextContent("Net value (you): -2");
    // an action card, only because the list names it, for the partner's note
    click("reset-staging-btn");
    click("stage-give-ac-sabo1");
    click("stage-receive-note-cf:sol");
    expect(screen.getByTestId("stage-status-valid")).toBeInTheDocument();
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledWith("cnsabo1>cf:sol");
  });

  it("only offers cards the server lists: no action card appears on the receive side", () => {
    renderDesk(proposeChoice(), "seat_1");
    expect(screen.queryByTestId("stage-receive-ac-sabo1")).not.toBeInTheDocument();
    expect(screen.getByTestId("stage-give-ac-sabo1")).toBeInTheDocument();
  });

  it("keeps the quick deals catalogue as a collapsible fallback that stages a deal", async () => {
    const onSubmit = renderDesk(proposeChoice(), "seat_1");
    const quick = screen.getByTestId("quick-deals");
    expect(quick.tagName).toBe("DETAILS");
    expect(quick).not.toHaveAttribute("open");
    click("trade-opt-cc2");
    expect(screen.getByTestId("stage-give-cm-value")).toHaveTextContent("2");
    expect(screen.getByTestId("stage-receive-cm-value")).toHaveTextContent("2");
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledWith("cc2");
  });

  it("clears the desk and starts clean on a new decision", () => {
    const { rerender } = render(
      <TradeDeskModal isOpen choice={proposeChoice("n1")} viewerSeat="seat_1" onSubmit={vi.fn()} onClose={vi.fn()} players={players} />,
    );
    click("stage-give-tg-inc");
    expect(screen.getByTestId("stage-give-tg-value")).toHaveTextContent("1");
    rerender(
      <TradeDeskModal isOpen choice={proposeChoice("n2")} viewerSeat="seat_1" onSubmit={vi.fn()} onClose={vi.fn()} players={players} />,
    );
    expect(screen.getByTestId("stage-give-tg-value")).toHaveTextContent("0");
  });

  it("uses the phone-friendly structure: stacking columns and a sticky action bar", () => {
    renderDesk(proposeChoice(), "seat_1");
    expect(screen.getByTestId("trade-staging-desk").querySelector(".trade-desk__columns")).not.toBeNull();
    expect(screen.getByTestId("trade-propose-actions")).toHaveClass("trade-desk__actions");
  });

  it("shows spectators and other seats only the notice, never the desk", () => {
    renderDesk(proposeChoice(), "seat_3");
    expect(screen.getByTestId("spectator-trade-notice")).toBeInTheDocument();
    expect(screen.queryByTestId("trade-staging-desk")).not.toBeInTheDocument();
    expect(screen.queryByTestId("trade-answer")).not.toBeInTheDocument();
  });
});

describe("TradeStagingDesk (free-form amounts)", () => {
  beforeEach(() => forgetNegotiation());

  it("lets unequal amounts past the list up to each purse, and sends a free-form id", async () => {
    const onSubmit = renderDesk(freeFormChoice(), "seat_1");
    // 3 trade goods held: the list only reaches 2, the purse reaches 3
    for (let i = 0; i < 3; i++) click("stage-give-tg-inc");
    expect(screen.getByTestId("stage-give-tg-inc")).toBeDisabled();
    expect(screen.getByTestId("stage-give-tg")).toHaveTextContent("any amount up to 3");
    // the partner's 5 trade goods and 2 commodities: ask for 5 + 1, not on any list
    for (let i = 0; i < 5; i++) click("stage-receive-tg-inc");
    click("stage-receive-cm-inc");
    expect(screen.getByTestId("stage-status-custom")).toHaveTextContent("Ready to propose");
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledWith("x;gt=3;rt=5;rc=1;");
  });

  it("still sends the listed id when the combination is on the list", async () => {
    const onSubmit = renderDesk(freeFormChoice(), "seat_1");
    click("stage-give-tg-inc");
    click("stage-receive-tg-inc");
    await act(async () => {
      click("propose-trade-btn");
    });
    expect(onSubmit).toHaveBeenCalledWith("1:1");
  });

  it("refuses more than a purse holds, and a server without free-form keeps the closed list", () => {
    renderDesk(freeFormChoice(), "seat_1");
    for (let i = 0; i < 4; i++) click("stage-receive-cm-inc"); // partner holds 2
    expect(screen.getByTestId("stage-receive-cm-inc")).toBeDisabled();
  });

  it("without the server's free-form flag a combination off the list cannot be proposed", () => {
    renderDesk(proposeChoice(), "seat_1");
    click("stage-give-tg-inc");
    click("stage-receive-cm-inc");
    expect(screen.getByTestId("stage-status-invalid")).toBeInTheDocument();
    expect(screen.getByTestId("propose-trade-btn")).toBeDisabled();
  });
});

describe("answer screen", () => {
  beforeEach(() => forgetNegotiation());

  it("shows the partner's offer in two columns with a net summary above the answers", () => {
    renderDesk(answerChoice(), "seat_2");
    expect(screen.getByTestId("offer-give")).toHaveTextContent("3 commodities");
    expect(screen.getByTestId("offer-receive")).toHaveTextContent("2 trade goods");
    expect(screen.getByTestId("offer-receive")).toHaveTextContent("Ceasefire (sol)");
    expect(screen.getByTestId("offer-net")).toHaveTextContent("You give 3 commodities");
    expect(screen.getByTestId("offer-net")).toHaveTextContent("Net value (you): +4");
    // card text for the note comes from the card database (alias before the colon)
    expect(screen.getByTestId("offer-receive")).toHaveTextContent("cannot move units to the active system");
    expect(screen.getByTestId("answer-opt-accept")).toBeInTheDocument();
    expect(screen.getByTestId("answer-opt-refuse")).toBeInTheDocument();
    expect(screen.getByTestId("answer-opt-counter")).toBeInTheDocument();
    expect(screen.getByTestId("counter-note")).toHaveTextContent("exactly one counter-offer");
  });

  it("falls back to the sentence when the prompt is not a recognised offer", () => {
    const choice = { ...answerChoice(), prompt: "Something unusual -- accept?" };
    renderDesk(choice, "seat_2");
    expect(screen.getByTestId("trade-offer-unparsed")).toBeInTheDocument();
    expect(screen.getByTestId("answer-opt-accept")).toBeInTheDocument();
  });

  it("submits accept and refuse as before", async () => {
    const onSubmit = renderDesk(answerChoice(), "seat_2");
    await act(async () => {
      click("answer-opt-refuse");
    });
    expect(onSubmit).toHaveBeenCalledWith("refuse");
  });

  it("counter sends 'counter', then pre-fills the proposal desk from their offer for editing", async () => {
    const onSubmit = renderDesk(answerChoice(), "seat_2");
    await act(async () => {
      click("answer-opt-counter");
    });
    expect(onSubmit).toHaveBeenCalledWith("counter");

    // The engine now asks seat_2 to propose to seat_1, from seat_2's own deal list.
    const counterDeals: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "c1",
      prompt: "transaction with Hacan",
      context: { subtype: "propose_transaction", target: { Player: "seat_1" } },
      options: [
        offer("tc2:3", { net: 0 }),
        offer("tc1:3", { net: 0 }),
        { id: "decline", label: "Offer nothing", kind: "decline" },
      ],
    };
    document.body.innerHTML = "";
    renderDesk(counterDeals, "seat_2");
    expect(screen.getByTestId("counter-prefill-note")).toHaveTextContent("one counter");
    // staged as seat_2 would propose it: give 3 commodities, receive 2 trade goods and the note,
    // all visible even though this seat's list holds none of that
    expect(screen.getByTestId("stage-give-cm-value")).toHaveTextContent("3");
    expect(screen.getByTestId("stage-receive-tg-value")).toHaveTextContent("2");
    expect(screen.getByTestId("stage-receive-note-ceasefire:sol")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("stage-status-invalid")).toBeInTheDocument();
    expect(screen.getByTestId("propose-trade-btn")).toBeDisabled();
    // the nearest listed deals are suggested, and one click fixes it
    click("stage-suggest-tc2:3");
    expect(screen.getByTestId("propose-trade-btn")).not.toBeDisabled();
  });
});
