import { test, expect, type APIRequestContext, type Locator, type Page } from "@playwright/test";
import { openPlayerGame } from "./lobbyHelpers";
import type { InitialSnapshotMsg, ChoiceOption } from "../src/protocol/types";
import { getUnitBaseType } from "../src/components/UnitIcon";

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
  expect(
    response.ok(),
    `snapshot: ${response.status()} ${response.ok() ? "" : await response.text()}`,
  ).toBe(true);
  return response.json();
}

const shipTypes = new Set([
  "warsun",
  "flagship",
  "dreadnought",
  "carrier",
  "cruiser",
  "destroyer",
  "fighter",
]);

async function expectFleetPills(page: Page, state: InitialSnapshotMsg, step: number) {
  const combat = state.view.board.combat!;
  const units = state.view.board.systems[combat.system_id].units;
  const start =
    combat.phase === "barrage" ||
    (combat.phase === "pre_roll" &&
      combat.round === 1 &&
      Object.keys(combat.barrage_hits ?? {}).length > 0)
      ? combat.barrage_start
      : combat.phase === "resolving_hits"
        ? combat.round_start
        : undefined;
  for (const [side, seat] of [
    ["attacker", combat.attacker],
    ["defender", combat.defender],
  ] as const) {
    const groups = new Map<string, { count: number; damaged: number }>();
    for (const unit of units) {
      if (unit.owner !== seat || unit.planet || !shipTypes.has(getUnitBaseType(unit.unit_type)))
        continue;
      const group = groups.get(unit.unit_type) ?? { count: 0, damaged: 0 };
      group.count++;
      if (unit.damaged) group.damaged++;
      groups.set(unit.unit_type, group);
    }
    const card = page.getByTestId(`${side}-fleet-card`);
    for (const [type, { count, damaged }] of groups) {
      const row = card.getByTestId(`unit-row-${type}`);
      await expect(row, `step ${step} ${side} ${type} count`).toContainText(`×${count}`);
      const pill = row.locator(".combat-unit-row__damaged-badge");
      if (damaged) {
        const damagedAtStart =
          start?.filter((unit) => unit.owner === seat && unit.unit_type === type && unit.damaged)
            .length ?? 0;
        const newDamage = start ? Math.max(0, damaged - damagedAtStart) : damaged;
        await expect(pill, `step ${step} ${side} ${type} damage`).toHaveText(
          `${damaged - newDamage} + ${newDamage} damaged`,
        );
      } else {
        await expect(pill, `step ${step} ${side} ${type} undamaged`).toHaveCount(0);
      }
    }
    for (const row of await card.locator('[data-testid^="unit-row-"]').all()) {
      const type = (await row.getAttribute("data-testid"))?.replace(/^unit-row-(lost-)?/, "");
      if (type && !groups.has(type)) {
        await expect(row, `step ${step} ${side} ${type} destroyed`).toContainText("×0");
        await expect(row.locator(".combat-unit-row__damaged-badge")).toHaveCount(0);
      }
    }
  }
}

/**
 * The seat that is asked now. A private reaction window (Sabotage against an action card) shows
 * publicly as "waiting for reactions" without naming the seat: ask each seat's own view.
 */
async function actorOf(
  request: APIRequestContext,
  game: string,
  seats: Record<string, string>,
  state: InitialSnapshotMsg,
): Promise<string | undefined> {
  if (state.turn_status.kind === "waiting_for_decision") return state.turn_status.seat;
  if (state.turn_status.kind !== "waiting_for_reactions") return undefined;
  for (const [id, token] of Object.entries(seats))
    if ((await snapshot(request, game, token)).pending_choice) return id;
  return undefined;
}

/**
 * Stages the hits of a sustain or casualty decision in the shared hit panel, which sends every hit
 * as one plan: a sustain on a ship of the offered type when the test chose to sustain, and
 * destroys for the rest. Destroys go first to ships that cannot sustain and are not of the
 * sustaining type (a destroy on an undamaged sustaining ship stages its sustain too).
 */
async function stageHits(panel: Locator, selected: ChoiceOption) {
  const sustained = selected.kind === "sustain" ? String(selected.payload?.unit ?? "") : "";
  if (sustained)
    await panel.locator(`[data-testid^="hit-sustain-${sustained}#"]:enabled`).first().click();
  const remaining = panel.getByTestId("hit-assignment-remaining");
  const plainRows = panel.locator('[data-testid^="hit-row-"]:not(:has([data-testid^="hit-sustain-"]))');
  const choices = [
    plainRows
      .filter({ hasNot: panel.page().locator(`[data-testid^="hit-staged-${sustained || "-"}#"]`) })
      .locator('[data-testid^="hit-destroy-"]:enabled'),
    plainRows.locator('[data-testid^="hit-destroy-"]:enabled'),
    panel.locator('[data-testid^="hit-destroy-"]:enabled'),
  ];
  for (let hit = 0; hit < 20 && !(await remaining.innerText()).startsWith("0 of"); hit++) {
    let staged = false;
    for (const destroy of choices) {
      if ((await destroy.count()) === 0) continue;
      await destroy.first().click();
      staged = true;
      break;
    }
    if (!staged) break;
  }
  await panel.getByTestId("hit-confirm").click();
}

