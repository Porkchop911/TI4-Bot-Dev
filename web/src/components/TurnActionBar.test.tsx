import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import { TurnActionBar } from "./TurnActionBar.tsx";
import { deriveReadOnlyTurnBar, deriveTurnBar, type TurnBarModel } from "../presentation/turnBar.ts";
import type { PendingChoiceDto, PlayerView } from "../protocol/types.ts";

const opt = (id: string, label: string, kind = "action") => ({ id, label, kind });
const tokens = { tactic: 3, fleet: 3, strategy: 2 };
const twoCards = [
  { card: "pok2diplomacy", used: false, option: "strategic|pok2diplomacy" },
  { card: "pok8imperial", used: false, option: "strategic|pok8imperial" },
];
const partners = [
  {
    seat: "b",
    faction: "hacan",
    available: true,
    in_contact: true,
    reason: null,
    trade_goods: 2,
    commodities: 3,
    promissory_notes: 1,
  },
  {
    seat: "d",
    faction: "xxcha",
    available: false,
    in_contact: false,
    reason: "No contact: not neighbours",
    trade_goods: 0,
    commodities: 1,
    promissory_notes: 0,
  },
];

function turnChoice(
  options: ReturnType<typeof opt>[],
  details: Record<string, unknown> = {},
  prompt = "action phase",
): PendingChoiceDto {
  return {
    actor: "a",
    nonce: "n1",
    prompt,
    options,
    details: { kind: "turn_menu", closing: false, tokens, strategy_cards: twoCards, partners, ...details },
  };
}

const fullMenu = () =>
  turnChoice([
    opt("strategic|pok2diplomacy", "take the strategic action of 2. Diplomacy"),
    opt("strategic|pok8imperial", "take the strategic action of 8. Imperial"),
    opt("tactical", "take a tactical action"),
    opt("component|trade|hacan", "open a transaction with hacan", "open_transaction"),
    opt("action_card|0", "play Reactor Meltdown", "component"),
    opt("faction|orbital_drop", "Orbital Drop: spend a strategy token to land 2 infantry", "component"),
  ]);

function modelOf(choice: PendingChoiceDto): TurnBarModel {
  const model = deriveTurnBar(choice);
  if (!model) throw new Error("no model");
  return model;
}

const click = (el: HTMLElement) =>
  act(async () => {
    fireEvent.click(el);
  });

