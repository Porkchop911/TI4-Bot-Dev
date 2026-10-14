import type { GalleryCase } from "./decisionGalleryCases.ts";

const actor = "gallery_seat";
const option = (id: string, label: string, kind?: string, payload?: Record<string, unknown>) => ({
  id,
  label,
  kind,
  payload,
});
const decline = (label = "decline") => option("decline", label, "decline");

/** Decisions the Porkchop911 merge added, shown as engine-built offer cards. */
export function offerCases(): GalleryCase[] {
  return [
    {
      workflow: "generic_selection",
      title: "Ground hit: sustain damage",
      fallback: "'use SUSTAIN DAMAGE' / decline -> the unit, the planet and what each answer does",
      note: "An effect (Deorbit Barrage and similar) assigned a hit to a named ground force; its owner may sustain it.",
      choice: {
        actor,
        nonce: "gallery-ground-sustain",
        prompt: "sustain the hit on mech on Jord (system 14)",
        context: { subtype: "ground_effect_sustain", source: { Rule: "87" } },
        options: [option("sustain", "use SUSTAIN DAMAGE", "ground_effect_sustain"), decline()],
        details: {
          kind: "offer",
          card: {
            title: "Sustain damage",
            tag: "ground hit",
            window: "An effect has assigned a hit to this ground force",
            text: "SUSTAIN DAMAGE cancels the hit and the unit stays, damaged. Without it the unit is destroyed.",
          },
          facts: [
            { label: "Unit", unit: "mech" },
            { label: "Where", planet: "jord", system: "14" },
          ],
          captions: {
            sustain: { label: "Sustain damage", hint: "The unit stays on the planet, damaged" },
            decline: { label: "Let it be destroyed", hint: "The unit is removed" },
          },
        },
      },
    },
    reduceCase(),
    {
      workflow: "generic_selection",
      title: "Ssruu: choose a unit for the copied agent",
      fallback: "'other_seat's cruiser (unit 1)' labels -> the copied agent as printed and each unit by place and owner",
      note: "Ssruu copies a Letnev or Sol agent: choose one unit that rolls an extra die this round.",
      choice: {
        actor,
        nonce: "gallery-ssruu-round",
        prompt: "Ssruu copying letnevagent: choose one unit for +1 combat die",
        context: { subtype: "leader_ssruu_round_agent_unit", source: { Content: "letnevagent" } },
        options: [
          option("other_seat|space|cruiser|0|0", "other_seat's cruiser (unit 1)", "leader_ssruu_round_agent_unit"),
          option("other_seat|jord|infantry|1|0", "other_seat's infantry on jord (unit 2)", "leader_ssruu_round_agent_unit"),
          option("gallery_seat|space|dreadnought|0|0", "gallery_seat's dreadnought (unit 1)", "leader_ssruu_round_agent_unit"),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Viscount Unlenn",
            tag: "agent (copied by Ssruu)",
            window: "At the start of a space combat round:",
            text: "You may exhaust this card to choose 1 ship in the active system: that ship rolls 1 additional die during this combat round.",
          },
          facts: [],
          captions: {
            "other_seat|space|cruiser|0|0": { label: "cruiser · In the fleet", hint: "Unit 1 of that kind there", seat: "other_seat" },
            "other_seat|jord|infantry|1|0": { label: "infantry · On Jord", hint: "Unit 2 of that kind there", seat: "other_seat" },
            "gallery_seat|space|dreadnought|0|0": { label: "dreadnought · In the fleet", hint: "Unit 1 of that kind there", seat: "gallery_seat" },
          },
        },
      },
    },
    {
      workflow: "planet_selection",
      title: "L1Z1X agent (copied): replace which infantry",
      fallback: "A list of 'replace infantry on X with a mech' labels -> the planet bar with the candidates on the map",
      note: "Ssruu copying I48S after an activation: several planets in the active system hold your infantry; pick the one that becomes a mech. The options now carry their planet, so the planet bar and the map take over.",
      choice: {
        actor,
        nonce: "gallery-l1z1x-copy",
        prompt: "I48S (Ssruu): replace which infantry in the active system?",
        context: { subtype: "leader_l1z1xagent_copy_planet", source: { Content: "l1z1xagent" } },
        options: [
          option("jord", "replace infantry on Jord with a mech", "leader_l1z1xagent_copy_planet", { planet: "jord", system: "14" }),
          option("lodor", "replace infantry on Lodor with a mech", "leader_l1z1xagent_copy_planet", { planet: "lodor", system: "14" }),
        ],
      },
    },
    {
      workflow: "generic_selection",
      title: "Slumberstate Computing: coexist or fight",
      fallback: "'fight for jord' / 'coexist on jord' -> the breakthrough as printed, the planet and its controller, what each answer does",
      note: "Titans' Coalescence would start a ground combat on a planet another seat controls; with no other units committed the Titans may coexist instead.",
      choice: {
        actor,
        nonce: "gallery-coexist",
        prompt: "Slumberstate Computing: coexist on jord instead of fighting",
        context: {
          subtype: "coalescence_coexist",
          source: { Content: "titansbt" },
          target: { Planet: { system: "14", planet: "jord" } },
        },
        options: [
          option("fight", "fight for jord", "coalescence"),
          option("coexist", "coexist on jord", "coalescence"),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Slumberstate Computing",
            tag: "breakthrough",
            text: "When COALESCENCE results in a ground combat, if you commit no other units, you may choose for your units to coexist instead.",
          },
          facts: [
            { label: "Planet", planet: "jord", system: "14" },
            { label: "Controlled by", seat: "other_seat" },
          ],
          captions: {
            fight: { label: "Fight for the planet", hint: "Ground combat decides who controls it" },
            coexist: { label: "Coexist", hint: "No combat; the controller keeps the planet" },
          },
        },
      },
    },
    {
      workflow: "generic_selection",
      title: "Take a revealed action card",
      fallback: "Card names only -> each shown card with its printed text, the way a hand discard shows cards",
      note: "Mageon Implants or Spy Net shows you another seat's hand; you take one card into yours. One option per distinct card.",
      choice: {
        actor,
        nonce: "gallery-take-revealed",
        prompt: "take 1 of other_seat's action cards",
        context: { subtype: "take_revealed_action_card", source: { FactionAbility: "mageon" } },
        options: [
          option("abs1", "Ancient Burial Sites", "take_revealed_card"),
          option("sab1", "Sabotage", "take_revealed_card"),
          option("rea1", "Reparations", "take_revealed_card"),
          decline(),
        ],
      },
    },
    {
      workflow: "generic_selection",
      title: "Reinforcements: choose where to place",
      fallback: "'place 2x infantry on jord in 14' labels -> the unit, how many, and each spot by its planet and system",
      note: "An ability places units from your reinforcements on a planet you control (or in a ship space): the spots are named, and each says how many go there.",
      choice: {
        actor,
        nonce: "gallery-reinforce-place",
        prompt: "place up to 2 infantry",
        context: { subtype: "place_units_from_reinforcements", source: { FactionAbility: "yso" } },
        options: [
          option("14|jord", "place 2x infantry on jord in 14", "place_unit", { system: "14", count: 2 }),
          option("26|lodor", "place 2x infantry on lodor in 26", "place_unit", { system: "26", count: 2 }),
          option("18|mr", "place 1x infantry on mr in 18", "place_unit", { system: "18", count: 1 }),
          decline(),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Place units from your reinforcements",
            tag: "placement",
            text: "Choose where they go.",
          },
          facts: [
            { label: "Unit", unit: "infantry" },
            { label: "Up to", value: 2 },
          ],
          captions: {
            "14|jord": { label: "Jord (system 14)", hint: "Place 2 infantry here" },
            "26|lodor": { label: "Lodor (system 26)", hint: "Place 2 infantry here" },
            "18|mr": { label: "Mecatol Rex (system 18)", hint: "Place 1 infantry here" },
            decline: { label: "Place none", hint: "Nothing is placed" },
          },
        },
      },
    },
    {
      workflow: "generic_selection",
      title: "Construction: PDS or an alternative",
      fallback: "'place pds on jord' / 'place 1 mech and 1 infantry ... instead' -> the planet and what each answer does",
      note: "After picking a PDS spot, a faction ability (Titans' Hecatoncheires) may replace the PDS. The question names the planet; the alternative keeps the engine's own wording.",
      choice: {
        actor,
        nonce: "gallery-pds-alternative",
        prompt: "place a PDS or an alternative",
        context: { subtype: "place_structure_pds_alternative", source: { Content: "place_structure" } },
        options: [
          option("pds", "place pds on jord", "build"),
          option("titans|hecatoncheires", "place 1 mech and 1 infantry on jord instead", "build"),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Place a PDS",
            tag: "construction",
            window: "A faction ability can replace this PDS",
            text: "You may place something else on this planet instead of the PDS.",
          },
          facts: [{ label: "Planet", planet: "jord", system: "14" }],
          captions: { pds: { label: "Place the PDS", hint: "As planned" } },
        },
      },
    },
    {
      workflow: "generic_selection",
      title: "Hacan commander: spend trade goods for votes",
      fallback: "Five 'spend N trade goods for 2N votes' labels -> an amount picker with the votes before and after",
      note: "Gila the Silvertongue: after your planets, spend any number of trade goods for two more votes each.",
      choice: {
        actor,
        nonce: "gallery-vote-goods",
        prompt: "spend trade goods for votes on for",
        context: { subtype: "vote_spend_trade_goods", source: { Content: "hacancommander" } },
        options: [
          option("spend|1", "spend 1 trade goods for 2 votes", "vote_trade_goods", { trade_goods: 1 }),
          option("spend|2", "spend 2 trade goods for 4 votes", "vote_trade_goods", { trade_goods: 2 }),
          option("spend|3", "spend 3 trade goods for 6 votes", "vote_trade_goods", { trade_goods: 3 }),
          option("spend|4", "spend 4 trade goods for 8 votes", "vote_trade_goods", { trade_goods: 4 }),
          decline(),
        ],
        details: {
          kind: "vote_trade_goods",
          card: {
            title: "Gila the Silvertongue",
            window: "When you cast votes:",
            text: "You may spend any number of trade goods: cast 2 additional votes for each trade good spent.",
          },
          outcome: "for",
          votes: 7,
          goods: 4,
          votes_per_good: 2,
        },
      },
    },
    {
      workflow: "generic_selection",
      title: "Research waiver: ignore prerequisites",
      fallback: "'return 1 infantry ...' / decline -> the technology and what the faction ability does",
      note: "Yin's commander lets a seat research a technology another seat owns without its prerequisites; the seat chooses the waiver, then pays for it.",
      choice: {
        actor,
        nonce: "gallery-research-waiver",
        prompt: "research ws: choose a prerequisite waiver",
        context: { subtype: "research_waiver", source: { FactionAbility: "research_waiver" } },
        options: [
          option("waiver|0", "return 1 infantry to reinforcements to ignore its prerequisites", "research_waiver"),
          decline(),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Research without prerequisites",
            tag: "faction ability",
            window: "A faction ability lets you research this technology without its prerequisites",
            text: "Choose how, and pay its cost on the next step. Declining researches nothing.",
          },
          facts: [{ label: "Technology", technology: "ws" }],
          captions: { decline: { label: "Don't use a waiver", hint: "Nothing is researched" } },
        },
      },
    },
    {
      workflow: "generic_selection",
      title: "Research waiver: choose the payment",
      fallback: "A list of payment labels -> the card says what the waiver is and which technology it buys",
      note: "The second step: which infantry (or captured unit) pays for the waived research.",
      choice: {
        actor,
        nonce: "gallery-waiver-pay",
        prompt: "return 1 infantry to reinforcements to ignore its prerequisites: choose the payment",
        context: { subtype: "research_waiver_payment", source: { FactionAbility: "research_waiver" } },
        options: [
          option("space|18", "return infantry from the space of system 18", "research_waiver_payment"),
          option("planet|jord", "return infantry from Jord", "research_waiver_payment"),
          decline(),
        ],
        details: {
          kind: "offer",
          card: {
            title: "Pay for the waiver",
            tag: "faction ability",
            window: "return 1 infantry to reinforcements to ignore its prerequisites",
            text: "Choose what pays for it. Declining researches nothing and pays nothing.",
          },
          facts: [{ label: "Technology", technology: "ws" }],
          captions: { decline: { label: "Don't pay", hint: "Nothing is researched or paid" } },
        },
      },
    },
    paymentCase({
      nonce: "gallery-crimson-pay",
      title: "Crimson commander: gain or convert",
      prompt: "Crimson commander: gain 1 commodity or convert 1 commodity to a trade good",
      subtype: "crimson_payment",
      source: "crimsoncommander",
      card: {
        title: "Ahk Siever",
        window: "At the end of a combat between any players:",
        text: "Gain 1 commodity or convert 1 of your commodities to a trade good.",
      },
      note: "The Crimson commander pays its holder after every combat; with room for a commodity and one to convert, the holder chooses.",
    }),
    paymentCase({
      nonce: "gallery-deepwrought-pay",
      title: "Deepwrought commander: gain or convert",
      prompt: "Deepwrought commander: gain 1 commodity or convert 1 to a trade good",
      subtype: "deepwrought_payment",
      source: "deepwroughtcommander",
      card: {
        title: "Aello",
        window: "When another player spends resources to research a technology",
        text: "That player may reduce the cost by 1; if they do, gain 1 commodity or convert 1 of your commodities to a trade good.",
      },
      note: "The holder is paid after another seat took 1 off its research cost with the Deepwrought commander.",
    }),
  ];
}

