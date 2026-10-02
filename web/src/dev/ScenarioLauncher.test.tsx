import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ScenarioLauncher } from "./ScenarioLauncher.tsx";

describe("ScenarioLauncher", () => {
  const mockScenarios = [
    {
      id: "tactical_action",
      title: "Tactical Action & Movement",
      category: "Tactical",
      description: "Activate an adjacent sector, move ships, and resolve movement.",
      player_count: 3,
      human_faction: "Federation of Sol",
      opponent_factions: ["Emirates of Hacan", "Barony of Letnev"],
    },
    {
      id: "space_combat",
      title: "Space Combat Encounter",
      category: "Combat",
      description: "Hostile Letnev ships in the adjacent system ready for space combat.",
      player_count: 3,
      human_faction: "Federation of Sol",
      opponent_factions: ["Emirates of Hacan", "Barony of Letnev"],
    },
  ];

  beforeEach(() => {
    sessionStorage.clear();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((url: string, init?: RequestInit) => {
        if (url === "/api/dev/scenarios") {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () => Promise.resolve(mockScenarios),
          });
        }
        if (url === "/api/dev/scenarios/launch") {
          const body = JSON.parse((init?.body as string) || "{}");
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () =>
              Promise.resolve({
                game_id: `dev_${body.scenario_id}_test123`,
                player_session: "session_token_xyz",
                player_id: "player_p1",
                scenario_id: body.scenario_id,
              }),
          });
        }
        return Promise.reject(new Error(`Unhandled URL: ${url}`));
      }),
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("fetches and renders scenarios", async () => {
    render(<ScenarioLauncher />);

    expect(screen.getByText(/Loading available scenarios/i)).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText("Tactical Action & Movement")).toBeInTheDocument();
      expect(screen.getByText("Space Combat Encounter")).toBeInTheDocument();
    });

    expect(screen.getByText("Tactical")).toBeInTheDocument();
    expect(screen.getByText("Combat")).toBeInTheDocument();
    expect(screen.getAllByText(/Federation of Sol/i)).toHaveLength(2);
  });

  it("launches a scenario and sets sessionStorage", async () => {
    render(<ScenarioLauncher />);

    await waitFor(() => {
      expect(screen.getByText("Tactical Action & Movement")).toBeInTheDocument();
    });

    const buttons = screen.getAllByRole("button", { name: /Launch Scenario/i });
    expect(buttons).toHaveLength(2);

    fireEvent.click(buttons[0]);

    await waitFor(() => {
      expect(sessionStorage.getItem("ti4.player-session:dev_tactical_action_test123")).toBe(
        "session_token_xyz",
      );
    });
  });
});
