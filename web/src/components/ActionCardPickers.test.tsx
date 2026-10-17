import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { BoardView, LobbyDto, PendingChoiceDto } from "../protocol/types.ts";

const ME = "player_me";
const RIVAL = "player_rival";
const lobby: LobbyDto = {
  game_id: "g",
  phase: "running",
  lobby_version: 1,
  host_player_id: ME,
  slots: [
    { slot_id: "s1", position: 1, occupant: ME, nickname: "Ana", connected: true, can_take_over: false },
    { slot_id: "s2", position: 2, occupant: RIVAL, nickname: "Bo", connected: true, can_take_over: false },
  ],
};

const show = (choice: PendingChoiceDto, board?: BoardView, onSubmit = vi.fn(async () => {})) => {
  render(
    <PlayerIdentityProvider lobby={lobby} seatingOrder={[ME, RIVAL]}>
      <PendingChoiceModal choice={choice} onSubmit={onSubmit} boardView={board} />
    </PlayerIdentityProvider>,
  );
  return onSubmit;
};

const base = { actor: ME, nonce: "n1" };

describe("Plagiarize", () => {
  it("shows each technology by name and who owns it, not the seat|tech id", () => {
    show({
      ...base,
      prompt: "Plagiarize: which technology to steal",
      context: { subtype: "plagiarize_pick_technology", source: { ActionCard: "plagiarize" } } as never,
      options: [
        { id: `${RIVAL}|det`, kind: "technology", label: `${RIVAL}'s Dark Energy Tap`, payload: {} },
        { id: `${RIVAL}|md_base`, kind: "technology", label: `${RIVAL}'s Magen Defense Grid`, payload: {} },
      ],
    });
    const rows = screen.getAllByTestId("choice-option");
    expect(within(rows[0]).getByText("Dark Energy Tap")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("technology-pick-owner")).toHaveTextContent("Owned by");
    expect(within(rows[0]).getByTestId("technology-pick-owner")).toHaveTextContent("Bo");
    expect(screen.queryByText(/player_rival/)).toBeNull();
    expect(screen.getByTestId("technology-pick-panel")).toHaveTextContent("Steal a technology");
  });
});

describe("Lucky Shot", () => {
  const lucky: PendingChoiceDto = {
    ...base,
    prompt: "Lucky Shot: destroy which ship",
    context: { subtype: "lucky_pick_ship", source: { ActionCard: "lucky" } } as never,
    options: [
      { id: "31|0", kind: "ship", label: `destroy ${RIVAL}'s cruiser in 31`, payload: { system: "31" } },
      { id: "31|2", kind: "ship", label: `destroy ${RIVAL}'s dreadnought in 31`, payload: { system: "31" } },
    ],
    details: {
      units: {
        "31|0": { system: "31", planet: null, unit: "cruiser", owner: RIVAL, damaged: false, cost: 2 },
        "31|2": { system: "31", planet: null, unit: "dreadnought2", owner: RIVAL, damaged: true, cost: 4 },
      },
    },
  };

  it("names the ship, its owner and the system, instead of the raw sentence", () => {
    show(lucky);
    const rows = screen.getAllByTestId("choice-option");
    expect(within(rows[1]).getAllByText("Dreadnought II (damaged)").length).toBeGreaterThan(0);
    const note = within(rows[1]).getByTestId("unit-pick-note");
    expect(note).toHaveTextContent("Bo");
    expect(note).toHaveTextContent("31");
    expect(note).toHaveTextContent("Destroyed");
    expect(screen.queryByText(/player_rival/)).toBeNull();
  });
});

describe("Public Disgrace, Overrule and Strategize", () => {
  it("lists Overrule's cards in the strategy-card grid with who holds each", () => {
    show({
      ...base,
      prompt: "Overrule: which card's primary ability to perform",
      context: { subtype: "overrule_pick_strategy card", source: { ActionCard: "overrule" } } as never,
      options: [
        { id: "te4construction", kind: "strategy card", label: `${RIVAL}'s Construction`, payload: {} },
        { id: "pok8imperial", kind: "strategy card", label: "Imperial", payload: {} },
      ],
      details: { holders: { te4construction: RIVAL, pok8imperial: null } },
    });
    const rows = screen.getAllByTestId("choice-option");
    expect(rows[0]).toHaveClass("strategy-draft-card");
    expect(within(rows[0]).getByTestId("strategy-card-holder")).toHaveTextContent("Held by");
    expect(within(rows[0]).getByTestId("strategy-card-holder")).toHaveTextContent("Bo");
    expect(within(rows[1]).getByTestId("strategy-card-holder")).toHaveTextContent("Unchosen");
    expect(within(rows[0]).getAllByTestId("strategy-performed")).toHaveLength(1);
    expect(screen.getByTestId("strategy-card-pick-panel")).toHaveTextContent("primary ability");
    // Trade goods on a card are only collected by taking it: Overrule does not.
    expect(screen.queryByTestId("strategy-goods-badge")).toBeNull();
    expect(screen.queryByText(/player_rival/)).toBeNull();
  });

  it("tells the player who made them choose again and which card they lost", () => {
    show({
      ...base,
      prompt: "choose a different strategy card",
      context: { subtype: "public_disgrace_choose_card", source: { ActionCard: "public_disgrace" } } as never,
      options: [
        { id: "pok3politics", kind: "strategy_card", label: "3. Politics", payload: {} },
        { id: "pok4construction", kind: "strategy_card", label: "4. Construction", payload: {} },
      ],
      details: { first: "pok5trade", played_by: RIVAL },
    });
    expect(screen.getAllByTestId("choice-option")[0]).toHaveClass("strategy-draft-card");
    const panel = screen.getByTestId("strategy-card-pick-panel");
    expect(panel).toHaveTextContent("Bo");
    expect(panel).toHaveTextContent("Trade");
  });
});

