import { describe, expect, it } from "vitest";
import {
  describeBlocker,
  describePayment,
  pendingFromEngine,
  toExactResult,
} from "./secondaryPreview.ts";
import { buildDryChoice, isDryNonce, nextDryStep } from "./dryChoice.ts";
import { detectStrategicAction } from "./strategicAction.ts";
import {
  describeBuilds,
  normalizePlan,
  parseStoredPlan,
  researchableEstimate,
  resolveStep,
  serializeStoredPlan,
  describePlan,
  type SecondaryPlan,
} from "./secondaryPlan.ts";
import { planBuilds } from "./productionDraft.ts";
import {
  engineProduction,
  enginePayment,
  engineResearch,
  engineWindow,
  playedLog,
  player,
  previewReply,
} from "../test/secondaryPrepFixtures.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const actionFor = (card: string, name: string) => {
  const players = [player("a", { strategy_cards: [card] }), player("b")];
  return {
    players,
    action: detectStrategicAction({ events: playedLog(name), players, phase: "action" })!,
  };
};

describe("the server's preview reply", () => {
  it("maps each outcome, and a refusal or an unavailable answer to 'use the estimate'", () => {
    const choice = engineResearch(["amd"]);
    expect(toExactResult(previewReply({ status: "question", choice, step: 1 }))).toMatchObject({
      kind: "question",
      choice,
      skipped: [],
    });
    expect(
      toExactResult(previewReply({ status: "would_not_be_asked", blocker: "cannot_pay_resources" })),
    ).toEqual({ kind: "not_asked", blocker: "cannot_pay_resources" });
    expect(toExactResult(previewReply({ status: "complete" }))).toEqual({ kind: "complete", unused: 0 });
    expect(toExactResult(previewReply({ status: "unavailable", detail: "x" })).kind).toBe("none");
    expect(
      toExactResult({ kind: "refused", reason: "already_asked", detail: "asked" }),
    ).toEqual({ kind: "none", why: "refused: already_asked" });
  });

  it("turns the engine's choice into the shell's decision under a client-made nonce", () => {
    const pending = pendingFromEngine(engineProduction([{ unit: "infantry", cost: 1, count: 2 }]), "prepare:k:produce");
    expect(pending.actor).toBe("b");
    expect(pending.nonce).toBe("prepare:k:produce");
    expect(isDryNonce(pending.nonce)).toBe(true);
    expect(pending.context?.subtype).toBe("produce_unit");
    expect(pending.options[0].payload?.unit).toBe("infantry");
    expect(pending.details?.fleet_supply).toEqual({ used: 1, limit: 4 });
  });

  it("says in words why a seat would not be asked, and how the engine would pay", () => {
    expect(describeBlocker("cannot_pay_resources")).toMatch(/cannot pay the 4 resources/);
    expect(describeBlocker("no_strategy_token")).toMatch(/kept and applies if that changes/);
    expect(describePayment({ cost: 4, planets: ["jord"], trade_goods: 2, worth: 6 })).toBe("Jord, 2 trade goods");
    expect(describePayment({ cost: 4, planets: [], trade_goods: 1, worth: 4 })).toBe("1 trade good");
  });
});

describe("dry choices built from the engine's exact question", () => {
  it("uses the engine's technology list as it is, including a technology the estimate cannot know (a skip)", () => {
    const { action, players } = actionFor("pok7technology", "Technology");
    const estimate = researchableEstimate(players[1]);
    const skipTech = ["ws", "dn2", "cv2", "fs2", "pd2"].find((id) => !estimate.includes(id))!;
    expect(skipTech).toBeTruthy();
    const exact = toExactResult(
      previewReply({
        status: "question",
        choice: engineResearch([skipTech, "amd"]),
        step: 1,
        payment: { cost: 4, planets: ["jord"], trade_goods: 0, worth: 4 },
      }),
    );
    const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "tech", exact })!;
    expect(dry.exact).toBe(true);
    expect(dry.approximate).toBeNull();
    expect(dry.choice.options.map((o) => o.id)).toEqual([skipTech, "amd", "decline"]);
    expect(dry.choice.context?.subtype).toBe("research_technology");
    expect(dry.choice.nonce).toBe(`prepare:${action.key}:tech`);
    expect(dry.payment).toEqual({ cost: 4, planets: ["jord"], trade_goods: 0, worth: 4 });
    // Without the exact answer the estimate is used again and says it is approximate.
    const fallback = buildDryChoice({ action, viewer: players[1], board: undefined, step: "tech", exact: null })!;
    expect(fallback.exact).toBe(false);
    expect(fallback.approximate).toMatch(/Prerequisite skips/);
    expect(fallback.choice.options.map((o) => o.id)).not.toContain(skipTech);
  });

  it("the exact window question keeps the engine's waivers and the Leadership purchase facts", () => {
    const { action, players } = actionFor("pok1leadership", "Leadership");
    const window = engineWindow("pok1leadership", "spend 3 influence for a command token", {
      kind: "strategy_secondary",
      costs_token: false,
      purchase: { cost: 3, influence_available: 7, max: 2, trade_goods: 1, trade_good_worth: 1, planets: [] },
    });
    const dry = buildDryChoice({
      action,
      viewer: players[1],
      board: undefined,
      step: "secondary",
      exact: { kind: "question", choice: window, skipped: [] },
    })!;
    expect(dry.exact).toBe(true);
    expect(dry.choice.details?.purchase).toMatchObject({ influence_available: 7 });
  });

  it("a not-asked or empty answer is no question: the estimate stands", () => {
    const { action, players } = actionFor("pok7technology", "Technology");
    for (const exact of [{ kind: "not_asked", blocker: "no_strategy_token" }, { kind: "complete", unused: 0 }] as const) {
      const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "tech", exact })!;
      expect(dry.exact).toBe(false);
    }
  });

  it("Warfare has a production step only with the engine's question (its build list cannot be estimated)", () => {
    const { action, players } = actionFor("pok6warfare", "Warfare");
    expect(nextDryStep("warfare", "secondary", { planets: 0 })).toBe("produce");
    expect(nextDryStep("warfare", "produce", { planets: 0 })).toBe("pay");
    expect(buildDryChoice({ action, viewer: players[1], board: undefined, step: "produce", exact: null })).toBeNull();
    const exact = toExactResult(
      previewReply({
        status: "question",
        choice: engineProduction([{ unit: "infantry", cost: 1, count: 2 }]),
        step: 1,
      }),
    );
    const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "produce", exact })!;
    expect(dry.exact).toBe(true);
    expect(dry.choice.context?.subtype).toBe("produce_unit");
    // The step belongs to Warfare only.
    const other = actionFor("pok7technology", "Technology");
    expect(
      buildDryChoice({ action: other.action, viewer: other.players[1], board: undefined, step: "produce", exact }),
    ).toBeNull();
  });
});

