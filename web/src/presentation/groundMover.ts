import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { getPlanetDetails } from "./planetSelection.ts";
import { isDeclineOption } from "./choiceModel.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";

type Mover = Pick<PendingChoiceDto, "context" | "options">;

export const isGroundMover = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  choice.context?.subtype === "ghost_squad_move";

export interface GroundMove {
  optionId: string;
  from: string;
  fromName: string;
  to: string;
  toName: string;
  unit: string;
  unitName: string;
  /** How many of that unit stand on the origin (the whole stack moves). */
  count: number;
  /** Your units of that type on the origin and the destination, before and after the move. */
  fromBefore: number;
  fromAfter: number;
  toBefore: number;
  toAfter: number;
}

export interface GroundMoverView {
  moves: GroundMove[];
  /** The option that ends the card ("Decline"), when offered. */
  doneId: string | null;
  /** The planets of the active system, with your ground forces on each. */
  planets: Array<{ id: string; name: string; forces: string }>;
}

const str = (value: unknown): string | null => (typeof value === "string" && value ? value : null);

/** `move|<from>|<to>|<unit>` and its stack size, from the payload or (older engines) the id and label. */
function parseMove(option: ChoiceOptionDto): { from: string; to: string; unit: string; count: number } | null {
  const payload = option.payload ?? {};
  let from = str(payload.from);
  let to = str(payload.to);
  let unit = str(payload.unit);
  if (!from || !to || !unit) {
    const [tag, f, t, u] = option.id.split("|");
    if (tag !== "move" || !f || !t || !u) return null;
    [from, to, unit] = [f, t, u];
  }
  const fromLabel = /move (\d+) /.exec(option.label)?.[1];
  const count =
    typeof payload.count === "number" && payload.count > 0 ? payload.count : fromLabel ? Number(fromLabel) : 1;
  return { from, to, unit, count };
}

const unitsOn = (board: BoardView | null | undefined, planet: string, owner: string, unit?: string): number => {
  let n = 0;
  for (const system of Object.values(board?.systems ?? {})) {
    for (const placed of system.units) {
      if (placed.planet === planet && placed.owner === owner && (unit === undefined || placed.unit_type === unit)) n += 1;
    }
  }
  return n;
};

/** Ghost Squad's question as moves between the planets of the active system, with counts before and after. */
export function describeGroundMover(choice: Mover & Partial<Pick<PendingChoiceDto, "actor">>, board?: BoardView | null): GroundMoverView | null {
  if (!isGroundMover(choice)) return null;
  const actor = choice.actor ?? "";
  const moves: GroundMove[] = [];
  const seen = new Set<string>();
  for (const option of choice.options) {
    if (isDeclineOption(option)) continue;
    const parsed = parseMove(option);
    if (!parsed) return null;
    const fromBefore = board ? unitsOn(board, parsed.from, actor, parsed.unit) : parsed.count;
    const toBefore = board ? unitsOn(board, parsed.to, actor, parsed.unit) : 0;
    moves.push({
      optionId: option.id,
      from: parsed.from,
      fromName: getPlanetDetails(parsed.from, board).name,
      to: parsed.to,
      toName: getPlanetDetails(parsed.to, board).name,
      unit: parsed.unit,
      unitName: getUnitDisplayName(parsed.unit, parsed.count),
      count: parsed.count,
      fromBefore,
      fromAfter: Math.max(0, fromBefore - parsed.count),
      toBefore,
      toAfter: toBefore + parsed.count,
    });
    seen.add(parsed.from);
    seen.add(parsed.to);
  }
  if (moves.length === 0) return null;
  const planets = [...seen].map((id) => {
    const forces = new Map<string, number>();
    for (const system of Object.values(board?.systems ?? {})) {
      for (const placed of system.units) {
        if (placed.planet === id && placed.owner === actor) {
          forces.set(placed.unit_type, (forces.get(placed.unit_type) ?? 0) + 1);
        }
      }
    }
    return {
      id,
      name: getPlanetDetails(id, board).name,
      forces: forces.size
        ? [...forces].map(([unit, n]) => `${n} ${getUnitDisplayName(unit)}`).join(", ")
        : "no ground forces",
    };
  });
  return { moves, doneId: choice.options.find(isDeclineOption)?.id ?? null, planets };
}
