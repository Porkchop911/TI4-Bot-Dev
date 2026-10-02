import { test, expect, type APIRequestContext } from "@playwright/test";
import { openPlayerGame } from "./lobbyHelpers";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

async function launch(request: APIRequestContext, scenario: string) {
  const response = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: scenario, seed: 42 },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()) as {
    game_id: string;
    player_id: string;
    player_session: string;
  };
}

async function snapshot(request: APIRequestContext, gameId: string, session: string) {
  const response = await request.get(`${backend}/api/games/${gameId}/snapshot`, {
    headers: { "x-ti4-player-session": session },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()) as InitialSnapshotMsg;
}

test.describe("Technology Research with Tech Skips", () => {
  test("renders research modal with faction techs first, tech skips toggling, resources, and research confirmation", async ({
    page,
    request,
  }) => {
    test.setTimeout(60_000);

    const {
      game_id: game,
      player_id: player,
      player_session: session,
    } = await launch(request, "research_tech_skips");

    const initial = await snapshot(request, game, session);
    expect(initial.pending_choice?.choice.context?.subtype).toBe("research_technology");

    await openPlayerGame(page, game, session);

    // Technology modal should be automatically open for research_technology workflow
    const modal = page.getByTestId("technology-modal");
    await expect(modal).toBeVisible();

    // Verify Select Research badge
    await expect(modal.getByText("Select Research")).toBeVisible();

    // Verify Faction Technologies section is rendered at the top
    const factionSection = page.getByTestId("faction-technologies-section");
    await expect(factionSection).toBeVisible();
    await expect(factionSection).toContainText("Faction Technologies");
    await expect(factionSection).toContainText("Sol");

    // Sol faction techs should be visible
    const so2Card = page.getByTestId("tech-card-so2");
    const ac2Card = page.getByTestId("tech-card-ac2");
    await expect(so2Card).toBeVisible();
    await expect(ac2Card).toBeVisible();

    // Available resources display
    const resourcesBar = page.getByTestId("tech-available-resources");
    await expect(resourcesBar).toBeVisible();
    await expect(resourcesBar).toContainText("Available Resources:");
    await expect(resourcesBar).toContainText("6 TG");

    // Tech skips row: Rigel III (ready, Biotic) and Tar'mann (exhausted, Propulsion)
    const rigelSkip = page.getByTestId("tech-skip-rigeliii");
    await expect(rigelSkip).toBeVisible();
    await expect(rigelSkip).toContainText("Ready");
    await expect(rigelSkip).toBeEnabled();

    const tarmannSkip = page.getByTestId("tech-skip-tarmann");
    await expect(tarmannSkip).toBeVisible();
    await expect(tarmannSkip).toContainText("(Exhausted)");
    await expect(tarmannSkip).toBeDisabled();

    // Spec Ops II (so2) requires 2 Biotic prereqs (GG).
    // Sol starts with 1 Biotic (Neural Motivator). Without Rigel III skip toggled -> so2 is unresearchable (opacity dimmed)
    await expect(so2Card).toHaveAttribute("data-researchable", "false");

    // Toggle Rigel III skip ON
    await rigelSkip.click();
    await expect(rigelSkip).toContainText("Active (+1)");

    // Spec Ops II should now be researchable (1 owned Biotic + 1 Biotic skip = 2 Biotic)
    await expect(so2Card).toHaveAttribute("data-researchable", "true");

    // Select Spec Ops II
    await so2Card.click();
    await expect(so2Card).toHaveAttribute("data-selected", "true");

    // Check selection status & cost
    await expect(page.getByText(/Selected:/)).toContainText("1 / 2");
    await expect(page.getByText("Cost: Free")).toBeVisible();

    // Confirm research button
    const confirmBtn = page.getByTestId("confirm-research-btn");
    await expect(confirmBtn).toBeEnabled();
    await expect(confirmBtn).toContainText("Confirm Research (1 Tech - Free)");

    // Click confirm research
    await confirmBtn.click();

    // Verify submission succeeds and game advances
    await expect
      .poll(async () => {
        const state = await snapshot(request, game, session);
        const sol = Object.values(state.view.players).find((p) => p.id === player);
        return sol?.technologies?.some((id) => id === "so2" || id === "specops2");
      })
      .toBe(true);
  });

  test("handles multi-tech research with prerequisite chaining (1st tech unlocks 2nd tech)", async ({
    page,
    request,
  }) => {
    test.setTimeout(60_000);

    const {
      game_id: game,
      player_id: player,
      player_session: session,
    } = await launch(request, "research_tech_skips");

    await openPlayerGame(page, game, session);

    const modal = page.getByTestId("technology-modal");
    await expect(modal).toBeVisible();

    // Graviton Laser System (gls) requires 1 Cybernetic prereq.
    // Sol has 0 Cybernetic techs and Rigel III is Biotic, Tar'mann is exhausted Propulsion.
    // Initially, gls is unresearchable (opacity dimmed)
    const glsCard = page.getByTestId("tech-card-gls");
    await expect(glsCard).toBeVisible();
    await expect(glsCard).toHaveAttribute("data-researchable", "false");
    await expect(page.getByTestId("first-tech-prereq-bonus")).not.toBeVisible();

    // Select 1st technology: Sarween Tools (st, 0 prereqs, Cybernetic)
    const stCard = page.getByTestId("tech-card-st");
    await expect(stCard).toBeVisible();
    await stCard.click();
    await expect(stCard).toHaveAttribute("data-selected", "true");

    // Status shows 1 / 2 and Cost: Free
    await expect(page.getByText(/Selected:/)).toContainText("1 / 2");
    await expect(page.getByText("Cost: Free")).toBeVisible();

    // Skips list now displays the Tech 1 (+1) prerequisite bonus!
    const bonusPill = page.getByTestId("first-tech-prereq-bonus");
    await expect(bonusPill).toBeVisible();
    await expect(bonusPill).toContainText("Sarween Tools");
    await expect(bonusPill).toContainText("Tech 1 (+1)");

    // Graviton Laser System (gls) now has its 1 Cybernetic prerequisite met by Sarween Tools -> researchable!
    await expect(glsCard).toHaveAttribute("data-researchable", "true");

    // Select 2nd technology: Graviton Laser System (gls)
    await glsCard.click();
    await expect(glsCard).toHaveAttribute("data-selected", "true");

    // Status shows 2 / 2 and Cost: 6 Resources
    await expect(page.getByText(/Selected:/)).toContainText("2 / 2");
    await expect(page.getByText("Cost: 6 Resources")).toBeVisible();

    // Confirm button displays 2 Techs - 6 Resources
    const confirmBtn = page.getByTestId("confirm-research-btn");
    await expect(confirmBtn).toBeEnabled();
    await expect(confirmBtn).toContainText("Confirm Research (2 Techs - 6 Resources)");

    // Click confirm
    await confirmBtn.click();

    // Verify both technologies are researched (st first, then gls)
    await expect
      .poll(async () => {
        const state = await snapshot(request, game, session);
        const sol = Object.values(state.view.players).find((p) => p.id === player);
        const hasSt = sol?.technologies?.some((id) => id === "st" || id === "sarween_tools");
        const hasGls = sol?.technologies?.some(
          (id) => id === "gls" || id === "graviton_laser_system",
        );
        return Boolean(hasSt && hasGls);
      })
      .toBe(true);
  });
});