/** Answers a combat decision in the actor's view: hits in the shared panel when it shows. */
async function answerCombat(page: Page, selector: string, subtype: string, selected: ChoiceOption) {
  const button = page.getByTestId(selector).first();
  if (subtype === "sustain_damage" || subtype === "assign_casualty") {
    const panel = page.getByTestId("hit-assignment-panel");
    await expect(panel.or(button).first()).toBeVisible();
    if (await panel.isVisible()) return stageHits(panel, selected);
  }
  await button.click();
}

test("a complete human battle stays public in four independent views", async ({
  browser,
  request,
}) => {
  test.setTimeout(180_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_combat_four_views", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: sol,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  expect(Object.keys(seats)).toHaveLength(3);
  const contexts = await Promise.all(Array.from({ length: 4 }, () => browser.newContext()));
  const pages = await Promise.all(contexts.map((context) => context.newPage()));
  const ids = Object.keys(seats);
  try {
    await Promise.all(ids.map((id, index) => openPlayerGame(pages[index], game, seats[id])));
    await pages[3].goto(`/games/${game}`);
    await pages[3].getByRole("button", { name: "Watch" }).click();
    const views = new Map(ids.map((id, index) => [id, pages[index]]));
    let played = 0;
    let sawGroupedOpportunity = false;
    let shieldHits: number | undefined;
    let shieldRound: number | undefined;
    let shieldWasSabotaged = false;
    let checkedShieldHits = false;
    let checkedShieldCallout = false;
    let sawSustain = false;
    let firstShieldRound: number | undefined;
    let sawTwoShieldsInOneWindow = false;
    const phases = new Set<string>();
    let sawBarrage = false;
    let sawNewDamage = false;
    let sawDestroyed = false;
    let sawReturnFire = false;
    let sawNextRound = false;
    let finished = false;
    for (let step = 0; step < 180; step++) {
      const current = await snapshot(request, game, seats[sol]);
      if (!current.view.board.combat) {
        finished = true;
        break;
      }
      const combat = current.view.board.combat;
      phases.add(combat.phase ?? "");
      expect(combat.battle_seq).toBeGreaterThan(0);
      if (combat.round > 1) {
        sawNextRound = true;
        if (combat.phase === "pre_roll") {
          expect(combat.dice_rolls ?? []).toHaveLength(0);
          expect(combat.attacker_hits).toBeFalsy();
          expect(combat.defender_hits).toBeFalsy();
        }
      }
      if (combat.barrage_dice?.length) {
        sawBarrage = true;
        expect(combat.barrage_dice.some((die) => die.unit.includes("destroyer"))).toBe(true);
        expect(combat.barrage_hits?.[combat.attacker]).toBeGreaterThanOrEqual(0);
      }
      if (combat.phase === "resolving_hits") {
        expect(combat.round_start?.length).toBeGreaterThan(0);
        expect(combat.attacker_hits).toBeGreaterThanOrEqual(0);
        expect(combat.defender_hits).toBeGreaterThanOrEqual(0);
        const start = combat.round_start ?? [];
        const now = current.view.board.systems[combat.system_id].units;
        sawNewDamage ||= now.some(
          (unit) =>
            unit.damaged &&
            !start.some(
              (old) => old.owner === unit.owner && old.unit_type === unit.unit_type && old.damaged,
            ),
        );
        sawDestroyed ||= start.some(
          (unit) =>
            start.filter((old) => old.owner === unit.owner && old.unit_type === unit.unit_type)
              .length >
            now.filter((live) => live.owner === unit.owner && live.unit_type === unit.unit_type)
              .length,
        );
        sawReturnFire ||=
          (combat.remaining_hits?.[combat.attacker] ?? 0) > 0 &&
          (combat.remaining_hits?.[combat.defender] ?? 0) > 0;
      }
      const actor =
        await actorOf(request, game, seats, current);
      expect(actor).toBeTruthy();
      const actorPage = views.get(actor!);
      expect(actorPage).toBeDefined();
      const docked = actorPage!.getByTestId("resume-combat-btn");
      if (await docked.isVisible()) await docked.click();
      const wrappedDock = actorPage!.getByTestId("resume-decision-btn");
      if (await wrappedDock.isVisible()) await wrappedDock.click();
      const offer = await snapshot(request, game, seats[actor!]);
      const choice = offer.pending_choice?.choice;
      expect(choice).toBeDefined();
      expect(choice?.player).toBe(actor);
      // Every view sees the public fight, only the actor sees the offer.
      for (let i = 0; i < pages.length; i++) {
        const id = ids[i];
        const viewer = await snapshot(request, game, id ? seats[id] : undefined);
        expect(viewer.view.board.combat?.system_id).toBe(current.view.board.combat.system_id);
        expect(viewer.view.board.combat?.phase).toBe(combat.phase);
        expect(viewer.view.board.combat?.round_start).toEqual(combat.round_start);
        await expect(pages[i].getByTestId("combat-phase")).toHaveAttribute(
          "data-phase",
          combat.phase!,
        );
        await expectFleetPills(pages[i], viewer, step);
        if (combat.phase === "pre_roll") {
          const odds = pages[i].getByTestId("combat-odds-card");
          await expect(odds, `step ${step}, viewer ${id ?? "spectator"}`).toBeVisible();
          await expect(odds.locator(".combat-odds-col__pct")).toHaveText([/\d+%/, /\d+%/]);
        } else {
          await expect(pages[i].getByTestId("combat-odds-card")).toHaveCount(0);
        }
        if (
          combat.phase === "pre_roll" &&
          !(combat.round === 1 && Object.keys(combat.barrage_hits ?? {}).length > 0)
        )
          await expect(pages[i].getByTestId("combat-round-hits")).toHaveCount(0);
        if (id !== actor) {
          expect(viewer.pending_choice).toBeFalsy();
          expect(
            viewer.view.players.find((p) => p.id === actor)?.held_action_cards,
          ).toBeUndefined();
          await expect(pages[i].getByTestId("combat-resolution-modal")).toBeVisible();
          await expect(pages[i].getByTestId("spectator-combat-notice")).toBeVisible();
          await expect(
            pages[i]
              .getByTestId("combat-resolution-modal")
              .getByRole("button", { name: /Play (Shields Holding|Direct Hit|Sabotage)/ }),
            `step ${step}, viewer ${id ?? "spectator"}, actor ${actor}`,
          ).toHaveCount(0);
        }
      }
      if (
        choice?.context?.space_battle ||
        choice?.context?.subtype === "announce_retreat" ||
        choice?.context?.subtype === "sustain_damage" ||
        choice?.context?.subtype === "assign_casualty"
      ) {
        await expect(
          actorPage!.getByTestId("combat-resolution-modal"),
          `step ${step} ${choice?.prompt}`,
        ).toBeVisible();
      }
      const options = choice!.options;
      const subtype = choice!.context?.subtype ?? "";
      if (shieldHits !== undefined && current.view.board.combat?.round !== shieldRound) {
        if (!shieldWasSabotaged && shieldHits <= 2) checkedShieldHits = true;
        shieldHits = undefined;
      }
      if (
        shieldHits !== undefined &&
        (subtype === "sustain_damage" || subtype === "assign_casualty")
      ) {
        if (!shieldWasSabotaged) {
          const remaining = Number(
            await actorPage!
              .getByTestId("combat-hits-callout")
              .locator(".combat-hits-callout__count")
              .textContent(),
          );
          expect
            .soft(remaining, "Shields Holding must cancel two hits before assignment")
            .toBe(Math.max(0, shieldHits - 2));
          checkedShieldHits = true;
        }
        shieldHits = undefined;
      }
      if (
        shieldHits !== undefined &&
        shieldHits <= 2 &&
        !shieldWasSabotaged &&
        !subtype.startsWith("reaction_") &&
        !subtype.startsWith("play_reaction_")
      ) {
        checkedShieldHits = true;
        shieldHits = undefined;
      }
      const card =
        options.find((opt) => opt.kind === "ability" && typeof opt.payload?.card === "string") ??
        options.find((opt) => opt.kind === "action_card") ??
        (actor === sol
          ? options.find(
              (opt) => opt.kind === "ability" && opt.label.startsWith("Choose an action card"),
            )
          : undefined);
      const hand =
        offer.view.players.find((player) => player.id === actor)?.held_action_cards ?? [];
      if (
        hand.includes("sh1") &&
        hand.includes("sh2") &&
        subtype.startsWith("reaction_") &&
        subtype.includes("HITS_TO_ASSIGN")
      ) {
        sawGroupedOpportunity = true;
        expect(options.filter((option) => option.kind === "ability")).toHaveLength(1);
        expect
          .soft(card?.label, "two copies of the only applicable card need one Play button")
          .toBe("Play Shields Holding");
        await expect
          .soft(actorPage!.getByRole("button", { name: "Play Shields Holding" }))
          .toBeVisible();
      }
      let selected: ChoiceOption;
      if (card && (actor === sol || String(card.payload?.card_name) === "Sabotage")) {
        selected = card;
      } else if (subtype === "announce_retreat") {
        selected = options.find((opt) => opt.id === "stay") ?? options[0];
      } else if (subtype.startsWith("reaction_")) {
        selected = options.find((opt) => opt.id === "decline") ?? options[0];
      } else if (subtype === "sustain_damage") {
        selected = !sawSustain
          ? (options.find((opt) => opt.kind === "sustain") ?? options[0])
          : (options.find((opt) => opt.id === "decline") ?? options[0]);
      } else {
        selected =
          options.find((opt) => opt.id !== "retreat" && opt.id !== "decline") ?? options[0];
      }
      if (selected.payload?.card_name === "Shields Holding") {
        shieldHits = current.view.board.combat?.hits_to_assign ?? undefined;
        shieldRound = current.view.board.combat?.round;
        shieldWasSabotaged = false;
      }
      if (selected.payload?.card_name === "Sabotage" && shieldHits !== undefined) {
        shieldWasSabotaged = true;
      }
      const selector =
        subtype.startsWith("reaction_") || subtype.startsWith("play_reaction_")
          ? selected.id === "decline"
            ? "pass-combat-reaction-btn"
            : selected.payload?.card_name === "Direct Hit"
              ? "play-direct-hit-btn"
              : `combat-reaction-${selected.id}`
          : subtype === "announce_retreat" || subtype === "retreat_to"
            ? `retreat-opt-${selected.id}`
            : subtype === "sustain_damage"
              ? selected.id === "decline"
                ? "decline-sustain-btn"
                : `sustain-opt-${selected.id}`
              : subtype === "assign_casualty"
                ? `casualty-opt-${selected.id}`
                : `combat-follow-up-${selected.id}`;
      if (!choice?.context && choice?.prompt.startsWith("remove a unit:")) {
        await actorPage!.getByRole("radio", { name: selected.label }).click();
        await actorPage!.getByRole("button", { name: "Confirm choice" }).click();
      } else {
        await answerCombat(actorPage!, selector, subtype, selected);
      }
      await expect
        .poll(async () => (await snapshot(request, game, seats[sol])).game_version)
        .toBeGreaterThan(current.game_version);
      const after = await snapshot(request, game, seats[sol]);
      if (selected.payload?.card_name === "Shields Holding" && actor === sol) {
        const heldShields = hand.filter((alias) => alias === "sh1" || alias === "sh2").length;
        if (heldShields === 2) {
          firstShieldRound = combat.round;
          const nextChoice = (await snapshot(request, game, seats[sol])).pending_choice?.choice;
          expect(nextChoice?.context?.subtype).toContain("HITS_TO_ASSIGN");
          expect(
            nextChoice?.options.some((option) => option.label === "Play Shields Holding"),
          ).toBe(true);
          await expect(
            actorPage!.getByRole("button", { name: "Play Shields Holding" }),
          ).toBeVisible();
        } else if (heldShields === 1 && firstShieldRound === combat.round) {
          sawTwoShieldsInOneWindow = true;
          expect(after.pending_choice?.choice.context?.subtype ?? "").not.toContain(
            "HITS_TO_ASSIGN",
          );
        }
      }
      if (selected.payload?.card_name === "Salvage" && !after.view.board.combat) {
        for (const page of pages) {
          await expect(page.getByTestId("combat-resolution-modal")).toHaveCount(0);
        }
      }
      if (subtype === "sustain_damage" && selected.kind === "sustain") {
        // The hit panel sends the other hits of this volley in the same plan: compare the
        // sustaining type only.
        const type = String(selected.payload?.unit ?? "");
        const ships = (state: InitialSnapshotMsg) =>
          state.view.board.systems[combat.system_id].units.filter(
            (unit) => unit.owner === actor && !unit.planet && unit.unit_type === type,
          );
        expect(ships(after).length, `step ${step}: sustain keeps the ship`).toBe(
          ships(current).length,
        );
        expect(
          ships(after).filter((unit) => unit.damaged).length,
          `step ${step}: exactly one ship sustains`,
        ).toBe(ships(current).filter((unit) => unit.damaged).length + 1);
        if (after.view.board.combat) {
          for (const page of pages) await expectFleetPills(page, after, step);
        }
      }
      if (
        selected.payload?.card_name === "Shields Holding" &&
        shieldHits !== undefined &&
        after.pending_choice?.choice.context?.subtype?.includes("HITS_TO_ASSIGN") &&
        after.view.board.combat?.round === shieldRound
      ) {
        const expectedHits = Math.max(0, shieldHits - 2);
        for (let i = 0; i < pages.length; i++) {
          const id = ids[i];
          const view = await snapshot(request, game, id ? seats[id] : undefined);
          // No hits left to assign: the field is left out.
          expect(
            view.view.board.combat?.hits_to_assign ?? 0,
            `Shields Holding must update incoming hits for ${id ?? "spectator"}`,
          ).toBe(expectedHits);
          const callout = pages[i].getByTestId("combat-hits-callout");
          if (expectedHits === 0) {
            await expect(callout, `cancelled hits in view ${id ?? "spectator"}`).toBeHidden();
          } else {
            await expect(
              callout.locator(".combat-hits-callout__count"),
              `remaining hits in view ${id ?? "spectator"}`,
            ).toHaveText(String(expectedHits));
          }
        }
        checkedShieldCallout = true;
      }
      if (
        sawGroupedOpportunity &&
        subtype.startsWith("reaction_") &&
        subtype.includes("HITS_TO_ASSIGN") &&
        selected.label === "Play Shields Holding"
      ) {
        const next = await snapshot(request, game, seats[sol]);
        expect
          .soft(
            next.pending_choice?.choice.context?.subtype?.startsWith("play_reaction_") ?? false,
            "one Shields Holding click must not ask for a physical copy",
          )
          .toBe(false);
        expect
          .soft(
            next.view.players.find((player) => player.id === sol)?.held_action_cards?.length,
            "one click consumes exactly one Shields Holding copy",
          )
          .toBe(hand.length - 1);
      }
      const expectedDetail =
        selected.payload?.card_name && !selected.label.startsWith("Choose an action card")
          ? `played ${selected.payload.card_name}`
          : subtype === "sustain_damage" && selected.id !== "decline"
            ? "sustained damage"
            : undefined;
      if (expectedDetail) {
        if (expectedDetail === "sustained damage") sawSustain = true;
        // Check before anyone answers the next reaction, not just at battle completion.
        const newlyLogged = (after.events ?? []).filter(
          (event) =>
            event.detail?.includes(expectedDetail) &&
            !(current.events ?? []).some((old) => old.id === event.id),
        );
        expect
          .soft(newlyLogged, `${expectedDetail} must be logged at this decision boundary`)
          .toHaveLength(1);
        if (newlyLogged.length === 1)
          for (let i = 0; i < pages.length; i++) {
            const id = ids[i];
            const view = await snapshot(request, game, id ? seats[id] : undefined);
            expect
              .soft(
                view.events?.some((event) => event.id === newlyLogged[0]?.id),
                `${expectedDetail} visible to ${id ?? "spectator"} before the next response`,
              )
              .toBe(true);
            // The modal intercepts clicks outside it; dock it to inspect the live log.
            const modal = pages[i].getByTestId("combat-resolution-modal");
            await expect(async () => {
              if (await modal.isVisible())
                await modal.getByTestId("close-combat-modal").click({ timeout: 1_000 });
            }).toPass();
            const generic = pages[i].getByTestId("pending-choice-dialog");
            if (await generic.isVisible())
              await generic.getByRole("button", { name: "Minimize decision" }).click();
            const toggle = pages[i].getByTestId("event-log-toggle");
            if ((await toggle.getAttribute("aria-expanded")) === "false") {
              // A new decision can open the actor's modal while the other views are checked.
              await expect(async () => {
                if (await modal.isVisible()) await modal.getByTestId("close-combat-modal").click();
                if (await generic.isVisible())
                  await generic.getByRole("button", { name: "Minimize decision" }).click();
                await toggle.click({ timeout: 1_000 });
              }).toPass();
            }
            const log = pages[i].getByTestId("event-log-list");
            await expect(async () => {
              // A completed reaction can hand off to combat before this view renders it.
              // Browse its collapsed stage instead of assuming the active path is still reactions.
              const reactions = log.getByRole("button", { name: /Reactions/ }).last();
              if (
                (await reactions.count()) > 0 &&
                (await reactions.getAttribute("aria-expanded")) === "false"
              ) {
                await reactions.click();
              }
              await expect(log, `live event log for ${id ?? "spectator"}`).toContainText(
                expectedDetail,
              );
            }).toPass({ timeout: 5_000 });
            const resume = pages[i].getByTestId("resume-combat-btn");
            if (await resume.isVisible()) await resume.click();
            const wrappedResume = pages[i].getByTestId("resume-decision-btn");
            if (await wrappedResume.isVisible()) await wrappedResume.click();
          }
      }
      const facts = (after.events ?? []).filter((event) => event.detail?.includes(" played "));
      for (const fact of facts.slice(played)) {
        for (let i = 0; i < pages.length; i++) {
          const id = ids[i];
          const view = await snapshot(request, game, id ? seats[id] : undefined);
          expect(view.events?.find((event) => event.id === fact.id)?.detail).toBe(fact.detail);
        }
      }
      played = facts.length;
    }
    expect(finished, "battle must end, including its victory window").toBe(true);
    expect(sawGroupedOpportunity, "the scenario must offer both copies together").toBe(true);
    expect(sawSustain, "the battle must exercise sustain damage").toBe(true);
    expect(phases.has("pre_roll"), "the opening must show pre-roll fleets").toBe(true);
    expect(phases.has("resolving_hits"), "normal rolls must reach hit resolution").toBe(true);
    expect(sawBarrage, "the first round must show anti-fighter barrage dice").toBe(true);
    expect(sawNewDamage, "sustain must mark a surviving ship damaged").toBe(true);
    expect(sawDestroyed, "casualties must reduce ship counts").toBe(true);
    expect(sawReturnFire, "both sides' hits must be queued together").toBe(true);
    expect(sawNextRound, "the battle must show later rounds").toBe(true);
    expect(checkedShieldHits, "Shields Holding must cancel hits before assignment").toBe(true);
    expect(checkedShieldCallout, "Shields Holding must update the live hits callout").toBe(true);
    expect(
      sawTwoShieldsInOneWindow,
      "both Shields Holding copies can resolve in one reaction window",
    ).toBe(true);
    expect(played).toBeGreaterThan(0);
    const snapshots = await Promise.all(ids.map((id) => snapshot(request, game, seats[id])));
    snapshots.push(await snapshot(request, game));
    for (const page of pages) await page.reload();
    for (const page of pages)
      await expect(page.getByTestId("combat-resolution-modal")).toHaveCount(0);
    const watch = pages[3].getByRole("button", { name: "Watch" });
    await expect(watch).toBeVisible();
    await watch.click();
    for (const view of snapshots) {
      expect(view.view.board.combat).toBeFalsy();
      expect(
        (view.events ?? []).filter((event) => event.detail?.includes(" played ")),
      ).toHaveLength(played);
    }
    for (const view of snapshots.slice(1)) {
      expect(view.events?.map((event) => event.detail)).toEqual(
        snapshots[0].events?.map((event) => event.detail),
      );
      expect(view.history?.cursor).toBe(snapshots[0].history?.cursor);
    }
    const publicPlays = (snapshots[0].events ?? []).filter((event) =>
      event.detail?.includes(" played "),
    );
    expect(
      publicPlays.filter((event) => event.detail?.endsWith("played Shields Holding")),
    ).toHaveLength(2);
    const holder = await snapshot(request, game, seats[sol]);
    for (const copy of ["sh1", "sh2"]) {
      expect(
        holder.view.players.find((player) => player.id === sol)?.held_action_cards ?? [],
      ).not.toContain(copy);
    }
    for (const id of ids.filter((id) => id !== sol)) {
      expect(
        (await snapshot(request, game, seats[id])).view.players.find((player) => player.id === sol)
          ?.held_action_cards,
      ).toBeUndefined();
    }
    for (const name of ["Shields Holding", "Sabotage", "Salvage"]) {
      expect(
        publicPlays.some((event) => event.detail?.endsWith(`played ${name}`)),
        `${name}: ${publicPlays.map((event) => event.detail).join("; ")}`,
      ).toBe(true);
    }
    for (const [index, page] of pages.entries()) {
      await expect(page.getByTestId("event-log-toggle"), `reloaded view ${index}`).toBeVisible();
      const recap = page.getByTestId("close-combat-modal");
      if (await recap.isVisible()) await recap.click();
      const generic = page.getByTestId("pending-choice-dialog");
      if (await generic.isVisible())
        await generic.getByRole("button", { name: "Minimize decision" }).click();
      await page.getByTestId("event-log-toggle").click();
      // Rounds and phases fold; an open phase lists its actions and stages in full.
      for (const label of [/Round 1/, /Action phase/]) {
        const branch = page.getByRole("button", { name: label });
        if ((await branch.getAttribute("aria-expanded")) === "false") await branch.click();
      }
      for (const play of publicPlays) {
        await expect(page.getByTestId("event-log-container")).toContainText(
          play.detail!.split(" played ")[1],
        );
      }
    }
  } finally {
    await Promise.all(contexts.map((context) => context.close()));
  }
});

test("undo during a Shields Holding reaction preserves a healthy, playable game", async ({
  browser,
  request,
}) => {
  test.setTimeout(90_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_combat_four_views", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: host,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  const contexts = await Promise.all(Object.keys(seats).map(() => browser.newContext()));
  const pages = await Promise.all(contexts.map((context) => context.newPage()));
  const views = new Map(Object.keys(seats).map((id, index) => [id, pages[index]]));
  try {
    await Promise.all(
      Object.keys(seats).map((id) => openPlayerGame(views.get(id)!, game, seats[id])),
    );
    const page = views.get(host)!;

    let atShields = false;
    for (let step = 0; step < 40; step++) {
      const state = await snapshot(request, game, seats[host]);
      const actor =
        await actorOf(request, game, seats, state);
      expect(actor).toBeTruthy();
      const offer = await snapshot(request, game, seats[actor!]);
      const choice = offer.pending_choice?.choice;
      expect(choice).toBeDefined();
      const subtype = choice!.context?.subtype ?? "";
      const shields = choice!.options.find(
        (option) =>
          subtype.includes("HITS_TO_ASSIGN") &&
          (option.label === "Play Shields Holding" ||
            option.label.startsWith("Choose an action card")),
      );
      if (shields && actor === host) {
        atShields = true;
        // Resolve the grouped card offer before rewinding the host timeline.
        await page.getByTestId(`combat-reaction-${shields.id}`).click();
        await expect
          .poll(async () => (await snapshot(request, game, seats[host])).game_version)
          .toBeGreaterThan(state.game_version);
        expect(
          (await snapshot(request, game, seats[host])).pending_choice?.choice.context?.subtype ??
            "",
        ).not.toContain("play_reaction_");
        break;
      }
      const selected =
        choice!.options.find((option) => option.payload?.card_name === "Direct Hit") ??
        choice!.options.find((option) => option.id === "stay" || option.id === "decline") ??
        choice!.options[0];
      const actorPage = views.get(actor!)!;
      const selector =
        subtype === "announce_retreat"
          ? `retreat-opt-${selected.id}`
          : subtype.startsWith("reaction_")
            ? selected.id === "decline"
              ? "pass-combat-reaction-btn"
              : selected.payload?.card_name === "Direct Hit"
                ? "play-direct-hit-btn"
                : `combat-reaction-${selected.id}`
            : subtype === "sustain_damage"
              ? selected.id === "decline"
                ? "decline-sustain-btn"
                : `sustain-opt-${selected.id}`
              : subtype === "assign_casualty"
                ? `casualty-opt-${selected.id}`
                : `combat-follow-up-${selected.id}`;
      await answerCombat(actorPage, selector, subtype, selected);
      await expect
        .poll(async () => (await snapshot(request, game, seats[host])).game_version)
        .toBeGreaterThan(state.game_version);
    }
    expect(atShields, "the seeded combat must open the Shields Holding reaction").toBe(true);
    const before = await snapshot(request, game, seats[host]);
    const response = await request.post(`${backend}/api/games/${game}/history`, {
      headers: { "x-ti4-player-session": seats[host] },
      data: { action: "undo", expected_version: before.game_version },
    });
    // Even a failed history request must not terminate the server.
    const health = await request.get(`${backend}/api/games/${game}/snapshot`, {
      headers: { "x-ti4-player-session": seats[host] },
    });
    expect(health.ok(), `snapshot after undo: ${health.status()}`).toBe(true);
    expect(
      response.ok(),
      `undo during Shields Holding: ${response.status()} ${await response.text()}`,
    ).toBe(true);
    const restored = (await health.json()) as InitialSnapshotMsg;
    expect(restored.history?.cursor).toBeLessThan(before.history!.cursor);
    const restoredActor =
      await actorOf(request, game, seats, restored);
    expect(restoredActor).toBeTruthy();
    expect(
      (await snapshot(request, game, seats[restoredActor!])).pending_choice?.choice,
    ).toBeDefined();
    await expect(page.getByTestId("combat-resolution-modal")).toBeVisible();
    await expect(page.getByTestId("combat-error-banner")).toBeHidden();
  } finally {
    await Promise.all(contexts.map((context) => context.close()));
  }
});

test("replaying Direct Hit, Sabotage and Shields Holding after undo keeps the game playable", async ({
  browser,
  request,
}) => {
  test.setTimeout(110_000);
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "ongoing_combat_four_views", seed: 42 },
  });
  expect(launch.ok(), await launch.text()).toBe(true);
  const {
    game_id: game,
    player_id: sol,
    test_seats: seats,
  } = (await launch.json()) as {
    game_id: string;
    player_id: string;
    test_seats: Record<string, string>;
  };
  const ids = Object.keys(seats);
  const contexts = await Promise.all(ids.map(() => browser.newContext()));
  const pages = await Promise.all(contexts.map((context) => context.newPage()));
  const views = new Map(ids.map((id, index) => [id, pages[index]]));
  try {
    await Promise.all(ids.map((id) => openPlayerGame(views.get(id)!, game, seats[id])));
    const letnev = (await snapshot(request, game, seats[sol])).view.players.find(
      (player) => player.faction === "letnev",
    )?.id;
    expect(letnev).toBeTruthy();

    async function advanceChain(replayed: boolean) {
      const seen = { sustain: replayed, directHit: false, sabotage: false, shields: false };
      for (let step = 0; step < 55; step++) {
        const before = await snapshot(request, game, seats[sol]);
        const actor =
          await actorOf(request, game, seats, before);
        expect(actor, `chain ${replayed ? "replayed" : "original"}, step ${step}`).toBeTruthy();
        const offer = await snapshot(request, game, seats[actor!]);
        const choice = offer.pending_choice?.choice;
        expect(choice).toBeDefined();
        const subtype = choice!.context?.subtype ?? "";
        const options = choice!.options;
        const actorPage = views.get(actor!)!;
        const resume = actorPage.getByTestId("resume-combat-btn");
        if (await resume.isVisible()) await resume.click();
        const wrappedResume = actorPage.getByTestId("resume-decision-btn");
        if (await wrappedResume.isVisible()) await wrappedResume.click();

        const named = (name: string) =>
          options.find(
            (option) => option.payload?.card_name === name || option.label === `Play ${name}`,
          );
        let selected: ChoiceOption;
        if (!seen.sustain && actor === letnev && subtype === "sustain_damage") {
          selected = options.find((option) => option.kind === "sustain")!;
          expect(selected).toBeDefined();
          seen.sustain = true;
        } else if (!seen.directHit && actor === sol && named("Direct Hit")) {
          selected = named("Direct Hit")!;
          seen.directHit = true;
        } else if (!seen.sabotage && actor === letnev && named("Sabotage")) {
          selected = named("Sabotage")!;
          seen.sabotage = true;
        } else if (!seen.shields && actor === sol && subtype.includes("HITS_TO_ASSIGN")) {
          selected =
            named("Shields Holding") ??
            options.find((option) => option.label.startsWith("Choose an action card"))!;
          expect(selected).toBeDefined();
          seen.shields = true;
        } else if (subtype.startsWith("play_reaction_") && actor === sol && seen.shields) {
          selected = options.find((option) => option.payload?.card_name === "Shields Holding")!;
          expect(selected).toBeDefined();
        } else if (subtype === "announce_retreat") {
          selected = options.find((option) => option.id === "stay")!;
        } else {
          selected = options.find((option) => option.id === "decline") ?? options[0];
        }

        const selector =
          subtype.startsWith("reaction_") || subtype.startsWith("play_reaction_")
            ? selected.id === "decline"
              ? "pass-combat-reaction-btn"
              : selected.payload?.card_name === "Direct Hit"
                ? "play-direct-hit-btn"
                : `combat-reaction-${selected.id}`
            : subtype === "announce_retreat" || subtype === "retreat_to"
              ? `retreat-opt-${selected.id}`
              : subtype === "sustain_damage"
                ? selected.id === "decline"
                  ? "decline-sustain-btn"
                  : `sustain-opt-${selected.id}`
                : subtype === "assign_casualty"
                  ? `casualty-opt-${selected.id}`
                  : `combat-follow-up-${selected.id}`;
        await answerCombat(actorPage, selector, subtype, selected);
        await expect
          .poll(async () => {
            const response = await request.get(`${backend}/api/games/${game}/snapshot`, {
              headers: { "x-ti4-player-session": seats[sol] },
            });
            expect(
              response.ok(),
              `${replayed ? "replay" : "original"} step ${step} after ${actor} ${selected.label}: ` +
                `${response.status()} ${response.ok() ? "" : await response.text()}`,
            ).toBe(true);
            return ((await response.json()) as InitialSnapshotMsg).game_version;
          })
          .toBeGreaterThan(before.game_version);
        const after = await snapshot(request, game, seats[sol]);
        if (
          seen.sustain &&
          seen.directHit &&
          seen.sabotage &&
          seen.shields &&
          (after.events ?? []).some((event) => event.detail?.endsWith("played Shields Holding")) &&
          !after.pending_choice?.choice.context?.subtype?.startsWith("play_reaction_")
        ) {
          return after;
        }
      }
      throw new Error(`Combat never completed reaction chain: ${JSON.stringify(seen)}`);
    }

    const first = await advanceChain(false);
    const directHit = first.events?.find((event) => event.detail?.endsWith("played Direct Hit"));
    expect(
      directHit?.decision_count,
      "Direct Hit must have a rewindable decision cursor",
    ).toBeGreaterThan(0);
    const cursor = directHit!.decision_count! - 1;
    const rewind = await request.post(`${backend}/api/games/${game}/history`, {
      headers: { "x-ti4-player-session": seats[sol] },
      data: { action: "restore_cursor", cursor, expected_version: first.game_version },
    });
    expect(rewind.ok(), `undo to Direct Hit: ${rewind.status()} ${await rewind.text()}`).toBe(true);
    const restored = await snapshot(request, game, seats[sol]);
    expect(restored.history?.cursor).toBe(cursor);
    expect(restored.view.players.find((player) => player.id === sol)?.held_action_cards).toContain(
      "dh1",
    );
    const second = await advanceChain(true);
    expect(second.view.board.combat).toBeTruthy();
    expect(second.events?.some((event) => event.detail?.endsWith("played Sabotage"))).toBe(true);
    // A replay failure may close the game session or the HTTP server entirely.
    const health = await request.get(`${backend}/api/games/${game}/snapshot`, {
      headers: { "x-ti4-player-session": seats[sol] },
    });
    expect(health.ok(), `snapshot after replay: ${health.status()} ${await health.text()}`).toBe(
      true,
    );
  } finally {
    await Promise.all(contexts.map((context) => context.close()));
  }
});
