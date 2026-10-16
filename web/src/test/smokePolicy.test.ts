import { describe, expect, it } from "vitest";
import {
  activationWeight,
  preferHitConfirm,
  preferTokenConfirm,
  preferTradeDesk,
  preferPayment,
  preferReadyConfirm,
  steerWeight,
  strongUnselected,
  isUnstage,
  classifyControl,
  serverProgressed,
  classifyCandidate,
  turnBarPool,
  isBarControl,
  pausedBatch,
  EXCLUDED_CONTROLS,
  isReactionSubtype,
  reactionTextProblem,
  preferTechWindowUse,
  CHAOS_FREE_OFFERS,
} from "../../e2e/smokePolicy.ts";

const c = (desc: string, checked = false) => ({ desc, checked });

describe("preferPayment (smoke harness payment policy)", () => {
  it("never clicks Pick on map, which only minimises the payment list", () => {
    const pick = { desc: "pick-on-map-btn Pick on map" };
    const planet = { desc: "planet-card-exhaust|jord" };
    expect(preferPayment([pick, planet])).toEqual([planet]);
  });

  it("presses the confirm button as soon as it is enabled", () => {
    const confirm = c("confirm-payment-btn | Confirm payment");
    const picked = preferPayment([
      c("planet-card-exhaust|jord | Jord", true),
      confirm,
      c("trade-goods-inc"),
    ]);
    expect(picked).toEqual([confirm]);
  });

  it("never un-toggles a staged planet while another control is available", () => {
    const staged = c("planet-card-exhaust|jord | Jord", true);
    const open = c("planet-card-exhaust|arinam | Arinam", false);
    expect(preferPayment([staged, open])).toEqual([open]);
  });

  it("falls back to the staged planet when it is the only control", () => {
    const staged = c("planet-card-exhaust|jord | Jord", true);
    expect(preferPayment([staged])).toEqual([staged]);
  });

  it("leaves non-payment controls alone", () => {
    const list = [
      c("choice-option | take a tactical action"),
      c("submit-choice-button | Confirm choice"),
    ];
    expect(preferPayment(list)).toEqual(list);
  });
});

describe("steerWeight (smoke harness steering)", () => {
  it("strongly prefers lifting the custodians over leaving them", () => {
    const yes = steerWeight("choice-option | remove it for a victory point");
    const no = steerWeight("choice-option | leave it");
    expect(yes).toBeGreaterThanOrEqual(50);
    expect(no).toBeLessThan(0.1);
  });

  it("loads cargo more readily than it moves more ships", () => {
    expect(
      steerWeight("rally-inc-cargo-65-infantry-space | +"),
    ).toBeGreaterThan(steerWeight("rally-inc-65-carrier | +"));
  });

  it("keeps a raider's ships in Mecatol", () => {
    expect(steerWeight("rally-inc-18-carrier | +")).toBeLessThan(
      steerWeight("rally-inc-65-carrier | +"),
    );
    expect(steerWeight("rally-inc-cargo-18-infantry-space | +")).toBe(25);
  });

  it("rarely spends a raider's trade goods on Munitions Reserves rerolls", () => {
    expect(
      steerWeight("choice-option | reroll this round's misses TG 12 → 10"),
    ).toBeLessThan(0.2);
  });

  it("weighs unknown controls 1", () => {
    expect(steerWeight("choice-option | something else")).toBe(1);
  });
});