describe("TurnActionBar", () => {
  it("shows the pools as pips with counts and a heading", () => {
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("turn-bar-heading")).toHaveTextContent("Choose an action");
    expect(screen.getByTestId("turn-bar-pool-tactic")).toHaveTextContent("3");
    expect(screen.getByTestId("turn-bar-pool-strategy")).toHaveTextContent("2");
    expect(screen.getByTestId("turn-bar-pool-tactic").querySelectorAll("i")).toHaveLength(3);
  });

  it("submits the matching option for Tactical", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    await click(screen.getByTestId("turn-bar-tactical"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("tactical");
  });

  it("shows one Strategic button per held card, each submitting its own option", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    const diplomacy = screen.getByTestId("turn-bar-strategic-pok2diplomacy");
    const imperial = screen.getByTestId("turn-bar-strategic-pok8imperial");
    expect(diplomacy).toHaveTextContent("2. Diplomacy");
    expect(imperial).toHaveTextContent("8. Imperial");
    expect(imperial).toHaveTextContent(/Immediately score 1 public objective/);
    await click(imperial);
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("strategic|pok8imperial");
  });

  it("keeps a single card as one Strategic action button", () => {
    const choice = turnChoice([opt("strategic", "take your strategic action"), opt("tactical", "t")], {
      strategy_cards: [{ card: "pok8imperial", used: false, option: "strategic" }],
    });
    render(<TurnActionBar model={modelOf(choice)} onSubmit={vi.fn()} />);
    const buttons = screen.getAllByTestId(/turn-bar-strategic-/);
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent("Strategic action");
  });

  it("marks a used card disabled with its reason and does not submit it", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice = turnChoice([opt("strategic", "s"), opt("tactical", "t")], {
      strategy_cards: [
        { card: "pok2diplomacy", used: true, option: null },
        { card: "pok8imperial", used: false, option: "strategic" },
      ],
    });
    render(<TurnActionBar model={modelOf(choice)} onSubmit={onSubmit} />);
    const used = screen.getByTestId("turn-bar-strategic-pok2diplomacy");
    expect(used).toHaveAttribute("aria-disabled", "true");
    expect(used).toHaveTextContent("Already used this round");
    await click(used);
    expect(onSubmit).not.toHaveBeenCalled();
    await click(screen.getByTestId("turn-bar-strategic-pok8imperial"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("strategic");
  });

  it("says why Pass and End turn are disabled and keeps them in place", () => {
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("turn-bar-pass")).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByTestId("turn-bar-pass")).toHaveTextContent("Use both cards first");
    expect(screen.getByTestId("turn-bar-end")).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByTestId("turn-bar-end")).toHaveTextContent("Act or pass first");
  });

  it("opens one Trade partner picker and submits the chosen partner's option", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    expect(screen.queryByTestId("turn-bar-menu-trade")).toBeNull();
    await click(screen.getByTestId("turn-bar-trade"));
    const menu = screen.getByTestId("turn-bar-menu-trade");
    expect(menu).toHaveTextContent("3 commodities");
    expect(menu).toHaveTextContent("1 promissory note");
    expect(screen.getByTestId("turn-bar-partner-xxcha")).toHaveTextContent(
      "No contact: not neighbours",
    );
    await click(screen.getByTestId("turn-bar-open-xxcha"));
    expect(onSubmit).not.toHaveBeenCalled();
    await click(screen.getByTestId("turn-bar-open-hacan"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("component|trade|hacan");
  });

  it("lists components and action cards and submits the row's option", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    expect(screen.getByTestId("turn-bar-cards")).toHaveTextContent("1");
    await click(screen.getByTestId("turn-bar-cards"));
    await click(screen.getByTestId("turn-bar-item-action_card|0"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("action_card|0");
    await click(screen.getByTestId("turn-bar-components"));
    expect(screen.getByTestId("turn-bar-menu-components")).toHaveTextContent("Faction abilities");
    await click(screen.getByTestId("turn-bar-item-faction|orbital_drop"));
    expect(onSubmit).toHaveBeenLastCalledWith("faction|orbital_drop");
  });

  it("spells out what each component does in its row: kind, owner, timing, cost and rules text", async () => {
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={vi.fn()} />);
    await click(screen.getByTestId("turn-bar-components"));
    const row = screen.getByTestId("turn-bar-item-faction|orbital_drop");
    expect(row).toHaveTextContent("Orbital Drop");
    expect(row).toHaveTextContent("Faction ability · The Federation of Sol");
    expect(row).toHaveTextContent("Cost: 1 strategy token");
    expect(row.querySelector('[data-testid="turn-bar-row-text"]')).toHaveTextContent(
      /place 2 infantry from your reinforcements on 1 planet you control/,
    );
  });

  it("sizes an open menu to the room above the bar and closes it with Close", async () => {
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={vi.fn()} />);
    const bar = screen.getByTestId("turn-action-bar");
    expect(bar.style.getPropertyValue("--turn-bar-menu-room")).toBe("");
    await click(screen.getByTestId("turn-bar-trade"));
    expect(bar).toHaveAttribute("data-menu-open", "trade");
    expect(bar.style.getPropertyValue("--turn-bar-menu-room")).toMatch(/^\d+px$/);
    expect(bar.style.getPropertyValue("--turn-bar-menu-top")).toMatch(/^\d+px$/);
    expect(bar.style.getPropertyValue("--turn-bar-menu-bottom")).toMatch(/^\d+px$/);
    expect(screen.getByTestId("turn-bar-partner-hacan")).toHaveTextContent("Hacan");
    await click(screen.getByTestId("turn-bar-menu-close"));
    expect(screen.queryByTestId("turn-bar-menu-trade")).toBeNull();
    expect(bar).not.toHaveAttribute("data-menu-open");
    expect(bar.style.getPropertyValue("--turn-bar-menu-room")).toBe("");
  });

  it("shows the card text in the side panel on hover and focus, and clears it after", async () => {
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={vi.fn()} />);
    const imperial = screen.getByTestId("turn-bar-strategic-pok8imperial");
    fireEvent.mouseEnter(imperial);
    expect(screen.getByTestId("turn-bar-info")).toHaveTextContent("Gain 1 victory point");
    expect(screen.getByTestId("turn-bar-info")).toHaveTextContent("Secondary:");
    fireEvent.mouseLeave(imperial);
    expect(screen.queryByTestId("turn-bar-info")).toBeNull();
    await act(async () => {
      screen.getByTestId("turn-bar-strategic-pok2diplomacy").focus();
    });
    expect(screen.getByTestId("turn-bar-info")).toHaveTextContent("Diplomacy");
  });

  it("ends the turn with Enter when it is highlighted", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const closing = turnChoice(
      [opt("end_turn", "end your turn", "end_turn")],
      { closing: true },
      "end your turn",
    );
    render(<TurnActionBar model={modelOf(closing)} onSubmit={onSubmit} />);
    const end = screen.getByTestId("turn-bar-end");
    expect(end).toHaveAttribute("aria-disabled", "false");
    expect(end).toHaveTextContent("Enter or click");
    fireEvent.keyDown(document.body, { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("end_turn");
  });

  it("does not end the turn with Enter while a button holds focus", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const closing = turnChoice([opt("end_turn", "end your turn", "end_turn")], { closing: true });
    render(<TurnActionBar model={modelOf(closing)} onSubmit={onSubmit} />);
    await act(async () => {
      screen.getByTestId("turn-bar-tactical").focus();
    });
    fireEvent.keyDown(screen.getByTestId("turn-bar-tactical"), { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("letter keys submit single options directly and open pickers; Escape closes them", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    fireEvent.keyDown(document.body, { key: "t" });
    expect(onSubmit).toHaveBeenLastCalledWith("tactical");
    fireEvent.keyDown(document.body, { key: "d" });
    expect(screen.getByTestId("turn-bar-menu-trade")).toBeInTheDocument();
    expect(screen.getByTestId("turn-bar-open-hacan")).toHaveFocus();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByTestId("turn-bar-menu-trade")).toBeNull();
    fireEvent.keyDown(document.body, { key: "c" });
    expect(screen.getByTestId("turn-bar-menu-components")).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "a" });
    expect(screen.getByTestId("turn-bar-menu-cards")).toBeInTheDocument();
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("P passes and S submits the only strategy card", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const one = turnChoice(
      [
        opt("strategic|pok2diplomacy", "take the strategic action of 2. Diplomacy"),
        opt("pass", "pass", "pass"),
      ],
      { strategy_cards: [twoCards[0]] },
    );
    render(<TurnActionBar model={modelOf(one)} onSubmit={onSubmit} />);
    await act(async () => {
      fireEvent.keyDown(document.body, { key: "s" });
    });
    expect(onSubmit).toHaveBeenLastCalledWith("strategic|pok2diplomacy");
    await act(async () => {
      fireEvent.keyDown(document.body, { key: "P" });
    });
    expect(onSubmit).toHaveBeenLastCalledWith("pass");
  });

  it("with several strategy cards S cycles focus and Enter on the focused card does not end the turn", async () => {
    const onSubmit = vi.fn();
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    const first = screen.getByTestId("turn-bar-strategic-pok2diplomacy");
    const second = screen.getByTestId("turn-bar-strategic-pok8imperial");
    fireEvent.keyDown(document.body, { key: "s" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: "s" });
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: "s" });
    expect(first).toHaveFocus();
    expect(onSubmit).not.toHaveBeenCalled();
    fireEvent.keyDown(first, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
    await click(first);
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("strategic|pok2diplomacy");
    expect(screen.getByLabelText("Keyboard shortcuts")).toHaveTextContent("S cycles");
  });

  it("disabled buttons ignore their key and flash the reason", async () => {
    const onSubmit = vi.fn();
    const closing = turnChoice([opt("end_turn", "end your turn", "end_turn")], { closing: true });
    render(<TurnActionBar model={modelOf(closing)} onSubmit={onSubmit} />);
    for (const k of ["t", "p", "s"]) fireEvent.keyDown(document.body, { key: k });
    for (const k of ["c", "a", "d"]) fireEvent.keyDown(document.body, { key: k });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.queryByTestId("turn-bar-menu-trade")).toBeNull();
    expect(screen.getByTestId("turn-bar-notice")).toBeInTheDocument();
  });

  it("ignores keys while typing, with modifiers, on repeat, inside a dialog", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <>
        <input data-testid="field" />
        <div role="dialog" aria-modal="false">
          <button data-testid="in-dialog">x</button>
        </div>
        <TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />
      </>,
    );
    fireEvent.keyDown(screen.getByTestId("field"), { key: "t" });
    fireEvent.keyDown(document.body, { key: "t", ctrlKey: true });
    fireEvent.keyDown(document.body, { key: "t", metaKey: true });
    fireEvent.keyDown(document.body, { key: "t", altKey: true });
    fireEvent.keyDown(document.body, { key: "t", repeat: true });
    await act(async () => {
      screen.getByTestId("in-dialog").focus();
    });
    fireEvent.keyDown(screen.getByTestId("in-dialog"), { key: "t" });
    expect(onSubmit).not.toHaveBeenCalled();
    await act(async () => {
      (document.activeElement as HTMLElement).blur();
    });
    fireEvent.keyDown(document.body, { key: "t" });
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("tactical");
  });

  it("ignores letter keys when read-only", async () => {
    const onSubmit = vi.fn();
    const player = {
      id: "b", passed: false, strategy_cards: ["pok2diplomacy"], exhausted_strategy_cards: [],
      tactic_tokens: 3, fleet_tokens: 3, strategic_tokens: 2, action_cards_count: 1,
    } as unknown as PlayerView;
    render(<TurnActionBar model={deriveReadOnlyTurnBar(player, "a")} onSubmit={onSubmit} />);
    for (const k of ["t", "p", "s", "c", "a", "d"]) fireEvent.keyDown(document.body, { key: k });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("shows a rejected submission and lets the player try again", async () => {
    const onSubmit = vi.fn().mockRejectedValueOnce(new Error("stale decision")).mockResolvedValue(undefined);
    render(<TurnActionBar model={modelOf(fullMenu())} onSubmit={onSubmit} />);
    await click(screen.getByTestId("turn-bar-tactical"));
    expect(screen.getByTestId("turn-bar-error")).toHaveTextContent("stale decision");
    await click(screen.getByTestId("turn-bar-tactical"));
    expect(onSubmit).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId("turn-bar-error")).toBeNull();
  });

  it("is read-only when it is not your turn: every button inert with the reason, no hints", async () => {
    const onSubmit = vi.fn();
    const player = {
      id: "b",
      passed: false,
      strategy_cards: ["pok2diplomacy", "pok8imperial"],
      exhausted_strategy_cards: [],
      tactic_tokens: 3,
      fleet_tokens: 3,
      strategic_tokens: 2,
      action_cards_count: 1,
    } as unknown as PlayerView;
    render(<TurnActionBar model={deriveReadOnlyTurnBar(player, "a")} onSubmit={onSubmit} />);
    expect(screen.getByTestId("turn-action-bar")).toHaveAttribute("data-mode", "readonly");
    expect(screen.getByTestId("turn-bar-heading")).toHaveTextContent(/^Waiting for/);
    for (const id of ["turn-bar-tactical", "turn-bar-pass", "turn-bar-end", "turn-bar-trade"]) {
      expect(screen.getByTestId(id)).toHaveAttribute("aria-disabled", "true");
      expect(screen.getByTestId(id)).toHaveTextContent("Not your turn");
      await click(screen.getByTestId(id));
    }
    expect(screen.getByTestId("turn-bar-strategic-pok2diplomacy")).toHaveTextContent("Not your turn");
    expect(screen.queryByLabelText("Keyboard shortcuts")).toBeNull();
    fireEvent.keyDown(document.body, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();
    // The seat can still read its own cards.
    fireEvent.mouseEnter(screen.getByTestId("turn-bar-strategic-pok2diplomacy"));
    expect(screen.getByTestId("turn-bar-info")).toHaveTextContent("Diplomacy");
  });
});

