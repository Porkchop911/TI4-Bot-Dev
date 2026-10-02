import type { ChoiceWorkflowKind } from "../presentation/choiceModel.ts";
import type { DecisionTargetDto, PendingChoiceDto } from "../protocol/types.ts";

// Illustrative UI inputs, not captured engine states or legal-game fixtures.
export const actor = "gallery_seat";
const option = (id: string, label: string, kind?: string, payload?: Record<string, unknown>) => ({
  id,
  label,
  kind,
  payload,
});
const finish = (id = "decline", label = "Finish") => option(id, label, "decline");
const system = { System: "18" } as const;

export interface GalleryCase {
  title: string;
  note: string;
  choice: PendingChoiceDto;
  workflow: ChoiceWorkflowKind;
  /** A scenario, not an additional workflow kind. */
  fallback?: string;
}

const cases = {
  system_activation: {
    subtype: "activate_system",
    options: [
      option("18", "Activate Mecatol Rex (#18)", "activate", { system: "18" }),
      option("24", "Activate Mehar Xull (#24)", "activate", { system: "24" }),
      option("26", "Activate Lodor (#26)", "activate", { system: "26" }),
      option("25", "Activate Quann (#25)", "activate", { system: "25" }),
      option("20", "Activate Vefun 5 (#20)", "activate", { system: "20" }),
      option("27", "Activate New Albion (#27)", "activate", { system: "27" }),
      option("28", "Activate Tequ'ran / Torkan (#28)", "activate", { system: "28" }),
      option("29", "Activate Qucen'n / Rarron (#29)", "activate", { system: "29" }),
      option("30", "Activate Centauri / Gral (#30)", "activate", { system: "30" }),
      option("31", "Activate Lazar / Sakulag (#31)", "activate", { system: "31" }),
      option("32", "Activate Dal Bootha / Xxehan (#32)", "activate", { system: "32" }),
      option("33", "Activate Corneeq / Resculon (#33)", "activate", { system: "33" }),
      option("34", "Activate Abyz / Fria (#34)", "activate", { system: "34" }),
      option("35", "Activate Bereq / Sem-Lore (#35)", "activate", { system: "35" }),
      option("36", "Activate Arinam / Meer (#36)", "activate", { system: "36" }),
      option("37", "Activate Arnor / Lor (#37)", "activate", { system: "37" }),
      option("38", "Activate Bereg / Lirta IV (#38)", "activate", { system: "38" }),
      option("41", "Activate Gravity Rift (#41)", "activate", { system: "41" }),
      option("39", "Activate Supernova (#39)", "activate", { system: "39" }),
      option("42", "Activate Nebula (#42)", "activate", { system: "42" }),
      option("40", "Activate Asteroid Field (#40)", "activate", { system: "40" }),
      option("45", "Activate Cormund (#45)", "activate", { system: "45" }),
      option("16", "Activate Hacan Home (#16)", "activate", { system: "16" }),
      option("12", "Activate Jol-Nar Home (#12)", "activate", { system: "12" }),
      option("43", "Activate Asteroid Field (#43)", "activate", { system: "43" }),
      option("13", "Activate Sardakk Home (#13)", "activate", { system: "13" }),
      option("65", "Activate Primor (#65)", "activate", { system: "65" }),
      option("44", "Activate Supernova (#44)", "activate", { system: "44" }),
      option("14", "Activate Xxcha Home (#14)", "activate", { system: "14" }),
      option("66", "Activate Hope's End (#66)", "activate", { system: "66" }),
      option("46", "Activate Nebula (#46)", "activate", { system: "46" }),
      option("10", "Activate Letnev Home (#10)", "activate", { system: "10" }),
      option("64", "Activate Atlas (#64)", "activate", { system: "64" }),
      option("79", "Activate Mallice (#79)", "activate", { system: "79" }),
    ],
    note: "Select almost any system directly on the map to activate (blocked systems 1, 19, 22 contain your command tokens).",
  },
  tactical_movement: {
    subtype: "movement_step",
    target: system,
    options: [
      option("move|24|0", "Move cruiser from 24", "move", {
        origin: "24",
        unit: "cruiser",
        capacity: 0,
      }),
      finish("done_moving", "Done moving"),
    ],
    note: "Staged movement and explicit finish.",
  },
  tactical_cargo: {
    subtype: "load_cargo",
    target: system,
    options: [
      option("load|infantry", "Load infantry", "load", {
        unit: "infantry",
        source: "jord",
        capacity_remaining: 2,
      }),
      finish("done_loading", "Done loading"),
    ],
    note: "Cargo tray; board counts are illustrative.",
  },
  tactical_invasion: {
    subtype: "commit_ground_forces",
    target: system,
    options: [
      option("commit|0|jord", "Land infantry on Jord", "land", {
        planet: "jord",
        unit: "infantry",
      }),
      finish("done_committing", "Done landing"),
    ],
    note: "Land one of the troops in space; the destination is rechecked after each landing.",
  },
  payment: {
    subtype: "pay_resources",
    outstanding: [{ kind: "resources", amount: 2, paid: 0 }],
    options: [
      option("exhaust|jord", "Exhaust Jord", "pay", {
        worth: 2,
        owed: 2,
        kind: "resources",
        planet_name: "Jord",
      }),
      option("trade_good", "Spend one trade good", "pay", { worth: 1 }),
      finish(),
    ],
    note: "Only ready, owned, offered planets can pay; the exhausted planet stays visible for context.",
  },
  production: {
    subtype: "produce_unit",
    target: system,
    outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
    options: [
      option("build|infantry|1", "Produce infantry", "produce", {
        unit: "infantry",
        cost: 1,
        count: 1,
        available_resources: 3,
      }),
      finish("done_producing", "Done producing"),
    ],
    note: "Production builder with capacity and resource preview.",
  },
  combat_sustain: {
    subtype: "sustain_damage",
    outstanding: [{ amount: 1 }],
    options: [
      option("sustain|dreadnought", "Sustain dreadnought", "sustain", { unit: "dreadnought" }),
      finish(),
    ],
    note: "Sustain or pass.",
  },
  combat_casualty: {
    subtype: "assign_casualty",
    target: system,
    outstanding: [{ amount: 2 }],
    options: [
      option("destroy|0", "Destroy fighter", "casualty", { unit: "fighter" }),
      option("destroy|2", "Destroy damaged dreadnought", "casualty", {
        unit: "dreadnought",
        damaged: true,
      }),
    ],
    note: "Two hits remain, but this offer assigns only the next hit; two fighters on the board share one offered ID.",
  },
  combat_retreat: {
    subtype: "retreat_to",
    target: system,
    options: [option("retreat|24", "Retreat to 24", "retreat", { system: "24" }), finish()],
    note: "Retreat from system 18 to offered destination 24, or pass.",
  },
  agenda_vote_outcome: {
    subtype: "cast_vote",
    options: [option("FOR", "For", "outcome"), option("AGAINST", "Against", "outcome")],
    note: "Choose an outcome.",
  },
  agenda_vote_planets: {
    subtype: "vote_exhaust_planet",
    options: [option("jord", "Exhaust Jord for 2 votes", "vote_planet"), finish()],
    note: "Exhaust an offered ready planet or finish voting.",
  },
  transaction_propose: {
    subtype: "propose_transaction",
    target: { Player: "other_seat" },
    options: [
      option("offer|1", "Offer one trade good", "offer", { net: -1, their_net: 1 }),
      finish(),
    ],
    note: "Propose terms to another seat.",
  },
  transaction_answer: {
    subtype: "answer_transaction",
    target: { Player: "other_seat" },
    options: [option("accept", "Accept offer", "accept"), finish("refuse", "Refuse offer")],
    note: "Accept or refuse incoming terms.",
  },
  action_card_reaction: {
    subtype: "play_reaction_after_ACTION_CARD_PLAYED",
    options: [option("sabotage", "Play Sabotage", "action_card"), finish("decline", "Pass")],
    note: "Reaction window with pass.",
  },
  objective_scoring: {
    subtype: "score_objective",
    options: [option("objective|1", "Score objective", "score"), finish()],
    note: "Currently uses generic choice modal.",
  },
  strategy_card_draft: {
    subtype: "draft_strategy_card",
    options: [option("pok1leadership", "Leadership"), option("pok3politics", "Politics")],
    note: "Choose a corpus strategy card with printed text.",
  },
  generic_selection: {
    subtype: "choose_option",
    options: [option("option_a", "Option A"), option("option_b", "Option B")],
    note: "An unknown decision uses the offered labels.",
  },
  technology_research: {
    subtype: "research_technology",
    options: [
      option("amd", "Antimass Deflectors", "research"),
      option("nm", "Neural Motivator", "research"),
      option("st", "Sarween Tools", "research"),
    ],
    note: "Technology modal with skips and color tracks.",
  },
} satisfies Record<
  ChoiceWorkflowKind,
  {
    subtype: string;
    options: PendingChoiceDto["options"];
    note: string;
    target?: DecisionTargetDto;
    outstanding?: NonNullable<PendingChoiceDto["context"]>["outstanding"];
  }
