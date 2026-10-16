import type { ChoiceWorkflowKind } from "../presentation/choiceModel.ts";
import { removeUnitCases } from "./removeUnitGalleryCases.ts";
import { offerCases } from "./offerGalleryCases.ts";
import type { DecisionTargetDto, PendingChoiceDto } from "../protocol/types.ts";

// Illustrative UI inputs, not captured engine states or legal-game fixtures.
export const actor = "gallery_seat";
const option = (
  id: string,
  label: string,
  kind?: string,
  payload?: Record<string, unknown>,
) => ({
  id,
  label,
  kind,
  payload,
});
const finish = (id = "decline", label = "Finish") =>
  option(id, label, "decline");
const system = { System: "18" } as const;

export interface GalleryCase {
  title: string;
  note: string;
  choice: PendingChoiceDto;
  workflow: ChoiceWorkflowKind;
  /** A scenario, not an additional workflow kind. */
  fallback?: string;
  /** Preview against a different board than the shared gallery board. */
  boardId?: "hit_assignment";
  /** Open the preview as the other seat (the viewer who is not the one deciding). */
  viewer?: "other";
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
      option("28", "Activate Tequ'ran / Torkan (#28)", "activate", {
        system: "28",
      }),
      option("29", "Activate Qucen'n / Rarron (#29)", "activate", {
        system: "29",
      }),
      option("30", "Activate Centauri / Gral (#30)", "activate", {
        system: "30",
      }),
      option("31", "Activate Lazar / Sakulag (#31)", "activate", {
        system: "31",
      }),
      option("32", "Activate Dal Bootha / Xxehan (#32)", "activate", {
        system: "32",
      }),
      option("33", "Activate Corneeq / Resculon (#33)", "activate", {
        system: "33",
      }),
      option("34", "Activate Abyz / Fria (#34)", "activate", { system: "34" }),
      option("35", "Activate Bereq / Sem-Lore (#35)", "activate", {
        system: "35",
      }),
      option("36", "Activate Arinam / Meer (#36)", "activate", {
        system: "36",
      }),
      option("37", "Activate Arnor / Lor (#37)", "activate", { system: "37" }),
      option("38", "Activate Bereg / Lirta IV (#38)", "activate", {
        system: "38",
      }),
      option("41", "Activate Gravity Rift (#41)", "activate", { system: "41" }),
      option("39", "Activate Supernova (#39)", "activate", { system: "39" }),
      option("42", "Activate Nebula (#42)", "activate", { system: "42" }),
      option("40", "Activate Asteroid Field (#40)", "activate", {
        system: "40",
      }),
      option("45", "Activate Cormund (#45)", "activate", { system: "45" }),
      option("16", "Activate Hacan Home (#16)", "activate", { system: "16" }),
      option("12", "Activate Jol-Nar Home (#12)", "activate", { system: "12" }),
      option("43", "Activate Asteroid Field (#43)", "activate", {
        system: "43",
      }),
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
  planet_selection: {
    subtype: "mining_initiative_pick_planet",
    source: { ActionCard: "mining_initiative" },
    options: [
      option("lodor", "Lodor", "planet", { planet: "lodor", system: "26" }),
      option("quann", "Quann", "planet", { planet: "quann" }),
      option("corneeq", "Corneeq", "planet", {
        planet: "corneeq",
        system: "33",
      }),
      option("resculon", "Resculon", "planet", {
        planet: "resculon",
        system: "33",
      }),
    ],
    note: "Pick a highlighted planet on the map (or a chip in the bar); the bar names the card and the action before you confirm.",
  },
  ready_planets: {
    subtype: "ready_planet",
    options: [
      option("lodor", "ready lodor", "ready", { planet: "lodor", system: "26" }),
      option("quann", "ready quann", "ready", { planet: "quann" }),
      option("corneeq", "ready corneeq", "ready", { planet: "corneeq", system: "33" }),
    ],
    note: "Diplomacy: tick up to two exhausted planets here or on the map, then one Confirm readies them together.",
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
      option("sustain|dreadnought", "Sustain dreadnought", "sustain", {
        unit: "dreadnought",
      }),
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
    options: [
      option("retreat|24", "Retreat to 24", "retreat", { system: "24" }),
      finish(),
    ],
    note: "Retreat from system 18 to offered destination 24, or pass.",
  },
  agenda_vote_outcome: {
    subtype: "cast_vote",
    options: [
      option("FOR", "For", "outcome"),
      option("AGAINST", "Against", "outcome"),
    ],
    note: "Choose an outcome.",
  },
  agenda_vote_planets: {
    subtype: "vote_exhaust_planet",
    options: [
      option("jord", "Exhaust Jord for 2 votes", "vote_planet"),
      finish(),
    ],
    note: "Exhaust an offered ready planet or finish voting.",
  },
  transaction_propose: {
    subtype: "propose_transaction",
    target: { Player: "other_seat" },
    options: [
      option("offer|1", "Offer one trade good", "offer", {
        net: -1,
        their_net: 1,
      }),
      finish(),
    ],
    note: "Propose terms to another seat.",
  },
  transaction_answer: {
    subtype: "answer_transaction",
    target: { Player: "other_seat" },
    options: [
      option("accept", "Accept offer", "accept"),
      finish("refuse", "Refuse offer"),
    ],
    note: "Accept or refuse incoming terms.",
  },
  action_card_reaction: {
    subtype: "play_reaction_after_ACTION_CARD_PLAYED",
    options: [
      option("sabo1", "play Sabotage", "action_card", { card: "sabo1", card_name: "Sabotage" }),
      finish("decline", "Pass"),
    ],
    note: "Reaction window with pass.",
  },
  objective_scoring: {
    subtype: "score_objective",
    options: [option("objective|1", "Score objective", "score"), finish()],
    note: "Currently uses generic choice modal.",
  },
  strategy_card_draft: {
    subtype: "draft_strategy_card",
    options: [
      option("pok1leadership", "Leadership"),
      option("pok3politics", "Politics"),
    ],
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
    source?: Record<string, unknown>;
    outstanding?: NonNullable<PendingChoiceDto["context"]>["outstanding"];
  }
>;

export const galleryCases: GalleryCase[] = (
  Object.keys(cases) as ChoiceWorkflowKind[]
).map((workflow) => {
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
          planet_selection: "Mining Initiative: mine which planet",
          ready_planets: "ready which planet",
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
        ...("source" in entry ? { source: entry.source } : {}),
        ...("outstanding" in entry ? { outstanding: entry.outstanding } : {}),
      },
      options: entry.options,
    },
  };
});

