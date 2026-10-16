import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameLogEntry } from "../protocol/client.ts";
import { EventLog, HISTORY_BUSY_REASON, buildEventTree } from "./EventLog.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto } from "../protocol/types.ts";

const decision = (cursor: number, stage = "movement", actor = "p1"): GameLogEntry => ({
  id: `event-${cursor}`,
  timestamp: "12:00",
  visibility: { visibility: "public" as const },
  decision_count: cursor,
  round: 1,
  phase: "action" as const,
  action_id: "action_1",
  action_type: "tactical",
  actor,
  stage,
  detail: `Choice ${cursor}`,
  version: 3,
  event: { kind: "decision_resolved" },
});
const wrap = (
  events: GameLogEntry[],
  props: Partial<React.ComponentProps<typeof EventLog>> = {},
) => (
  <PlayerIdentityProvider lobby={null} seatingOrder={["p1", "p2"]}>
    <EventLog events={events} isOpen onToggle={vi.fn()} cursor={100} {...props} />
  </PlayerIdentityProvider>
);

const rolled = (cursor: number, faces: number[], hitsOn?: number): GameLogEntry => ({
  id: `roll-${cursor}-${faces.join("")}`,
  timestamp: "12:00",
  visibility: { visibility: "public" as const },
  decision_count: cursor,
  round: 1,
  phase: "action" as const,
  actor: "p1",
  detail: "p1 rolled Gravity rift in system 67: 2 (ship destroyed)",
  event: { kind: "dice_rolled" },
  roll: {
    index: 0,
    roller: "p1",
    reason: "gravity rift",
    label: "Gravity rift",
    faces,
    hits_on: hitsOn,
    system: "67",
    outcome: "ship destroyed",
  },
});

describe("dice rolls in the event log", () => {
  it("lists a roll as its own line with the faces, hits marked, and nothing to undo", () => {
    const entries = [decision(1), rolled(1, [2, 9], 4), decision(2)];
    render(
      wrap(entries, {
        onRestore: vi.fn(),
        currentPath: { round: 1, phase: "action" as const, action_id: "action_1", stage: "movement" },
      }),
    );
    const row = screen.getByTestId("event-log-roll");
    expect(row).toHaveTextContent("Gravity rift in system 67");
    expect(row).toHaveTextContent("ship destroyed");
    const dice = row.querySelectorAll(".event-log__die");
    expect([...dice].map((d) => [d.textContent, d.getAttribute("data-hit")])).toEqual([
      ["2", "no"],
      ["9", "yes"],
    ]);
    expect(row.querySelector(".event-log__undo")).toBeNull();
    // The decisions around it keep their undo buttons and are not merged with it.
    expect(screen.getAllByTestId("event-log-entry")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: /Undo from decision/ })).toHaveLength(2);
  });

  it("keeps a roll between the decisions it happened between, without ending the action", () => {
    const tree = buildEventTree([decision(1), rolled(1, [2], 4), decision(2, "movement")]);
    const phase = tree[0].children[0];
    expect(phase.children.map((n) => n.kind)).toEqual(["marker", "marker", "decision", "roll", "decision"]);
    expect(phase.count).toBe(2);
  });
});

