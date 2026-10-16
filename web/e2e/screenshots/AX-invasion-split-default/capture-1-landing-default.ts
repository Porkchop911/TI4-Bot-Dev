import { test, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// unified-latest-2026-10-08); the default "after" regenerates the current layout.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

interface Planet {
  id: string;
  label: string;
  resources: number;
  influence: number;
  traits?: string[];
  legendary?: boolean;
  attachments?: string[];
  /** Rival units standing on it. */
  rivals?: string[];
}

/** Opens the landing tray for system 18 with synthetic planets and ground forces in space. */
async function openLanding(page: Page, planets: Planet[], infantry: number, mechs = 0) {
  const base = galleryBoard.systems["18"];
  const units = [
    ...Array.from({ length: infantry }, () => ({ owner: actor, unit_type: "infantry", damaged: false })),
    ...Array.from({ length: mechs }, () => ({ owner: actor, unit_type: "mech", damaged: false })),
    ...planets.flatMap((p) =>
      (p.rivals ?? []).map((unit_type) => ({ owner: opponent.id, unit_type, damaged: false, planet: p.id })),
    ),
  ];
  const board = {
    ...galleryBoard,
    map_tiles: galleryBoard.map_tiles?.map((tile) =>
      tile.system_id === "18"
        ? {
            ...tile,
            planets: planets.map(({ id, label, resources, influence, traits, legendary }) => ({
              id,
              label,
              resources,
              influence,
              ...(traits ? { traits } : {}),
              ...(legendary ? { legendary } : {}),
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
          planets.map((p) => [p.id, { planet_id: p.id, exhausted: false, attachments: p.attachments }]),
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
    players: [playerWithHand(), opponent],
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

const three: Planet[] = [
  { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
  { id: "moll", label: "Moll", resources: 1, influence: 3, traits: ["hazardous"] },
  { id: "quann", label: "Quann", resources: 2, influence: 1, traits: ["industrial"] },
];

const scenarios: { id: string; planets: Planet[]; infantry: number; mechs?: number }[] = [
  { id: "three-uninhabited", planets: three, infantry: 6 },
  {
    id: "mixed-defended",
    planets: three.map((p) => (p.id === "moll" ? { ...p, rivals: ["infantry", "infantry", "pds"] } : p)),
    infantry: 5,
    mechs: 1,
  },
  { id: "one-planet", planets: [three[0]!], infantry: 4 },
  {
    id: "legendary-attachment-mecatol",
    planets: [
      { id: "avernus", label: "Avernus", resources: 2, influence: 0, legendary: true },
      { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"], attachments: ["bioticstat"] },
      { id: "mr", label: "Mecatol Rex", resources: 1, influence: 6 },
    ],
    infantry: 6,
  },
];

for (const viewport of [
  { id: "desktop", width: 1280, height: 900 },
  { id: "phone", width: 390, height: 844 },
]) {
  for (const scenario of scenarios) {
    test(`${scenario.id}-${viewport.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await openLanding(page, scenario.planets, scenario.infantry, scenario.mechs ?? 0);
      await shot(page, testInfo, `${prefix}-${scenario.id}-${viewport.id}`, {
        of: page.locator(".invasion-landing-body"),
        pad: 12,
      });
    });
  }
}
