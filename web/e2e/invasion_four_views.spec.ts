import { test, expect, type APIRequestContext } from "@playwright/test";
import { openPlayerGame } from "./lobbyHelpers";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8180"}`;

async function snapshot(
  request: APIRequestContext,
  game: string,
  token?: string,
): Promise<InitialSnapshotMsg> {
  const response = await request.get(
    `${backend}/api/games/${game}/snapshot`,
    token ? { headers: { "x-ti4-player-session": token } } : {},
  );
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}

test("invasion lands, fights and hands off consistently in four independent views", async ({
  browser,
  request,
}) => {
  test.setTimeout(180_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_invasion_four_views", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: invader,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  const ids = [invader, ...Object.keys(seats).filter((id) => id !== invader)];
  const contexts = await Promise.all(Array.from({ length: 4 }, () => browser.newContext()));
  const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));
  try {
    const advisorInputs: Array<Record<string, unknown>> = [];
    await pages[0].route("**/advisor/ground_odds", async (route) => {
      const input = route.request().postDataJSON() as {
        attacker: { units: Record<string, number> };
      };
      advisorInputs.push(input as unknown as Record<string, unknown>);
      const count = Object.values(input.attacker.units).reduce((total, n) => total + n, 0);
      await route.fulfill({
        json: { attacker_win_rate: Math.min(0.95, count / 10), simulations: 2000 },
      });
    });
    await Promise.all(ids.map((id, index) => openPlayerGame(pages[index], game, seats[id])));
    await pages[3].goto(`/games/${game}`);
    await pages[3].getByRole("button", { name: "Watch" }).click();

    const samePublic = async () => {
      const views = await Promise.all(
        pages.map((_, i) => snapshot(request, game, i < 3 ? seats[ids[i]] : undefined)),
      );
      for (const view of views.slice(1)) {
        expect(view.view.board.invasion).toEqual(views[0].view.board.invasion);
        expect(view.view.board.systems[views[0].view.board.invasion!.system_id]).toEqual(
          views[0].view.board.systems[views[0].view.board.invasion!.system_id],
        );
      }
      return views;
    };
    let views = await samePublic();
    const invasion = views[0].view.board.invasion!;
    expect(invasion.planets).toHaveLength(1);
    const planet = invasion.planets[0];
    for (const page of pages) await expect(page.getByTestId("invasion-overlay")).toBeVisible();
    expect(views[3].pending_choice).toBeFalsy();
    // The start-of-invasion reaction is already visible; no space-combat recap blocks it.
    let choice = views[0].pending_choice?.choice;
    expect(choice?.context?.invasion_seq).toBe(invasion.invasion_seq);
    for (let step = 0; step < 15 && choice?.context?.subtype !== "commit_ground_forces"; step++) {
      const actor = choice!.player;
      const page = pages[ids.indexOf(actor)];
      const decline =
        choice!.options.find((option) => option.kind === "decline") ?? choice!.options[0];
      await page
        .getByTestId("invasion-overlay")
        .getByRole("button", {
          name: new RegExp(decline.label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
        })
        .first()
        .click();
      await expect
        .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
        .toBeGreaterThan(views[0].game_version);
      views = await samePublic();
      choice = views[0].pending_choice?.choice;
    }
    expect(choice?.context?.subtype).toBe("commit_ground_forces");
    const tray = pages[0].getByTestId("invasion-landing-tray");
    await expect(tray).toBeVisible();
    await tray.getByRole("button", { name: planet, exact: true }).click();
    const infantry = tray.getByRole("button", { name: /land infantry.*in space/i }).first();
    await infantry.click();
    await expect(pages[0].getByTestId("invasion-odds")).toContainText(
      "Projected odds if these forces land",
    );
    const firstOdds = await pages[0].getByTestId("invasion-odds").textContent();
    await infantry.click();
    await expect
      .poll(async () => pages[0].getByTestId("invasion-odds").textContent())
      .not.toBe(firstOdds);
    expect(advisorInputs.length).toBeGreaterThanOrEqual(2);
    for (const page of pages.slice(1))
      await expect(page.getByTestId("invasion-odds")).toHaveCount(0);
    await tray.getByRole("button", { name: "Confirm landings" }).click();
    await expect
      .poll(
        async () =>
          (await snapshot(request, game, seats[invader])).view.board.systems[
            invasion.system_id
          ].units.filter((unit) => unit.owner === invader && unit.planet === planet).length,
      )
      .toBe(2);
    views = await samePublic();
    await expect(tray.getByText("Submitting landings one at a time…")).toHaveCount(0);
    await tray.getByRole("button", { name: "Done committing" }).click();

    let sawRound = false;
    for (let step = 0; step < 40; step++) {
      const state = await snapshot(request, game, seats[invader]);
      if (!state.view.board.invasion) break;
      const active =
        state.turn_status.kind === "waiting_for_decision" ? state.turn_status.seat : invader;
      const offered = (await snapshot(request, game, seats[active])).pending_choice?.choice;
      if (!offered) continue;
      if (state.view.board.invasion.last_step?.kind === "ground_round") {
        sawRound = true;
        const publicViews = await samePublic();
        expect(publicViews[3].view.board.invasion?.last_step?.planet).toBe(planet);
        for (const page of pages) await expect(page.getByTestId("invasion-step")).toBeVisible();
      }
      const page = pages[ids.indexOf(active)];
      const button =
        offered.context?.subtype === "fight_ground_combat_round"
          ? page.getByRole("button", { name: "Fight next round" }).first()
          : page
              .getByTestId("invasion-overlay")
              .getByRole("button", { name: offered.options[0].label })
              .first();
      await button.click();
      await expect
        .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
        .toBeGreaterThan(state.game_version);
    }
    expect(sawRound).toBe(true);
    await expect
      .poll(async () => (await snapshot(request, game, seats[invader])).turn_status.kind)
      .toBe("waiting_for_decision");
    const done = await snapshot(request, game, seats[invader]);
    expect(done.view.board.invasion).toBeFalsy();
    const nextActor =
      done.turn_status.kind === "waiting_for_decision" ? done.turn_status.seat : invader;
    expect((await snapshot(request, game, seats[nextActor])).pending_choice?.choice).toBeDefined();
  } finally {
    await Promise.all(contexts.map((ctx) => ctx.close()));
  }
});