describe("hierarchical event log", () => {
  it("disables undo and redo with the reason while a history change is running", () => {
    const entries = [decision(1)];
    const props = {
      onRestore: vi.fn(),
      onChangeHistory: vi.fn(),
      redoCount: 2,
      currentPath: { round: 1, phase: "action" as const, action_id: "action_1", stage: "movement" },
    };
    const { rerender } = render(wrap(entries, { ...props, busy: false }));
    expect(screen.getByRole("button", { name: "Undo from decision 1" })).not.toBeDisabled();
    rerender(wrap(entries, { ...props, busy: true }));
    const undo = screen.getByRole("button", { name: "Undo from decision 1" });
    expect(undo).toBeDisabled();
    expect(undo).toHaveAttribute("title", HISTORY_BUSY_REASON);
    const redo = screen.getByRole("button", { name: "Redo one" });
    expect(redo).toBeDisabled();
    expect(redo).toHaveAttribute("title", HISTORY_BUSY_REASON);
  });

  it("preserves repeated stage order and merges same-cursor private facts", () => {
    const entries = [
      decision(1),
      {
        ...decision(1),
        id: "private-1",
        visibility: { visibility: "seat" as const, seat: "p1" },
        private_detail: "Only P1",
      },
      decision(2, "combat", "p2"),
      decision(3),
    ];
    const tree = buildEventTree(entries);
    const phase = tree[0].children[0];
    const phaseEvents = phase.children;
    // With flattened structure: [action marker, stage1 marker, decision1, stage2 marker, decision2, stage1 marker, decision3]
    const stageMarkers = phaseEvents.filter((e) => e.kind === "marker" && e.stage);
    const decisions = phaseEvents.filter((e) => e.kind === "decision");
    expect(stageMarkers.map((stage) => stage.label)).toEqual(["Movement", "Combat", "Movement"]);
    expect(tree[0].count).toBe(3);
    expect(decisions[0].entry?.private_detail).toBe("Only P1");
  });

  it("mounts only the current path; lets readers expand, collapse and undo real cursors", () => {
    const restore = vi.fn();
    const entries = [
      {
        ...decision(0),
        id: "start",
        event: { kind: "game_initialized" as const, round: 1, phase: "action" as const, speaker: "p1" },
      },
      decision(1),
      decision(2, "combat", "p2"),
      {
        ...decision(2),
        round: undefined,
        phase: undefined,
        id: "round2",
        event: { kind: "phase_transition" as const, round: 2, phase: "strategy" as const },
      },
      { ...decision(3), round: 2, phase: "strategy" as const, action_id: undefined, stage: undefined },
    ];
    render(
      wrap(entries, {
        currentPath: { round: 1, phase: "action" as const, action_id: "action_1", stage: "combat" },
        onRestore: restore,
      }),
    );
    expect(screen.getAllByText(/Round [12]/)).toHaveLength(2);
    // With flattened structure, the phase is opened automatically by currentPath
    // All events in the phase are visible: game_initialized marker, action marker, stage markers, decisions
    const entries1 = screen.getAllByTestId("event-log-entry");
    expect(entries1.length).toBeGreaterThan(1);
    expect(screen.getByText("Choice 1")).toBeInTheDocument();
    expect(screen.getByText("Choice 2")).toBeInTheDocument();
    const actor = screen.getByLabelText("Participant at position 1");
    expect(actor).toHaveAttribute("title", "Participant at position 1");
    fireEvent.click(screen.getByRole("button", { name: "Undo from decision 1" }));
    expect(restore).toHaveBeenCalledWith(0);
    fireEvent.click(screen.getByRole("button", { name: /Round 1/ }));
    // Collapsing the round should hide all its events
    expect(screen.queryByText("Choice 1")).toBeNull();
  });

  it("colors participant names without repeating seat labels in decisions or symbol tooltips", () => {
    const a = `player_${"a".repeat(64)}`;
    const b = `player_${"b".repeat(64)}`;
    const lobby: LobbyDto = {
      game_id: "game",
      phase: "running",
      lobby_version: 1,
      host_player_id: a,
      slots: [a, b].map((occupant, index) => ({
        slot_id: `slot_${index + 1}`,
        position: index + 1,
        occupant,
        nickname: "az2",
        ready: true,
        connected: true,
        can_take_over: false,
      })),
    };
    const entries = [
      {
        ...decision(0),
        id: "start",
        event: { kind: "game_initialized" as const, round: 1, phase: "action" as const, speaker: a },
      },
      { ...decision(1, "movement", a), detail: `${a} landed sol_infantry on kraag` },
      { ...decision(2, "movement", b), detail: `${b} supported ${a}` },
    ];
    render(
      <PlayerIdentityProvider lobby={lobby} seatingOrder={[a, b]}>
        <EventLog
          events={entries}
          isOpen
          onToggle={vi.fn()}
          currentPath={{ round: 1, phase: "action" as const, action_id: "action_1", stage: "movement" }}
        />
      </PlayerIdentityProvider>,
    );
    const rows = screen.getAllByTestId("event-log-entry");
    expect(rows[1].querySelector(".event-log__body")).toHaveTextContent(
      "az2 landed sol_infantry on kraag",
    );
    expect(rows[1].querySelector(".event-log__body")).not.toHaveTextContent("Position 1");
    expect(rows[1].querySelector(".event-log__participant")).toHaveStyle({ color: "#3B82F6" });
    expect(rows[2].querySelectorAll(".event-log__participant")).toHaveLength(2);
    expect(rows[2].querySelectorAll(".event-log__participant")[0]).toHaveStyle({
      color: "#4ADE80",
    });
    expect(rows[2].querySelectorAll(".event-log__participant")[1]).toHaveStyle({
      color: "#3B82F6",
    });
    const symbol = rows[1].querySelector(".event-log__actor");
    expect(symbol).toHaveAttribute("title", "az2 (● Position 1)");
    expect(symbol).toHaveAttribute("aria-label", "az2 (● Position 1)");
  });

  it("retains unknown historical decisions and more than 500 events", () => {
    const entries = Array.from({ length: 520 }, (_, i) => ({
      ...decision(i + 1),
      round: undefined,
      phase: undefined,
      action_id: undefined,
      stage: undefined,
    }));
    const tree = buildEventTree(entries);
    expect(tree).toHaveLength(1);
    expect(tree[0].count).toBe(520);
    const phaseEvents = tree[0].children[0].children;
    // With flattened structure, we should have 520 decision events
    const decisionEvents = phaseEvents.filter((e) => e.kind === "decision");
    expect(decisionEvents).toHaveLength(520);
  });

  it("keeps legacy action decisions under their phase without inventing actions", () => {
    const entries = [
      decision(1),
      {
        ...decision(2),
        action_id: undefined,
        stage: undefined,
        detail: "A historical action choice",
      },
    ];
    const phase = buildEventTree(entries)[0].children[0];
    const phaseEvents = phase.children;
    // With flattened structure: [action marker, stage marker, decision1, decision2 (no stage)]
    const actionMarker = phaseEvents.find((e) => e.actionId);
    const decision2 = phaseEvents.find((e) => e.entry?.detail === "A historical action choice");
    expect(actionMarker?.label).toBe("Tactical action");
    expect(decision2?.entry?.detail).toBe("A historical action choice");
    const stageMarker = phaseEvents.find((e) => e.stage === "movement");
    expect(stageMarker?.label).toBe("Movement");
  });

  it("renders action card names with tooltips when mentioned in detail text", () => {
    const entries = [
      {
        ...decision(1),
        detail: "Player played Bribery to increase votes",
      },
      {
        ...decision(2),
        detail: "Discarded Ancient Burial Sites",
      },
    ];
    render(
      wrap(entries, {
        currentPath: { round: 1, phase: "action" as const, action_id: "action_1", stage: "movement" },
      }),
    );

    // Check that action card names are in the document
    const entries_rendered = screen.getAllByTestId("event-log-entry");
    expect(entries_rendered.length).toBeGreaterThan(0);
    const body1 = entries_rendered[0]?.querySelector(".event-log__body");
    const body2 = entries_rendered[1]?.querySelector(".event-log__body");

    expect(body1).toHaveTextContent("Bribery");
    expect(body2).toHaveTextContent("Ancient Burial Sites");

    // Check for tooltip elements with data-testid or class
    const tooltips = document.querySelectorAll(".event-log__action-card");
    expect(tooltips.length).toBeGreaterThanOrEqual(2);

    // Verify tooltips have title attributes
    tooltips.forEach((tooltip) => {
      expect(tooltip).toHaveAttribute("title");
      const title = tooltip.getAttribute("title");
      expect(title).toBeTruthy();
      expect(title?.length).toBeGreaterThan(0);
    });
  });
});

