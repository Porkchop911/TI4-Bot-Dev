import React from "react";
import { describeUnit, resolveUnit } from "../presentation/factionInfo.ts";
import { useSeatInfo } from "../presentation/SeatInfoContext.tsx";
import {
  describeUnitStats,
  formatDice,
  formatUnitCost,
  keywordChips,
  type DiceStat,
  type UnitStats,
} from "../presentation/unitStats.ts";
import { CostValue } from "./PlanetValueIcons.tsx";
import "./UnitBuildStats.css";

/** What the build option itself charges: the batch's cost and size as the engine offered it. */
export interface OptionPrice {
  /** Resources for one batch (after any discount). */
  cost?: number;
  /** The cost before a one-time discount. */
  printedCost?: number;
  /** Units per batch (2 for "2x fighter for 1"). */
  count?: number;
  free?: boolean;
}

/** The unit the seat would get for a build key: its own flagship, mech, variant or II card. */
export function useBuildUnit(unit: string, seat?: string | null) {
  const { faction, technologies } = useSeatInfo(seat);
  const resolved = resolveUnit(unit, faction, technologies);
  return resolved ? { resolved, name: describeUnit(resolved).name, stats: describeUnitStats(resolved) } : undefined;
}

export function costText(price: OptionPrice, stats: UnitStats | undefined): string | undefined {
  if (price.free) return "Free";
  const { cost, count } = price;
  if (cost !== undefined) {
    if (count !== undefined && count > 1) return `${count} for ${cost}`;
    const catalog = stats?.cost;
    // A fighter or infantry batch offered as a single unit still reads "2 for 1".
    if (catalog !== undefined && catalog > 0 && catalog < 1 && cost === 1) return formatUnitCost(catalog);
    return String(cost);
  }
  return stats?.cost === undefined ? undefined : formatUnitCost(stats.cost);
}

const diceLabel = (name: string, stat: DiceStat) =>
  `${name}: hits on ${stat.hitsOn} or higher, ${stat.chancePercent} percent per die, ${stat.dice} ${stat.dice === 1 ? "die" : "dice"}`;

/**
 * The numbers of one build option, visible without hovering: cost, combat with its hit chance,
 * movement and capacity, then ability chips. The info card keeps the full printed text.
 */
export const UnitBuildStats: React.FC<{
  unit: string;
  seat?: string | null;
  price?: OptionPrice;
  name: string;
}> = ({ unit, seat, price = {}, name }) => {
  const built = useBuildUnit(unit, seat);
  const stats = built?.stats;
  const cost = costText(price, stats);
  const chips = stats ? keywordChips(stats) : [];
  const discounted =
    price.printedCost !== undefined && price.cost !== undefined && price.printedCost > price.cost;
  return (
    <ul className="unit-stats" aria-label={`${name} stats`} data-testid="unit-stats">
      {cost !== undefined && (
        <li className="unit-stats__item unit-stats__item--cost" data-stat="cost">
          <span className="unit-stats__label">Cost</span> <strong>
            <CostValue text={cost} />
          </strong>
          {discounted && (
            <s className="unit-stats__was" title={`printed cost ${price.printedCost} resources`}>
              {price.printedCost}
            </s>
          )}
        </li>
      )}
      {stats?.combat && (
        <li
          className="unit-stats__item"
          data-stat="combat"
          aria-label={diceLabel("Combat", stats.combat)}
        >
          <span className="unit-stats__label" aria-hidden="true">Combat</span>{" "}
          <strong aria-hidden="true">{formatDice(stats.combat)}</strong>
        </li>
      )}
      {stats?.move !== undefined && (
        <li className="unit-stats__item" data-stat="move">
          <span className="unit-stats__label">Move</span> <strong>{stats.move}</strong>
        </li>
      )}
      {stats?.capacity !== undefined && (
        <li className="unit-stats__item" data-stat="capacity">
          <span className="unit-stats__label">Capacity</span> <strong>{stats.capacity}</strong>
        </li>
      )}
      {chips.map((chip) => (
        <li
          key={chip.text}
          className="unit-stats__chip"
          data-stat="keyword"
          title={chip.title}
        >
          {chip.text}
        </li>
      ))}
    </ul>
  );
};
