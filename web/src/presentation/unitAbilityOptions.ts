import type { BoardView, ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";
import { getPlanetDetails } from "./planetSelection.ts";
import { planetOccupants, systemTitle } from "./systemFacts.ts";

export interface UnitAbilityInfo {
  /** "Move Infantry", "Deploy Mech", "Produce Carrier II". */
  title: string;
  /** The unit shown as an icon, when the option is about one. */
  iconType: string | null;
  /** Where it goes and what it costs, one short line each. */
  lines: string[];
  /** Every system the option touches, for a map highlight. */
  systems: string[];
}

const str = (v: unknown): string | null => (typeof v === "string" && v ? v : null);
const num = (v: unknown): number | null => (typeof v === "number" ? v : null);

function unitName(unit: string): string {
  return `${getUnitDisplayName(unit)}${/2$/.test(unit) ? " II" : ""}`;
}

function place(planet: string | null, system: string | null, board?: BoardView | null): string {
  if (!system) return planet ? getPlanetDetails(planet, board).name : "";
  const where = systemTitle(system, board);
  if (!planet || planet === "space") return `the space area of ${where}`;
  return `${getPlanetDetails(planet, board).name} in ${where}`;
}

function occupants(system: string | null, planet: string | null, board?: BoardView | null) {
  if (!planet || planet === "space") return null;
  const here = planetOccupants(system, planet, board);
  return here.length ? `Already there: ${here.map((o) => o.summary).join("; ")}` : null;
}

/**
 * Names and places for the unit-moving abilities that used to show raw ids: Transit Diodes
 * ("move hacan_mech from arretze to hercant"), Orbital Drop ("deploy 1 sol_mech for 3 resources"),
 * Sling Relay and Chaos Mapping ("produce 1x sol_carrier2 for 3"). `null` for any other option.
 */
export function describeUnitAbilityOption(
  choice: Pick<PendingChoiceDto, "prompt" | "context">,
  option: ChoiceOption,
  board?: BoardView | null,
): UnitAbilityInfo | null {
  const p = option.payload ?? {};
  const unit = str(p.unit);
  const system = str(p.system);
  const planet = str(p.planet);

  if (option.id.startsWith("transit|") && unit) {
    const fromSystem = str(p.source_system);
    const fromPlace = str(p.source);
    const toSystem = str(p.destination_system);
    return {
      title: `Move ${unitName(unit)}`,
      iconType: unit,
      lines: [
        `From ${place(fromPlace, fromSystem, board)}`,
        `To ${place(planet, toSystem, board)}`,
        occupants(toSystem, planet, board),
      ].filter((l): l is string => Boolean(l)),
      systems: [fromSystem, toSystem].filter((s): s is string => Boolean(s)),
    };
  }

  if ((p.orbital_drop_deploy === true || option.id.startsWith("deploy|")) && unit) {
    const cost = num(p.cost);
    return {
      title: `Deploy ${unitName(unit)}`,
      iconType: unit,
      lines: [
        `On ${place(planet, system, board)}`,
        ...(cost !== null ? [`Cost: ${cost} resources`] : []),
        occupants(system, planet, board),
      ].filter((l): l is string => Boolean(l)),
      systems: system ? [system] : [],
    };
  }

  if (option.id.startsWith("build|") && unit && /(^|: )produce one unit in /.test(choice.prompt)) {
    const cost = num(p.cost);
    const count = num(p.count) ?? 1;
    return {
      title: `Produce ${count > 1 ? `${count} ` : ""}${getUnitDisplayName(unit, count)}${/2$/.test(unit) ? " II" : ""}`,
      iconType: unit,
      lines: [
        ...(system ? [`In ${place(null, system, board)}`] : []),
        ...(cost !== null ? [`Cost: ${cost}`] : []),
      ],
      systems: system ? [system] : [],
    };
  }

  if (option.kind === "production" && str(p.technology) && system) {
    return {
      title: `Produce one unit in ${systemTitle(system, board)}`,
      iconType: null,
      lines: [],
      systems: [system],
    };
  }
  return null;
}