describe("activationWeight (smoke harness steering)", () => {
  it("favours Mecatol and enemy systems, and avoids unreachable ones", () => {
    expect(activationWeight("18", true, false, false)).toBe(40);
    // An unclaimed trait planet with the landing party in place, or a frontier ship, is worth a lot.
    expect(activationWeight("69", true, false, false, false, true)).toBe(1500);
    expect(activationWeight("69", true, false, false, false, false)).toBe(5);
    expect(activationWeight("35", true, true, false)).toBe(30);
    expect(activationWeight("35", true, false, false)).toBe(5);
    // A defended planet outranks a bare enemy fleet, but an unreachable one is still skipped.
    expect(activationWeight("35", true, true, false, true)).toBe(60);
    expect(activationWeight("35", false, true, false, true)).toBe(0.2);
    // Own ground forces waiting beside a defended planet: activate in place, ahead of everything.
    expect(activationWeight("35", false, true, true, true)).toBe(2000);
    expect(activationWeight("35", true, true, true, false)).toBe(30);
    expect(activationWeight("35", false, true, false)).toBe(0.2);
  });

  it("makes a seat with ground forces waiting in Mecatol activate it in place", () => {
    // Dominant over ~35 other activations, each weighing at most 30.
    expect(activationWeight("18", false, false, true)).toBeGreaterThanOrEqual(
      35 * 30,
    );
    // Only Mecatol, and only with ground forces there.
    expect(activationWeight("35", false, false, true)).toBe(0.2);
    expect(activationWeight("18", false, false, false)).toBe(0.2);
  });
});

describe("strongUnselected (smoke harness steering)", () => {
  const no = c("choice-option | leave it VP 0 → 0", true);
  const yes = c("choice-option | remove it for a victory point VP 0 → 1");

  it("chooses the custodians removal before submitting the preselected 'no'", () => {
    expect(strongUnselected([no, yes])).toEqual(yes);
  });

  it("lets the submit go ahead once the strong option is selected", () => {
    expect(
      strongUnselected([
        { ...no, checked: false },
        { ...yes, checked: true },
      ]),
    ).toBeUndefined();
  });

  it("ignores ordinary options", () => {
    expect(
      strongUnselected([
        c("choice-option | end your turn"),
        c("rally-inc-cargo-1-infantry-space | +"),
      ]),
    ).toBeUndefined();
  });
});

describe("isUnstage (smoke harness)", () => {
  it("never treats a choice option as taking staging back", () => {
    expect(
      isUnstage("choice-option | remove it for a victory point VP 0 → 1"),
    ).toBe(false);
  });

  it("treats the invasion overlay's bare custodians button as an option", () => {
    expect(isUnstage("remove it for a victory point")).toBe(false);
    expect(steerWeight("remove it for a victory point")).toBeGreaterThan(
      50 * steerWeight("leave it"),
    );
  });

  it("still recognises real take-back controls", () => {
    expect(isUnstage("rally-dec-18-carrier | −")).toBe(true);
    expect(isUnstage("produce-dec | Remove Fighter (0.5 cost)")).toBe(true);
    expect(isUnstage("reset-selection-btn | Reset selection")).toBe(true);
  });
});

describe("classifyCandidate (staged technology cards)", () => {
  it("treats a selected technology card as a take-back control, not a submit", () => {
    const desc = "tech-card-gls | Graviton Laser System";
    expect(classifyCandidate(desc, true)).toEqual({ unstage: true, commit: false });
    expect(classifyCandidate(desc, false)).toEqual({ unstage: false, commit: false });
  });

  it("treats the clear-second-pick control as take-back", () => {
    expect(isUnstage("tech-clear-second | Clear second pick")).toBe(true);
    expect(classifyCandidate("tech-clear-second | Clear second pick", false).unstage).toBe(true);
  });
});

describe("preferHitConfirm", () => {
  it("confirms the staged hits once every hit is assigned", () => {
    const confirm = c("hit-confirm | Confirm hits");
    expect(
      preferHitConfirm([
        c("hit-destroy-fighter|intact | Destroy Fighter"),
        confirm,
      ]),
    ).toEqual([confirm]);
  });

  it("keeps every staging control while hits are left", () => {
    const stages = [
      c("hit-destroy-fighter|intact | +"),
      c("hit-auto-assign | Auto-assign"),
    ];
    expect(preferHitConfirm(stages)).toEqual(stages);
  });

  it("counts the minus control as taking a hit back", () => {
    expect(
      isUnstage("hit-remove-fighter|intact | Remove a hit from Fighters | −"),
    ).toBe(true);
    expect(isUnstage("hit-destroy-fighter|intact | Destroy Fighters | +")).toBe(
      false,
    );
  });
});

