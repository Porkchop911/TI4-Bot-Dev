// The technology tree of the demo: colour, prerequisites and the printed text of each technology.
import type { TechColor } from "../model";

export interface Tech {
  id: string;
  name: string;
  /** The colour of the card. A unit upgrade has none. */
  color: TechColor | null;
  /** The prerequisites, one letter for each symbol. */
  needs: string;
  text: string;
  /** A technology of the viewer's faction. It has a row of its own in the tree. */
  faction?: boolean;
}

const tech = (color: TechColor | null, name: string, needs: string, text: string): Tech => ({
  id: name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-$/, ""),
  name,
  color,
  needs,
  text,
});

export const TECHS: Tech[] = [
  tech(
    "B",
    "Antimass Deflectors",
    "",
    "Your ships can move into and through asteroid fields. Other players’ Space Cannon rolls against your ships get −1.",
  ),
  tech(
    "B",
    "Dark Energy Tap",
    "",
    "After you perform a tactical action in a system with a frontier token and your ships, explore that token. Your ships can retreat into adjacent systems without units of other players.",
  ),
  tech(
    "B",
    "Gravity Drive",
    "B",
    "After you activate a system, apply +1 to the move value of 1 of your ships.",
  ),
  tech(
    "B",
    "Sling Relay",
    "B",
    "ACTION: Exhaust this card to produce 1 ship in any system that contains 1 of your space docks.",
  ),
  tech(
    "B",
    "Fleet Logistics",
    "BB",
    "During each of your turns of the action phase, you may perform 2 actions instead of 1.",
  ),
  tech(
    "B",
    "Light/Wave Deflector",
    "BBB",
    "Your ships can move through systems that contain other players’ ships.",
  ),
  tech("G", "Neural Motivator", "", "During the status phase, draw 2 action cards instead of 1."),
  tech(
    "G",
    "Psychoarchaeology",
    "",
    "You can use technology specialties on planets you control without exhausting them. During the action phase, you can exhaust planets with technology specialties to gain 1 trade good.",
  ),
  tech(
    "G",
    "Dacxive Animators",
    "G",
    "After you win a ground combat, you may place 1 infantry from your reinforcements on that planet.",
  ),
  tech(
    "G",
    "Bio-Stims",
    "G",
    "You may exhaust this card at the end of your turn to ready 1 of your planets that has a technology specialty or 1 of your other technologies.",
  ),
  tech(
    "G",
    "Hyper Metabolism",
    "GG",
    "During the status phase, gain 3 command tokens instead of 2.",
  ),
  tech(
    "G",
    "X-89 Bacterial Weapon",
    "GGG",
    "ACTION: Exhaust this card and choose 1 planet in a system that contains 1 or more of your ships that have Bombardment; destroy all infantry on that planet.",
  ),
  tech(
    "Y",
    "Sarween Tools",
    "",
    "When 1 or more of your units use Production, reduce the combined cost of the produced units by 1.",
  ),
  tech(
    "Y",
    "Scanlink Drone Network",
    "",
    "When you activate a system, you may explore 1 planet in that system that contains 1 or more of your units.",
  ),
  tech(
    "Y",
    "Graviton Laser System",
    "Y",
    "You may exhaust this card before 1 or more of your units use Space Cannon; hits produced by those units must be assigned to non-fighter ships if able.",
  ),
  tech(
    "Y",
    "Predictive Intelligence",
    "Y",
    "At the end of your turn, you may exhaust this card to redistribute your command tokens. When you cast votes, you may cast 3 additional votes.",
  ),
  tech(
    "Y",
    "Transit Diodes",
    "YY",
    "You may exhaust this card at the start of your turn during the action phase; remove up to 4 of your ground forces from the game board and place them on 1 or more planets you control.",
  ),
  tech(
    "Y",
    "Integrated Economy",
    "YYY",
    "After you gain control of a planet, you may produce any number of units on that planet that have a combined cost equal to or less than that planet’s resource value.",
  ),
  tech(
    "R",
    "Plasma Scoring",
    "",
    "When 1 or more of your units use Bombardment or Space Cannon, 1 of those units may roll 1 additional die.",
  ),
  tech(
    "R",
    "AI Development Algorithm",
    "",
    "When you research a unit upgrade technology, you may exhaust this card to ignore any 1 prerequisite. When your units use Production, you may exhaust this card to reduce the combined cost by the number of unit upgrade technologies that you own.",
  ),
  tech(
    "R",
    "Magen Defense Grid",
    "R",
    "At the start of ground combat on a planet that contains 1 or more of your structures, you may produce 1 hit and assign it to 1 of your opponent’s ground forces.",
  ),
  tech(
    "R",
    "Self Assembly Routines",
    "R",
    "After 1 or more of your units use Production, you may exhaust this card to place 1 mech from your reinforcements on a planet you control in that system.",
  ),
  tech(
    "R",
    "Duranium Armor",
    "RR",
    "During each combat round, after you assign hits to your units, repair 1 of your damaged units that did not use Sustain Damage during this combat round.",
  ),
  tech(
    "R",
    "Assault Cannon",
    "RRR",
    "At the start of a space combat in a system that contains 3 or more of your non-fighter ships, your opponent must destroy 1 of their non-fighter ships.",
  ),
  tech(
    null,
    "Spec Ops II",
    "GG",
    "Infantry: combat 6. After this unit is destroyed, roll 1 die. If the result is 5 or greater, place the unit on this card; at the start of your next turn, place it on a planet you control in your home system.",
  ),
  tech(
    null,
    "Fighter II",
    "GB",
    "Fighter: combat 8, move 2. This unit may move without being transported. Fighters in excess of your ships’ capacity count against your fleet pool.",
  ),
  tech(null, "Destroyer II", "RR", "Destroyer: combat 8, move 2, Anti-Fighter Barrage 6 (×3)."),
  tech(null, "Cruiser II", "GYR", "Cruiser: combat 6, move 3, capacity 1."),
  tech(null, "Advanced Carrier II", "BB", "Carrier: combat 9, move 2, capacity 8, Sustain Damage."),
  tech(
    null,
    "Dreadnought II",
    "BBY",
    "Dreadnought: combat 5, move 2, capacity 1, Sustain Damage, Bombardment 5. “Direct Hit” cards are no longer effective against this type of ship.",
  ),
  tech(
    null,
    "War Sun",
    "RRRY",
    "War Sun: cost 12, combat 3 (×3), move 2, capacity 6, Sustain Damage, Bombardment 3 (×3). Other players’ units in this system lose Planetary Shield.",
  ),
  tech(
    null,
    "PDS II",
    "RY",
    "PDS: Planetary Shield, Space Cannon 5. You may use this unit’s Space Cannon against ships that are in adjacent systems.",
  ),
  tech(
    null,
    "Space Dock II",
    "YY",
    "Space dock: this unit’s Production value is equal to 4 more than the resource value of this planet. Up to 3 fighters in this system do not count against your ships’ capacity.",
  ),
  // Faction technologies. Sol has unit upgrades only (Spec Ops II, Advanced Carrier II); these two
  // are in the demo so that the row of a faction with other technologies can be checked.
  {
    ...tech(
      "B",
      "Chaos Mapping",
      "B",
      "Other players cannot activate asteroid fields that contain 1 or more of your ships. At the start of your turn during the action phase, you may produce 1 unit in a system that contains at least 1 of your units that has Production.",
    ),
    faction: true,
  },
  {
    ...tech(
      "Y",
      "Quantum Datahub Node",
      "YYY",
      "At the end of the strategy phase, you may spend 1 token from your strategy pool and give another player 3 of your trade goods. If you do, give 1 of your strategy cards to that player and take 1 of their strategy cards.",
    ),
    faction: true,
  },
];

export const TECH_BY_ID: Record<string, Tech> = Object.fromEntries(
  TECHS.map((item) => [item.id, item]),
);
export const TECH_COLORS: [TechColor, string][] = [
  ["B", "Blue"],
  ["G", "Green"],
  ["Y", "Yellow"],
  ["R", "Red"],
];
