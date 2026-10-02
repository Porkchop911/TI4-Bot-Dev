import React, { useMemo, useState, useEffect, useRef } from "react";
import {
  PendingChoiceDto,
  PlayerView,
  BoardView,
  PlacedUnitView,
  CombatDieRoll,
} from "../protocol/types.ts";
import { getCombatPayload, ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { Dialog } from "../primitives/index.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { UnitIcon, getUnitBaseType, getUnitDisplayName } from "./UnitIcon.tsx";
import { BattleOddsResponse } from "../protocol/advisorTypes.ts";
import { fetchBattleOdds, buildBattleRequest } from "../services/advisorService.ts";
import { getActionCardMeta } from "../protocol/contentCatalog.ts";
import {
  getExpectedSpaceHits,
  hasSustainDamage,
  canParticipateInSpaceCombat,
} from "../presentation/mapOverlays.ts";

export interface SpaceCombatOverlayProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  recentDiceRolls?: CombatDieRoll[];
  board?: BoardView | null;
  players?: Record<string, PlayerView> | PlayerView[] | null;
  activeSystemId?: string | null;
  isMinimized?: boolean;
  onMinimize?: (minimized: boolean) => void;
  advisorUrl?: string;
}

/** Standard base transport capacity per ship type in TI4 */
export function getBaseCapacity(unitType: string): number {
  const base = getUnitBaseType(unitType);
  switch (base) {
    case "carrier":
      return 4;
    case "warsun":
      return 6;
    case "dreadnought":
      return 1;
    case "flagship":
      return 3;
    default:
      return 0;
  }
}

/** Returns true if unit is a non-fighter ship that consumes fleet supply */
export function isNonFighterShip(unitType: string): boolean {
  const base = getUnitBaseType(unitType);
  return (
    base === "warsun" ||
    base === "flagship" ||
    base === "dreadnought" ||
    base === "carrier" ||
    base === "cruiser" ||
    base === "destroyer"
  );
}

/** Returns true if unit is transported / requires capacity in space */
export function requiresCapacity(unitType: string): boolean {
  const base = getUnitBaseType(unitType);
  return base === "fighter" || base === "infantry" || base === "mech";
}

// Fleet estimate used while the advisor is pending or unavailable.
// Uses canonical combat hit probabilities and public player context (factions, tech upgrades).
function expectedHits(unitType: string, player?: PlayerView | null): number {
  return getExpectedSpaceHits(unitType, player);
}

export interface FleetStats {
  nonFighterCount: number;
  fleetSupply: number;
  isOverFleetSupply: boolean;
  totalCapacity: number;
  usedCapacity: number;
  isOverCapacity: boolean;
  units: PlacedUnitView[];
  groupedUnits: {
    unitType: string;
    count: number;
    damagedCount: number;
    undamagedCount: number;
  }[];
}

export function computeFleetStats(units: PlacedUnitView[], fleetSupply = 3): FleetStats {
  const spaceUnits = units.filter((u) => !u.planet);
  let nonFighterCount = 0;
  let totalCapacity = 0;
  let usedCapacity = 0;

  const groupMap = new Map<
    string,
    { unitType: string; count: number; damagedCount: number; undamagedCount: number }
  >();

  for (const u of spaceUnits) {
    const isShip = isNonFighterShip(u.unit_type);
    if (isShip) {
      nonFighterCount += 1;
    }
    totalCapacity += getBaseCapacity(u.unit_type);
    if (requiresCapacity(u.unit_type)) {
      usedCapacity += 1;
    }

    const normType = u.unit_type.toLowerCase();
    const existing = groupMap.get(normType);
    if (existing) {
      existing.count += 1;
      if (u.damaged) {
        existing.damagedCount += 1;
      } else {
        existing.undamagedCount += 1;
      }
    } else {
      groupMap.set(normType, {
        unitType: u.unit_type,
        count: 1,
        damagedCount: u.damaged ? 1 : 0,
        undamagedCount: u.damaged ? 0 : 1,
      });
    }
  }

  return {
    nonFighterCount,
    fleetSupply,
    isOverFleetSupply: nonFighterCount > fleetSupply,
    totalCapacity,
    usedCapacity,
    isOverCapacity: usedCapacity > totalCapacity,
    units: spaceUnits,
    groupedUnits: Array.from(groupMap.values()).sort(
      (a, b) =>
        getUnitBaseType(a.unitType).localeCompare(getUnitBaseType(b.unitType)) ||
        a.unitType.localeCompare(b.unitType),
    ),
  };
}