describe("preferReadyConfirm (Diplomacy ready bar)", () => {
  it("confirms once the bar allows it", () => {
    const confirm = c("ready-bar-confirm | Confirm: ready 2 planets | Confirm");
    expect(preferReadyConfirm([c("ready-planet-chip-jord | ✓ Jord"), confirm])).toEqual([confirm]);
  });

  it("taps only planets that are not picked yet, so picking cannot loop", () => {
    const open = c("ready-planet-chip-lodor | Lodor");
    expect(preferReadyConfirm([c("ready-planet-chip-jord | ✓ Jord"), open])).toEqual([open]);
  });

  it("never clicks the bar's Hide or Clear, and leaves other decisions alone", () => {
    expect(EXCLUDED_CONTROLS.test("ready-bar-collapse")).toBe(true);
    expect(EXCLUDED_CONTROLS.test("ready-bar-clear")).toBe(true);
    expect(EXCLUDED_CONTROLS.test("ready-bar-confirm")).toBe(false);
    const other = [c("choice-option | x")];
    expect(preferReadyConfirm(other)).toBe(other);
  });
});

describe("preferTokenConfirm", () => {
  it("confirms once every token is assigned", () => {
    const confirm = c("token-confirm | Confirm tokens");
    expect(
      preferTokenConfirm([c("token-minus-fleet | Remove a token from Fleet | −"), confirm]),
    ).toEqual([confirm]);
  });

  it("only adds tokens while some are left, so staging cannot loop", () => {
    const plus = c("token-plus-fleet | Add a token to Fleet | +");
    expect(
      preferTokenConfirm([
        c("token-minus-fleet | Remove a token from Fleet | −"),
        c("token-reset | Reset"),
        plus,
      ]),
    ).toEqual([plus]);
  });

  it("leaves other decisions alone", () => {
    const stages = [c("choice-option | tactic pool")];
    expect(preferTokenConfirm(stages)).toEqual(stages);
  });

  describe("payment override controls", () => {
    const change = c("token-payment-change | Change payment");
    const planet = c("token-payment-planet-jord | jord · 2 influence · ready");
    const goods = c("token-payment-goods-plus | Spend one trade good more | +");
    const auto = c("token-payment-auto | Use Auto-pay");
    const plus = c("token-plus-fleet | Add a token to Fleet | +");
    const confirm = c("token-confirm | Confirm tokens and purchase");

    it("never offers the override toggles, with or without a random roll", () => {
      for (const rng of [undefined, () => 0, () => 0.99]) {
        const picked = preferTokenConfirm([change, planet, goods, plus], rng);
        expect(picked).toEqual([plus]);
        expect(preferTokenConfirm([change, planet, goods, confirm], rng)).toEqual([confirm]);
      }
    });

    it("recovers with Use Auto-pay only when nothing else can progress", () => {
      expect(preferTokenConfirm([planet, goods, auto])).toEqual([auto]);
      expect(preferTokenConfirm([planet, auto, plus])).toEqual([plus]);
    });
  });

  describe("Leadership purchase stepper", () => {
    const buyPlus = c("token-buy-plus | Buy one token more | +");
    const buyMinus = c("token-buy-minus | Buy one token less | −");
    const plus = c("token-plus-fleet | Add a token to Fleet | +");
    const confirm = c("token-confirm | No purchase");

    it("never buys without a random source, and never takes a purchase back", () => {
      expect(preferTokenConfirm([buyPlus, buyMinus, plus])).toEqual([plus]);
      expect(preferTokenConfirm([buyMinus, confirm])).toEqual([confirm]);
    });

    it("sometimes buys one more, even when Confirm is already enabled", () => {
      expect(preferTokenConfirm([buyPlus, confirm], () => 0)).toEqual([buyPlus]);
      expect(preferTokenConfirm([buyPlus, plus], () => 0)).toEqual([buyPlus]);
      expect(preferTokenConfirm([buyPlus, confirm], () => 0.99)).toEqual([confirm]);
      expect(preferTokenConfirm([buyMinus, buyPlus, plus], () => 0.99)).toEqual([plus]);
    });

    it("always ends: with the purchase staged only plus and then Confirm are chosen", () => {
      // Purchase at its limit: no buy control is clickable, so the roll cannot restart it.
      expect(preferTokenConfirm([buyMinus, plus], () => 0)).toEqual([plus]);
      expect(preferTokenConfirm([buyMinus, confirm], () => 0)).toEqual([confirm]);
    });
  });
});

