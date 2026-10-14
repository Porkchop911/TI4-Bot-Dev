import { describe, expect, it } from "vitest";
import {
  classifyChoiceWorkflow,
  deriveChoiceRendererModel,
  isPlanetSelectionChoice,
} from "./choiceModel.ts";
import {
  describePlanetDecision,
  describeStructureStep,
  formatStructureStep,
  formatDecisionSource,
  formatPlanetStats,
  getPlanetDetails,
  groupPlanetCandidates,
  resolveMapTargetSelection,
  resolveOptionSystem,
} from "./planetSelection.ts";
import { buildBoardPresentationModel, deriveActorTargetHighlights } from "./boardPresentation.ts";
import { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";

const decline: ChoiceOptionDto = { id: "decline", kind: "decline", label: "decline" };
const planet = (id: string, payload: Record<string, unknown>, label = id, kind = "planet") => ({
  id,
  kind,
  label,
  payload,
});

const choiceOf = (
  subtype: string,
  options: ChoiceOptionDto[],
  extra: Partial<PendingChoiceDto> = {},
  source?: Record<string, unknown>,
): PendingChoiceDto => ({
  nonce: `n-${subtype}`,
  actor: "p1",
  prompt: "choose",
  options,
  context: { subtype, ...(source ? { source } : {}) },
  ...extra,
});

const board: BoardView = {
  systems: {
    "26": {
      system_id: "26",
      command_tokens: [],
      units: [],
      planets: { lodor: { planet_id: "lodor", controlled_by: "p1", exhausted: false } },
    },
    "33": {
      system_id: "33",
      command_tokens: [],
      units: [],
      planets: {
        corneeq: { planet_id: "corneeq", controlled_by: "p1", exhausted: false },
        resculon: { planet_id: "resculon", controlled_by: "p1", exhausted: true },
      },
    },
  },
  map_tiles: [
    {
      system_id: "26",
      label: "Lodor",
      q: 0,
      r: 0,
      planets: [{ id: "lodor", label: "Lodor", resources: 3, influence: 1 }],
    },
    {
      system_id: "33",
      label: "Corneeq / Resculon",
      q: 1,
      r: 0,
      planets: [
        { id: "corneeq", label: "Corneeq", resources: 1, influence: 2, traits: ["cultural"] },
        {
          id: "resculon",
          label: "Resculon",
          resources: 2,
          influence: 0,
          tech_specialties: ["warfare"],
        },
      ],
    },
  ],
};

const miningInitiative = choiceOf(
  "mining_initiative_pick_planet",
  [
    planet("lodor", { planet: "lodor", system: "26" }, "Lodor"),
    planet("corneeq", { planet: "corneeq", system: "33" }, "Corneeq"),
    planet("resculon", { planet: "resculon" }, "Resculon"),
  ],
  { prompt: "Mining Initiative: mine which planet" },
  { ActionCard: "mining_initiative" },
);

describe("planet_selection classification", () => {
  it.each([
    ["mining_initiative_pick_planet", [planet("lodor", { planet: "lodor", system: "26" })]],
    ["plague_pick_planet", [planet("0", { planet: "lodor" }, "Lodor")]],
    ["reparations_exhaust", [planet("x|lodor", { planet: "lodor", system: "26" })]],
    ["scanlink_explore", [planet("26|lodor", { planet: "lodor", system: "26" }, "explore lodor")]],
    [
      "peace_accords_annex",
      [planet("lodor", { planet: "lodor" }, "gain control of lodor"), decline],
    ],
    ["stellar_converter_choose_target", [planet("lodor", { planet: "lodor", system: "26" })]],
    ["defense_act_choose_pds", [planet("lodor", { planet: "lodor", system: "26", unit: "pds" })]],
  ])("classifies %s as planet_selection", (subtype, options) => {
    const choice = choiceOf(subtype, options as ChoiceOptionDto[]);
    expect(isPlanetSelectionChoice(choice)).toBe(true);
    expect(deriveChoiceRendererModel(choice, "p1")?.workflow).toBe("planet_selection");
  });

  it("classifies planet × structure choices and keeps the decline option", () => {
    const choice = choiceOf("place_structure", [
      planet(
        "pds|26|lodor",
        { planet: "lodor", system: "26", unit: "pds" },
        "place pds on lodor",
        "build",
      ),
      planet(
        "spacedock|26|lodor",
        { planet: "lodor", system: "26", unit: "spacedock" },
        "place spacedock on lodor",
        "build",
      ),
      decline,
    ]);
    const model = deriveChoiceRendererModel(choice, "p1");
    expect(model?.workflow).toBe("planet_selection");
    expect(model?.declineOption?.id).toBe("decline");
    expect(model?.isOptional).toBe(true);
  });

  it("accepts board-less extra options next to planets (Bio-Stims, The Acropolis)", () => {
    const bioStims = choiceOf("bio_stims_ready", [
      planet(
        "ready|planet|lodor",
        { planet: "lodor", technology: "bs" },
        "Bio-Stims: ready lodor",
        "ready",
      ),
      {
        id: "ready|technology|st",
        kind: "ready_technology",
        label: "Bio-Stims: ready Sarween Tools",
        payload: { technology: "st", bio_stims: true },
      },
      decline,
    ]);
    expect(classifyChoiceWorkflow(bioStims)).toBe("planet_selection");
    const acropolis = choiceOf("legendary_acropolis", [
      planet("planet|lodor", { planet: "lodor" }, "ready lodor"),
      { id: "relic|crown", label: "ready The Crown" },
    ]);
    expect(classifyChoiceWorkflow(acropolis)).toBe("planet_selection");
  });

  it("routes an Elect Planet cast_vote to planet_selection but keeps outcome votes on the ballot", () => {
    const elect = choiceOf("cast_vote", [
      planet("vote|lodor", { planet: "lodor" }, "Lodor", "vote"),
      planet("vote|corneeq", { planet: "corneeq" }, "Corneeq", "vote"),
      decline,
    ]);
    expect(classifyChoiceWorkflow(elect)).toBe("planet_selection");
    const outcome = choiceOf("cast_vote", [
      { id: "for", kind: "vote", label: "For" },
      { id: "against", kind: "vote", label: "Against" },
    ]);
    expect(classifyChoiceWorkflow(outcome)).toBe("agenda_vote_outcome");
  });

  it("does not reclassify existing planet-carrying workflows", () => {
    const cases: Array<[PendingChoiceDto, string]> = [
      [
        choiceOf("pay_resources", [
          planet("exhaust|lodor", { planet: "lodor", worth: 3 }, "Lodor", "pay"),
          { id: "trade_good", kind: "pay", label: "TG" },
        ]),
        "payment",
      ],
      [
        choiceOf("activate_system", [
          {
            id: "26",
            kind: "activate",
            label: "Lodor",
            payload: { system: "26", planet: "lodor" },
          },
        ]),
        "system_activation",
      ],
      [
        choiceOf("commit_ground_forces", [
          planet("commit|0|lodor", { planet: "lodor", unit: "infantry" }, "Land", "land"),
          decline,
        ]),
        "tactical_invasion",
      ],
      [
        choiceOf("place_unit", [
          planet("place|lodor", { planet: "lodor", unit: "infantry" }, "Place on Lodor", "place"),
        ]),
        "production",
      ],
      [
        choiceOf("vote_exhaust_planet", [
          planet("lodor", { planet: "lodor" }, "Exhaust Lodor for 1 votes", "vote_planet"),
          decline,
        ]),
        "agenda_vote_planets",
      ],
      [
        choiceOf("bombardment_target", [planet("lodor", { planet: "lodor", system: "26" })]),
        "generic_selection",
      ],
      [
        choiceOf("crashlanding_choose_planet", [planet("lodor", { planet: "lodor" })], {
          context: { subtype: "crashlanding_choose_planet", invasion_seq: 2 },
        }),
        "generic_selection",
      ],
      [
        choiceOf("movement", [
          {
            id: "m",
            kind: "tactical_move",
            label: "Move",
            payload: { system: "18", to: "18", planet: "mecatol_rex" },
          },
        ]),
        "generic_selection",
      ],
      [
        choiceOf("choose_option", [
          planet("lodor", { planet: "lodor" }),
          { id: "move|x", kind: "move", label: "Move", payload: { origin: "18" } },
        ]),
        "generic_selection",
      ],
    ];
    for (const [choice, workflow] of cases) {
      expect(classifyChoiceWorkflow(choice), choice.context?.subtype).toBe(workflow);
    }
  });
});

describe("planet selection helpers", () => {
  it("formats the decision source and the action from context.source and the prompt", () => {
    expect(formatDecisionSource({ ActionCard: "mining_initiative" })).toBe("Mining Initiative");
    expect(formatDecisionSource({ Content: "bs" })).toBe("Bio-Stims");
    expect(
      formatDecisionSource({ StrategyCard: { card: "pok4construction", secondary: true } }),
    ).toBe("Construction (secondary)");
    expect(describePlanetDecision(miningInitiative)).toEqual({
      sourceLabel: "Mining Initiative",
      actionPrompt: "mine which planet",
      actionVerb: "mine",
    });
    const acropolis = choiceOf(
      "legendary_acropolis",
      [],
      { prompt: "The Acropolis: ready what" },
      {
        Content: "legendary",
      },
    );
    expect(describePlanetDecision(acropolis).sourceLabel).toBe("The Acropolis");
    const plague = choiceOf(
      "plague_pick_planet",
      [],
      { prompt: "Plague: which planet" },
      {
        ActionCard: "plague",
      },
    );
    expect(describePlanetDecision(plague)).toMatchObject({
      sourceLabel: "Plague",
      actionVerb: "target",
    });
  });

  it("names an Elect Planet vote instead of showing a bare rule number", () => {
    const vote = choiceOf("cast_vote", [], { prompt: "vote for which outcome" }, { Rule: "8.10" });
    expect(describePlanetDecision(vote)).toEqual({
      sourceLabel: "Agenda Vote",
      actionPrompt: "vote for which outcome",
      actionVerb: "vote for",
    });
  });

  it("resolves the system from payload.system or the board, and planet stats", () => {
    expect(resolveOptionSystem(miningInitiative.options[0], board)).toBe("26");
    expect(resolveOptionSystem(miningInitiative.options[2], board)).toBe("33");
    const details = getPlanetDetails("resculon", board);
    expect(details).toMatchObject({ name: "Resculon", systemId: "33", exhausted: true });
    expect(details.techSpecialties).toEqual(["warfare"]);
    expect(formatPlanetStats(getPlanetDetails("lodor", board))).toBe("3R/1I");
    // Content catalog fallback when the planet is not on the board.
    expect(getPlanetDetails("mecatol_rex").name).toBe("Mecatol Rex");
    expect(groupPlanetCandidates(miningInitiative, board).map((c) => c.systemId)).toEqual([
      "26",
      "33",
      "33",
    ]);
  });

  it("matches planet clicks by payload.planet regardless of the option id format", () => {
    expect(resolveMapTargetSelection(miningInitiative, "33", "resculon", board)).toEqual({
      kind: "select",
      optionId: "resculon",
      planetId: "resculon",
    });
    const indexed = choiceOf("plague_pick_planet", [planet("0", { planet: "lodor" }, "Lodor")]);
    expect(resolveMapTargetSelection(indexed, "26", "lodor", board)).toMatchObject({
      optionId: "0",
    });
  });

  it("selects only the planet when several options share it", () => {
    const structures = choiceOf("place_structure", [
      planet("pds|26|lodor", { planet: "lodor", system: "26", unit: "pds" }),
      planet("spacedock|26|lodor", { planet: "lodor", system: "26", unit: "spacedock" }),
      decline,
    ]);
    expect(resolveMapTargetSelection(structures, "26", "lodor", board)).toEqual({
      kind: "select",
      optionId: undefined,
      planetId: "lodor",
    });
  });

  it("applies the single-candidate rule to hex clicks in planet mode", () => {
    expect(resolveMapTargetSelection(miningInitiative, "26", undefined, board)).toEqual({
      kind: "select",
      optionId: "lodor",
      planetId: "lodor",
    });
    expect(resolveMapTargetSelection(miningInitiative, "33", undefined, board)).toEqual({
      kind: "ignore",
    });
  });

  it("keeps payment, vote basket and system activation fallbacks", () => {
    const payment = choiceOf("pay_resources", [
      { id: "exhaust|lodor", kind: "pay", label: "Lodor", payload: { worth: 3 } },
    ]);
    expect(resolveMapTargetSelection(payment, "26", "lodor")).toMatchObject({
      optionId: "exhaust|lodor",
    });
    const vote = choiceOf("vote_exhaust_planet", [
      { id: "lodor", kind: "vote_planet", label: "x" },
    ]);
    expect(resolveMapTargetSelection(vote, "26", "lodor")).toMatchObject({ optionId: "lodor" });
    const activation = choiceOf("activate_system", [
      { id: "activate|26", kind: "activate", label: "x", payload: { system: "26" } },
    ]);
    expect(resolveMapTargetSelection(activation, "26")).toEqual({
      kind: "select",
      optionId: "activate|26",
      planetId: null,
    });
  });
});

describe("board planet targeting mode", () => {
  it("highlights candidate planets without making their hexes targets", () => {
    const targets = deriveActorTargetHighlights(miningInitiative, "p1", board);
    expect(targets.targetMode).toBe("planet");
    expect(targets.isActivationMode).toBe(false);
    expect([...targets.targetablePlanetIds].sort()).toEqual(["corneeq", "lodor", "resculon"]);
    expect(targets.targetableSystemIds.size).toBe(0);
    expect(deriveActorTargetHighlights(miningInitiative, "p2", board).targetMode).toBeNull();
  });

  it("marks only a system's single candidate planet for hex clicks", () => {
    const model = buildBoardPresentationModel(board, ["p1"], [], miningInitiative, "p1");
    const lodor = model.tiles.find((t) => t.systemId === "26")!;
    const pair = model.tiles.find((t) => t.systemId === "33")!;
    expect(lodor.isCandidateTarget).toBe(false);
    expect(lodor.singleCandidatePlanetId).toBe("lodor");
    expect(pair.isCandidateTarget).toBe(false);
    expect(pair.singleCandidatePlanetId).toBeNull();
    expect(pair.planets.every((p) => p.isCandidateTarget)).toBe(true);
  });

  it("keeps activation as system mode and leaves invasions out of planet mode", () => {
    const activation = choiceOf("activate_system", [
      { id: "26", kind: "activate", label: "x", payload: { system: "26" } },
    ]);
    const activationTargets = deriveActorTargetHighlights(activation, "p1", board);
    expect(activationTargets.targetMode).toBe("system");
    expect(activationTargets.isActivationMode).toBe(true);
    const invading: BoardView = {
      ...board,
      invasion: {
        system_id: "26",
        invasion_seq: 1,
        invader: "p1",
        phase: "bombardment",
        planets: ["lodor"],
        current_planet: null,
        defender: null,
        ground_round: 0,
      },
    };
    expect(deriveActorTargetHighlights(miningInitiative, "p1", invading).targetMode).toBeNull();
  });
});

describe("describeStructureStep", () => {
  const place = (details?: Record<string, unknown>, subtype = "place_structure") => ({
    context: { subtype },
    details,
  });
  it("reads the step, the total and the PDS-only flag", () => {
    expect(describeStructureStep(place({ step: 2, of: 2, only_pds: true }) as never)).toEqual({
      step: 2,
      of: 2,
      onlyPds: true,
    });
    expect(formatStructureStep({ step: 1, of: 2, onlyPds: false })).toBe("Structure 1 of 2");
    expect(formatStructureStep({ step: 2, of: 2, onlyPds: true })).toBe(
      "Structure 2 of 2 · PDS only",
    );
  });
  it("is null for other decisions or a missing step", () => {
    expect(describeStructureStep(place({ step: 1, of: 2 }, "cast_vote") as never)).toBeNull();
    expect(describeStructureStep(place(undefined) as never)).toBeNull();
    expect(describeStructureStep(place({ step: "1", of: 2 }) as never)).toBeNull();
  });
});
