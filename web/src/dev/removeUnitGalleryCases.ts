import type { GalleryCase } from "./decisionGalleryCases.ts";

const actor = "gallery_seat";
const remove = (id: string, label: string) => ({ id, label, kind: "remove" });

/** "Remove a unit" over the fleet supply or capacity: system 18 holds the shared gallery fleet. */
export function removeUnitCases(): GalleryCase[] {
  const make = (
    nonce: string,
    title: string,
    reason: string,
    details: Record<string, unknown>,
    note: string,
  ): GalleryCase => ({
    workflow: "generic_selection",
    title,
    fallback: "Unit names and icons, where each is, how far over, what leaves with it",
    note,
    choice: {
      actor,
      nonce,
      prompt: `remove a unit: over ${reason} in 18`,
      details: { reason, system: "18", ...details },
      options: [remove("remove|0", "remove dreadnought"), remove("remove|2", "remove fighter")],
    },
  });
  return [
    manyReactionsCase(),
    ...unitAbilityCases(),
    make(
      "gallery-remove-supply",
      "Remove a unit: over fleet supply",
      "fleet supply",
      { fleet_limit: 1, fleet_charged: 3 },
      "Engine details give the supply and the limit; the board gives the fleet and its cargo.",
    ),
    make(
      "gallery-remove-capacity",
      "Remove a unit: over capacity",
      "capacity",
      { capacity_consumed: 3, capacity_transport: 2 },
      "Capacity: fighters and ground forces against the places the ships offer.",
    ),
  ];
}

/** A reaction window with more than four cards keeps the reaction dialog (it once became a bare list). */
function manyReactionsCase(): GalleryCase {
  const cards: Array<[string, string]> = [
    ["sabo1", "Sabotage"],
    ["direct_hit", "Direct Hit"],
    ["skilled_retreat", "Skilled Retreat"],
    ["counterstroke", "Counterstroke"],
    ["reflective_shielding", "Reflective Shielding"],
  ];
  return {
    workflow: "action_card_reaction",
    title: "Reaction window with five cards",
    fallback: "Used to fall into the generic list; now the reaction dialog with every card and Pass",
    note: "More than four options no longer leave the reaction dialog; the bar scrolls.",
    choice: {
      actor,
      nonce: "gallery-reaction-many",
      prompt: "reaction_after_SYSTEM_ACTIVATED",
      context: {
        subtype: "reaction_after_SYSTEM_ACTIVATED",
        optional: true,
        source: { Reaction: "SYSTEM_ACTIVATED" },
      },
      options: [
        ...cards.map(([id, name]) => ({
          id,
          label: `Play ${name}`,
          kind: "ability",
          payload: { card: id, card_name: name },
        })),
        { id: "decline", label: "Pass", kind: "decline" },
      ],
    },
  };
}

/** Orbital Drop, Sling Relay and Transit Diodes: unit names, planets and systems instead of ids. */
function unitAbilityCases(): GalleryCase[] {
  return [
    {
      workflow: "planet_selection",
      title: "Orbital Drop: deploy a mech",
      fallback: "Names the mech, the planet and system, and the cost; Pass stays",
      note: "The option payload already carries unit, planet, system and cost.",
      choice: {
        actor,
        nonce: "gallery-orbital-drop",
        prompt: "Orbital Drop: deploy a mech on jord",
        context: { subtype: "orbital_drop_deploy_mech", optional: true },
        options: [
          {
            id: "deploy|sol_mech|1",
            label: "deploy 1 sol_mech for 3 resources",
            kind: "produce",
            payload: { unit: "sol_mech", count: 1, cost: 3, system: "18", planet: "jord", orbital_drop_deploy: true },
          },
          { id: "decline", label: "Pass", kind: "decline" },
        ],
      },
    },
    {
      workflow: "generic_selection",
      title: "Sling Relay: produce one unit",
      fallback: "Names the ship and the system, and the cost",
      note: "Context-less production prompt; the options carry unit, system and cost.",
      choice: {
        actor,
        nonce: "gallery-sling-relay",
        prompt: "produce one unit in 18",
        options: [
          {
            id: "build|sol_carrier2|1",
            label: "produce 1x sol_carrier2 for 3",
            kind: "produce",
            payload: { unit: "sol_carrier2", count: 1, cost: 3, system: "18" },
          },
          {
            id: "build|destroyer|1",
            label: "produce 1x destroyer for 1",
            kind: "produce",
            payload: { unit: "destroyer", count: 1, cost: 1, system: "18" },
          },
        ],
      },
    },
    {
      workflow: "planet_selection",
      title: "Transit Diodes: redeploy a ground force",
      fallback: "Names the unit, the source and target planet with their systems, and who is already there",
      note: "From the option payload and the board.",
      choice: {
        actor,
        nonce: "gallery-transit-diodes",
        prompt: "Transit Diodes: move a ground force",
        context: { subtype: "transit_diodes_redeploy", optional: true },
        options: [
          {
            id: "transit|18|jord|infantry|18|exhausted",
            label: "move infantry from jord to exhausted",
            kind: "transit",
            payload: { source_system: "18", source: "jord", unit: "infantry", destination_system: "18", planet: "exhausted" },
          },
          { id: "decline", label: "Pass", kind: "decline" },
        ],
      },
    },
  ];
}