describe("choice rows (smoke harness)", () => {
  it("never reads a choice-option label as a submit", () => {
    expect(classifyControl("choice-option | commit infantry to lor in 36").commit).toBe(false);
    expect(classifyControl("choice-option | Decline").commit).toBe(false);
    expect(classifyControl("choice-option | Pass").commit).toBe(false);
    expect(classifyControl("submit-choice | Confirm").commit).toBe(true);
  });
});

describe("turn bar controls (smoke harness)", () => {
  const tactical = c("turn-bar-tactical | Tactical action 3 tactic tokens left T");
  const imperial = c(
    "turn-bar-strategic-pok8imperial | 8. Imperial Immediately score 1 public objective if you remove",
  );
  const diplomacy = c("turn-bar-strategic-pok2diplomacy | 2. Diplomacy Choose 1 system");
  const trade = c("turn-bar-trade | Trade 3 partners · 2 in contact D");
  const components = c("turn-bar-components | Components 2 usable C");
  const open = c("turn-bar-open-hacan | Open");
  const item = c("turn-bar-item-faction|orbital_drop | Orbital Drop");
  const end = c("turn-bar-end | End turn Enter Enter or click");

  it("treats a bar button as one click that submits, not as staging", () => {
    expect(classifyControl(tactical.desc)).toEqual({ unstage: false, commit: true });
    expect(classifyControl(diplomacy.desc)).toEqual({ unstage: false, commit: true });
    expect(classifyControl(end.desc)).toEqual({ unstage: false, commit: true });
    expect(classifyControl(open.desc)).toEqual({ unstage: false, commit: true });
    expect(classifyControl(item.desc)).toEqual({ unstage: false, commit: true });
  });

  it("does not read a card's effect text as a take-back control", () => {
    // The Imperial text above contains "remove"; the old test-id-free rule would unstage it.
    expect(isUnstage(imperial.desc)).toBe(false);
    expect(classifyControl(imperial.desc).unstage).toBe(false);
  });

  it("treats the menu toggles as opening a list, not submitting", () => {
    expect(classifyControl(trade.desc)).toEqual({ unstage: false, commit: false });
    expect(classifyControl(components.desc)).toEqual({ unstage: false, commit: false });
    expect(classifyControl("turn-bar-cards | Action cards 2 A").commit).toBe(false);
  });

  it("keeps non-bar controls classified as before", () => {
    expect(classifyControl("decline-trade-btn | Decline")).toEqual({
      unstage: false,
      commit: true,
    });
    expect(classifyControl("hit-reset | Reset").unstage).toBe(true);
  });

  it("chooses among every enabled bar button when no menu is open", () => {
    const pool = turnBarPool([tactical, imperial, trade, c("choice-option | stray")]);
    expect(pool).toEqual([tactical, imperial, trade]);
  });

  it("answers an open menu first, never a second toggle", () => {
    expect(turnBarPool([tactical, trade, open])).toEqual([open]);
    expect(turnBarPool([components, item, tactical])).toEqual([item]);
  });

  it("is not the bar's business when there is no bar", () => {
    expect(turnBarPool([c("choice-option | a"), c("confirm-payment-btn")])).toBeNull();
    expect(isBarControl("choice-option | a")).toBe(false);
  });

  it("steers toward tactical play, rarely trades or passes, and takes End turn", () => {
    expect(steerWeight(tactical.desc)).toBeGreaterThan(steerWeight(imperial.desc));
    expect(steerWeight(imperial.desc)).toBeGreaterThan(steerWeight(trade.desc));
    expect(steerWeight(trade.desc)).toBeLessThan(1);
    expect(steerWeight("turn-bar-pass | Pass")).toBeLessThan(1);
    expect(steerWeight(end.desc)).toBeGreaterThan(steerWeight(components.desc));
    // The test id decides, not the words in a card's text.
    expect(steerWeight("turn-bar-components | Components take a tactical action")).toBe(6);
    expect(steerWeight("turn-bar-cards | Action cards 2 A")).toBeGreaterThan(steerWeight(trade.desc));
  });

  it("prefers playing a reaction over passing it", () => {
    expect(steerWeight("play-reaction-btn-reaction:sol:SHIP_MOVED:after | Play Rescue")).toBeGreaterThan(
      steerWeight("pass-reaction-btn | Pass (Spacebar)"),
    );
  });

  it("gives every split strategic button the same weight so each card gets played", () => {
    expect(steerWeight(imperial.desc)).toBe(steerWeight(diplomacy.desc));
  });
});