describe("the bar through the renderer", () => {
  const me = {
    id: "a",
    passed: false,
    strategy_cards: ["pok2diplomacy", "pok8imperial"],
    exhausted_strategy_cards: [],
    tactic_tokens: 3,
    fleet_tokens: 3,
    strategic_tokens: 2,
    action_cards_count: 1,
  } as unknown as PlayerView;

  it("replaces the turn-menu modal and submits the engine option id", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <ChoiceRendererDispatcher
        choice={fullMenu()}
        viewerSeat="a"
        players={{ a: me }}
        onSubmit={onSubmit}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("turn-action-bar")).toBeInTheDocument();
    expect(screen.queryByTestId("decision-modal")).toBeNull();
    await click(screen.getByTestId("turn-bar-strategic-pok2diplomacy"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("strategic|pok2diplomacy");
  });

  it("keeps the old list for options the bar cannot place", () => {
    const odd = turnChoice([opt("mystery", "do something", "component")], {
      strategy_cards: [],
      partners: [],
    });
    render(
      <ChoiceRendererDispatcher
        choice={odd}
        viewerSeat="a"
        players={{ a: me }}
        onSubmit={vi.fn()}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("turn-action-bar")).toBeNull();
    expect(screen.getByText("do something")).toBeInTheDocument();
  });

  it("shows the read-only bar to a seat waiting for another player's turn", () => {
    const theirs: PendingChoiceDto = { ...fullMenu(), actor: "b" };
    render(
      <ChoiceRendererDispatcher
        choice={theirs}
        viewerSeat="a"
        players={{ a: me }}
        turn={{ phase: "action", activePlayer: "b" }}
        onSubmit={vi.fn()}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("turn-action-bar")).toHaveAttribute("data-mode", "readonly");
  });

  it("shows no bar outside the action phase", () => {
    render(
      <ChoiceRendererDispatcher
        choice={null}
        viewerSeat="a"
        players={{ a: me }}
        turn={{ phase: "strategy", activePlayer: "b" }}
        onSubmit={vi.fn()}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("turn-action-bar")).toBeNull();
  });
});