const reduceCase = (): GalleryCase => ({
  workflow: "generic_selection",
  title: "Deepwrought commander: reduce research",
  fallback: "'reduce by 1' / decline -> the commander as printed, the cost before and after, and who is paid",
  note: "While researching with a resource payment, another seat holding the Deepwrought commander offers 1 off; if taken, its holder is paid.",
  choice: {
    actor,
    nonce: "gallery-deepwrought-reduce",
    prompt: "Deepwrought commander: reduce this research by 1 (pays other_seat)",
    context: { subtype: "deepwrought_reduce_research", source: { Content: "deepwroughtcommander" } },
    options: [option("reduce", "reduce by 1", "research"), decline()],
    details: {
      kind: "offer",
      card: {
        title: "Aello",
        tag: "commander",
        window: "When another player spends resources to research a technology",
        text: "That player may reduce the cost by 1; if they do, gain 1 commodity or convert 1 of your commodities to a trade good.",
      },
      facts: [{ label: "Research cost (resources)", from: 4, to: 3 }],
      captions: {
        reduce: { label: "Reduce the cost by 1", hint: "The commander's holder is paid a commodity or a trade good" },
        decline: { label: "Pay in full", hint: "Nobody is paid" },
      },
    },
  },
});

/** The commanders that pay "gain 1 commodity or convert 1 to a trade good" (Crimson, Deepwrought). */
function paymentCase(spec: {
  nonce: string;
  title: string;
  prompt: string;
  subtype: string;
  source: string;
  card: { title: string; window: string; text: string };
  note: string;
}): GalleryCase {
  return {
    workflow: "generic_selection",
    title: spec.title,
    fallback: "Two plain labels -> the commander as printed, commodities and trade goods now, and before/after on each answer",
    note: spec.note,
    choice: {
      actor,
      nonce: spec.nonce,
      prompt: spec.prompt,
      context: { subtype: spec.subtype, source: { Content: spec.source } },
      options: [
        option("gain", "gain 1 commodity", "economy"),
        option("convert", "convert 1 commodity to a trade good", "economy"),
      ],
      details: {
        kind: "offer",
        card: { ...spec.card, tag: "commander" },
        facts: [
          { label: "Commodities", value: "1 of 3" },
          { label: "Trade goods", value: 4 },
        ],
        captions: {
          gain: { label: "Gain 1 commodity", hint: "Commodities 1 → 2" },
          convert: {
            label: "Convert 1 commodity to a trade good",
            hint: "Commodities 1 → 0, trade goods 4 → 5",
          },
        },
      },
    },
  };
}