describe("copy replay", () => {
  const replay = { text: '{"format":"ti4-replay"}', filename: "ti4-replay-g1.json" };

  it("has no button unless the viewer can fetch a replay", () => {
    render(wrap([decision(1)]));
    expect(screen.queryByTestId("copy-replay-btn")).toBeNull();
  });

  it("copies the replay, shows a polite status and disables the button while loading", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    let release!: () => void;
    const onFetchReplay = vi.fn(
      () =>
        new Promise<typeof replay>((resolve) => {
          release = () => resolve(replay);
        }),
    );
    render(wrap([decision(1)], { onFetchReplay }));
    const button = screen.getByTestId("copy-replay-btn");
    fireEvent.click(button);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(onFetchReplay).toHaveBeenCalledOnce();
    release();
    const status = await screen.findByText(/Replay copied/);
    expect(writeText).toHaveBeenCalledWith(replay.text);
    expect(status).toBe(screen.getByTestId("copy-replay-status"));
    expect(status).toHaveAttribute("role", "status");
    expect(button).not.toBeDisabled();
  });

  it("shows a visible error when the server refuses", async () => {
    const onFetchReplay = vi.fn().mockRejectedValue(new Error("not your game"));
    render(wrap([decision(1)], { onFetchReplay }));
    fireEvent.click(screen.getByTestId("copy-replay-btn"));
    expect(await screen.findByText("Could not load the replay: not your game")).toBeVisible();
    expect(screen.getByTestId("copy-replay-status")).toHaveAttribute("data-state", "error");
  });
});