describe("pausedBatch (batch results the harness must not treat as failures)", () => {
  it("reads a plan the server stopped at a reaction window", () => {
    expect(
      pausedBatch(
        JSON.stringify({
          batch_id: "b",
          interrupted: {
            applied_steps: 1,
            remaining_steps: [{ kind: "done_moving" }],
            offered: { subtype: "reaction_after_SHIP_MOVED" },
          },
        }),
      ),
    ).toEqual({ applied: 1, remaining: 1, waiting: "reaction_after_SHIP_MOVED" });
  });
  it("is null for a whole batch and for text that is not a batch result", () => {
    expect(pausedBatch(JSON.stringify({ batch_id: "b", active: true }))).toBeNull();
    expect(pausedBatch("not json")).toBeNull();
  });
  it("lets the continue button submit and leaves dismiss alone", () => {
    expect(classifyControl("paused-plan-continue | Continue plan").commit).toBe(true);
  });
});

describe("reaction dialog contract (smoke harness)", () => {
  it("clicks Play and Pass but never the card-text, compact, pin or map controls", () => {
    const clickable = [
      "play-reaction-btn-reaction:hacan:ACTION_CARD_PLAYED:when | Play Sabotage",
      "play-reaction-btn-use | Use Instinct Training",
      "pass-reaction-btn | Pass (Spacebar)",
    ];
    const inert = [
      "reaction-inspect-text-Uprising | Shrink",
      "reaction-inspect-text-Uprising | Show full text",
      "reaction-inspect-compact | Compact card text",
      "reaction-inspect-show-on-map | Show on map",
      "pin-reaction-toggle | Pin",
    ];
    for (const desc of clickable) expect(EXCLUDED_CONTROLS.test(desc), desc).toBe(false);
    for (const desc of inert) expect(EXCLUDED_CONTROLS.test(desc), desc).toBe(true);
    for (const desc of clickable) expect(classifyControl(desc).commit, desc).toBe(true);
    for (const desc of inert) expect(steerWeight(desc), desc).toBeLessThanOrEqual(1);
  });

  it("recognises every reaction decision subtype, including the two ability steps", () => {
    for (const subtype of [
      "reaction_when_ACTION_CARD_PLAYED",
      "reaction_after_SYSTEM_ACTIVATED",
      "play_reaction_after_SYSTEM_ACTIVATED",
      "instinct_training_cancel",
      "l1z1x_agent_swap",
    ])
      expect(isReactionSubtype(subtype), subtype).toBe(true);
    expect(isReactionSubtype("activate_system")).toBe(false);
  });

  it("flags raw engine ids and doubled verbs in the dialog text", () => {
    expect(reactionTextProblem("Anna activated System 27. Now you can play Sabotage.")).toBeNull();
    expect(reactionTextProblem("when ACTION_CARD_PLAYED")).toMatch(/raw engine id/);
    expect(reactionTextProblem("Play play Sabotage")).toMatch(/doubled verb/);
  });
});