>;

export const galleryCases: GalleryCase[] = (Object.keys(cases) as ChoiceWorkflowKind[]).map(
  (workflow) => {
    const entry = cases[workflow];
    return {
      workflow,
      title: workflow.replaceAll("_", " "),
      note: entry.note,
      choice: {
        actor,
        nonce: `gallery-${workflow}`,
        prompt: (
          {
            system_activation: "Choose a system to activate",
            tactical_movement: "Move units to Mecatol Rex",
            tactical_cargo: "Load units into your fleet",
            tactical_invasion: "Land ground forces on Jord",
            payment: "Spend resources to pay",
            production: "Produce units in Mecatol Rex",
            combat_sustain: "Choose a unit to sustain damage",
            combat_casualty: "Assign a casualty",
            combat_retreat: "Choose a retreat destination",
            agenda_vote_outcome: "Choose an outcome",
            agenda_vote_planets: "Spend influence to vote",
            transaction_propose: "Propose a trade",
            transaction_answer: "Answer the trade offer",
            action_card_reaction: "Respond to the action card",
            objective_scoring: "Score an objective",
            strategy_card_draft: "Choose a strategy card",
            technology_research: "Research a technology",
            generic_selection: "Choose an option",
          } satisfies Record<ChoiceWorkflowKind, string>
        )[workflow],
        context: {
          subtype: entry.subtype,
          ...("target" in entry ? { target: entry.target } : {}),
          ...("outstanding" in entry ? { outstanding: entry.outstanding } : {}),
        },
        options: entry.options,
      },
    };
  },
);

