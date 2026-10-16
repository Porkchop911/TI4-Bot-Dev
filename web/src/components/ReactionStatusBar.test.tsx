import { act } from "react";
import { beforeEach, describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ReactionStatusBar } from "./ReactionStatusBar.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";
import { fallbackCases } from "../dev/decisionGalleryCases.ts";
import { COMPACT_KEY, SHRUNK_KEY } from "../presentation/cardTextPrefs.ts";
import { partialContext } from "../test/wire.ts";

describe("ReactionStatusBar", () => {
  it("renders reaction card options and handles click", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "40",
      prompt: "Play Sabotage to cancel Action Card?",
      context: partialContext({
        subtype: "play_reaction_when_action_card_played",
      }),
      options: [
        { id: "sabotage", label: "Sabotage", kind: "reaction", payload: {} },
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByTestId("reaction-bar-prompt")).toHaveTextContent(
      "A reaction window opened: when an action card is played.",
    );
    expect(screen.queryByText(/play_reaction|ACTION_CARD_PLAYED/)).toBeNull();
    const playBtn = screen.getByTestId("play-reaction-btn-sabotage");
    expect(playBtn).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(playBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("sabotage");
  });

  it("offers every card of a window with more than four options, with names and pass", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const ids = ["sabotage", "shard_of_the_throne", "direct_hit", "skilled_retreat", "counterstroke", "reflective_shielding"];
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "42",
      prompt: "reaction_after_SYSTEM_ACTIVATED",
      context: partialContext({ subtype: "reaction_after_SYSTEM_ACTIVATED", optional: true, source: { Reaction: "SYSTEM_ACTIVATED" } }),
      options: [
        ...ids.map((id) => ({ id, label: `Play ${id}`, kind: "reaction", payload: {} })),
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };
    render(<ReactionStatusBar isOpen={true} choice={choice} viewerSeat="seat_1" onSubmit={onSubmit} />);
    for (const id of ids) expect(screen.getByTestId(`play-reaction-btn-${id}`)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByTestId("pass-reaction-btn"));
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("the card-pick step of a multi-card window shows a Back button that declines without playing", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "43",
      prompt: "play an action card (when HITS_TO_ASSIGN)",
      context: partialContext({ subtype: "play_reaction_when_HITS_TO_ASSIGN", optional: true }),
      options: [
        { id: "sh1", label: "play Shields Holding", kind: "action_card", payload: { card: "sh1", card_name: "Shields Holding" } },
        { id: "dh1", label: "play Direct Hit", kind: "action_card", payload: { card: "dh1", card_name: "Direct Hit" } },
        { id: "decline", label: "Decline", kind: "decline", payload: {} },
      ],
    };
    render(<ReactionStatusBar isOpen={true} choice={choice} viewerSeat="seat_1" onSubmit={onSubmit} />);
    expect(screen.getByTestId("play-reaction-btn-sh1")).toBeInTheDocument();
    expect(screen.getByTestId("play-reaction-btn-dh1")).toBeInTheDocument();
    const back = screen.getByTestId("pass-reaction-btn");
    expect(back).toHaveTextContent(/Back/);
    expect(screen.queryByTestId("play-reaction-btn-decline")).toBeNull();
    await act(async () => {
      fireEvent.click(back);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
    expect(onSubmit).not.toHaveBeenCalledWith("sh1");
  });

  it("handles pass button click", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "41",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage", payload: {}, kind: "" },
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    const passBtn = screen.getByTestId("pass-reaction-btn");
    await act(async () => {
      fireEvent.click(passBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("triggers pass on Spacebar keypress", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "42",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage", payload: {}, kind: "" },
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    await act(async () => {
      fireEvent.keyDown(window, { key: " ", code: "Space" });
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("triggers reaction play on Enter keypress", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "43",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage", payload: {}, kind: "" },
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    await act(async () => {
      fireEvent.keyDown(window, { key: "Enter", code: "Enter" });
    });
    expect(onSubmit).toHaveBeenCalledWith("sabotage");
  });

  it("displays spectator notice for non-active viewer", () => {
    const onSubmit = vi.fn();

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "44",
      prompt: "Reaction opportunity",
      options: [{ id: "sabotage", label: "Sabotage", payload: {}, kind: "" }],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByTestId("spectator-reaction-notice")).toHaveTextContent(
      "Waiting for reactions...",
    );
  });

  it("displays error badge when lastError is provided", () => {
    const onSubmit = vi.fn();

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "45",
      prompt: "Reaction opportunity",
      options: [{ id: "decline", label: "Pass", payload: {}, kind: "" }],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        lastError="Reaction window expired"
      />,
    );

    expect(screen.getByTestId("reaction-error-badge")).toHaveTextContent("Reaction window expired");
  });

  it("M14: supports action card descriptions in option payload", () => {
    const onSubmit = vi.fn();

    // M14: Action cards can now include description field for trigger information
    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "46",
      prompt: "React with action card?",
      context: partialContext({
        subtype: "play_reaction_when_action_card_played",
      }),
      options: [
        {
          id: "decoy_operation",
          label: "Decoy Operation",
          kind: "reaction",
          payload: {},
        },
        { id: "decline", label: "Pass", kind: "decline", payload: {} },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    // Action card button should render the label
    const cardButton = screen.getByTestId("play-reaction-btn-decoy_operation");
    expect(cardButton).toBeInTheDocument();
    expect(cardButton).toHaveTextContent("Decoy Operation");
  });

  it("Phase 0: names the card, shows its text, no doubled verb, no viewer as trigger", () => {
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "50",
      prompt: "when ACTION_CARD_PLAYED",
      context: partialContext({
        subtype: "reaction_when_ACTION_CARD_PLAYED",
        optional: true,
        source: { Reaction: "ACTION_CARD_PLAYED" },
      }),
      options: [
        {
          id: "reaction:Sol:ACTION_CARD_PLAYED:when",
          kind: "ability",
          label: "Play Sabotage",
          payload: { card: "sabo1", card_name: "Sabotage" },
        },
        {
          id: "inner",
          kind: "action_card",
          label: "play Decoy Operation",
          payload: { card: "decoy", card_name: "Decoy Operation" },
        },
        { id: "decline", kind: "decline", label: "Pass", payload: {} },
      ],
    };
    render(
      <ReactionStatusBar
        isOpen
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        events={[
          {
            visibility: { visibility: "public" as const },
            id: "e1",
            timestamp: "t",
            actor: "seat_2",
            detail: "seat_2 played Mining Initiative",
            event: { kind: "decision_resolved" },
          },
        ]}
      />,
    );
    const bar = screen.getByTestId("reaction-status-bar");
    expect(bar).not.toHaveTextContent(/Play play/i);
    expect(bar).not.toHaveTextContent(/Triggered by/);
    expect(screen.getByTestId("play-reaction-btn-reaction:Sol:ACTION_CARD_PLAYED:when")).toHaveTextContent(
      /^Play Sabotage$/,
    );
    expect(bar).toHaveTextContent("Cancel that action card.");
    expect(screen.getByTestId("reaction-bar-prompt")).toHaveTextContent(
      "played the action card Mining Initiative.",
    );
    expect(screen.getByTestId("reaction-can-now")).toHaveTextContent(
      "Before this resolves, you can play Sabotage or play Decoy Operation.",
    );
  });

  it("shows a faction ability or leader reaction by its printed name, with no raw engine id", () => {
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "55",
      prompt: "after SYSTEM_ACTIVATED",
      context: partialContext({
        subtype: "reaction_after_SYSTEM_ACTIVATED",
        optional: true,
        source: { Reaction: "SYSTEM_ACTIVATED" },
      }),
      options: [
        {
          id: "ability:empyrean:aetherpassage:SYSTEM_ACTIVATED:after",
          kind: "ability",
          label: "Aetherpassage (after system activated)",
          payload: {},
        },
        {
          id: "leader:l1z1x:l1z1xagent:SYSTEM_ACTIVATED:after",
          kind: "ability",
          label: "Agent I48S (after system activated)",
          payload: {},
        },
        { id: "decline", kind: "decline", label: "Pass", payload: {} },
      ],
    };
    render(<ReactionStatusBar isOpen choice={choice} viewerSeat="seat_1" onSubmit={vi.fn()} events={[]} />);
    const bar = screen.getByTestId("reaction-status-bar");
    expect(bar).toHaveTextContent("Aetherpassage");
    expect(bar).toHaveTextContent("I48S");
    expect(bar.textContent ?? "").not.toMatch(/\b[A-Z]{3,}_[A-Z_]{3,}\b/);
  });

  describe("Phase 2: typed trigger, card text controls", () => {
    const offer = (): PendingChoiceDto => ({
      actor: "seat_1",
      nonce: "60",
      prompt: "when ACTION_CARD_PLAYED",
      context: partialContext({
        subtype: "reaction_when_ACTION_CARD_PLAYED",
        optional: true,
        source: { Reaction: "ACTION_CARD_PLAYED" },
        trigger: {
          kind: "action_card_played",
          event_type: "ACTION_CARD_PLAYED",
          event_id: 3,
          relation: "when",
          actor: "seat_2",
          card: "uprising",
        },
      }),
      options: [
        {
          id: "reaction:Sol:ACTION_CARD_PLAYED:when",
          kind: "ability",
          label: "Play Sabotage",
          payload: { card: "sabo1", card_name: "Sabotage" },
        },
        { id: "decline", kind: "decline", label: "Pass", payload: {} },
      ],
    });

    beforeEach(() => {
      window.localStorage.clear();
    });

    it("says who played which card with its full text, then what you can do now", () => {
      render(
        <ReactionStatusBar isOpen choice={offer()} viewerSeat="seat_1" onSubmit={vi.fn()} />,
      );
      expect(screen.getByTestId("reaction-trigger-context")).toHaveAttribute(
        "data-trigger-source",
        "trigger",
      );
      expect(screen.getByTestId("reaction-bar-prompt")).toHaveTextContent(
        "played the action card Uprising.",
      );
      expect(screen.getByTestId("reaction-trigger-card")).toHaveTextContent(
        /Uprising.*Exhaust 1 non-home planet/s,
      );
      expect(screen.getByTestId("reaction-can-now")).toHaveTextContent(
        "Before this resolves, you can play Sabotage.",
      );
      expect(screen.getByTestId("reaction-card-window")).toHaveTextContent(
        "Window: When another player plays an action card",
      );
    });

    it("names the planet and player an action card targets, and the discarded card of a Reverse Engineer window", () => {
      const played = offer();
      played.context!.trigger = { ...played.context!.trigger!, card: "uprising", system: "27", planet: "mecatol_rex", subject: "seat_3" };
      const { unmount } = render(
        <ReactionStatusBar isOpen choice={played} viewerSeat="seat_1" onSubmit={vi.fn()} />,
      );
      expect(screen.getByTestId("reaction-trigger-targets")).toHaveTextContent("Target: planet Mecatol Rex, player");
      expect(screen.getByTestId("reaction-trigger-system-27")).toBeInTheDocument();
      unmount();
      const discarded = offer();
      discarded.context!.trigger = { kind: "action_card_discarded", event_type: "ACTION_CARD_DISCARDED", event_id: 5, relation: "after", actor: "seat_2", card: "uprising" };
      render(<ReactionStatusBar isOpen choice={discarded} viewerSeat="seat_1" onSubmit={vi.fn()} />);
      expect(screen.getByTestId("reaction-bar-prompt")).toHaveTextContent("discarded the action card Uprising.");
      expect(screen.getByTestId("reaction-trigger-card")).toHaveTextContent(/Uprising.*Exhaust 1 non-home planet/s);
      expect(screen.queryByTestId("reaction-trigger-targets")).toBeNull();
    });

    it("shrinks one card to its first sentence, remembers it per card, and can show it again", () => {
      const first = render(
        <ReactionStatusBar isOpen choice={offer()} viewerSeat="seat_1" onSubmit={vi.fn()} />,
      );
      const trigger = screen.getByTestId("reaction-trigger-card");
      const full = trigger.querySelector("p")!.textContent!;
      expect(full.length).toBeGreaterThan(40);
      fireEvent.click(screen.getByTestId("reaction-inspect-text-Uprising"));
      const shrunk = screen.getByTestId("reaction-trigger-card").querySelector("p")!.textContent!;
      expect(shrunk.length).toBeLessThan(full.length);
      expect(full.startsWith(shrunk)).toBe(true);
      expect(screen.getByTestId("reaction-inspect-text-Uprising")).toHaveTextContent(
        "Show full text",
      );
      expect(JSON.parse(window.localStorage.getItem(SHRUNK_KEY)!)).toEqual(["Uprising"]);
      first.unmount();
      // A new session: the same card comes back compact, the other card stays full.
      render(<ReactionStatusBar isOpen choice={offer()} viewerSeat="seat_1" onSubmit={vi.fn()} />);
      expect(screen.getByTestId("reaction-trigger-card").querySelector("p")!.textContent).toBe(
        shrunk,
      );
      fireEvent.click(screen.getByTestId("reaction-inspect-text-Uprising"));
      expect(screen.getByTestId("reaction-trigger-card").querySelector("p")!.textContent).toBe(
        full,
      );
      expect(JSON.parse(window.localStorage.getItem(SHRUNK_KEY)!)).toEqual([]);
    });

    it("new players see every card in full; the compact switch collapses all of them", () => {
      render(<ReactionStatusBar isOpen choice={offer()} viewerSeat="seat_1" onSubmit={vi.fn()} />);
      const before = screen.getByTestId("reaction-trigger-card").querySelector("p")!.textContent!;
      expect(screen.getByTestId("reaction-inspect-compact")).not.toBeChecked();
      fireEvent.click(screen.getByTestId("reaction-inspect-compact"));
      expect(window.localStorage.getItem(COMPACT_KEY)).toBe("1");
      expect(
        screen.getByTestId("reaction-trigger-card").querySelector("p")!.textContent!.length,
      ).toBeLessThan(before.length);
    });

    it("still renders when localStorage throws", () => {
      const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
        throw new Error("blocked");
      });
      render(<ReactionStatusBar isOpen choice={offer()} viewerSeat="seat_1" onSubmit={vi.fn()} />);
      expect(screen.getByTestId("play-reaction-btn-reaction:Sol:ACTION_CARD_PLAYED:when")).toBeVisible();
      spy.mockRestore();
    });

    it("links the system involved to the map without blocking Pass", async () => {
      const onShowSystem = vi.fn();
      const onSubmit = vi.fn().mockResolvedValue(undefined);
      const choice = offer();
      choice.context!.trigger = {
        kind: "system_activated",
        event_type: "SYSTEM_ACTIVATED",
        event_id: 5,
        relation: "after",
        actor: "seat_2",
        system: "27",
      };
      choice.context!.subtype = "reaction_after_SYSTEM_ACTIVATED";
      render(
        <ReactionStatusBar
          isOpen
          choice={choice}
          viewerSeat="seat_1"
          onSubmit={onSubmit}
          onShowSystem={onShowSystem}
        />,
      );
      fireEvent.click(screen.getByTestId("reaction-inspect-show-on-map"));
      expect(onShowSystem).toHaveBeenCalledWith("27");
      expect(screen.getByTestId("reaction-trigger-system-27")).toBeVisible();
      await act(async () => {
        fireEvent.click(screen.getByTestId("pass-reaction-btn"));
      });
      expect(onSubmit).toHaveBeenCalledWith("decline");
    });

    it("every gallery reaction case says what happened without engine ids or doubled verbs", () => {
      const cases = fallbackCases.filter((item) => item.workflow === "action_card_reaction");
      expect(cases.length).toBeGreaterThanOrEqual(8);
      for (const item of cases) {
        const { unmount } = render(
          <ReactionStatusBar
            isOpen
            choice={item.choice}
            viewerSeat={item.choice.actor}
            onSubmit={vi.fn()}
          />,
        );
        const bar = screen.getByTestId("reaction-status-bar");
        const text = bar.textContent ?? "";
        expect(text, item.title).not.toMatch(/\b[A-Z]{3,}_[A-Z_]{3,}\b/);
        expect(text, item.title).not.toMatch(/Play play/i);
        expect(screen.getByTestId("reaction-trigger-context"), item.title).toBeVisible();
        expect(screen.getByTestId("pass-reaction-btn"), item.title).toBeVisible();
        for (const option of item.choice.options.filter((o) => o.kind !== "decline")) {
          expect(screen.getByTestId(`play-reaction-btn-${option.id}`), item.title).toBeVisible();
        }
        unmount();
      }
    });
  });

  describe("per-card never offer", () => {
    const sabotage: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "60",
      prompt: "when ACTION_CARD_PLAYED",
      context: partialContext({
        subtype: "reaction_when_ACTION_CARD_PLAYED",
        optional: true,
        source: { Reaction: "ACTION_CARD_PLAYED" },
      }),
      options: [
        {
          id: "reaction:Sol:ACTION_CARD_PLAYED:when",
          kind: "ability",
          label: "Play Sabotage",
          payload: { card: "sabo1", card_name: "Sabotage" },
        },
        { id: "decline", kind: "decline", label: "Pass", payload: {} },
      ],
    };

    it("sets the mode on the server and passes this window when it is the only card", async () => {
      const onSubmit = vi.fn().mockResolvedValue(undefined);
      const onSetReactionMode = vi.fn();
      render(
        <ReactionStatusBar
          isOpen
          choice={sabotage}
          viewerSeat="seat_1"
          onSubmit={onSubmit}
          onSetReactionMode={onSetReactionMode}
        />,
      );
      await act(async () => {
        fireEvent.click(screen.getByTestId("reaction-inspect-never-Sabotage"));
      });
      expect(onSetReactionMode).toHaveBeenCalledWith("Sabotage", "never");
      expect(onSubmit).toHaveBeenCalledWith("decline");
    });

    it("shows the server's state and offers the card again on untick, without passing", async () => {
      const onSubmit = vi.fn().mockResolvedValue(undefined);
      const onSetReactionMode = vi.fn();
      render(
        <ReactionStatusBar
          isOpen
          choice={sabotage}
          viewerSeat="seat_1"
          onSubmit={onSubmit}
          reactionModes={{ Sabotage: "never" }}
          onSetReactionMode={onSetReactionMode}
        />,
      );
      const box = screen.getByTestId("reaction-inspect-never-Sabotage");
      expect(box).toBeChecked();
      await act(async () => {
        fireEvent.click(box);
      });
      expect(onSetReactionMode).toHaveBeenCalledWith("Sabotage", "always");
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("has no control for a viewer who cannot change modes, and keeps the play and pass ids", () => {
      render(<ReactionStatusBar isOpen choice={sabotage} viewerSeat="seat_1" onSubmit={vi.fn()} />);
      expect(screen.queryByTestId("reaction-inspect-never-Sabotage")).toBeNull();
      expect(
        screen.getByTestId("play-reaction-btn-reaction:Sol:ACTION_CARD_PLAYED:when"),
      ).toBeInTheDocument();
      expect(screen.getByTestId("pass-reaction-btn")).toBeInTheDocument();
    });
  });
});
