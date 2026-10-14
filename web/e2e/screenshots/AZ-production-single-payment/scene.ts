import { expect, type Page } from "@playwright/test";
import { PROTOCOL_VERSION, type ChoiceOptionDto, type ClientMessage, type PlacedUnitView } from "../../../src/protocol/types";
import { GAME_ID, openMockedGame, type MockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, actor } from "../_shared/players";
import { leadershipBoard } from "../V-leadership-map-payment/leadership";

/** Which side of the change a run is: `before` asks for each build's payment (the setting turned off). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const SETTING_KEY = "player_single_production_payment";
const SYSTEM = "18";
const COST = 2; // a cruiser

/** Resources each planet pays (Jord and Quann 2, Lodor 3). */
const WORTH: Record<string, number> = { jord: 2, lodor: 3, quann: 2 };

export type Interruption = "none" | "reaction" | "mismatch";

export interface Ledger {
  /** Every batch the page sent, in order. */
  batches: { kind: string; steps: unknown[] }[];
  /** Payment questions the engine asked the player. */
  payQuestions: number;
}

export interface ProductionScene {
  game: MockedGame;
  ledger: Ledger;
}

const piece = (unit_type: string): PlacedUnitView => ({ unit_type, owner: actor, damaged: false });

const reactionWindow = (): { prompt: string; context: Record<string, unknown>; options: ChoiceOptionDto[] } => ({
  prompt: "after units are produced: use a card?",
  context: { subtype: "reaction_after_UNITS_PRODUCED" },
  options: [
    { id: "reaction:cost_cutter", kind: "ability", label: "Play Mining Initiative", payload: { card: "mining_initiative" } },
    { id: "decline", kind: "decline", label: "Decline" },
  ],
});

/**
 * A game where the viewer produces cruisers in Mecatol Rex (system 18) and the page talks to a small
 * scripted engine: one cruiser costs 2, the engine asks to pay build by build, overpayment is kept
 * as credit for the next build, and placement is left out. Batches go to /batches like the real
 * client sends them; the reaction answer goes over the socket.
 */
