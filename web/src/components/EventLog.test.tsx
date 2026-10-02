import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GameLogEntry } from "../protocol/client.ts";
import { EventLog, buildEventTree } from "./EventLog.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto } from "../protocol/types.ts";

const decision = (cursor: number, stage = "movement", actor = "p1"): GameLogEntry => ({
  id: `event-${cursor}`,
  timestamp: "12:00",
  visibility: "public",
  decision_count: cursor,
  round: 1,
  phase: "action",
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

describe("hierarchical event log", () => {
  it("preserves repeated stage order and merges same-cursor private facts", () => {
    const entries = [
      decision(1),
      {
        ...decision(1),
        id: "private-1",
        visibility: "seat" as const,
        seat: "p1",
        private_detail: "Only P1",
      },
      decision(2, "combat", "p2"),
      decision(3),
    ];
    const tree = buildEventTree(entries);
    const stages = tree[0].children[0].children[0].children;
    expect(stages.map((stage) => stage.label)).toEqual(["Movement", "Combat", "Movement"]);
    expect(tree[0].count).toBe(3);
    expect(stages[0].children[0].entry?.private_detail).toBe("Only P1");
  });

  it("mounts only the current path; lets readers expand, collapse and undo real cursors", () => {
    const restore = vi.fn();
    const entries = [
      {
        ...decision(0),
        id: "start",
        event: { kind: "game_initialized" as const, round: 1, phase: "action", speaker: "p1" },
      },
      decision(1),
      decision(2, "combat", "p2"),
      {
        ...decision(2),
        round: undefined,
        phase: undefined,
        id: "round2",
        event: { kind: "phase_transition" as const, round: 2, phase: "strategy" },
      },
      { ...decision(3), round: 2, phase: "strategy", action_id: undefined, stage: undefined },
    ];
    render(
      wrap(entries, {
        currentPath: { round: 1, phase: "action", action_id: "action_1", stage: "combat" },
        onRestore: restore,
      }),
    );
    expect(screen.getAllByText(/Round [12]/)).toHaveLength(2);
    expect(screen.getAllByTestId("event-log-entry")).toHaveLength(2); // phase marker and combat leaf
    expect(screen.queryByText("Choice 1")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Movement/ }));
    expect(screen.getByText("Choice 1")).toBeInTheDocument();
    const actor = screen.getByLabelText("Participant at position 1");
    expect(actor).toHaveAttribute("title", "Participant at position 1");
    fireEvent.click(screen.getByRole("button", { name: "Undo from decision 1" }));
    expect(restore).toHaveBeenCalledWith(0);
    fireEvent.click(screen.getByRole("button", { name: /Round 1/ }));
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
        event: { kind: "game_initialized" as const, round: 1, phase: "action", speaker: a },
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
          currentPath={{ round: 1, phase: "action", action_id: "action_1", stage: "movement" }}
        />
      </PlayerIdentityProvider>,
    );
    const rows = screen.getAllByTestId("event-log-entry");
    expect(rows[1].querySelector(".event-log__body")).toHaveTextContent(
      "az2 landed sol_infantry on kraag",
    );
    expect(rows[1].querySelector(".event-log__body")).not.toHaveTextContent("Position 1");
    expect(rows[1].querySelector(".event-log__participant")).toHaveStyle({ color: "#E69F00" });
    expect(rows[2].querySelectorAll(".event-log__participant")).toHaveLength(2);
    expect(rows[2].querySelectorAll(".event-log__participant")[0]).toHaveStyle({
      color: "#56B4E9",
    });
    expect(rows[2].querySelectorAll(".event-log__participant")[1]).toHaveStyle({
      color: "#E69F00",
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
    expect(tree[0].children[0].children).toHaveLength(520);
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
    expect(phase.children.map((child) => child.label)).toEqual(["Tactical action", ""]);
    expect(phase.children[1].entry?.detail).toBe("A historical action choice");
    expect(phase.children[0].children[0].label).toBe("Movement");
  });
});