describe("turn redo controls", () => {
  it("has no controls unless the viewer may redo a turn", () => {
    render(wrap([decision(1)]));
    expect(screen.queryByTestId("turn-redo-btn")).toBeNull();
  });

  it("offers a seated player their own last turn, or the last two", () => {
    const onRequest = vi.fn();
    render(
      wrap([decision(1)], {
        turnRedo: { onRequest, disabled: false, viewerSeat: "p1" },
      }),
    );
    expect(screen.queryByTestId("turn-redo-seat")).toBeNull();
    const button = screen.getByTestId("turn-redo-btn");
    expect(button.textContent).toBe("Redo my last turn");
    fireEvent.click(button);
    expect(onRequest).toHaveBeenLastCalledWith({ turns: 1, seat: undefined });
    fireEvent.change(screen.getByTestId("turn-redo-turns"), { target: { value: "2" } });
    fireEvent.click(button);
    expect(onRequest).toHaveBeenLastCalledWith({ turns: 2, seat: undefined });
  });

  it("lets the host pick another seat and says so on the button", () => {
    const onRequest = vi.fn();
    render(
      wrap([decision(1)], {
        turnRedo: {
          onRequest,
          disabled: false,
          viewerSeat: "p1",
          seats: [
            { id: "p1", label: "Ana" },
            { id: "p2", label: "Bo" },
          ],
        },
      }),
    );
    fireEvent.change(screen.getByTestId("turn-redo-seat"), { target: { value: "p2" } });
    const button = screen.getByTestId("turn-redo-btn");
    expect(button.textContent).toBe("Redo their last turn");
    fireEvent.click(button);
    expect(onRequest).toHaveBeenLastCalledWith({ turns: 1, seat: "p2" });
  });

  it("waits while a redo is running or in flight", () => {
    render(
      wrap([decision(1)], {
        turnRedo: { onRequest: vi.fn(), disabled: true, viewerSeat: "p1" },
      }),
    );
    expect(screen.getByTestId("turn-redo-btn")).toBeDisabled();
  });
});