test("Parley interrupts a non-atomic landing pipeline and preserves the remaining actor-only draft", async ({
  browser,
  request,
}) => {
  test.setTimeout(90_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_invasion_parley", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: invader,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  const ids = Object.keys(seats);
  const contexts = await Promise.all(Array.from({ length: 4 }, () => browser.newContext()));
  const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));
  try {
    await Promise.all(ids.map((id, i) => openPlayerGame(pages[i], game, seats[id])));
    await pages[3].goto(`/games/${game}`);
    await pages[3].getByRole("button", { name: "Watch" }).click();
    const invaderPage = pages[ids.indexOf(invader)];
    for (let step = 0; step < 15; step++) {
      const state = await snapshot(request, game, seats[invader]);
      if (state.pending_choice?.choice.context?.subtype === "commit_ground_forces") break;
      const actor =
        state.turn_status.kind === "waiting_for_decision" ? state.turn_status.seat : invader;
      const offer = (await snapshot(request, game, seats[actor])).pending_choice?.choice;
      expect(offer).toBeDefined();
      const decline =
        offer!.options.find((option) => option.kind === "decline") ?? offer!.options[0];
      await pages[ids.indexOf(actor)]
        .getByTestId("invasion-overlay")
        .getByRole("button", { name: decline.label })
        .first()
        .click();
      await expect
        .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
        .toBeGreaterThan(state.game_version);
    }
    const state = await snapshot(request, game, seats[invader]);
    const planet = state.view.board.invasion!.planets[0];
    const tray = invaderPage.getByTestId("invasion-landing-tray");
    await tray.getByRole("button", { name: planet, exact: true }).click();
    const infantry = tray.getByRole("button", { name: /land infantry.*in space/i }).first();
    await infantry.click();
    await infantry.click();
    await tray.getByRole("button", { name: "Confirm landings" }).click();
    const defender = ids.find(
      (id) =>
        id !== invader &&
        id !== state.view.players.find((player) => player.faction === "hacan")?.id,
    )!;
    await expect
      .poll(async () =>
        (await snapshot(request, game, seats[defender])).pending_choice?.choice.options.some(
          (option) => option.label === "Play Parley",
        ),
      )
      .toBe(true);
    const paused = await snapshot(request, game, seats[invader]);
    expect(
      paused.view.board.systems[state.view.board.invasion!.system_id].units.filter(
        (unit) => unit.owner === invader && unit.planet === planet,
      ),
    ).toHaveLength(1);
    await expect(invaderPage.getByTestId("invasion-interrupted-draft")).toContainText("infantry →");
    for (const [index, page] of pages.entries())
      if (ids[index] !== invader) {
        await expect(page.getByTestId("invasion-interrupted-draft")).toHaveCount(0);
      }
    await pages[ids.indexOf(defender)].getByRole("button", { name: "Play Parley" }).click();
    await expect
      .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
      .toBeGreaterThan(paused.game_version);
    await expect(tray).toBeVisible();
    await expect(tray).toContainText("Remaining draft: infantry");
    await expect
      .poll(
        async () =>
          (await snapshot(request, game, seats[invader])).pending_choice?.choice.context?.subtype,
      )
      .toBe("commit_ground_forces");
  } finally {
    await Promise.all(contexts.map((ctx) => ctx.close()));
  }
});