export async function openProduction(
  page: Page,
  { interruption = "none" as Interruption, single = prefix !== "before" } = {},
): Promise<ProductionScene> {
  await page.addInitScript(
    ([key, value]) => {
      try {
        localStorage.setItem(key, value);
      } catch {
        // unavailable: the default applies
      }
    },
    [SETTING_KEY, single ? "true" : "false"],
  );
  const ledger: Ledger = { batches: [], payQuestions: 0 };
  const exhausted = new Set<string>();
  const sim = { built: 0, credit: 0, owed: 0, version: 40, queuedReaction: false };

  const ready = () => Object.keys(WORTH).filter((id) => !exhausted.has(id));
  const worthLeft = () => ready().reduce((sum, id) => sum + WORTH[id], 0) + sim.credit;

  const produceChoice = () => ({
    prompt: `Produce units in system ${SYSTEM}`,
    context: {
      subtype: "produce_unit",
      target: { System: SYSTEM },
      outstanding: [{ kind: "production_capacity", amount: 6, paid: sim.built }],
    },
    options: [
      {
        id: "build|cruiser|1",
        kind: "produce",
        label: `produce cruiser for ${COST}`,
        payload: { unit: "cruiser", cost: COST, printed_cost: COST, count: 1, production_spent: 1, available_resources: worthLeft() - sim.credit, credit: sim.credit },
      },
      { id: "done_producing", kind: "decline", label: "Done producing", payload: {} },
    ] as ChoiceOptionDto[],
  });

  const payChoice = (owed: number, only?: string[]) => ({
    prompt: `Pay ${owed} resources for the cruiser`,
    context: {
      subtype: "pay_resources",
      target: { System: SYSTEM },
      outstanding: [{ kind: "Resources", amount: owed, paid: 0 }],
    },
    options: (only ?? ready()).map((id) => ({
      id: `exhaust|${id}`,
      kind: "pay",
      label: `exhaust ${id} for ${WORTH[id]} resources`,
      payload: { worth: WORTH[id], planet_name: id[0].toUpperCase() + id.slice(1) },
    })) as ChoiceOptionDto[],
  });

  const board = () => ({
    ...leadershipBoard,
    systems: {
      ...leadershipBoard.systems,
      [SYSTEM]: {
        ...leadershipBoard.systems[SYSTEM],
        units: [...leadershipBoard.systems[SYSTEM].units, ...Array.from({ length: sim.built }, () => piece("cruiser"))],
        planets: {
          ...leadershipBoard.systems[SYSTEM].planets,
          jord: { ...leadershipBoard.systems[SYSTEM].planets.jord, exhausted: exhausted.has("jord") },
        },
      },
      "26": { ...leadershipBoard.systems["26"], planets: { lodor: { ...leadershipBoard.systems["26"].planets.lodor, exhausted: exhausted.has("lodor") } } },
      "25": { ...leadershipBoard.systems["25"], planets: { quann: { ...leadershipBoard.systems["25"].planets.quann, exhausted: exhausted.has("quann") } } },
    },
  });

  let game!: MockedGame;
  const show = (choice: { prompt: string; context: Record<string, unknown>; options: ChoiceOptionDto[] }) => {
    sim.version += 1;
    const snapshot = game.snapshot as unknown as Record<string, any>;
    snapshot.game_version = sim.version;
    snapshot.view.board = board();
    snapshot.pending_choice = { nonce: `az-${sim.version}`, choice: { player: actor, ...choice } };
    snapshot.turn_status = { kind: "waiting_for_decision", seat: actor, phase: "action", round: 2, stage: String(choice.context.subtype) };
  };

  /** The engine's next question after a build is started: its payment, or a reaction window first. */
  const afterBuildStarted = () => {
    const owed = Math.max(0, COST - sim.credit);
    sim.credit -= COST - owed;
    if (owed === 0) {
      sim.built += 1;
      show(produceChoice());
      return;
    }
    sim.owed = owed;
    if (interruption === "reaction" && sim.built === 1 && !sim.queuedReaction) {
      sim.queuedReaction = true;
      show(reactionWindow());
      return;
    }
    ledger.payQuestions += 1;
    const mismatched = interruption === "mismatch" && sim.built === 1;
    show(payChoice(owed, mismatched ? ["quann"] : undefined));
  };

  await page.route(`**/api/games/${GAME_ID}/batches`, async (route) => {
    const body = route.request().postDataJSON() as { plan: { kind: string; steps: Record<string, unknown>[] } };
    ledger.batches.push({ kind: body.plan.kind, steps: body.plan.steps });
    if (body.plan.kind === "production") {
      for (const _ of body.plan.steps) afterBuildStarted();
    } else if (body.plan.kind === "payment") {
      let paid = 0;
      for (const step of body.plan.steps) {
        const planet = String(step.planet);
        exhausted.add(planet);
        paid += WORTH[planet];
      }
      sim.credit += paid - sim.owed;
      sim.built += 1;
      show(produceChoice());
    }
    const { type: _type, ...snapshot } = game.snapshot as unknown as Record<string, unknown>;
    await route.fulfill({ json: { snapshot, active: true } });
  });

  game = await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 0 }), opponent],
    board: leadershipBoard,
    choice: { ...produceChoice(), nonce: "az-40" },
    onClientMessage: (message: ClientMessage, reply) => {
      if (message.type !== "submit_choice") return;
      // The reaction window is declined: the build's payment follows.
      if (message.option_id === "decline" && sim.queuedReaction && sim.owed > 0) {
        ledger.payQuestions += 1;
        show(payChoice(sim.owed));
        const { events: _events, ...base } = game.snapshot;
        reply({ type: "action_accepted", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, game_version: sim.version, option_id: message.option_id });
        reply({ ...base, type: "state_update" });
      }
    },
  });
  await page.getByTestId("production-builder-drawer").waitFor();
  return { game, ledger };
}

/** Stages three cruisers and confirms the builds. */
export async function stageAndConfirm(page: Page, count = 3) {
  for (let i = 0; i < count; i++) await page.getByTestId("produce-unit-btn-build|cruiser|1").click();
  await expect(page.getByTestId("produce-count-build|cruiser|1")).toHaveText(String(count));
}

export async function confirmBuilds(page: Page) {
  await page.getByRole("button", { name: "Confirm builds" }).click();
}

/** Waits for the payment panel of the current question and for its suggestion to be staged. */
export async function waitForPanel(page: Page) {
  await page.getByTestId("payment-drawer").waitFor();
  await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
}

export const phone = { width: 390, height: 844 };
