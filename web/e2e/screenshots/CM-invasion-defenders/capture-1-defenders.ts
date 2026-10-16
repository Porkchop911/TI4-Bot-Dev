import { expect, test, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard } from "../_shared/fixtures";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

interface Planet {
  id: string;
  label: string;
  resources: number;
  influence: number;
  traits?: string[];
  /** The opponent's units standing on it ("mech!" = damaged mech). */
  rivals?: string[];
  controlledBy?: string;
}

/** Opens the landing tray for system 18 with synthetic planets, defenders and forces in space. */
async function openLanding(
  page: Page,
  planets: Planet[],
  infantry: number,
  mechs: number,
  options: { myTechs?: string[]; rivalTechs?: string[] } = {},
) {
  const base = galleryBoard.systems["18"];
  const units = [
    ...Array.from({ length: infantry }, () => ({ owner: actor, unit_type: "infantry", damaged: false })),
    ...Array.from({ length: mechs }, () => ({ owner: actor, unit_type: "mech", damaged: false })),
    ...planets.flatMap((p) =>
      (p.rivals ?? []).map((unit) => ({
        owner: opponent.id,
        unit_type: unit.replace("!", ""),
        damaged: unit.endsWith("!"),
        planet: p.id,
      })),
    ),
  ];
  const board = {
    ...galleryBoard,
    map_tiles: galleryBoard.map_tiles?.map((tile) =>
      tile.system_id === "18"
        ? {
            ...tile,
            planets: planets.map(({ id, label, resources, influence, traits }) => ({
              id,
              label,
              resources,
              influence,
              ...(traits ? { traits } : {}),
            })),
          }
        : tile,
    ),
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...base,
        units,
        planets: Object.fromEntries(
          planets.map((p) => [
            p.id,
            { planet_id: p.id, exhausted: false, controlled_by: p.controlledBy ?? null },
          ]),
        ),
      },
    },
    invasion: {
      system_id: "18",
      invasion_seq: 1,
      invader: actor,
      phase: "landing" as const,
      planets: planets.map((p) => p.id),
      current_planet: null,
      defender: null,
      ground_round: 0,
      odds_context: {},
    },
  };
  const unitTypes = mechs > 0 ? ["infantry", "mech"] : ["infantry"];
  await openMockedGame(page, {
    players: [
      playerWithHand(options.myTechs ? { technologies: options.myTechs } : {}),
      withPlayer(opponent, options.rivalTechs ? { technologies: options.rivalTechs } : {}),
    ],
    board,
    choice: {
      prompt: "Land ground forces",
      context: { subtype: "commit_ground_forces", target: { System: "18" } },
      options: [
        ...planets.flatMap((p) =>
          unitTypes.map((unit) => ({
            id: `land|${p.id}|${unit}`,
            label: `Land ${unit} on ${p.label}`,
            kind: "land",
            payload: { planet: p.id, unit },
          })),
        ),
        { id: "done_committing", label: "Done committing", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("invasion-planet-values").first().waitFor();
}

const scenarios: {
  id: string;
  planets: Planet[];
  infantry: number;
  mechs: number;
  myTechs?: string[];
  rivalTechs?: string[];
  /** Expected texts in the defender block(s), in planet order. */
  expect: string[];
  scd: string[];
}[] = [
  {
    id: "1-mixed",
    planets: [
      {
        id: "moll",
        label: "Moll",
        resources: 1,
        influence: 3,
        traits: ["hazardous"],
        rivals: ["infantry", "infantry", "infantry", "mech", "pds", "space_dock"],
        controlledBy: opponent.id,
      },
      { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
      {
        id: "quann",
        label: "Quann",
        resources: 2,
        influence: 1,
        traits: ["industrial"],
        rivals: ["pds"],
        controlledBy: opponent.id,
      },
    ],
    infantry: 5,
    mechs: 1,
    expect: ["1 Mech", "3 Infantry", "1 PDS", "1 Space Dock", "no ground forces"],
    scd: ["1 die (PDS hits on 6)", "1 die (PDS hits on 6)"],
  },
  {
    id: "2-pds2-damaged-mech",
    planets: [
      {
        id: "moll",
        label: "Moll",
        resources: 1,
        influence: 3,
        traits: ["hazardous"],
        rivals: ["infantry", "infantry", "mech!", "pds", "pds"],
        controlledBy: opponent.id,
      },
    ],
    infantry: 4,
    mechs: 1,
    rivalTechs: ["pds2"],
    expect: ["1 Mech (1 damaged)", "2 Infantry", "2 PDS"],
    scd: ["2 dice (2× PDS hits on 5)", "~1.2 hits"],
  },
  {
    id: "3-l4-silenced",
    planets: [
      {
        id: "moll",
        label: "Moll",
        resources: 1,
        influence: 3,
        traits: ["hazardous"],
        rivals: ["infantry", "pds"],
        controlledBy: opponent.id,
      },
    ],
    infantry: 3,
    mechs: 0,
    myTechs: ["l4"],
    expect: ["1 Infantry", "1 PDS"],
    scd: ["silenced by your L4 Disruptors"],
  },
];

for (const viewport of [
  { id: "desktop", width: 1280, height: 900 },
  { id: "390x844", width: 390, height: 844 },
]) {
  for (const scenario of scenarios) {
    test(`${scenario.id}-${viewport.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await openLanding(page, scenario.planets, scenario.infantry, scenario.mechs, scenario);
      const blocks = page.getByTestId("invasion-defenders");
      await blocks.first().waitFor();
      const text = (await blocks.allTextContents()).join(" | ");
      for (const fragment of scenario.expect) expect(text).toContain(fragment);
      const scd = (await page.getByTestId("invasion-scd").allTextContents()).join(" | ");
      for (const fragment of scenario.scd) expect(scd).toContain(fragment);
      // The block sits inside the planet card, above the + and − steppers.
      const card = page.locator(".invasion-planet-landing-card").first();
      await expect(card.getByTestId("invasion-defenders")).toBeVisible();
      if (viewport.width < 500) {
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
      }
      await shot(page, testInfo, `${scenario.id}-${viewport.id}`, {
        of: page.locator(".invasion-landing-body"),
        pad: 12,
      });
    });
  }
}
