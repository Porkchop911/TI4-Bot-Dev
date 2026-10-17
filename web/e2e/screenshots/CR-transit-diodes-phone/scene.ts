import type { Browser, Page } from "@playwright/test";
import type { BoardView, ChoiceOption, PlacedUnitView, SystemView } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { actor, galleryBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { DEVICES } from "../BN-phone-detection/scene";

/** `before` shots come from the commit before the change (assertions on the new UI are skipped). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export type Size = { label: string; width: number; height: number };
export const SIZES: Size[] = [
  { label: "390x844", width: 390, height: 844 },
  { label: "360x740", width: 360, height: 740 },
];

/** Planets the viewing seat controls (system, planet) and its ground forces on them. */
const CONTROLLED: [string, string][] = [
  ["18", "jord"],
  ["18", "exhausted"],
  ["19", "wellon"],
  ["22", "tarmann"],
  ["26", "lodor"],
  ["25", "quann"],
  ["24", "mehar_xull"],
  ["20", "vefun_5"],
  ["27", "new_albion"],
  ["27", "starpoint"],
  ["29", "qucenn"],
  ["29", "rarron"],
  ["1", "sol_jord"],
];

/** Extra ground forces on top of the gallery board's (infantry on Jord, Tar'Mann, Sol's Jord, in 18's space). */
const EXTRA: Record<string, { unit: string; planet?: string }[]> = {
  "18": [{ unit: "mech", planet: "jord" }],
  "26": [{ unit: "infantry", planet: "lodor" }, { unit: "infantry", planet: "lodor" }],
  "27": [{ unit: "mech", planet: "starpoint" }, { unit: "infantry", planet: "new_albion" }],
  "29": [{ unit: "infantry", planet: "rarron" }],
  "1": [{ unit: "mech", planet: "sol_jord" }],
};

export function transitBoard(): BoardView {
  const systems: Record<string, SystemView> = { ...galleryBoard.systems };
  for (const [systemId, planetId] of CONTROLLED) {
    const base = systems[systemId] ?? { system_id: systemId, command_tokens: [], planets: {}, units: [] };
    systems[systemId] = {
      ...base,
      planets: {
        ...base.planets,
        [planetId]: { planet_id: planetId, exhausted: base.planets?.[planetId]?.exhausted ?? false, controlled_by: actor },
      },
    };
  }
  for (const [systemId, extra] of Object.entries(EXTRA)) {
    const base = systems[systemId];
    systems[systemId] = {
      ...base,
      units: [
        ...base.units,
        ...extra.map((e): PlacedUnitView => ({ unit_type: e.unit, owner: actor, damaged: false, ...(e.planet ? { planet: e.planet } : {}) })),
      ],
    };
  }
  return { ...galleryBoard, systems };
}

const GROUND = new Set(["infantry", "mech"]);

/** The engine's `transit_options` (crates/ti4-engine/src/technology.rs) over a board view. */
export function transitOptions(board: BoardView): ChoiceOption[] {
  const destinations: [string, string][] = [];
  for (const [systemId, sys] of Object.entries(board.systems))
    for (const [planetId, p] of Object.entries(sys.planets ?? {}))
      if (p.controlled_by === actor) destinations.push([systemId, planetId]);
  const seen = new Set<string>();
  const options: ChoiceOption[] = [];
  for (const [sourceSystem, sys] of Object.entries(board.systems).sort(([a], [b]) => a.localeCompare(b))) {
    for (const unit of sys.units) {
      if (unit.owner !== actor || !GROUND.has(unit.unit_type)) continue;
      const source = unit.planet ?? "space";
      for (const [destinationSystem, planet] of destinations) {
        if (sourceSystem === destinationSystem && source === planet) continue;
        const key = `${sourceSystem}|${source}|${unit.unit_type}|${planet}`;
        if (seen.has(key)) continue;
        seen.add(key);
        options.push({
          id: `transit|${sourceSystem}|${source}|${unit.unit_type}|${destinationSystem}|${planet}`,
          label: `move ${unit.unit_type} from ${source} to ${planet}`,
          kind: "transit",
          payload: { source_system: sourceSystem, source, unit: unit.unit_type, destination_system: destinationSystem, planet },
        });
      }
    }
  }
  return options;
}

export async function openTransit(browser: Browser, size: Size, baseURL?: string, sent: unknown[] = []) {
  const device = size.width > 720 ? {} : DEVICES.mobile;
  const context = await browser.newContext({
    ...device,
    viewport: { width: size.width, height: size.height },
    screen: { width: size.width, height: size.height },
    colorScheme: "dark",
    baseURL,
  });
  const page = await context.newPage();
  const board = transitBoard();
  const options = transitOptions(board);
  await openMockedGame(page, {
    players: [playerWithHand({ technologies: ["td"] }), opponent],
    board,
    choice: {
      prompt: "Transit Diodes: redeploy ground forces (4 left)",
      context: { subtype: "transit_diodes_redeploy" },
      options: [...options, { id: "decline", label: "finish redeployment", kind: "decline" }],
    },
    onClientMessage: (message) => sent.push(message),
  });
  return { context, page, options };
}

export async function horizontalOverflow(page: Page) {
  return page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
}