describe("system pick controls", () => {
  it("never clicks the list-hiding map button, and the map confirm is a commit", () => {
    expect(EXCLUDED_CONTROLS.test("system-pick-inspect-map-btn | Choose on the map")).toBe(true);
    expect(classifyControl("confirm-activation-btn | Confirm")).toEqual({
      unstage: false,
      commit: true,
    });
    // The list stays the path: option rows are choice-option, the submit is a commit.
    expect(classifyControl("choice-option | 14").unstage).toBe(false);
    expect(classifyControl("submit-choice-button | Confirm choice").commit).toBe(true);
  });
});

describe("Leadership map payment (Pay on the map)", () => {
  const payOnMap = c("token-pay-on-map | Pay on the map");
  const confirm = c("token-confirm | Confirm tokens and purchase");
  const plus = c("token-plus-fleet | Add a token to Fleet | +");
  const mapPlanet = c("planet-jord | Exhaust planet Jord for 2 influence");

  it("takes the map route only now and then, and only once everything is assigned", () => {
    expect(preferTokenConfirm([payOnMap, confirm, mapPlanet], () => 0)).toEqual([payOnMap]);
    expect(preferTokenConfirm([payOnMap, confirm, mapPlanet], () => 0.99)).toEqual([confirm]);
    expect(preferTokenConfirm([payOnMap, plus, mapPlanet], () => 0)).toEqual([plus]);
    expect(preferTokenConfirm([payOnMap, confirm])).toEqual([confirm]);
  });

  it("never toggles the map's payable planets while the token panel is up", () => {
    expect(preferTokenConfirm([mapPlanet, plus])).toEqual([plus]);
    // A payment dialog without token controls keeps its map targets.
    expect(preferTokenConfirm([mapPlanet])).toEqual([mapPlanet]);
  });

  describe("the payment bar", () => {
    const barConfirm = c("token-bar-confirm | Confirm tokens and purchase");
    const barAuto = c("token-bar-auto | Auto-pay");
    const goods = c("token-bar-goods-plus | Spend one trade good more | +");
    const resume = c("resume-choice-button | Open token panel");

    it("confirms when enabled, else restores Auto-pay, else goes back to the panel", () => {
      expect(preferTokenConfirm([mapPlanet, goods, barAuto, resume, barConfirm])).toEqual([barConfirm]);
      expect(preferTokenConfirm([mapPlanet, goods, barAuto, resume])).toEqual([barAuto]);
      expect(preferTokenConfirm([mapPlanet, goods, resume, c("token-bar-tally")])).toEqual([resume]);
    });
  });
});

describe("preferTradeDesk", () => {
  const propose = c("propose-trade-btn | Propose Deal");
  const decline = c("decline-trade-btn | Offer Nothing");
  const suggest = c("stage-suggest-offer_1 | Give 1 trade good, receive 1 commodity");
  const stage = c("stage-give-trade_goods-inc | Increase Trade goods | +");

  it("proposes once the staged combination is a listed deal", () => {
    expect(preferTradeDesk([stage, decline, propose])).toEqual([propose]);
    expect(preferTradeDesk([stage, decline, propose], () => 0.5)).toEqual([propose]);
  });

  it("takes a suggested listed deal while the staged one is not available", () => {
    expect(preferTradeDesk([stage, decline, suggest])).toEqual([suggest]);
  });

  it("now and then offers nothing instead, and leaves other screens alone", () => {
    expect(preferTradeDesk([stage, decline, propose], () => 0)).toEqual([decline]);
    const answer = [c("answer-opt-accept | Accept"), c("answer-opt-refuse | Refuse")];
    expect(preferTradeDesk(answer, () => 0)).toEqual(answer);
    expect(preferTradeDesk([stage, decline], () => 0)).toEqual([stage, decline]);
  });
});