test("a separate two-planet invasion keeps ground-round evidence scoped to each planet", async ({
  browser,
  request,
}) => {
  test.setTimeout(90_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_invasion_coexistence", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: invader,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  try {
    await openPlayerGame(page, game, seats[invader]);
    let state = await snapshot(request, game, seats[invader]);
    const planets = state.view.board.invasion!.planets;
    expect(planets).toHaveLength(2);
    for (
      let step = 0;
      step < 15 && state.pending_choice?.choice.context?.subtype !== "commit_ground_forces";
      step++
    ) {
      const offer = state.pending_choice?.choice;
      expect(offer?.player).toBe(invader);
      const decline =
        offer!.options.find((option) => option.kind === "decline") ?? offer!.options[0];
      await page
        .getByTestId("invasion-overlay")
        .getByRole("button", { name: decline.label })
        .first()
        .click();
      await expect
        .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
        .toBeGreaterThan(state.game_version);
      state = await snapshot(request, game, seats[invader]);
    }
    const tray = page.getByTestId("invasion-landing-tray");
    expect(
      state.view.board.systems[state.view.board.invasion!.system_id].units.filter(
        (unit) => unit.owner === invader && !unit.planet && unit.unit_type === "infantry",
      ).length,
    ).toBe(6);
    for (const planet of planets) {
      await tray.getByRole("button", { name: planet, exact: true }).click();
      const planetCard = tray.locator(".invasion-planet-landing-card").filter({
        has: page.getByRole("button", { name: planet, exact: true }),
      });
      for (let copy = 0; copy < 3; copy++) {
        await planetCard
          .getByRole("button", { name: /land infantry.*in space/i })
          .first()
          .click();
      }
    }
    await tray.getByRole("button", { name: "Confirm landings" }).click();
    await expect
      .poll(async () => {
        const current = await snapshot(request, game, seats[invader]);
        return planets.map(
          (planet) =>
            current.view.board.systems[state.view.board.invasion!.system_id].units.filter(
              (unit) => unit.owner === invader && unit.planet === planet,
            ).length,
        );
      })
      .toEqual([3, 3]);
    await tray.getByRole("button", { name: "Done committing" }).click();
    const seen = new Set<string>();
    const fought = new Set<string>();
    for (let step = 0; step < 30; step++) {
      const current = await snapshot(request, game, seats[invader]);
      if (!current.view.board.invasion) break;
      if (current.view.board.invasion.last_step?.kind === "ground_round") {
        const result = current.view.board.invasion.last_step;
        expect(planets).toContain(result.planet);
        expect(result.dice.every((die) => die.planet === result.planet)).toBe(true);
        seen.add(result.planet);
      }
      const offer = current.pending_choice?.choice;
      if (!offer) continue;
      if (offer.context?.subtype === "fight_ground_combat_round") {
        const planet = current.view.board.invasion.current_planet;
        expect(planets).toContain(planet);
        fought.add(planet!);
      }
      const next =
        offer.context?.subtype === "fight_ground_combat_round"
          ? page.getByRole("button", { name: "Fight next round" }).first()
          : page
              .getByTestId("invasion-overlay")
              .getByRole("button", { name: offer.options[0].label })
              .first();
      await next.click();
      await expect
        .poll(async () => (await snapshot(request, game, seats[invader])).game_version)
        .toBeGreaterThan(current.game_version);
    }
    expect(fought).toEqual(new Set(planets));
    // The final roll can complete the invasion in the same step; no active invasion snapshot
    // remains then. Every intermediate round that is exposed must stay planet-local.
    expect(seen.size).toBeGreaterThan(0);
  } finally {
    await ctx.close();
  }
});