describe("event log scroll following (mocked layout)", () => {
  // jsdom has no layout: every entry is 26px, the box is 320px, and scrollTop is a plain field.
  const ROW = 26;
  const BOX = 320;
  const proto = HTMLElement.prototype;
  const originals = {
    scrollHeight: Object.getOwnPropertyDescriptor(proto, "scrollHeight"),
    clientHeight: Object.getOwnPropertyDescriptor(proto, "clientHeight"),
    scrollTop: Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop"),
  };
  const tops = new WeakMap<Element, number>();
  beforeEach(() => {
    Object.defineProperty(proto, "scrollHeight", {
      configurable: true,
      get(this: HTMLElement) {
        return this.matches(".event-log__list")
          ? this.querySelectorAll('[data-testid="event-log-entry"]').length * ROW + 80
          : 0;
      },
    });
    Object.defineProperty(proto, "clientHeight", {
      configurable: true,
      get(this: HTMLElement) {
        return this.matches(".event-log__list") ? BOX : 0;
      },
    });
    Object.defineProperty(Element.prototype, "scrollTop", {
      configurable: true,
      get(this: Element) {
        return tops.get(this) ?? 0;
      },
      set(this: Element, value: number) {
        tops.set(this, value);
      },
    });
  });
  afterEach(() => {
    for (const [key, target] of [
      ["scrollHeight", proto],
      ["clientHeight", proto],
      ["scrollTop", Element.prototype],
    ] as const) {
      const original = originals[key];
      if (original) Object.defineProperty(target, key, original);
      else delete (target as unknown as Record<string, unknown>)[key];
    }
  });

  const init = {
    ...decision(0),
    id: "start",
    event: { kind: "game_initialized" as const, round: 1, phase: "action" as const, speaker: "p1" },
  };
  const many = (n: number) => [init, ...Array.from({ length: n }, (_, i) => decision(i + 1))];
  const path = { round: 1, phase: "action" as const };
  const list = () => screen.getByTestId("event-log-list");
  const gap = () => list().scrollHeight - list().clientHeight - list().scrollTop;
  const scrollTo = (top: number) => {
    list().scrollTop = top;
    fireEvent.scroll(list());
  };

  it("opens at the newest entry and follows new events and bursts while pinned", () => {
    const { rerender } = render(wrap(many(40), { currentPath: path }));
    expect(gap()).toBe(0);
    rerender(wrap(many(41), { currentPath: path }));
    expect(gap()).toBe(0);
    rerender(wrap(many(49), { currentPath: path }));
    expect(gap()).toBe(0);
  });

  it("does not move a reader who scrolled up, and offers a jump to the latest", () => {
    const { rerender } = render(wrap(many(40), { currentPath: path }));
    scrollTo(120);
    rerender(wrap(many(43), { currentPath: path }));
    expect(list().scrollTop).toBe(120);
    expect(screen.getByTestId("event-log-jump")).toHaveTextContent("3 new events");
    fireEvent.click(screen.getByTestId("event-log-jump"));
    expect(gap()).toBe(0);
    expect(screen.queryByTestId("event-log-jump")).toBeNull();
  });

  it("follows into a newly opened round when pinned", () => {
    const first = many(40);
    const { rerender } = render(wrap(first, { currentPath: path }));
    const next = [
      ...first,
      {
        ...decision(41),
        id: "round2",
        round: undefined,
        phase: undefined,
        event: { kind: "phase_transition" as const, round: 2, phase: "strategy" as const },
      },
      { ...decision(42), round: 2, phase: "strategy" as const },
    ];
    rerender(wrap(next, { currentPath: { round: 2, phase: "strategy" as const } }));
    expect(screen.getByText("Choice 42")).toBeInTheDocument();
    expect(gap()).toBe(0);
  });

  it("keeps manual expansion across a reconnect (same history key) but drops it for a new one", () => {
    const events = [
      { ...decision(0), id: "start", event: { kind: "game_initialized" as const, round: 1, phase: "action" as const, speaker: "p1" } },
      { ...decision(1), round: 1 },
      {
        ...decision(2),
        id: "round2",
        round: undefined,
        phase: undefined,
        event: { kind: "phase_transition" as const, round: 2, phase: "action" as const },
      },
      { ...decision(3), round: 2 },
    ];
    const props = { currentPath: { round: 2, phase: "action" as const } };
    const { rerender } = render(wrap(events, { ...props, historyKey: 7 }));
    fireEvent.click(screen.getByRole("button", { name: /Round 1/ }));
    fireEvent.click(screen.getAllByRole("button", { name: /Action phase/ })[0]);
    expect(screen.getByText("Choice 1")).toBeInTheDocument();
    rerender(wrap([...events], { ...props, historyKey: 7 }));
    expect(screen.getByText("Choice 1")).toBeInTheDocument();
    rerender(wrap([...events], { ...props, historyKey: 8 }));
    expect(screen.queryByText("Choice 1")).toBeNull();
  });

  it("restores the position after the list was hidden and shown again", () => {
    const { rerender } = render(wrap(many(40), { currentPath: path }));
    scrollTo(150);
    rerender(wrap(many(40), { currentPath: path, isOpen: false }));
    rerender(wrap(many(42), { currentPath: path, isOpen: false }));
    rerender(wrap(many(42), { currentPath: path }));
    expect(list().scrollTop).toBe(150);
  });

  it("reopens at the bottom when the reader was pinned", () => {
    const { rerender } = render(wrap(many(40), { currentPath: path }));
    rerender(wrap(many(40), { currentPath: path, isOpen: false }));
    rerender(wrap(many(44), { currentPath: path }));
    expect(gap()).toBe(0);
  });
});