describe("controls the harness never clicks as decisions", () => {
  it("excludes the info buttons and the secondary-prep chrome", () => {
    for (const desc of [
      "unit-info-cruiser",
      "Cruiser: unit details",
      "faction-info-button",
      "secondary-prep-chip",
      "prepare-banner",
      "prep-save",
      "top-menu-button | Menu",
      "top-menu-close | Close",
      "agenda-strip-toggle | Agenda 1/2: Minister of Peace · For/Against · Your vote",
      "prep-fold | Hide details",
      "detail-panel-fold | Fold Politics details",
      "history-error-dismiss | Dismiss",
    ]) {
      expect(EXCLUDED_CONTROLS.test(desc), desc).toBe(true);
    }
    for (const desc of ["produce-unit-btn-cruiser", "confirm-builds", "done-producing-btn", "trade-opt-1", "answer-opt-accept", "token-bar-confirm"]) {
      expect(EXCLUDED_CONTROLS.test(desc), desc).toBe(false);
    }
  });
});

describe("preferTechWindowUse (optional technology windows)", () => {
  const options = [
    c("choice-option | exhaust tarmann"),
    c("choice-option | exhaust semlore"),
    c("choice-option | Decline"),
    c("submit-choice-button | Submit"),
  ];
  const transit = [
    c("choice-option | move infantry from 70 to 01"),
    c("choice-option | finish redeployment"),
    c("submit-choice-button | Submit"),
  ];
  const descs = (cs: { desc: string }[]) => cs.map((x) => x.desc);

  it("moves a unit at the first Transit Diodes prompt and finishes at the next", () => {
    const first = preferTechWindowUse("transit_diodes_redeploy", "Transit Diodes: redeploy ground forces (4 left)", transit, 0);
    expect(descs(first)).toEqual(["choice-option | move infantry from 70 to 01", "submit-choice-button | Submit"]);
    const next = preferTechWindowUse("transit_diodes_redeploy", "Transit Diodes: redeploy ground forces (3 left)", transit, 5);
    expect(descs(next)).toEqual(["choice-option | finish redeployment", "submit-choice-button | Submit"]);
  });

  it("finishes Transit Diodes through its own panel's button", () => {
    const panel = [c("planet-archonvail | Target planet Archon Vail"), c("decline-planet-btn | Finish redeployment")];
    expect(descs(preferTechWindowUse("transit_diodes_redeploy", "x (4 left)", panel, 0))).toEqual(["planet-archonvail | Target planet Archon Vail"]);
    expect(descs(preferTechWindowUse("transit_diodes_redeploy", "x (3 left)", panel, 0))).toEqual(["decline-planet-btn | Finish redeployment"]);
  });

  it("never declines Psychoarchaeology or Bio-Stims while a candidate is offered", () => {
    for (const subtype of ["psychoarchaeology_exhaust_specialty", "bio_stims_ready"]) {
      expect(descs(preferTechWindowUse(subtype, "", options, 9))).not.toContain("choice-option | Decline");
    }
  });

  it("answers the first Chaos Mapping offers freely and declines later ones", () => {
    expect(preferTechWindowUse("chaos_mapping_choose_system", "Chaos Mapping", options, CHAOS_FREE_OFFERS - 1)).toBe(options);
    expect(descs(preferTechWindowUse("chaos_mapping_choose_system", "Chaos Mapping", options, CHAOS_FREE_OFFERS))).toEqual([
      "choice-option | Decline",
      "submit-choice-button | Submit",
    ]);
  });

  it("leaves other subtypes and a disabled policy alone", () => {
    expect(preferTechWindowUse("draft_strategy_card", "", options, 99)).toBe(options);
    expect(preferTechWindowUse("chaos_mapping_choose_system", "", options, 99, false)).toBe(options);
  });
});

describe("serverProgressed (smoke harness)", () => {
  it("counts a version bump and a game-over push without one", () => {
    expect(serverProgressed({ game_version: 5, turn_status: { kind: "active_turn" } }, 5)).toBe(false);
    expect(serverProgressed({ game_version: 6, turn_status: { kind: "active_turn" } }, 5)).toBe(true);
    expect(serverProgressed({ game_version: 5, turn_status: { kind: "game_over" } }, 5)).toBe(true);
  });
});