export const fallbackCases: GalleryCase[] = [
  {
    workflow: "planet_selection",
    title: "Place a structure",
    fallback: "Planet × structure → choose on the planet",
    note: "Several options share a planet; after picking it, the bar offers one button per structure.",
    choice: {
      actor,
      nonce: "gallery-place-structure",
      prompt: "place a structure",
      context: {
        subtype: "place_structure",
        source: { Content: "place_structure" },
      },
      options: [
        option("pds|18|jord", "place pds on jord", "build", {
          planet: "jord",
          system: "18",
          unit: "pds",
        }),
        option("spacedock|18|jord", "place spacedock on jord", "build", {
          planet: "jord",
          system: "18",
          unit: "spacedock",
        }),
        option("pds|26|lodor", "place pds on lodor", "build", {
          planet: "lodor",
          system: "26",
          unit: "pds",
        }),
        finish(),
      ],
      details: { step: 1, of: 2 },
    },
  },
  {
    workflow: "planet_selection",
    title: "Construction: second structure",
    fallback: "Step label 2 of 2 with the PDS-only note",
    note: "Construction's second placement must be a PDS: the bar says which step this is and offers only PDS options.",
    choice: {
      actor,
      nonce: "gallery-place-structure-2",
      prompt: "place a structure",
      context: {
        subtype: "place_structure",
        source: { Content: "place_structure" },
      },
      options: [
        option("pds|18|jord", "place pds on jord", "build", {
          planet: "jord",
          system: "18",
          unit: "pds",
        }),
        option("pds|26|lodor", "place pds on lodor", "build", {
          planet: "lodor",
          system: "26",
          unit: "pds",
        }),
        finish(),
      ],
      details: { step: 2, of: 2, only_pds: true },
    },
  },
  {
    workflow: "planet_selection",
    title: "Bio-Stims",
    fallback: "Planets + technologies → planet pick with extra chips",
    note: "Ready a planet on the map, or an exhausted technology from the bar.",
    choice: {
      actor,
      nonce: "gallery-bio-stims",
      prompt: "Bio-Stims",
      context: { subtype: "bio_stims_ready", source: { Content: "bs" } },
      options: [
        option("ready|planet|gral", "Bio-Stims: ready gral", "ready", {
          planet: "gral",
          system: "30",
          technology: "bs",
        }),
        option(
          "ready|technology|st",
          "Bio-Stims: ready Sarween Tools",
          "ready_technology",
          {
            technology: "st",
            bio_stims: true,
          },
        ),
        finish(),
      ],
    },
  },
  {
    workflow: "planet_selection",
    title: "Elect Planet vote",
    fallback: "cast_vote with planet outcomes → map pick",
    note: "An Elect Planet agenda: vote for a planet on the map, or abstain.",
    choice: {
      actor,
      nonce: "gallery-elect-planet",
      prompt: "vote for which outcome",
      context: { subtype: "cast_vote", source: { Rule: "8.10" } },
      options: [
        option("vote|lodor", "Lodor", "vote", {
          planet: "lodor",
          system: "26",
          current_votes: 3,
        }),
        option("vote|bereg", "Bereg", "vote", {
          planet: "bereg",
          system: "38",
          current_votes: 0,
        }),
        option("vote|lirta_iv", "Lirta IV", "vote", {
          planet: "lirta_iv",
          system: "38",
        }),
        finish("decline", "Abstain"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Diplomacy: choose a system",
    fallback: "Bare system numbers -> named systems with facts, also clickable on the map",
    note: "Each option shows the system name, planets with resources and influence, ships and command tokens; Choose on the map highlights the candidates and offers a confirm bar. The list keeps working.",
    choice: {
      actor,
      nonce: "gallery-diplomacy-system",
      prompt: "choose a system to lock down",
      context: {
        subtype: "diplomacy_choose_system",
        source: { StrategyCard: { card: "Diplomacy", secondary: false } },
      },
      options: [
        option("18", "18", "system"),
        option("26", "26", "system"),
        option("25", "25", "system"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Unexpected Action: recall your token",
    fallback: "Bare system numbers -> named systems with facts, also clickable on the map",
    note: "The action card's recall of one of your own tokens uses the same system picker as Diplomacy and Warfare.",
    choice: {
      actor,
      nonce: "gallery-unexpected-recall",
      prompt: "Unexpected Action: recall your token from where",
      context: {
        subtype: "unexpected_pick_recall",
        source: { Content: "unexpected" },
      },
      options: [
        option("25", "recall your token from 25", "recall"),
        option("26", "recall your token from 26", "recall"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Warfare: recall a command token",
    fallback: "Bare system numbers -> named systems with facts, also clickable on the map",
    note: "Same system facts for each system holding one of your command tokens.",
    choice: {
      actor,
      nonce: "gallery-warfare-recall",
      prompt: "recall a command token",
      context: {
        subtype: "warfare_recall_token",
        source: { StrategyCard: { card: "Warfare", secondary: false } },
      },
      options: [
        option("18", "18", "system"),
        option("26", "26", "system"),
        option("25", "25", "system"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Context header on a generic decision",
    fallback: "Source card, topic, phase and rule above the question",
    note: "The shared header shows who is asking (the Politics secondary), what it is about, and when it was asked, using only the context the server already sends.",
    choice: {
      actor,
      nonce: "gallery-context-header",
      prompt: "place the redistribution agenda where",
      context: {
        subtype: "politics_place_agenda",
        source: { StrategyCard: { card: "pok3politics", secondary: true } },
        phase: "Action",
        round: 2,
      },
      options: [
        option("top", "on top of the deck"),
        option("bottom", "on the bottom"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Politics: place the agenda",
    fallback: "The agenda card with its printed text above top/bottom",
    note: "The agenda's name, type and text come from the server's display-only details; the two options are unchanged.",
    choice: {
      actor,
      nonce: "gallery-politics-agenda",
      prompt: "place minister_of_commerce where",
      details: {
        agenda: {
          id: "minister_of_commerce",
          name: "Minister of Commerce",
          type: "Law",
          target: "Player",
          text1: "At the start of the status phase, the elected player gains 1 trade good for each planet they control.",
          text2: "",
        },
      },
      context: {
        subtype: "politics_place_agenda",
        source: { StrategyCard: { card: "pok3politics", secondary: false } },
        phase: "Action",
        round: 2,
      },
      options: [
        option("top", "on top of the deck", "agenda"),
        option("bottom", "on the bottom", "agenda"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Politics: choose the speaker",
    fallback: "Each candidate with faction, VP, speaker order and the role",
    note: "Candidates show faction and colour, current VP and what the speaker role gives.",
    choice: {
      actor,
      nonce: "gallery-politics-speaker",
      prompt: "who becomes speaker",
      details: { seats: { Hacan: "other_seat" } },
      context: {
        subtype: "politics_choose_speaker",
        source: { StrategyCard: { card: "pok3politics", secondary: false } },
        phase: "Action",
        round: 2,
      },
      options: [option("Hacan", "Hacan becomes speaker", "speaker")],
    },
  },
  {
    workflow: "generic_selection",
    title: "Trade: let another player replenish",
    fallback: "Commodity counts and VP per candidate",
    note: "Each candidate shows how many commodities they hold and what replenishing gives.",
    choice: {
      actor,
      nonce: "gallery-trade-replenish",
      prompt: "let another player replenish commodities",
      details: { seats: { Hacan: "other_seat" } },
      context: {
        subtype: "trade_choose_replenish",
        source: { StrategyCard: { card: "pok5trade", secondary: false } },
        phase: "Action",
        round: 2,
      },
      options: [
        option("Hacan", "Hacan replenishes commodities", "replenish"),
        option("done", "nobody else replenishes", "decline"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Hacan agent: gain or replenish",
    fallback: "Names, effects and a reason line instead of raw seat ids",
    note: "Each choice says what it does; the header line gives a reason to prefer one.",
    choice: {
      actor,
      nonce: "gallery-hacan-agent",
      prompt: "Leader: gain 2 commodities or replenish another player",
      context: {
        subtype: "leader_hacanagent_branch",
        source: { FactionAbility: "hacanagent" },
        phase: "Action",
        round: 2,
      },
      options: [
        option("self", "gain 2 commodities", "leader_hacanagent_branch"),
        option("other_seat", "replenish other_seat", "leader_hacanagent_branch"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Discard over the hand limit",
    fallback: "Card names, phase and printed text instead of bare labels",
    note: "Each action card offered for discard shows its name, when it is played and its text, and the header says how many cards are held.",
    choice: {
      actor,
      nonce: "gallery-hand-limit",
      prompt: "over the hand limit — discard one of 8",
      context: {
        subtype: "discard_over_hand_limit",
        source: { Rule: "2.4" },
        phase: "Status",
        round: 3,
      },
      options: [
        option("4", "Ancient Burial Sites", "discard"),
        option("3", "Direct Hit", "discard"),
        option("2", "Bribery", "discard"),
        option("6", "Sabotage", "discard"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Return a secret objective",
    fallback: "Objective name, condition and points instead of an alias",
    note: "Secret objectives show their name, their condition and what they are worth, so the player can decide which one to give up.",
    choice: {
      actor,
      nonce: "gallery-secret-limit",
      prompt: "return a secret objective to the deck",
      context: {
        subtype: "return_over_secret_hand_limit",
        source: { Rule: "45.4" },
        phase: "Status",
        round: 3,
      },
      options: [
        option("baf", "return baf", "return"),
        option("ans", "return ans", "return"),
        option("dp", "return dp", "return"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Strategy card secondary",
    fallback: "Named action with the card, who played it and the tokens left",
    note: "A follower's secondary: the card and its printed secondary text, who played it, how many strategy tokens remain, and one button that says what spending them does.",
    choice: {
      actor,
      nonce: "gallery-strategy-secondary",
      prompt: "spend a strategy token to draw two action cards",
      options: [
        option("no", "decline", "strategy"),
        option("yes", "draw", "strategy"),
      ],
      details: {
        kind: "strategy_secondary",
        card: "pok3politics",
        played_by: "other_seat",
        tokens_left: 3,
        costs_token: true,
      },
    },
  },
  {
    workflow: "objective_scoring",
    title: "Imperial: you hold Mecatol Rex",
    fallback: "Objective overview plus the +1 VP outcome card",
    note: "Imperial's primary: score a public objective (optional) in the objectives overview. The card above it always says what follows: +1 VP because you hold Mecatol Rex.",
    choice: {
      actor,
      nonce: "gallery-imperial-mecatol",
      prompt: "score a public objective with Imperial",
      context: { subtype: "imperial_score_objective" },
      options: [
        option("expand_borders", "expand_borders", "objective"),
        finish("decline", "decline"),
      ],
      details: {
        kind: "imperial",
        controls_mecatol: true,
        secrets_held: 2,
        secrets_max: 3,
      },
    },
  },
  {
    workflow: "objective_scoring",
    title: "Imperial: draw a secret",
    fallback: "Objective overview plus the draw-a-secret outcome card",
    note: "Imperial's primary without Mecatol Rex: the same overview, and the card says you draw a secret objective afterwards, with your hand (2 of 3).",
    choice: {
      actor,
      nonce: "gallery-imperial-secret",
      prompt: "score a public objective with Imperial",
      context: { subtype: "imperial_score_objective" },
      options: [
        option("expand_borders", "expand_borders", "objective"),
        finish("decline", "decline"),
      ],
      details: {
        kind: "imperial",
        controls_mecatol: false,
        secrets_held: 2,
        secrets_max: 3,
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Trade: replenish another player",
    fallback: "Seats with commodities now and after a replenish",
    note: "The Trade primary after your own gain: each candidate seat with its commodities and what a replenish gives them, one button per seat, and a way to stop.",
    choice: {
      actor,
      nonce: "gallery-trade-replenish",
      prompt: "let another player replenish commodities",
      context: { subtype: "trade_choose_replenish" },
      options: [
        option("hacan", "hacan replenishes commodities", "replenish"),
        option("sol", "sol replenishes commodities", "replenish"),
        option("done", "nobody else replenishes", "decline"),
      ],
      details: {
        seats: { hacan: "other_seat", sol: "third_seat" },
        commodities: { hacan: { have: 1, max: 6 }, sol: { have: 0, max: 4 } },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Gain command tokens",
    fallback: "Per-pool +/- panel with pips; one batch submits the whole gain",
    note: "Gaining three command tokens: assign each to a pool with + and -, see the count, the total, the tokens remaining and pips for tokens already there versus new ones. Confirm only unlocks when all are assigned.",
    choice: {
      actor,
      nonce: "gallery-gain-command-tokens",
      prompt: "gain a command token into which pool",
      options: [
        option("tactic_tokens", "tactic pool", "pool"),
        option("fleet_tokens", "fleet pool", "pool"),
        option("strategic_tokens", "strategy pool", "pool"),
      ],
      details: {
        kind: "command_tokens",
        mode: "gain",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 7,
        tokens_to_place: 3,
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Leadership: gain and buy command tokens",
    fallback: "Per-pool +/- panel plus a purchase stepper; one batch answers the whole Leadership primary",
    note: "Leadership primary: three free tokens and the influence purchase on one screen. Buy tokens with a stepper bounded by what 9 influence pays for (Jord plus trade goods, chosen like Auto-pay), assign every token to a pool, and Confirm sends the pools, purchases and payments as one batch.",
    choice: {
      actor,
      nonce: "gallery-leadership-gain-buy",
      prompt: "gain a command token into which pool",
      context: { subtype: "gain_command_token" },
      options: [
        option("tactic_tokens", "tactic pool", "pool"),
        option("fleet_tokens", "fleet pool", "pool"),
        option("strategic_tokens", "strategy pool", "pool"),
      ],
      details: {
        kind: "command_tokens",
        mode: "gain",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 8,
        tokens_to_place: 3,
        purchase: {
          cost: 3,
          influence_available: 9,
          max: 3,
          trade_goods: 7,
          trade_good_worth: 1,
          planets: [{ id: "jord", worth: 2 }],
        },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Leadership: change which planets pay",
    fallback: "Leadership gain and buy with Change payment: choose the planets to exhaust and the trade goods to spend",
    note: "Leadership primary with three planets and trade goods to pay with. Auto-pay is the default; Change payment lists each ready planet (influence value) and a trade-good stepper, with the running account (paid, owed, remainder, waste) and a reason when the engine would not take the combination. Use Auto-pay resets it. Confirm stays disabled while the payment is short or pays more than the bill needs.",
    choice: {
      actor,
      nonce: "gallery-leadership-change-payment",
      prompt: "gain a command token into which pool",
      context: { subtype: "gain_command_token" },
      options: [
        option("tactic_tokens", "tactic pool", "pool"),
        option("fleet_tokens", "fleet pool", "pool"),
        option("strategic_tokens", "strategy pool", "pool"),
      ],
      details: {
        kind: "command_tokens",
        mode: "gain",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 12,
        tokens_to_place: 3,
        purchase: {
          cost: 3,
          influence_available: 12,
          max: 4,
          trade_goods: 2,
          trade_good_worth: 1,
          planets: [
            { id: "jord", worth: 2 },
            { id: "arcturus", worth: 4 },
            { id: "lodor", worth: 3 },
          ],
        },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Leadership secondary: buy command tokens",
    fallback: "Same panel for the follower: only the purchase and its pools",
    note: "Leadership secondary: the follower's yes/no question becomes the same panel with nothing free to place. Confirming with no purchase answers no.",
    choice: {
      actor,
      nonce: "gallery-leadership-secondary-buy",
      prompt: "spend 3 influence for a command token",
      context: { subtype: "buy_token_with_influence" },
      options: [
        option("no", "spend nothing further", "strategy"),
        option("yes", "spend 3 influence", "strategy"),
      ],
      details: {
        kind: "strategy_secondary",
        card: "pok1leadership",
        played_by: "seat_2",
        costs_token: false,
        tokens_left: 2,
        mode: "buy",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 8,
        tokens_to_place: 0,
        purchase: {
          cost: 3,
          influence_available: 6,
          max: 2,
          trade_goods: 1,
          trade_good_worth: 1,
          planets: [{ id: "arcturus", worth: 4 }],
        },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Redistribute command tokens",
    fallback: "Per-pool +/- panel over the arrangement options",
    note: "Status-phase redistribution of the nine tokens held: move tokens between pools; only arrangements the engine offers can be confirmed.",
    choice: {
      actor,
      nonce: "gallery-redistribute-command-tokens",
      prompt: "redistribute your command tokens",
      options: Array.from({ length: 10 }, (_, tactic) =>
        Array.from({ length: 10 - tactic }, (_, offset) => {
          const fleet = offset + 2;
          const strategic = 9 - tactic - fleet;
          return strategic >= 0
            ? option(
                `${tactic}|${fleet}|${strategic}`,
                `tactic ${tactic} / fleet ${fleet} / strategy ${strategic}`,
                "redistribute",
              )
            : null;
        }),
      )
        .flat()
        .filter((entry) => entry !== null),
      details: {
        kind: "command_tokens",
        mode: "redistribute",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 7,
        total: 9,
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Exploration card: choose the reward",
    fallback: "Bare reward labels -> the exploration card as printed above the options",
    note: "A reward choice of an exploration card (Abandoned Warehouses, Local Fabricators, ...) shows the card's name, type and printed text.",
    choice: {
      actor,
      nonce: "gallery-explore-reward",
      prompt: "Abandoned Warehouses",
      context: {
        subtype: "abandoned_warehouses_choose_reward",
        source: { Content: "abandoned_warehouses" },
      },
      options: [
        option("gain", "gain 2 commodities", "explore"),
        option("convert", "convert up to 2 commodities to trade goods", "explore"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Imperial Rider: predict the outcome",
    fallback: "'predict FOR' options -> the rider as printed above and each outcome in words",
    note: "The riders (Imperial, Construction, Diplomacy, ...) ask which outcome of the agenda to predict; the printed card above says what a correct prediction pays and that you cannot vote.",
    choice: {
      actor,
      nonce: "gallery-predict-outcome",
      prompt: "Imperial Rider: predict the agenda outcome",
      context: {
        subtype: "predict_agenda_outcome",
        source: { Rule: "8" },
      },
      options: [
        option("FOR", "predict FOR", "prediction"),
        option("AGAINST", "predict AGAINST", "prediction"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Divert Funding: which technology to return",
    fallback: "Technology names only -> colour, prerequisites and printed text per technology, and what this half of the card does",
    note: "Divert Funding (and the other cards that ask for a technology) shows each technology as printed; the header says whether it is returned or researched.",
    choice: {
      actor,
      nonce: "gallery-divert-pick",
      prompt: "Divert Funding: which technology to return",
      context: {
        subtype: "divert_funding_pick_technology",
        source: { ActionCard: "divert" },
      },
      options: [
        option("pa", "Psychoarchaeology", "technology"),
        option("amd", "Antimass Deflectors", "technology"),
        option("st", "Sarween Tools", "technology"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Scuttle: which ship to scuttle",
    fallback: "Raw ship ids -> named ships with their system and the trade goods each pays out",
    note: "Scuttle (and Refit Troops, for infantry) names each unit, says where it is and what the card does to it; the printed card is above the options.",
    choice: {
      actor,
      nonce: "gallery-scuttle-pick",
      prompt: "Scuttle: which ship to scuttle",
      context: {
        subtype: "scuttle_pick_ship",
        source: { ActionCard: "scuttle" },
      },
      options: [
        option("26|0", "dreadnought2 in 26", "ship"),
        option("25|1", "cruiser in 25", "ship"),
        option("25|2", "carrier in 25", "ship"),
      ],
      details: {
        units: {
          "26|0": { system: "26", planet: null, unit: "dreadnought2", damaged: true, cost: 4 },
          "25|1": { system: "25", planet: null, unit: "cruiser", damaged: false, cost: 2 },
          "25|2": { system: "25", planet: null, unit: "carrier", damaged: false, cost: 3 },
        },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Munitions Reserves: reroll misses",
    fallback: "Yes/no with the cost in the label -> the printed ability, the price against the trade goods held, and Skip",
    note: "Letnev's Munitions Reserves is offered at the start of every space combat round, before any dice are rolled, and costs 2 trade goods each time.",
    choice: {
      actor,
      nonce: "gallery-munitions",
      prompt: "spend 2 trade goods for Munitions Reserves",
      context: {
        subtype: "munitions_reserves_reroll",
        source: { FactionAbility: "munitions" },
      },
      options: [
        option("munitions", "reroll this round's misses", "ability"),
        option("decline", "decline", "decline"),
      ],
      details: {
        kind: "ability_offer",
        ability: {
          name: "Munitions Reserves",
          window: "At the start of each round of space combat",
          effect: "You may spend 2 trade goods to re-roll any number of your dice during that combat round.",
        },
        cost: { trade_goods: 2, have: 5 },
      },
    },
  },
  {
    workflow: "generic_selection",
    title: "Manipulate Investments: place a trade good",
    fallback: "Plain list of card names -> the strategy card grid with the trade goods already on each card and the progress",
    note: "Five placements, one question each: the card grid shows each card's printed text and the trade goods lying on it, and the header says which good this is and how many different cards are still owed.",
    choice: {
      actor,
      nonce: "gallery-investments-pick",
      prompt: "Manipulate Investments: place a trade good on which strategy card",
      context: {
        subtype: "investments_pick_strategy_card",
        source: { ActionCard: "investments" },
      },
      options: [
        "pok1leadership",
        "pok2diplomacy",
        "pok3politics",
        "pok4construction",
        "pok5trade",
        "pok6warfare",
        "pok7technology",
        "pok8imperial",
      ].map((id) => option(id, `place a trade good on ${id}`, "strategy_card")),
      details: { step: 2, of: 5, distinct_owed: 2 },
    },
  },
  {
    workflow: "generic_selection",
    title: "Legendary planet abilities",
    fallback: "Raw ability labels -> the planet that carries each ability and its printed text",
    note: "The end-of-turn (and when-you-pass) legendary menu shows each ready ability with its planet, stats and card text; Decline ends the window.",
    choice: {
      actor,
      nonce: "gallery-legendary-menu",
      prompt: "use a legendary planet ability",
      context: {
        subtype: "legendary_end_of_turn",
        source: { Content: "legendary" },
      },
      options: [
        option("primor", "The Atrament", "legendary"),
        option("mirage", "Mirage Flight Academy", "legendary"),
        option("decline", "decline", "decline"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Spy: rob which player",
    fallback: "Plain list of seats -> the printed card, and each player's VP, trade goods, commodities and what the card takes",
    note: "Player picks (Spy, Insubordination, Signal Jamming, Diplomatic Pressure, ...) show the card's printed text above and each candidate's standing under their name.",
    choice: {
      actor,
      nonce: "gallery-spy-pick",
      prompt: "Spy: rob which player",
      context: {
        subtype: "spy_pick_player",
        source: { ActionCard: "spy" },
      },
      options: [
        option("other_seat", "take a card from other_seat", "player"),
        option("third_seat", "take a card from third_seat", "player"),
      ],
    },
  },
  {
    workflow: "generic_selection",
    title: "Predictive Intelligence: restack tokens",
    fallback: "One move per question -> per-pool +/- panel; the fewest moves are answered in turn",
    note: "Predictive Intelligence redistributes at the end of the turn, one token per question. The panel plans the whole arrangement and sends the moves one after another, then finishes.",
    choice: {
      actor,
      nonce: "gallery-pi-restack",
      prompt: "Predictive Intelligence: redistribute command tokens",
      context: {
        subtype: "predictive_intelligence_redistribute",
        source: { Content: "pi" },
      },
      options: [
        ...["tactic|fleet", "tactic|strategy", "fleet|tactic", "fleet|strategy", "strategy|tactic", "strategy|fleet"].map(
          (id) => option(id, `move 1 token from ${id.replace("|", " to ")}`, "redistribute"),
        ),
        option("done", "finish redistribution", "decline"),
      ],
      details: {
        kind: "command_tokens",
        mode: "restack",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: 7,
        total: 9,
      },
    },
  },
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
      options: [
        option("a", "Option A"),
        option("b", "Option B"),
        option("c", "Option C"),
      ],
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
  {
    workflow: "combat_sustain",
    boardId: "hit_assignment",
    title: "Assign hits across a mixed fleet",
    fallback: "Five hits staged in one panel instead of one click per hit",
    note: "Eight fighters and two destroyers are grouped; the dreadnoughts (sustain) and carriers (cargo) are one row each. Stage with - / +, then confirm the whole plan.",
    choice: {
      actor,
      nonce: "gallery-hit-assignment",
      prompt: "cancel a hit at 18",
      context: {
        subtype: "sustain_damage",
        target: system,
        outstanding: [{ kind: "UnitsToRemove", amount: 5, paid: 0 }],
      },
      options: [
        option("sustain|10", "sustain damage on dreadnought", "sustain", {
          unit: "dreadnought",
        }),
        option("decline", "take the hit", "decline"),
      ],
    },
  },
  {
    workflow: "combat_sustain",
    boardId: "hit_assignment",
    title: "Sustain with no amount stated (space cannon / barrage)",
    fallback: "One hit staged in the panel; never '0 of 0 hits' with no control",
    note: "Hits absorbed outside a combat window used to carry no amount, which the model read as 0. The panel treats an unstated amount as one hit and the engine now states the real count.",
    choice: {
      actor,
      nonce: "gallery-hit-unstated",
      prompt: "cancel a hit at 18",
      context: { subtype: "sustain_damage", target: system },
      options: [
        option("sustain|10", "sustain damage on dreadnought", "sustain", {
          unit: "dreadnought",
        }),
        option("decline", "take the hit", "decline"),
      ],
    },
  },
  ...reactionCases(),
  ...removeUnitCases(),
  ...offerCases(),
  ...turnBarCases(),
];

/** The reaction dialog's scenarios: what happened and why the seat may answer. */
function reactionCases(): GalleryCase[] {
  const other = "other_seat";
  const trigger = (
    kind: string,
    event_type: string,
    relation: "when" | "after",
    rest: Record<string, unknown> = {},
  ) => ({ kind, event_type, event_id: 12, relation, ...rest });
  const outer = (card: string, name: string, event: string, relation: "when" | "after") =>
    option(`reaction:hacan:${event}:${relation}`, `Play ${name}`, "ability", {
      card,
      card_name: name,
    });
  const make = (
    nonce: string,
    title: string,
    fallback: string,
    note: string,
    subtype: string,
    options: PendingChoiceDto["options"],
    triggerDto: Record<string, unknown> | null,
    source: Record<string, unknown> = {},
  ): GalleryCase => ({
    workflow: "action_card_reaction",
    title,
    fallback,
    note,
    choice: {
      actor,
      nonce,
      prompt: subtype,
      context: {
        subtype,
        optional: true,
        ...(Object.keys(source).length ? { source } : {}),
        ...(triggerDto
          ? { trigger: triggerDto as unknown as NonNullable<PendingChoiceDto["context"]>["trigger"] }
          : {}),
      },
      options: [...options, finish("decline", "Pass")],
    },
  });
  return [
    make(
      "gallery-reaction-sabotage",
      "Reaction: Sabotage",
      "Another seat played an action card: who, which card and its full text, then Sabotage",
      "Trigger from the engine: the played card with its printed text, then the card you can answer with.",
      "reaction_when_ACTION_CARD_PLAYED",
      [outer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when")],
      trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
      { Reaction: "ACTION_CARD_PLAYED" },
    ),
    make(
      "gallery-reaction-instinct",
      "Reaction: Instinct Training",
      "A technology that cancels the card, used or declined",
      "Not a card: the printed technology text and a Use button.",
      "instinct_training_cancel",
      [option("use", "cancel it", "technology")],
      trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
      { Content: "it" },
    ),
    make(
      "gallery-reaction-activated-yours",
      "Reaction: system activated, you have units there",
      "Activation of a system you have units in, with Show on map",
      "The activated system links to the map and says how many of your units stand there.",
      "reaction_after_SYSTEM_ACTIVATED",
      [outer("decoy", "Decoy Operation", "SYSTEM_ACTIVATED", "after")],
      trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "18" }),
      { Reaction: "SYSTEM_ACTIVATED" },
    ),
    make(
      "gallery-reaction-activated-none",
      "Reaction: system activated, none of your units",
      "Activation of a system without your units",
      "No presence note; the window text says why the card applies.",
      "reaction_after_SYSTEM_ACTIVATED",
      [outer("decoy", "Decoy Operation", "SYSTEM_ACTIVATED", "after")],
      trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "27" }),
      { Reaction: "SYSTEM_ACTIVATED" },
    ),
    make(
      "gallery-reaction-ship-moved",
      "Reaction: ships moved",
      "Moved ships named by type and count",
      "The engine's SHIP_MOVED now carries the destination and the arriving units.",
      "reaction_after_SHIP_MOVED",
      [outer("rescue", "Rescue", "SHIP_MOVED", "after")],
      trigger("ship_moved", "SHIP_MOVED", "after", {
        actor: other,
        system: "26",
        units: [
          { owner: other, unit_type: "cruiser", count: 2 },
          { owner: other, unit_type: "dreadnought", count: 1 },
        ],
      }),
      { Reaction: "SHIP_MOVED" },
    ),
    make(
      "gallery-reaction-agenda",
      "Reaction: agenda revealed",
      "No actor: the speaker flips the agenda",
      "Phase and agenda events name nobody; the sentence says what was revealed.",
      "reaction_when_AGENDA_REVEALED",
      [outer("sabo1", "Sabotage", "AGENDA_REVEALED", "when")],
      trigger("agenda_revealed", "AGENDA_REVEALED", "when", { agenda: "mutiny" }),
      { Reaction: "AGENDA_REVEALED" },
    ),
    make(
      "gallery-reaction-combat",
      "Reaction: combat starts",
      "A space combat begins in a system",
      "Combat windows name the attacker and the system.",
      "reaction_after_COMBAT_ROUND_STARTED",
      [outer("sh1", "Shields Holding", "COMBAT_ROUND_STARTED", "after")],
      trigger("combat_started", "COMBAT_ROUND_STARTED", "after", {
        actor: other,
        subject: actor,
        system: "18",
      }),
      { Reaction: "COMBAT_ROUND_STARTED" },
    ),
    make(
      "gallery-reaction-two-cards",
      "Reaction: two different cards",
      "Inner step: pick one of two cards, each with its window and text",
      "Several differently named cards for one window repeat the same trigger.",
      "play_reaction_after_SYSTEM_ACTIVATED",
      [
        option("fs1", "play Flank Speed", "action_card", { card: "fs1", card_name: "Flank Speed" }),
        option("decoy", "play Decoy Operation", "action_card", { card: "decoy", card_name: "Decoy Operation" }),
      ],
      trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "18" }),
    ),
    make(
      "gallery-reaction-no-trigger",
      "Reaction: no trigger data",
      "An older server: only the window is known",
      "Without a trigger the dialog says only that a window opened, never a raw engine id.",
      "reaction_after_SHIP_DESTROYED",
      [outer("sabo1", "Sabotage", "SHIP_DESTROYED", "after")],
      null,
      { Reaction: "SHIP_DESTROYED" },
    ),
  ];
}

function turnBarCases(): GalleryCase[] {
  const tokens = { tactic: 3, fleet: 3, strategy: 2 };
  const partners = [
    {
      seat: "other_seat",
      faction: "hacan",
      available: true,
      in_contact: true,
      reason: null,
      trade_goods: 2,
      commodities: 3,
      promissory_notes: 1,
    },
    {
      seat: "seat_c",
      faction: "letnev",
      available: true,
      in_contact: true,
      reason: null,
      trade_goods: 0,
      commodities: 2,
      promissory_notes: 2,
    },
    {
      seat: "seat_d",
      faction: "xxcha",
      available: false,
      in_contact: false,
      reason: "No contact: not neighbours",
      trade_goods: 1,
      commodities: 1,
      promissory_notes: 0,
    },
  ];
  const trade = [
    option("component|trade|hacan", "open a transaction with hacan", "open_transaction"),
    option("component|trade|letnev", "open a transaction with letnev", "open_transaction"),
  ];
  const components = [
    option("faction|orbital_drop", "Orbital Drop: spend a strategy token to land 2 infantry", "component"),
    option("component|tech|sr", "use Sling Relay", "component"),
    option("action_card|0", "play Reactor Meltdown", "component"),
    option("action_card|1", "play Ghost Ship", "component"),
  ];
  const menu = (
    nonce: string,
    options: ReturnType<typeof option>[],
    details: Record<string, unknown>,
    prompt = "action phase",
  ): PendingChoiceDto => ({
    actor,
    nonce,
    prompt,
    context: { subtype: `prompt:${prompt}` } as PendingChoiceDto["context"],
    options,
    details: { kind: "turn_menu", closing: false, tokens, partners, ...details },
  });
  const oneCard = [{ card: "pok8imperial", used: false, option: "strategic" }];
  const twoCards = [
    { card: "pok2diplomacy", used: false, option: "strategic|pok2diplomacy" },
    { card: "pok8imperial", used: false, option: "strategic|pok8imperial" },
  ];
  return [
    {
      workflow: "generic_selection",
      title: "Turn bar: your turn, one strategy card",
      fallback: "Action phase menu becomes the persistent bottom bar (5-6 players)",
      note: "One Strategic action button; Pass stays disabled until the card is used; Trade opens a partner picker (press D).",
      choice: menu(
        "gallery-turn-bar-one",
        [
          option("strategic", "take your strategic action", "action"),
          option("tactical", "take a tactical action", "action"),
          ...trade,
          ...components,
        ],
        { strategy_cards: oneCard },
      ),
    },
    {
      workflow: "generic_selection",
      title: "Turn bar: two strategy cards",
      fallback: "One Strategic button per held card (3-4 players)",
      note: "Each card has its own name, one-line effect and used/available state; hover shows the full text.",
      choice: menu(
        "gallery-turn-bar-two",
        [
          option("strategic|pok2diplomacy", "take the strategic action of 2. Diplomacy", "action"),
          option("strategic|pok8imperial", "take the strategic action of 8. Imperial", "action"),
          option("tactical", "take a tactical action", "action"),
          ...trade,
          ...components,
        ],
        { strategy_cards: twoCards },
      ),
    },
    {
      workflow: "generic_selection",
      title: "Turn bar: strategic card used",
      fallback: "A used card stays on the bar, disabled with its reason",
      note: "Diplomacy is used, Imperial remains and keeps the bare strategic option id.",
      choice: menu(
        "gallery-turn-bar-used",
        [
          option("strategic", "take your strategic action", "action"),
          option("tactical", "take a tactical action", "action"),
          ...trade,
        ],
        {
          strategy_cards: [
            { card: "pok2diplomacy", used: true, option: null },
            { card: "pok8imperial", used: false, option: "strategic" },
          ],
        },
      ),
    },
    {
      workflow: "generic_selection",
      title: "Turn bar: end your turn",
      fallback: "The end-turn modal becomes a highlighted End turn button",
      note: "After an action: End turn is highlighted (Enter), Tactical and Strategic say Already acted, trading stays open.",
      choice: menu(
        "gallery-turn-bar-end",
        [option("end_turn", "end your turn", "end_turn"), ...trade],
        {
          closing: true,
          tokens: { tactic: 2, fleet: 3, strategy: 2 },
          strategy_cards: twoCards.map((c) => ({ ...c, option: null })),
        },
        "end your turn",
      ),
    },
    {
      workflow: "generic_selection",
      title: "Turn bar: not your turn",
      fallback: "Read-only bar while another seat acts",
      note: "Every button is disabled with the reason; the pools and your own strategy cards stay readable.",
      viewer: "other",
      choice: menu(
        "gallery-turn-bar-readonly",
        [option("tactical", "take a tactical action", "action")],
        { strategy_cards: oneCard },
      ),
    },
  ];
}