export const SpaceCombatOverlay: React.FC<SpaceCombatOverlayProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isOpen,
  onClose,
  lastError,
  recentDiceRolls,
  board,
  players,
  activeSystemId,
  isMinimized = false,
  onMinimize,
  advisorUrl,
}) => {
  const display = usePlayerIdentity();

  // Normalize players map
  const playersMap = useMemo<Record<string, PlayerView>>(() => {
    if (!players) return {};
    if (Array.isArray(players)) {
      return Object.fromEntries(players.map((p) => [p.id, p]));
    }
    return players;
  }, [players]);

  const subtype = choice?.context?.subtype ?? "";
  const constraints = model?.outstanding?.[0] ?? choice?.context?.outstanding?.[0];
  const hitsOwed =
    model?.selectionMode.mode === "casualty"
      ? model.selectionMode.hitsToAssign
      : model?.selectionMode.mode === "sustain"
        ? model.selectionMode.hitsRemaining
        : (constraints?.amount ?? board?.combat?.hits_to_assign);

  const isSustainStage = subtype === "sustain_damage" || model?.workflow === "combat_sustain";
  const isCasualtyStage =
    subtype === "assign_casualty" ||
    model?.workflow === "combat_casualty" ||
    choice?.options.some((o) => o.kind === "casualty");
  const isRetreatStage =
    subtype === "announce_retreat" ||
    subtype === "retreat_to" ||
    model?.workflow === "combat_retreat";
  const isReactionStage = Boolean(
    choice?.context?.subtype.startsWith("reaction_") ||
    choice?.context?.subtype.startsWith("play_reaction_"),
  );
  const phase =
    board?.combat?.phase ??
    (isRetreatStage && subtype === "retreat_to"
      ? "retreating"
      : isSustainStage || isCasualtyStage
        ? "resolving_hits"
        : "pre_roll");
  const isPreRoll = phase === "pre_roll";
  const isResolving = phase === "resolving_hits" || phase === "barrage";
  const showBarrageRecap =
    isPreRoll &&
    board?.combat?.round === 1 &&
    Object.keys(board.combat.barrage_hits ?? {}).length > 0;
  const showAssignment = isResolving || showBarrageRecap;
  const reactionTiming = subtype.includes("SUSTAIN_DAMAGE_USED")
    ? "Your opponent sustained damage to cancel your hit."
    : subtype.includes("HITS_TO_ASSIGN")
      ? "Before assigning incoming hits"
      : subtype.includes("ACTION_CARD_PLAYED")
        ? "In response to an action card"
        : subtype.includes("SPACE_COMBAT_WON")
          ? "After winning space combat"
          : subtype.includes("SPACE_COMBAT_STARTED")
            ? "At the start of space combat"
            : subtype.includes("COMBAT_ROUND_STARTED")
              ? "At the start of this combat round"
              : (choice?.prompt.replaceAll("_", " ") ?? "Choose a reaction");

  // Identify system where combat is taking place
  const combatSystemId =
    board?.combat?.system_id ??
    (choice?.context?.target && "System" in choice.context.target
      ? choice.context.target.System
      : null) ??
    activeSystemId ??
    board?.active_system ??
    "Combat";

  // Identify units in combat system
  const systemUnits = useMemo(() => {
    if (!board?.systems || !combatSystemId) return [];
    return board.systems[combatSystemId]?.units ?? [];
  }, [board, combatSystemId]);

  // Discover sides
  const { attackerSeat, defenderSeat } = useMemo(() => {
    if (board?.combat) {
      return {
        attackerSeat: board.combat.attacker,
        defenderSeat: board.combat.defender,
      };
    }
    const spaceUnits = systemUnits.filter((u) => !u.planet);
    const owners = Array.from(new Set(spaceUnits.map((u) => u.owner)));
    const active = choice?.actor ?? owners[0] ?? "Attacker";
    const opponent = owners.find((o) => o !== active) ?? owners[1] ?? "Defender";
    return { attackerSeat: active, defenderSeat: opponent };
  }, [board?.combat, systemUnits, choice]);

  // Compute fleet stats for both sides
  const attackerStats = useMemo(() => {
    const units = systemUnits.filter((u) => u.owner === attackerSeat);
    const supply = playersMap[attackerSeat]?.fleet_tokens ?? 3;
    return computeFleetStats(units, supply);
  }, [systemUnits, attackerSeat, playersMap]);

  const defenderStats = useMemo(() => {
    const units = systemUnits.filter((u) => u.owner === defenderSeat);
    const supply = playersMap[defenderSeat]?.fleet_tokens ?? 3;
    return computeFleetStats(units, supply);
  }, [systemUnits, defenderSeat, playersMap]);

  // Remember rows within a battle so losing the last ship of a type cannot move
  // the next casualty target under a player's pointer (even if that ship never rolled).
  const fleetRows = useRef<{ battle: string; types: Map<string, Set<string>> }>({
    battle: "",
    types: new Map(),
  });
  const battleId = `${combatSystemId}:${attackerSeat}:${defenderSeat}:${board?.combat?.battle_seq ?? 0}`;
  if (fleetRows.current.battle !== battleId) {
    fleetRows.current = { battle: battleId, types: new Map() };
  }

  const [advisorResult, setAdvisorResult] = useState<{
    key: string;
    odds?: BattleOddsResponse;
    failed?: boolean;
  } | null>(null);

  const battleParams = useMemo(() => {
    const attackerUnits: Record<string, number> = {};
    const attackerDamaged: Record<string, number> = {};
    for (const g of attackerStats.groupedUnits) {
      const base = getUnitBaseType(g.unitType);
      if (base === "spacedock" || base === "pds") continue;
      attackerUnits[base] = (attackerUnits[base] ?? 0) + g.count;
      if (g.damagedCount > 0) {
        attackerDamaged[base] = (attackerDamaged[base] ?? 0) + g.damagedCount;
      }
    }

    const defenderUnits: Record<string, number> = {};
    const defenderDamaged: Record<string, number> = {};
    for (const g of defenderStats.groupedUnits) {
      const base = getUnitBaseType(g.unitType);
      if (base === "spacedock" || base === "pds") continue;
      defenderUnits[base] = (defenderUnits[base] ?? 0) + g.count;
      if (g.damagedCount > 0) {
        defenderDamaged[base] = (defenderDamaged[base] ?? 0) + g.damagedCount;
      }
    }

    return {
      attackerFaction: playersMap[attackerSeat]?.faction,
      attackerUnits,
      attackerDamaged,
      defenderFaction: playersMap[defenderSeat]?.faction,
      defenderUnits,
      defenderDamaged,
    };
  }, [attackerStats, defenderStats, playersMap, attackerSeat, defenderSeat]);

  const battleKey =
    isPreRoll && board?.combat
      ? JSON.stringify([battleId, board.combat.round, phase, battleParams, advisorUrl])
      : null;

  const fallbackOdds = useMemo(() => {
    const power = (stats: FleetStats, player?: PlayerView | null) => {
      const combatUnits = stats.units.filter((unit) =>
        canParticipateInSpaceCombat(unit.unit_type, player),
      );
      const hits = combatUnits.reduce((sum, unit) => sum + expectedHits(unit.unit_type, player), 0);
      const health = combatUnits.reduce(
        (sum, unit) => sum + (unit.damaged ? 1 : hasSustainDamage(unit.unit_type, player) ? 2 : 1),
        0,
      );
      return (hits + 0.1) * (health + 0.1);
    };
    const attacker = power(attackerStats, attackerSeat ? playersMap[attackerSeat] : null);
    const defender = power(defenderStats, defenderSeat ? playersMap[defenderSeat] : null);
    const attWinPct = Math.round((attacker / (attacker + defender)) * 100);
    return {
      attWinPct,
      defWinPct: 100 - attWinPct,
      mutWinPct: 0,
      attSub: "",
      defSub: "",
      avgRounds: "",
      tag: "Rough fleet estimate · simulation pending",
      tagClass: "combat-odds-card__tag--offline",
    };
  }, [attackerStats, defenderStats]);

  useEffect(() => {
    if (!battleKey || (!isOpen && !choice)) return;

    const hasAttackerUnits = Object.keys(battleParams.attackerUnits).length > 0;
    const hasDefenderUnits = Object.keys(battleParams.defenderUnits).length > 0;

    if (!hasAttackerUnits || !hasDefenderUnits) {
      return;
    }

    let active = true;
    const controller = new AbortController();

    const request = buildBattleRequest({
      ...battleParams,
      simulations: 2000,
    });

    fetchBattleOdds(request, { signal: controller.signal, baseUrl: advisorUrl })
      .then((res) => {
        if (!active) return;
        setAdvisorResult({ key: battleKey, odds: res });
      })
      .catch((_err: unknown) => {
        if (!active || controller.signal.aborted) return;
        setAdvisorResult({ key: battleKey, failed: true });
      });

    return () => {
      active = false;
      controller.abort();
    };
  }, [isOpen, Boolean(choice), battleKey, advisorUrl]);

  // Merged odds for rendering
  const displayedOdds = useMemo(() => {
    const advisorOdds = advisorResult?.key === battleKey ? advisorResult.odds : undefined;
    if (advisorOdds) {
      const attWinPct = Math.round(advisorOdds.attacker_win_rate * 100);
      const defWinPct = Math.round(advisorOdds.defender_win_rate * 100);
      const mutWinPct = Math.round(advisorOdds.mutual_destruction_rate * 100);
      const attExpSurvivors = Object.values(advisorOdds.attacker_expected_survivors)
        .reduce((a, b) => a + b, 0)
        .toFixed(1);
      const defExpSurvivors = Object.values(advisorOdds.defender_expected_survivors)
        .reduce((a, b) => a + b, 0)
        .toFixed(1);

      return {
        attWinPct,
        defWinPct,
        mutWinPct,
        attSub: `~${attExpSurvivors} survivors`,
        defSub: `~${defExpSurvivors} survivors`,
        avgRounds: advisorOdds.average_rounds.toFixed(1),
        tag: `Simulated (${advisorOdds.simulations.toLocaleString()} rollouts)`,
        tagClass: "combat-odds-card__tag--simulated",
      };
    }

    if (!battleKey) return null;
    return advisorResult?.key === battleKey && advisorResult.failed
      ? { ...fallbackOdds, tag: "Rough fleet estimate · advisor unavailable" }
      : fallbackOdds;
  }, [advisorResult, battleKey, fallbackOdds]);

  // Gather dice feed
  const activeDiceFeed: CombatDieRoll[] = useMemo(() => {
    if (!showAssignment) return [];
    if (phase === "barrage" || showBarrageRecap) return board?.combat?.barrage_dice ?? [];
    if (board?.combat?.dice_rolls && board.combat.dice_rolls.length > 0) {
      return board.combat.dice_rolls;
    }
    if (recentDiceRolls && recentDiceRolls.length > 0 && !board?.combat) return recentDiceRolls;
    return [];
  }, [recentDiceRolls, board?.combat, phase, showAssignment, showBarrageRecap]);
  const hasRollResults =
    showAssignment &&
    (activeDiceFeed.length > 0 ||
      showBarrageRecap ||
      (phase === "resolving_hits" &&
        board?.combat?.attacker_hits != null &&
        board?.combat?.defender_hits != null));

  const rollsBySide = useMemo(() => {
    const sides = new Map<string, Map<string, CombatDieRoll[]>>();
    for (const die of activeDiceFeed) {
      // Legacy rolls without a player belong to the attacker.
      const seat = die.player ?? attackerSeat;
      const byType = sides.get(seat) ?? new Map<string, CombatDieRoll[]>();
      const type = getUnitBaseType(die.unit);
      byType.set(type, [...(byType.get(type) ?? []), die]);
      sides.set(seat, byType);
    }
    return sides;
  }, [activeDiceFeed, attackerSeat]);

  const renderRollBadge = (seat: string, unitType: string) => {
    const type = getUnitBaseType(unitType);
    const dice = rollsBySide.get(seat)?.get(type) ?? [];
    const hits = dice.filter((die) => die.hit).length;
    const name = getUnitDisplayName(type);
    const results = dice.map((die) => `${die.roll} (${die.target}+) ${die.hit ? "hit" : "miss"}`);
    const rollSummary = `${name}: ${hits} hit${hits === 1 ? "" : "s"} from ${dice.length} roll${dice.length === 1 ? "" : "s"}.`;
    return (
      <span
        className="combat-unit-row__rolls"
        data-testid={`combat-roll-group-${seat}-${type}`}
        tabIndex={0}
        aria-label={`${rollSummary}${results.length ? ` ${results.join(", ")}` : ""}`}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        {hits} hit{hits === 1 ? "" : "s"}
        <span className="combat-unit-row__roll-tooltip" aria-hidden="true">
          <strong>
            {name} · {dice.length} roll{dice.length === 1 ? "" : "s"}
          </strong>
          {dice.map((die, index) => (
            <span key={index} className="combat-unit-row__roll-result" data-hit={die.hit}>
              {die.roll} vs {die.target}+ — {die.hit ? "HIT" : "MISS"}
            </span>
          ))}
        </span>
      </span>
    );
  };

  const casualtyOptions =
    choice?.options.filter((o) => o.id !== "decline" && o.kind !== "decline") ?? [];

  const activeCasualtyStats = choice?.actor === attackerSeat ? attackerStats : defenderStats;
  const unmatchedCasualties = isCasualtyStage
    ? casualtyOptions.filter(
        (opt) =>
          !activeCasualtyStats.groupedUnits.some((group) =>
            matchesUnitType(group.unitType, getCombatPayload(opt).unit, opt.label),
          ),
      )
    : [];

  const attackerHitsDealt = useMemo(() => {
    if (phase === "barrage" || showBarrageRecap)
      return board?.combat?.barrage_hits?.[attackerSeat] ?? 0;
    if (!isResolving) return 0;
    if (typeof board?.combat?.attacker_hits === "number") {
      return board.combat.attacker_hits;
    }
    return activeDiceFeed.filter((d) => d.hit && (d.player ? d.player === attackerSeat : true))
      .length;
  }, [board?.combat, activeDiceFeed, attackerSeat, phase, isResolving, showBarrageRecap]);

  const defenderHitsDealt = useMemo(() => {
    if (phase === "barrage" || showBarrageRecap)
      return board?.combat?.barrage_hits?.[defenderSeat] ?? 0;
    if (!isResolving) return 0;
    if (typeof board?.combat?.defender_hits === "number") {
      return board.combat.defender_hits;
    }
    return activeDiceFeed.filter((d) => d.hit && (d.player ? d.player === defenderSeat : false))
      .length;
  }, [board?.combat, activeDiceFeed, defenderSeat, phase, isResolving, showBarrageRecap]);

  const attackerPlayer = playersMap[attackerSeat];
  const defenderPlayer = playersMap[defenderSeat];
  const hasDirectHit = (player?: PlayerView) =>
    Boolean(
      player?.held_action_cards?.some((card) => card === "direct_hit" || /^dh[1-4]$/.test(card)),
    );
  const attackerHasDirectHit = hasDirectHit(attackerPlayer);
  const defenderHasDirectHit = hasDirectHit(defenderPlayer);
  const isAttackerDeciding = choice?.actor === attackerSeat;
  const opponentHasDirectHit = isAttackerDeciding ? defenderHasDirectHit : attackerHasDirectHit;

  const renderActionCards = (seat: string) => {
    const player = playersMap[seat];
    if (!player) return null;
    const isViewer = seat === viewerSeat;
    return (
      <div className="combat-fleet-card__cards" data-testid={`combat-cards-${seat}`}>
        <span>Action cards: {player.action_cards_count}</span>
        {isViewer && player.held_action_cards && player.held_action_cards.length > 0 && (
          <ul className="combat-fleet-card__card-list">
            {player.held_action_cards.map((id, index) => {
              const card = getActionCardMeta(id);
              return (
                <li key={`${id}-${index}`} title={`${card.name} — ${card.description}`}>
                  {card.name}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    );
  };

  function matchesUnitType(
    groupType: string,
    optUnit: string | undefined,
    optLabel: string,
  ): boolean {
    const target = (optUnit ?? optLabel).toLowerCase();
    const groupNorm = groupType.toLowerCase();
    const baseGroup = getUnitBaseType(groupType).toLowerCase();
    const baseTarget = getUnitBaseType(target).toLowerCase();
    return (
      baseGroup === baseTarget ||
      groupNorm.includes(baseTarget) ||
      target.includes(baseGroup) ||
      groupNorm === target
    );
  }

  const renderFleetUnits = (
    stats: FleetStats,
    seat: string,
    isActor: boolean,
    isDirectSubmitting: boolean,
    submitDirect: (optionId: string) => Promise<void>,
  ) => {
    const rolledTypes = rollsBySide.get(seat);
    const knownTypes = fleetRows.current.types.get(seat) ?? new Set<string>();
    const start =
      phase === "barrage" || showBarrageRecap
        ? board?.combat?.barrage_start
        : board?.combat?.round_start;
    if (showAssignment)
      for (const unit of start ?? [])
        if (unit.owner === seat) knownTypes.add(getUnitBaseType(unit.unit_type));
    for (const group of stats.groupedUnits) knownTypes.add(getUnitBaseType(group.unitType));
    for (const type of rolledTypes?.keys() ?? []) knownTypes.add(type);
    fleetRows.current.types.set(seat, knownTypes);
    if (knownTypes.size === 0) {
      return <div className="combat-empty-fleet">No ships remaining</div>;
    }

    const isCurrentDecider = isActor && choice?.actor === seat;

    const matchedSustainIds = new Set<string>();

    const rows = stats.groupedUnits.map((group) => {
      const started = (start ?? []).filter(
        (unit) =>
          unit.owner === seat && unit.unit_type.toLowerCase() === group.unitType.toLowerCase(),
      );
      const destroyed = Math.max(0, started.length - group.count);
      const newlyDamaged = !start
        ? 0
        : Math.max(0, group.damagedCount - started.filter((unit) => unit.damaged).length);
      // The pill compares damage carried into this round with damage sustained
      // during it. The total ship count (including undamaged ships) is shown by ×.
      const previouslyDamaged = showAssignment && start ? group.damagedCount - newlyDamaged : 0;
      const damageInPill = showAssignment && start ? newlyDamaged : group.damagedCount;
      // Casualty stage options for this group
      const matchingCasualties =
        isCurrentDecider && isCasualtyStage
          ? casualtyOptions.filter((opt) => {
              const payload = getCombatPayload(opt);
              return matchesUnitType(group.unitType, payload.unit, opt.label);
            })
          : [];

      // Sustain stage options for this group
      const matchingSustains =
        isCurrentDecider && isSustainStage
          ? (choice?.options ?? [])
              .filter((o) => o.id !== "decline" && o.kind !== "decline")
              .filter((opt) => {
                const payload = getCombatPayload(opt);
                return matchesUnitType(group.unitType, payload.unit, opt.label);
              })
          : [];

      for (const opt of matchingSustains) {
        matchedSustainIds.add(opt.id);
      }

      const isCasualtyInteractive = matchingCasualties.length > 0;
      const isSustainInteractive = matchingSustains.length > 0;

      const handleRowClick = () => {
        if (isDirectSubmitting) return;
        if (matchingCasualties.length === 1) {
          void submitDirect(matchingCasualties[0].id);
        } else if (matchingCasualties.length > 1) {
          const damagedOpt = matchingCasualties.find(
            (o) => getCombatPayload(o).damaged || o.label.toLowerCase().includes("damaged"),
          );
          void submitDirect((damagedOpt ?? matchingCasualties[0]).id);
        }
      };

      return (
        <div
          key={group.unitType}
          className={`combat-unit-row ${isCasualtyInteractive ? "combat-unit-row--interactive combat-unit-row--casualty" : ""}`}
          data-testid={`unit-row-${group.unitType}`}
          role={isCasualtyInteractive ? "button" : undefined}
          tabIndex={isCasualtyInteractive ? 0 : undefined}
          onClick={isCasualtyInteractive ? handleRowClick : undefined}
          onKeyDown={
            isCasualtyInteractive
              ? (e) => {
                  if (e.key === "Enter" || e.key === " ") handleRowClick();
                }
              : undefined
          }
        >
          <UnitIcon type={group.unitType} size={20} />
          <span className="combat-unit-row__name">
            {getUnitDisplayName(group.unitType, group.count)}
          </span>
          <span className="combat-unit-row__count">×{group.count}</span>
          {showAssignment && destroyed > 0 && (
            <span
              className="combat-unit-row__loss"
              data-testid={`combat-destroyed-${seat}-${group.unitType}`}
              title={`${destroyed} destroyed this step`}
            >
              −{destroyed}
            </span>
          )}
          {(group.damagedCount > 0 || newlyDamaged > 0) && (
            <span
              className="combat-unit-row__damaged-badge"
              data-testid={
                showAssignment && newlyDamaged > 0
                  ? `combat-new-damage-${seat}-${group.unitType}`
                  : undefined
              }
              title={
                showAssignment && start
                  ? `${previouslyDamaged} previously damaged, ${newlyDamaged} newly damaged this step`
                  : "Surviving ships damaged"
              }
            >
              {previouslyDamaged} + {damageInPill} damaged
            </span>
          )}
          {hasRollResults && renderRollBadge(seat, group.unitType)}

          {/* Integrated Casualty Actions */}
          {isCasualtyInteractive && (
            <div className="combat-unit-row__actions">
              {matchingCasualties.length === 1 ? (
                <span
                  className="combat-unit-row__click-hint"
                  data-testid={`casualty-opt-${matchingCasualties[0].id}`}
                >
                  💥 Click to assign
                </span>
              ) : (
                matchingCasualties.map((opt) => {
                  const payload = getCombatPayload(opt);
                  const isDamaged = payload.damaged || opt.label.toLowerCase().includes("damaged");
                  return (
                    <button
                      key={opt.id}
                      type="button"
                      data-testid={`casualty-opt-${opt.id}`}
                      disabled={isDirectSubmitting}
                      className={`combat-unit-row__action-btn ${isDamaged ? "combat-unit-row__action-btn--damaged" : ""}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        void submitDirect(opt.id);
                      }}
                    >
                      💥 {isDamaged ? "Destroy Damaged" : "Destroy Fresh"}
                    </button>
                  );
                })
              )}
            </div>
          )}

          {/* Integrated Sustain Actions */}
          {isSustainInteractive && (
            <div className="combat-unit-row__actions">
              {matchingSustains.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  data-testid={`sustain-opt-${opt.id}`}
                  disabled={isDirectSubmitting}
                  className="combat-unit-row__sustain-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    void submitDirect(opt.id);
                  }}
                >
                  🛡️ Sustain
                </button>
              ))}
            </div>
          )}
        </div>
      );
    });
    const presentTypes = new Set(
      stats.groupedUnits.map((group) => getUnitBaseType(group.unitType)),
    );
    for (const type of knownTypes) {
      if (presentTypes.has(getUnitBaseType(type))) continue;
      rows.push(
        <div
          key={`lost-${type}`}
          className="combat-unit-row combat-unit-row--lost"
          data-testid={`unit-row-lost-${type}`}
        >
          <UnitIcon type={type} size={20} />
          <span className="combat-unit-row__name">{getUnitDisplayName(type)}</span>
          <span className="combat-unit-row__count" title="No ships remaining">
            ×0
          </span>
          {showAssignment &&
            (start ?? []).some(
              (unit) => unit.owner === seat && getUnitBaseType(unit.unit_type) === type,
            ) && (
              <span
                className="combat-unit-row__loss"
                data-testid={`combat-destroyed-${seat}-${type}`}
                title="Destroyed this step"
              >
                −
                {
                  (start ?? []).filter(
                    (unit) => unit.owner === seat && getUnitBaseType(unit.unit_type) === type,
                  ).length
                }
              </span>
            )}
          {hasRollResults && renderRollBadge(seat, type)}
        </div>,
      );
    }
    // Keep destroyed types in their original slots instead of moving surviving targets.
    return rows.sort((a, b) =>
      String(a.key)
        .replace(/^lost-/, "")
        .localeCompare(String(b.key).replace(/^lost-/, "")),
    );
  };

  // When docked / minimized, render a non-intrusive sticky pill
  if (isMinimized) {
    const isActor = choice && (!viewerSeat || choice.actor === viewerSeat);
    return (
      <div
        className="combat-arena-dock choice-banner choice-minimized-pill"
        data-testid="combat-docked-pill"
      >
        <div className="combat-arena-dock__info">
          <span className="combat-arena-dock__badge">⚔️ SPACE COMBAT</span>
          <span className="combat-arena-dock__system">
            System {combatSystemId} — {display(attackerSeat).label} vs {display(defenderSeat).label}
          </span>
          {hitsOwed != null && (
            <span className="combat-arena-dock__hits">({hitsOwed} hits to resolve)</span>
          )}
          {isActor && <span className="combat-arena-dock__alert-pill">Your Decision Required</span>}
        </div>
        <button
          type="button"
          data-testid="resume-combat-btn"
          onClick={() => onMinimize?.(false)}
          className="button button--primary button--sm"
        >
          {isActor ? "Resume Decision" : "View Combat"}
        </button>
      </div>
    );
  }

  if (!isOpen && !choice) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) {
          phase === "complete" ? onClose() : onMinimize ? onMinimize(true) : onClose();
        }
      }}
    >
      <Dialog.Content
        data-testid="combat-resolution-modal"
        className="combat-dialog combat-arena-dialog choice-workflow-dialog"
      >
        <div className="panel choice-workflow-modal combat-arena-panel">
          {/* Header */}
          <Dialog.Title as="h2" className="visually-hidden">
            Space Combat — System {combatSystemId}
          </Dialog.Title>
          <DecisionHeader
            title={`Space Combat · System ${combatSystemId} · Round ${board?.combat?.round ?? 1} · ${phase === "pre_roll" ? "Before rolls" : phase === "barrage" ? "Anti-fighter barrage" : phase === "resolving_hits" ? "Resolve hits" : phase === "retreating" ? "Retreat / next round" : "Combat complete"}`}
            onMinimize={() =>
              phase === "complete" ? onClose() : onMinimize ? onMinimize(true) : onClose()
            }
            minimizeLabel={phase === "complete" ? "Close combat" : "Minimize combat"}
            minimizeIcon={phase === "complete" ? "×" : "−"}
            titleTestId="combat-stage-title"
            minimizeTestId="close-combat-modal"
          />
          <div data-testid="combat-phase" data-phase={phase} className="combat-action-prompt">
            {phase === "pre_roll"
              ? "Before rolls"
              : phase === "barrage"
                ? "Anti-fighter barrage"
                : phase === "resolving_hits"
                  ? "Roll results · Resolve hits"
                  : phase === "retreating"
                    ? "Retreat / next round"
                    : "Combat complete"}
          </div>
          {(phase === "barrage" || showBarrageRecap) && (
            <div className="combat-barrage-note" data-testid="combat-barrage-results">
              <strong>Anti-fighter barrage</strong>
              <span>
                Hits automatically destroy opposing fighters. Waylay lets the defender assign hits
                to other ships instead.
              </span>
            </div>
          )}

          {choice ? (
            <WorkflowShell
              choice={choice}
              model={model}
              viewerSeat={viewerSeat}
              onSubmit={onSubmit}
              lastError={lastError}
              spectatorNotice={`Observing combat resolution in progress for ${display(choice.actor).label}...`}
              spectatorNoticeTestId="spectator-combat-notice"
              errorTestId="combat-error-banner"
            >
              {({ isActor, isDirectSubmitting, declineOption, submitDirect }) => (
                <>
                  {/* Both Sides Arena Grid */}
                  <div className="combat-arena-sides-grid" data-testid="combat-arena-sides">
                    {/* Attacker Side */}
                    <div
                      className={`combat-fleet-card ${choice?.actor === attackerSeat ? "combat-fleet-card--active" : ""}`}
                      data-testid="attacker-fleet-card"
                    >
                      <div className="combat-fleet-card__header">
                        <span className="combat-fleet-card__role">ATTACKER</span>
                        <span className="combat-fleet-card__name">
                          {display(attackerSeat).label}
                        </span>
                        {playersMap[attackerSeat] && (
                          <span className="combat-fleet-card__faction">
                            ({playersMap[attackerSeat].faction})
                          </span>
                        )}
                        {hasRollResults && (
                          <span
                            className="combat-fleet-card__hits"
                            data-testid="attacker-hits-dealt"
                          >
                            💥 {attackerHitsDealt} hit{attackerHitsDealt === 1 ? "" : "s"}
                          </span>
                        )}
                      </div>

                      {/* Gauges: Fleet Supply and Capacity */}
                      <div className="combat-fleet-card__gauges">
                        <div
                          className="combat-gauge-stat"
                          data-alert={attackerStats.isOverFleetSupply}
                          title="Non-fighter ships vs Fleet Supply pool"
                          data-testid="attacker-fleet-supply-gauge"
                        >
                          <span className="combat-gauge-stat__label">Fleet Supply:</span>
                          <span className="combat-gauge-stat__value">
                            {attackerStats.nonFighterCount} / {attackerStats.fleetSupply}
                          </span>
                        </div>
                        <div
                          className="combat-gauge-stat"
                          data-alert={attackerStats.isOverCapacity}
                          title="Capacity consumed by fighters/infantry vs total transport capacity"
                          data-testid="attacker-capacity-gauge"
                        >
                          <span className="combat-gauge-stat__label">Capacity:</span>
                          <span className="combat-gauge-stat__value">
                            {attackerStats.usedCapacity} / {attackerStats.totalCapacity}
                          </span>
                        </div>
                      </div>

                      {/* Units in fight */}
                      <div className="combat-fleet-card__units" data-testid="attacker-units-list">
                        {renderFleetUnits(
                          attackerStats,
                          attackerSeat,
                          isActor,
                          isDirectSubmitting,
                          submitDirect,
                        )}
                      </div>
                      {renderActionCards(attackerSeat)}
                    </div>

                    {/* Center Stage: Scorecard, Odds & Hits */}
                    <div className="combat-arena-center">
                      {/* Round Hits Scorecard */}
                      {hasRollResults && (
                        <div className="combat-round-hits-card" data-testid="combat-round-hits">
                          <div className="combat-round-hits__header">
                            {phase === "barrage" || showBarrageRecap
                              ? "Anti-fighter barrage"
                              : `Round ${board?.combat?.round ?? 1}`}{" "}
                            Hits Produced
                          </div>
                          <div className="combat-round-hits__grid">
                            <div
                              className="combat-round-hits__player"
                              data-testid="attacker-round-hits"
                            >
                              <span className="combat-round-hits__count combat-round-hits__count--attacker">
                                {attackerHitsDealt}
                              </span>
                              <span className="combat-round-hits__label">
                                {display(attackerSeat).label}
                              </span>
                            </div>
                            <div className="combat-round-hits__divider">vs</div>
                            <div
                              className="combat-round-hits__player"
                              data-testid="defender-round-hits"
                            >
                              <span className="combat-round-hits__count combat-round-hits__count--defender">
                                {defenderHitsDealt}
                              </span>
                              <span className="combat-round-hits__label">
                                {display(defenderSeat).label}
                              </span>
                            </div>
                          </div>
                        </div>
                      )}

                      {/* Combat Odds Analysis */}
                      {isPreRoll && (
                        <div className="combat-odds-card" data-testid="combat-odds-card">
                          <div className="combat-odds-card__title">
                            <span>Combat Odds · Fleets only</span>
                            {displayedOdds && (
                              <span
                                className={`combat-odds-card__tag ${displayedOdds.tagClass}`}
                                data-testid="combat-odds-tag"
                              >
                                {displayedOdds.tag}
                              </span>
                            )}
                          </div>
                          {displayedOdds && (
                            <div className="combat-odds-card__bars">
                              <div className="combat-odds-col">
                                <span className="combat-odds-col__pct">
                                  {displayedOdds.attWinPct}%
                                </span>
                                {displayedOdds.attSub && (
                                  <span className="combat-odds-col__exp">
                                    {displayedOdds.attSub}
                                  </span>
                                )}
                              </div>
                              <div className="combat-odds-bar">
                                <div
                                  className="combat-odds-bar__att"
                                  style={{ width: `${displayedOdds.attWinPct}%` }}
                                  title={`Attacker Win: ${displayedOdds.attWinPct}%`}
                                />
                                {displayedOdds.mutWinPct > 0 && (
                                  <div
                                    className="combat-odds-bar__mutual"
                                    style={{ width: `${displayedOdds.mutWinPct}%` }}
                                    title={`Mutual Destruction: ${displayedOdds.mutWinPct}%`}
                                  />
                                )}
                                <div
                                  className="combat-odds-bar__def"
                                  style={{ width: `${displayedOdds.defWinPct}%` }}
                                  title={`Defender Win: ${displayedOdds.defWinPct}%`}
                                />
                              </div>
                              <div className="combat-odds-col combat-odds-col--right">
                                <span className="combat-odds-col__pct">
                                  {displayedOdds.defWinPct}%
                                </span>
                                {displayedOdds.defSub && (
                                  <span className="combat-odds-col__exp">
                                    {displayedOdds.defSub}
                                  </span>
                                )}
                              </div>
                            </div>
                          )}
                          {displayedOdds?.avgRounds && (
                            <div
                              className="combat-odds-card__sub-detail"
                              data-testid="combat-odds-sub-detail"
                            >
                              Avg {displayedOdds.avgRounds} rounds
                              {displayedOdds.mutWinPct > 0
                                ? ` • ${displayedOdds.mutWinPct}% mutual wipe`
                                : ""}
                            </div>
                          )}
                        </div>
                      )}

                      {/* Hits Remaining Banner */}
                      {isResolving && hitsOwed != null && hitsOwed > 0 && (
                        <div className="combat-hits-callout" data-testid="combat-hits-callout">
                          <span className="combat-hits-callout__count">{hitsOwed}</span>
                          <span className="combat-hits-callout__label">
                            Hit{hitsOwed > 1 ? "s" : ""} to Resolve
                          </span>
                        </div>
                      )}
                    </div>

                    {/* Defender Side */}
                    <div
                      className={`combat-fleet-card ${choice?.actor === defenderSeat ? "combat-fleet-card--active" : ""}`}
                      data-testid="defender-fleet-card"
                    >
                      <div className="combat-fleet-card__header">
                        <span className="combat-fleet-card__role">DEFENDER</span>
                        <span className="combat-fleet-card__name">
                          {display(defenderSeat).label}
                        </span>
                        {playersMap[defenderSeat] && (
                          <span className="combat-fleet-card__faction">
                            ({playersMap[defenderSeat].faction})
                          </span>
                        )}
                        {hasRollResults && (
                          <span
                            className="combat-fleet-card__hits"
                            data-testid="defender-hits-dealt"
                          >
                            💥 {defenderHitsDealt} hit{defenderHitsDealt === 1 ? "" : "s"}
                          </span>
                        )}
                      </div>

                      {/* Gauges: Fleet Supply and Capacity */}
                      <div className="combat-fleet-card__gauges">
                        <div
                          className="combat-gauge-stat"
                          data-alert={defenderStats.isOverFleetSupply}
                          title="Non-fighter ships vs Fleet Supply pool"
                          data-testid="defender-fleet-supply-gauge"
                        >
                          <span className="combat-gauge-stat__label">Fleet Supply:</span>
                          <span className="combat-gauge-stat__value">
                            {defenderStats.nonFighterCount} / {defenderStats.fleetSupply}
                          </span>
                        </div>
                        <div
                          className="combat-gauge-stat"
                          data-alert={defenderStats.isOverCapacity}
                          title="Capacity consumed by fighters/infantry vs total transport capacity"
                          data-testid="defender-capacity-gauge"
                        >
                          <span className="combat-gauge-stat__label">Capacity:</span>
                          <span className="combat-gauge-stat__value">
                            {defenderStats.usedCapacity} / {defenderStats.totalCapacity}
                          </span>
                        </div>
                      </div>

                      {/* Units in fight */}
                      <div className="combat-fleet-card__units" data-testid="defender-units-list">
                        {renderFleetUnits(
                          defenderStats,
                          defenderSeat,
                          isActor,
                          isDirectSubmitting,
                          submitDirect,
                        )}
                      </div>
                      {renderActionCards(defenderSeat)}
                    </div>
                  </div>

                  {/* Workflow Action Controls */}
                  <div className="combat-action-area">
                    {isActor && isReactionStage && (
                      <div className="workflow-inline">
                        <div className="combat-action-prompt">{reactionTiming}</div>
                        {choice.options
                          .filter((opt) => opt.kind !== "decline" && opt.id !== "decline")
                          .map((opt) => {
                            const alias =
                              typeof opt.payload?.card === "string" ? opt.payload.card : null;
                            const card = alias ? getActionCardMeta(alias) : null;
                            return (
                              <div key={opt.id} className="combat-card-offer">
                                <div className="combat-card-offer__body">
                                  <span className="combat-card-offer__eyebrow">
                                    {card ? "ACTION CARD" : "REACTION"}
                                  </span>
                                  <strong className="combat-card-offer__title">
                                    {card?.name ?? opt.label}
                                  </strong>
                                  {card && (
                                    <p className="combat-card-offer__description">
                                      {card.description}
                                    </p>
                                  )}
                                </div>
                                <button
                                  type="button"
                                  className="button button--primary"
                                  data-testid={
                                    card?.name === "Direct Hit"
                                      ? "play-direct-hit-btn"
                                      : `combat-reaction-${opt.id}`
                                  }
                                  disabled={isDirectSubmitting}
                                  onClick={() => void submitDirect(opt.id)}
                                >
                                  {card ? `Play ${card.name}` : opt.label}
                                </button>
                              </div>
                            );
                          })}
                        {declineOption && (
                          <button
                            type="button"
                            className="button button--secondary"
                            data-testid={
                              subtype.includes("SUSTAIN_DAMAGE_USED")
                                ? "pass-direct-hit-btn"
                                : "pass-combat-reaction-btn"
                            }
                            disabled={isDirectSubmitting}
                            onClick={() => void submitDirect(declineOption.id)}
                          >
                            Pass
                          </button>
                        )}
                      </div>
                    )}
                    {isActor &&
                      !isReactionStage &&
                      !isSustainStage &&
                      !isCasualtyStage &&
                      !isRetreatStage && (
                        <div className="workflow-inline" data-testid="combat-follow-up">
                          <div className="combat-action-prompt">{choice.prompt}</div>
                          {choice.options.map((opt) => (
                            <button
                              key={opt.id}
                              type="button"
                              className="button button--secondary"
                              data-testid={`combat-follow-up-${opt.id}`}
                              disabled={isDirectSubmitting}
                              onClick={() => void submitDirect(opt.id)}
                            >
                              {opt.label}
                            </button>
                          ))}
                        </div>
                      )}
                    {/* Stage 1: Sustain Damage */}
                    {isActor && isSustainStage && (
                      <div className="workflow-inline">
                        {opponentHasDirectHit && (
                          <div
                            className="combat-direct-hit-banner"
                            data-testid="direct-hit-threat-banner"
                          >
                            Opponent holds Direct Hit — sustained ships may be destroyed.
                          </div>
                        )}

                        <div className="combat-action-prompt">
                          Sustain on an eligible ship below, or take the hit.
                        </div>

                        {/* Fallback sustain buttons when no unit rows exist on board */}
                        {(() => {
                          const activeStats =
                            choice.actor === attackerSeat ? attackerStats : defenderStats;
                          const hasUnitRows = activeStats.groupedUnits.length > 0;
                          if (!hasUnitRows) {
                            return (
                              <div className="combat-action-buttons">
                                {choice.options
                                  .filter((o) => o.id !== "decline" && o.kind !== "decline")
                                  .map((opt) => (
                                    <button
                                      key={opt.id}
                                      type="button"
                                      data-testid={`sustain-opt-${opt.id}`}
                                      onClick={() => void submitDirect(opt.id)}
                                      disabled={isDirectSubmitting}
                                      className="button button--secondary combat-sustain-btn"
                                    >
                                      <span className="combat-btn-icon">🛡️</span>
                                      <span className="combat-btn-label">{opt.label}</span>
                                      <span className="combat-btn-sub">Sustain Hit</span>
                                    </button>
                                  ))}
                              </div>
                            );
                          }
                          return null;
                        })()}

                        {declineOption && (
                          <div className="combat-decline-row">
                            <button
                              type="button"
                              data-testid="decline-sustain-btn"
                              onClick={() => void submitDirect(declineOption.id)}
                              disabled={isDirectSubmitting}
                              className="button button--secondary"
                            >
                              {declineOption.label || "Do Not Sustain Damage"}
                            </button>
                          </div>
                        )}
                      </div>
                    )}

                    {/* Stage 2: Assign Casualty */}
                    {isActor && isCasualtyStage && unmatchedCasualties.length > 0 && (
                      <div className="workflow-inline">
                        <div className="combat-casualty-options-grid">
                          {unmatchedCasualties.map((opt) => (
                            <button
                              key={opt.id}
                              type="button"
                              data-testid={`casualty-opt-${opt.id}`}
                              className="button button--secondary combat-casualty-btn"
                              disabled={isDirectSubmitting}
                              onClick={() => void submitDirect(opt.id)}
                            >
                              {opt.label}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Stage 3: Retreat */}
                    {isActor && isRetreatStage && (
                      <div className="workflow-inline">
                        <div className="combat-action-prompt">
                          {subtype === "announce_retreat"
                            ? "Choose whether to announce a retreat before combat rounds commence:"
                            : "Select an adjacent system to retreat your surviving fleet to:"}
                        </div>

                        <div className="combat-action-buttons">
                          {choice.options.map((opt) => (
                            <button
                              key={opt.id}
                              type="button"
                              data-testid={`retreat-opt-${opt.id}`}
                              onClick={() => void submitDirect(opt.id)}
                              disabled={isDirectSubmitting}
                              className="button button--secondary"
                            >
                              {opt.label}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </>
              )}
            </WorkflowShell>
          ) : (
            <>
              {/* Both Sides Read-only Arena Grid */}
              <div className="combat-arena-sides-grid" data-testid="combat-arena-sides">
                <div className="combat-fleet-card" data-testid="attacker-fleet-card">
                  <div className="combat-fleet-card__header">
                    <span className="combat-fleet-card__role">ATTACKER</span>
                    <span className="combat-fleet-card__name">{display(attackerSeat).label}</span>
                    {hasRollResults && (
                      <span className="combat-fleet-card__hits" data-testid="attacker-hits-dealt">
                        💥 {attackerHitsDealt} hit{attackerHitsDealt === 1 ? "" : "s"}
                      </span>
                    )}
                  </div>
                  <div className="combat-fleet-card__units" data-testid="attacker-units-list">
                    {renderFleetUnits(attackerStats, attackerSeat, false, false, async () => {})}
                  </div>
                  {renderActionCards(attackerSeat)}
                </div>

                <div className="combat-arena-center">
                  {hasRollResults && (
                    <div className="combat-round-hits-card" data-testid="combat-round-hits">
                      <div className="combat-round-hits__header">
                        {phase === "barrage" || showBarrageRecap
                          ? "Anti-fighter barrage"
                          : `Round ${board?.combat?.round ?? 1}`}{" "}
                        Hits Produced
                      </div>
                      <div className="combat-round-hits__grid">
                        <div
                          className="combat-round-hits__player"
                          data-testid="attacker-round-hits"
                        >
                          <span className="combat-round-hits__count combat-round-hits__count--attacker">
                            {attackerHitsDealt}
                          </span>
                          <span className="combat-round-hits__label">
                            {display(attackerSeat).label}
                          </span>
                        </div>
                        <div className="combat-round-hits__divider">vs</div>
                        <div
                          className="combat-round-hits__player"
                          data-testid="defender-round-hits"
                        >
                          <span className="combat-round-hits__count combat-round-hits__count--defender">
                            {defenderHitsDealt}
                          </span>
                          <span className="combat-round-hits__label">
                            {display(defenderSeat).label}
                          </span>
                        </div>
                      </div>
                    </div>
                  )}
                  {isPreRoll && (
                    <div className="combat-odds-card" data-testid="combat-odds-card">
                      <div className="combat-odds-card__title">Combat Odds · Fleets only</div>
                      {displayedOdds && (
                        <>
                          <div
                            className={`combat-odds-card__tag ${displayedOdds.tagClass}`}
                            data-testid="combat-odds-tag"
                          >
                            {displayedOdds.tag}
                          </div>
                          <div className="combat-odds-card__bars">
                            <div className="combat-odds-col">
                              <span className="combat-odds-col__pct">
                                {displayedOdds.attWinPct}%
                              </span>
                            </div>
                            <div className="combat-odds-bar">
                              <div
                                className="combat-odds-bar__att"
                                style={{ width: `${displayedOdds.attWinPct}%` }}
                                title={`Attacker Win: ${displayedOdds.attWinPct}%`}
                              />
                              {displayedOdds.mutWinPct > 0 && (
                                <div
                                  className="combat-odds-bar__mutual"
                                  style={{ width: `${displayedOdds.mutWinPct}%` }}
                                  title={`Mutual Destruction: ${displayedOdds.mutWinPct}%`}
                                />
                              )}
                              <div
                                className="combat-odds-bar__def"
                                style={{ width: `${displayedOdds.defWinPct}%` }}
                                title={`Defender Win: ${displayedOdds.defWinPct}%`}
                              />
                            </div>
                            <div className="combat-odds-col combat-odds-col--right">
                              <span className="combat-odds-col__pct">
                                {displayedOdds.defWinPct}%
                              </span>
                            </div>
                          </div>
                        </>
                      )}
                    </div>
                  )}
                </div>

                <div className="combat-fleet-card" data-testid="defender-fleet-card">
                  <div className="combat-fleet-card__header">
                    <span className="combat-fleet-card__role">DEFENDER</span>
                    <span className="combat-fleet-card__name">{display(defenderSeat).label}</span>
                    {hasRollResults && (
                      <span className="combat-fleet-card__hits" data-testid="defender-hits-dealt">
                        💥 {defenderHitsDealt} hit{defenderHitsDealt === 1 ? "" : "s"}
                      </span>
                    )}
                  </div>
                  <div className="combat-fleet-card__units" data-testid="defender-units-list">
                    {renderFleetUnits(defenderStats, defenderSeat, false, false, async () => {})}
                  </div>
                  {renderActionCards(defenderSeat)}
                </div>
              </div>

              <div
                className="combat-spectator-waiting"
                data-testid={
                  phase === "complete" ? "combat-complete-notice" : "spectator-combat-notice"
                }
              >
                {phase === "complete"
                  ? "Combat complete"
                  : `Observing space combat in System ${combatSystemId}...`}
              </div>
            </>
          )}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