describe("a prepared Warfare production", () => {
  const plan = (production: SecondaryPlan["production"]): SecondaryPlan => ({
    card: "pok6warfare",
    follow: true,
    production,
  });
  const real = (over?: Parameters<typeof engineProduction>[1]): PendingChoiceDto =>
    pendingFromEngine(
      engineProduction(
        [
          { unit: "infantry", cost: 1, count: 2 },
          { unit: "carrier", cost: 3, count: 1 },
        ],
        over,
      ),
      "n-real",
    );

  it("is stored by meaning, round-trips, and is dropped for other cards", () => {
    const stored = plan({ builds: ["infantry", "carrier"], payment: { planets: ["jord"], tradeGoods: 1 } });
    const raw = serializeStoredPlan({ v: 1, actionKey: "k", generation: 1, plan: stored });
    expect(raw).not.toMatch(/build\||exhaust\|/);
    expect(parseStoredPlan(raw)?.plan).toEqual(stored);
    expect(normalizePlan({ ...stored, card: "pok7technology" })).toEqual({ card: "pok7technology", follow: true });
    expect(describePlan(stored)).toBe("Follow Warfare: build infantry, carrier");
    expect(describeBuilds(["infantry", "infantry", "carrier"])).toBe("2 x infantry, carrier");
    // Garbage is dropped, not thrown.
    const damaged = JSON.stringify({
      v: 1,
      actionKey: "k",
      generation: 0,
      plan: { card: "pok6warfare", follow: true, production: { builds: [3, null], payment: { planets: "x" } } },
    });
    expect(parseStoredPlan(damaged)?.plan).toEqual({ card: "pok6warfare", follow: true });
  });

  it("validates against the real question: units offered, capacity left, resources to spend", () => {
    expect(planBuilds(real(), ["infantry", "carrier"])).toMatchObject({ ok: true, destination: "18" });
    expect(planBuilds(real(), ["dreadnought"])).toMatchObject({ ok: false, reason: /can no longer be built/ });
    // Capacity 3: two infantry batches (2 each) do not fit.
    expect(planBuilds(real(), ["infantry", "infantry"])).toMatchObject({ ok: false, reason: /capacity/ });
    // 5 resources: carrier + carrier costs 6.
    expect(planBuilds(real({ capacity: 9 }), ["carrier", "carrier"])).toMatchObject({
      ok: false,
      reason: /cost more than you can spend/,
    });
  });

  it("resolves to the builder's own path, or to 'Needs review' with the reason", () => {
    const ok = resolveStep(plan({ builds: ["infantry", "carrier"] }), real(), "b");
    expect(ok).toEqual({
      kind: "production",
      destination: "18",
      units: ["infantry", "carrier"],
      text: "Build infantry, carrier",
    });
    expect(resolveStep(plan({ builds: ["dreadnought"] }), real(), "b")).toMatchObject({ kind: "review" });
    // Once sent, the builder's later offers are its own queue's, not the plan's.
    expect(resolveStep(plan({ builds: ["infantry"] }), real(), "b", new Set(["production"]))).toEqual({
      kind: "none",
    });
    // Another seat's or no real decision resolves to nothing.
    expect(resolveStep(plan({ builds: ["infantry"] }), real(), "c")).toEqual({ kind: "none" });
  });

  it("pays the first build as prepared when it still covers the bill, else asks for review", () => {
    const payment = pendingFromEngine(enginePayment(4), "n-pay");
    const wanted = plan({ builds: ["infantry"], payment: { planets: ["jord"], tradeGoods: 0 } });
    // Not before the production itself was sent.
    expect(resolveStep(wanted, payment, "b")).toEqual({ kind: "none" });
    const done = new Set(["production"]);
    const resolved = resolveStep(wanted, payment, "b", done);
    expect(resolved).toMatchObject({ kind: "payment", steps: [{ kind: "exhaust", planet: "jord" }] });
    // A payment that no longer covers what is owed.
    const short = plan({ builds: ["infantry"], payment: { planets: ["arinam"], tradeGoods: 0 } });
    expect(resolveStep(short, payment, "b", done)).toMatchObject({ kind: "review", reason: /no longer covers/ });
    // A planet that is no longer offered.
    const gone = plan({ builds: ["infantry"], payment: { planets: ["lodor"], tradeGoods: 0 } });
    expect(resolveStep(gone, payment, "b", done)).toMatchObject({ kind: "review", reason: /can no longer be exhausted/ });
    // Used once: a later payment is paid by hand.
    expect(resolveStep(wanted, payment, "b", new Set(["production", "payment"]))).toEqual({ kind: "none" });
  });
});