export const fallbackCases: GalleryCase[] = [
  {
    workflow: "generic_selection",
    title: "Unknown subtype",
    fallback: "Unknown subtype → generic modal",
    note: "Unknown engine subtypes fall back to the generic single-choice modal.",
    choice: {
      actor,
      nonce: "gallery-unknown",
      prompt: "Unknown subtype",
      context: { subtype: "gallery_unrecognized" },
      options: [option("offered_option", "Offered option")],
    },
  },
  {
    workflow: "generic_selection",
    title: "Missing context",
    fallback: "No context → generic modal",
    note: "A choice with no context still renders offered IDs.",
    choice: {
      actor,
      nonce: "gallery-no-context",
      prompt: "No context",
      options: [option("offered_option", "Offered option")],
    },
  },
  {
    workflow: "generic_selection",
    title: "Bounded multi-select",
    fallback: "Unknown subtype + constraint → generic multi-select",
    note: "The UI stages choices; a mock acknowledgement does not establish a next decision.",
    choice: {
      actor,
      nonce: "gallery-multi",
      prompt: "Select two",
      context: {
        subtype: "gallery_bounded",
        outstanding: [{ min_selection: 2, max_selection: 2 }],
      },
      options: [option("a", "Option A"), option("b", "Option B"), option("c", "Option C")],
    },
  },
  {
    workflow: "tactical_movement",
    title: "Empty movement",
    fallback: "Explicit finish, no movable units",
    note: "Done moving is the only offered option.",
    choice: {
      actor,
      nonce: "gallery-empty-move",
      prompt: "No units can move",
      context: { subtype: "movement_step", target: system },
      options: [finish("done_moving", "Done moving")],
    },
  },
  {
    workflow: "tactical_movement",
    title: "Missing movement finish",
    fallback: "Missing explicit finish → visible error",
    note: "No arbitrary option is submitted as a finish.",
    choice: {
      actor,
      nonce: "gallery-missing-finish",
      prompt: "No finish offered",
      context: { subtype: "movement_step", target: system },
      options: [option("unrelated", "Unrelated option", "other")],
    },
  },
  {
    workflow: "generic_selection",
    title: "No options",
    fallback: "Empty generic choice → disabled submit",
    note: "Illustrative invalid/unactionable choice; investigate server state if encountered live.",
    choice: {
      actor,
      nonce: "gallery-no-options",
      prompt: "No options offered",
      context: { subtype: "gallery_empty" },
      options: [],
    },
  },
];