describe("Repeal Law", () => {
  it("shows each law by name with what it does and what it was elected onto", () => {
    show({
      ...base,
      prompt: "repeal which law",
      context: { subtype: "repeal_pick_repeal", source: { ActionCard: "repeal" } } as never,
      options: [
        { id: "checks", kind: "repeal", label: "checks", payload: {} },
        { id: "minister_commerce", kind: "repeal", label: "minister_commerce", payload: {} },
      ],
      details: {
        laws: {
          checks: { name: "Checks and Balances", type: "Law", target: "For/Against", text1: "For: a.", text2: "Against: b.", outcome: "for" },
          minister_commerce: { name: "Minister of Commerce", type: "Law", target: "Elect Player", text1: "Gains a trade good.", text2: "", outcome: RIVAL },
        },
      },
    });
    const rows = screen.getAllByTestId("choice-option");
    expect(within(rows[0]).getByText("Checks and Balances")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("law-passed")).toHaveTextContent("Passed: For");
    expect(within(rows[0]).getByTestId("law-pick-text")).toHaveTextContent("For: a.");
    expect(within(rows[1]).getByTestId("law-elected")).toHaveTextContent(/Elected: .*Bo/);
    expect(screen.queryByText("minister_commerce")).toBeNull();
  });
});

describe("Fire Team", () => {
  const fireTeam = (current: number): PendingChoiceDto => ({
    ...base,
    nonce: `n${current}`,
    prompt: "reroll die 1 of infantry",
    context: { subtype: "fire_team_reroll", source: { ActionCard: "fire_team" } } as never,
    options: [
      { id: `reroll|0:${current}`, kind: "reroll_die", label: "reroll", payload: {} },
      { id: "decline", kind: "decline", label: "Decline", payload: {} },
    ],
    details: {
      current,
      dice: [
        { unit: "infantry", unit_index: 0, die: 0, face: 3, bonus: 0, hits_on: 8, rerolled: false },
        { unit: "infantry", unit_index: 0, die: 1, face: 9, bonus: 0, hits_on: 8, rerolled: false },
      ],
    },
  });

  it("shows the dice instead of one radio per die", () => {
    show(fireTeam(0));
    expect(screen.getByTestId("reroll-panel")).toBeInTheDocument();
    expect(screen.getAllByTestId("reroll-die")).toHaveLength(2);
    expect(screen.queryByTestId("choice-option")).toBeNull();
    expect(screen.queryByTestId("submit-choice-button")).toBeNull();
  });
});

describe("Ghost Squad", () => {
  it("shows moves between planets with counts, and answers with the move", async () => {
    const onSubmit = show(
      {
        ...base,
        prompt: "Ghost Squad: move ground forces in 30",
        context: { subtype: "ghost_squad_move", source: { ActionCard: "ghost_squad" } } as never,
        options: [
          { id: "move|mellon|zohbat|infantry", kind: "move", label: "move 1 infantry from mellon to zohbat", payload: { from: "mellon", to: "zohbat", unit: "infantry", count: 1 } },
          { id: "decline", kind: "decline", label: "Decline", payload: {} },
        ],
      },
      {
        systems: {
          "30": {
            system_id: "30",
            command_tokens: [],
            planets: {},
            units: [{ unit_type: "infantry", owner: ME, planet: "mellon", damaged: false, galvanized: false }],
          },
        },
      },
    );
    expect(screen.getByTestId("ground-mover")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("mover-move"));
    await Promise.resolve();
    expect(onSubmit).toHaveBeenCalledWith("move|mellon|zohbat|infantry");
  });
});

describe("Bribery", () => {
  it("steps a number of trade goods instead of listing 0..N", () => {
    show({
      ...base,
      prompt: "Bribery: how many trade goods to spend?",
      context: { subtype: "bribery_pick_count", source: { ActionCard: "bribery" } } as never,
      options: [0, 1, 2].map((n) => ({ id: String(n), kind: "count", label: `spend ${n} trade goods`, payload: {} })),
    });
    expect(screen.getByTestId("number-stepper")).toBeInTheDocument();
    expect(screen.queryByTestId("choice-option")).toBeNull();
  });
});

describe("Lie in Wait and Seize Artifact", () => {
  it("shows the laid-open cards by name and text and says whose hand", () => {
    show({
      ...base,
      prompt: "Lie in Wait: take which action card",
      context: { subtype: "lieinwait_pick_action_card", source: { ActionCard: "lieinwait" } } as never,
      options: [
        { id: "spy", kind: "action_card", label: `take spy from ${RIVAL}`, payload: {} },
        { id: "bribery", kind: "action_card", label: `take bribery from ${RIVAL}`, payload: {} },
      ],
      details: { from: RIVAL },
    });
    const rows = screen.getAllByTestId("choice-option");
    expect(within(rows[0]).getByText("Spy")).toBeInTheDocument();
    expect(within(rows[1]).getByTestId("card-option-text")).toHaveTextContent("additional vote");
    expect(screen.getByText(/You looked at Bo's hand/)).toBeInTheDocument();
    expect(screen.getByTestId("submit-choice-button")).toHaveTextContent("Take card");
  });
});
