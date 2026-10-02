import React, { useMemo, useState, useEffect, useRef, useCallback } from "react";
import type { BoardView, PlayerView, PendingChoiceDto } from "../protocol/types.ts";
import type { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { Dialog } from "../primitives/index.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { computePlayerStats } from "../presentation/playerStats.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";
import {
  TECH_TRACKS,
  TECH_TIERS,
  UNIT_UPGRADE_TECH_IDS,
  hydrateTech,
  hasResearchedTech,
  isTechExhausted,
  techMatches,
  getFactionTechIds,
  getTechnologyTrack,
  isTechAllowedForFaction,
  getControlledSpecialtyPlanets,
  checkTechPrerequisites,
  type HydratedTech,
} from "../presentation/technologyData.ts";
import "./TechnologyModal.css";

export interface TechnologyModalProps {
  isOpen: boolean;
  onClose: () => void;
  players?: Record<string, PlayerView> | PlayerView[];
  board?: BoardView | null;
  viewerSeat?: string | null;
  choice?: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  onSubmit?: (optionId: string) => Promise<void>;
  lastError?: string | null;
  // Legacy / manual selection mode props:
  selectableTechIds?: readonly string[];
  selectedTechId?: string | null;
  onSelectTech?: (techId: string) => void;
  onConfirmResearch?: (techId: string) => void;
  isResearchMode?: boolean;
}

const TRACK_ACCENTS: Record<string, string> = {
  PROPULSION: "#38bdf8",
  BIOTIC: "#4ade80",
  CYBERNETIC: "#facc15",
  WARFARE: "#f87171",
};

export const TechnologyModal: React.FC<TechnologyModalProps> = ({
  isOpen,
  onClose,
  players,
  board,
  viewerSeat,
  choice,
  model,
  onSubmit,
  lastError,
  selectableTechIds,
  selectedTechId,
  onSelectTech,
  onConfirmResearch,
  isResearchMode = false,
}) => {
  const display = usePlayerIdentity();

  const playerList = useMemo<PlayerView[]>(() => {
    if (!players) return [];
    if (Array.isArray(players)) return players;
    return Object.values(players);
  }, [players]);

  const viewerPlayer = useMemo(() => {
    if (viewerSeat) {
      const found = playerList.find((p) => p.id === viewerSeat);
      if (found) return found;
    }
    if (choice?.actor) {
      const found = playerList.find((p) => p.id === choice.actor);
      if (found) return found;
    }
    return playerList[0] ?? null;
  }, [playerList, viewerSeat, choice?.actor]);

  const activeResearchMode =
    isResearchMode ||
    Boolean(
      choice &&
      (choice.context?.subtype === "research_technology" ||
        choice.options.some((o) => o.kind === "research")),
    );

  const contextRecord = choice?.context as Record<string, unknown> | undefined;
  const isPrimaryTechnology = Boolean(
    choice?.context?.subtype === "research_technology" &&
    (contextRecord?.strategy_card === "pok7technology" ||
      contextRecord?.strategy_card === "Technology" ||
      (choice.context.source &&
        typeof choice.context.source === "object" &&
        "StrategyCard" in choice.context.source &&
        (choice.context.source as { StrategyCard: { card: string; secondary: boolean } })
          .StrategyCard?.card === "Technology" &&
        !(choice.context.source as { StrategyCard: { card: string; secondary: boolean } })
          .StrategyCard?.secondary)),
  );

  const isSecondaryTechnology = Boolean(
    choice?.context?.subtype === "research_technology" &&
    choice.context.source &&
    typeof choice.context.source === "object" &&
    "StrategyCard" in choice.context.source &&
    (choice.context.source as { StrategyCard: { card: string; secondary: boolean } }).StrategyCard
      ?.card === "Technology" &&
    Boolean(
      (choice.context.source as { StrategyCard: { card: string; secondary: boolean } }).StrategyCard
        ?.secondary,
    ),
  );

  const maxSelectable = isPrimaryTechnology
    ? 2
    : model?.selectionMode.mode === "multi"
      ? model.selectionMode.max
      : 1;

  // Selected technologies state (supports up to 2 for primary research)
  const [selectedTechs, setSelectedTechs] = useState<string[]>(() => {
    if (selectedTechId) return [selectedTechId];
    return [];
  });

  useEffect(() => {
    if (selectedTechId && !selectedTechs.includes(selectedTechId)) {
      setSelectedTechs([selectedTechId]);
    }
  }, [selectedTechId]);

  // Tech Skips toggling state
  const [toggledPlanets, setToggledPlanets] = useState<Record<string, boolean>>({});

  const togglePlanetSkip = (planetId: string) => {
    setToggledPlanets((prev) => ({
      ...prev,
      [planetId]: !prev[planetId],
    }));
  };

  const controlledSpecialties = useMemo(() => {
    return getControlledSpecialtyPlanets(board, viewerPlayer?.id);
  }, [board, viewerPlayer?.id]);

  const toggledSkipCounts = useMemo(() => {
    const counts: Record<"PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE", number> = {
      PROPULSION: 0,
      BIOTIC: 0,
      CYBERNETIC: 0,
      WARFARE: 0,
    };
    for (const p of controlledSpecialties) {
      if (!p.isExhausted && toggledPlanets[p.planetId]) {
        counts[p.specialty] += 1;
      }
    }
    return counts;
  }, [controlledSpecialties, toggledPlanets]);

  const ownedTrackCounts = useMemo(() => {
    const counts: Record<"PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE", number> = {
      PROPULSION: 0,
      BIOTIC: 0,
      CYBERNETIC: 0,
      WARFARE: 0,
    };
    if (!viewerPlayer?.technologies) return counts;
    for (const track of TECH_TRACKS) {
      counts[track.id] = viewerPlayer.technologies.filter((techId) =>
        track.techIds.some((trackTechId) => techMatches(techId, trackTechId)),
      ).length;
    }
    return counts;
  }, [viewerPlayer?.technologies]);

  const legalOptionIds = useMemo(() => {
    if (choice?.options) {
      return choice.options.filter((o) => o.kind === "research").map((o) => o.id);
    }
    return selectableTechIds ?? null;
  }, [choice?.options, selectableTechIds]);

  const effectiveTrackCounts = useMemo(() => {
    const counts = { ...ownedTrackCounts };
    if (maxSelectable === 2 && selectedTechs.length >= 1) {
      const firstTrack = getTechnologyTrack(selectedTechs[0]);
      if (firstTrack) {
        counts[firstTrack] = (counts[firstTrack] || 0) + 1;
      }
    }
    return counts;
  }, [maxSelectable, ownedTrackCounts, selectedTechs]);

  const canResearchAlone = useCallback(
    (tech: HydratedTech): boolean => {
      if (!isTechAllowedForFaction(tech, viewerPlayer?.faction)) return false;
      if (viewerPlayer && hasResearchedTech(viewerPlayer, tech.id)) return false;
      if (legalOptionIds && !legalOptionIds.some((optId) => techMatches(optId, tech.id))) {
        return false;
      }
      return checkTechPrerequisites(
        tech,
        ownedTrackCounts,
        toggledSkipCounts,
        viewerPlayer?.faction,
      );
    },
    [legalOptionIds, ownedTrackCounts, toggledSkipCounts, viewerPlayer],
  );

  const isTechResearchable = (tech: HydratedTech): boolean => {
    if (!activeResearchMode) return true;
    if (viewerPlayer && hasResearchedTech(viewerPlayer, tech.id)) return false;
    if (!isTechAllowedForFaction(tech, viewerPlayer?.faction)) return false;

    // In 2-tech selection mode when a 1st tech is selected:
    if (maxSelectable === 2 && selectedTechs.length >= 1) {
      // The currently selected 1st tech was valid alone
      if (techMatches(tech.id, selectedTechs[0])) {
        return true;
      }
      // Candidate 2nd tech uses effectiveTrackCounts (includes +1 from 1st tech)
      return checkTechPrerequisites(
        tech,
        effectiveTrackCounts,
        toggledSkipCounts,
        viewerPlayer?.faction,
      );
    }

    return canResearchAlone(tech);
  };

  // Resources calculation
  const availableResources = useMemo(() => {
    if (!viewerPlayer) return { planet: 0, tradeGoods: 0, total: 0 };
    const stats = board ? computePlayerStats(viewerPlayer, board) : { remainingResources: 0 };
    const planet = stats.remainingResources;
    const tradeGoods = viewerPlayer.trade_goods ?? 0;
    return { planet, tradeGoods, total: planet + tradeGoods };
  }, [viewerPlayer, board]);

  // Cost calculation
  const researchCost = useMemo(() => {
    if (isPrimaryTechnology) {
      // 1st tech is Free, 2nd tech costs 6 resources
      return selectedTechs.length === 2 ? 6 : 0;
    }
    if (isSecondaryTechnology) {
      return 4;
    }
    if (selectedTechs.length > 0 && choice?.options) {
      const match = choice.options.find((o) => o.id === selectedTechs[0]);
      if (typeof match?.payload?.cost === "number") {
        return match.payload.cost;
      }
    }
    return 0;
  }, [isPrimaryTechnology, isSecondaryTechnology, selectedTechs, choice?.options]);

  const canConfirm = useMemo(() => {
    if (selectedTechs.length === 0) return false;
    if (isPrimaryTechnology && selectedTechs.length === 2 && availableResources.total < 6) {
      return false;
    }
    if (isSecondaryTechnology && availableResources.total < 4) {
      return false;
    }
    return true;
  }, [selectedTechs.length, isPrimaryTechnology, isSecondaryTechnology, availableResources.total]);

  // Two-step submission pipeline for Option B
  const pendingPipelineRef = useRef<{
    initialNonce?: string;
    nextTarget: string | "decline";
  } | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (!pendingPipelineRef.current) return;
    const { initialNonce, nextTarget } = pendingPipelineRef.current;
    if (
      choice &&
      choice.nonce !== initialNonce &&
      choice.context?.subtype === "research_technology"
    ) {
      pendingPipelineRef.current = null;
      if (onSubmit) {
        if (nextTarget === "decline") {
          const declineOpt = choice.options?.find(
            (o) => o.id === "decline" || o.kind === "decline",
          );
          if (declineOpt) {
            onSubmit(declineOpt.id).finally(() => setIsSubmitting(false));
          } else {
            setIsSubmitting(false);
          }
        } else {
          const targetOpt = choice.options?.find((o) => techMatches(o.id, nextTarget));
          if (targetOpt) {
            onSubmit(targetOpt.id).finally(() => setIsSubmitting(false));
          } else {
            setIsSubmitting(false);
          }
        }
      }
    }
  }, [choice, onSubmit]);

  const handleConfirm = async () => {
    if (!canConfirm || selectedTechs.length === 0) return;
    setIsSubmitting(true);

    if (isPrimaryTechnology) {
      pendingPipelineRef.current = {
        initialNonce: choice?.nonce,
        nextTarget: selectedTechs.length === 2 ? selectedTechs[1] : "decline",
      };
      if (onSubmit) {
        await onSubmit(selectedTechs[0]);
      } else if (onConfirmResearch) {
        onConfirmResearch(selectedTechs[0]);
        setIsSubmitting(false);
      }
    } else {
      if (onSubmit) {
        await onSubmit(selectedTechs[0]);
        setIsSubmitting(false);
      } else if (onConfirmResearch) {
        onConfirmResearch(selectedTechs[0]);
        setIsSubmitting(false);
      }
    }
  };

  const handleDecline = async () => {
    const declineOpt = choice?.options?.find((o) => o.id === "decline");
    if (declineOpt && onSubmit) {
      setIsSubmitting(true);
      await onSubmit(declineOpt.id);
      setIsSubmitting(false);
    }
  };

  const handleSelectCard = (techId: string) => {
    if (!activeResearchMode) return;
    if (onSelectTech) onSelectTech(techId);

    setSelectedTechs((prev) => {
      if (prev.includes(techId)) {
        const remaining = prev.filter((id) => id !== techId);
        // If we deselected tech 1 and tech 2 cannot stand alone as 1st tech, clear selection
        if (remaining.length === 1 && !canResearchAlone(hydrateTech(remaining[0]))) {
          return [];
        }
        return remaining;
      }
      if (maxSelectable === 1) {
        return [techId];
      }
      if (prev.length >= maxSelectable) {
        return [prev[0], techId];
      }
      return [...prev, techId];
    });
  };

  // Re-validate selection if planet skips or player tech changes
  useEffect(() => {
    setSelectedTechs((prev) => {
      if (prev.length === 0) return prev;
      const firstTech = hydrateTech(prev[0]);
      if (!canResearchAlone(firstTech)) return [];
      if (prev.length === 2) {
        const firstTrack = getTechnologyTrack(prev[0]);
        const trackWithFirst = { ...ownedTrackCounts };
        if (firstTrack) {
          trackWithFirst[firstTrack] = (trackWithFirst[firstTrack] || 0) + 1;
        }
        const secondTech = hydrateTech(prev[1]);
        const secondValid =
          isTechAllowedForFaction(secondTech, viewerPlayer?.faction) &&
          (!viewerPlayer || !hasResearchedTech(viewerPlayer, secondTech.id)) &&
          checkTechPrerequisites(
            secondTech,
            trackWithFirst,
            toggledSkipCounts,
            viewerPlayer?.faction,
          );
        if (!secondValid) return [prev[0]];
      }
      return prev;
    });
  }, [toggledSkipCounts, ownedTrackCounts, canResearchAlone, viewerPlayer]);

  const factionTechs = useMemo<HydratedTech[]>(() => {
    if (!viewerPlayer?.faction) return [];
    return getFactionTechIds(viewerPlayer.faction).map(hydrateTech);
  }, [viewerPlayer?.faction]);

  const unitUpgradeTechs = useMemo<HydratedTech[]>(() => {
    return UNIT_UPGRADE_TECH_IDS.map(hydrateTech);
  }, []);

  if (!isOpen) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content
        data-testid="technology-modal"
        className="choice-workflow-dialog technology-dialog"
      >
        <div className="panel choice-workflow-modal technology-modal-panel">
          {/* Header */}
          <div className="technology-modal__header">
            <div className="technology-modal__title-group">
              <Dialog.Title as="h2" className="technology-modal__title">
                Technologies
                {activeResearchMode && (
                  <span
                    style={{
                      fontSize: 12,
                      fontWeight: 600,
                      background: "var(--color-accent)",
                      color: "#0f172a",
                      padding: "2px 8px",
                      borderRadius: 12,
                    }}
                  >
                    Select Research
                  </span>
                )}
              </Dialog.Title>
              <div className="technology-modal__subtitle">
                {activeResearchMode
                  ? "Choose technology to research. Available resources and tech skips are shown below."
                  : "Public color technologies and unit upgrades"}
              </div>
            </div>

            {/* Player Roster Legend */}
            {playerList.length > 0 && (
              <div className="technology-modal__roster" data-testid="tech-modal-roster">
                <span className="technology-modal__roster-label">Players:</span>
                {playerList.map((player) => {
                  const identity = display(player.id);
                  const isDarkColor = identity.position === 8;
                  return (
                    <span
                      key={player.id}
                      className="player-tech-marker"
                      style={{ color: identity.color }}
                      title={`${identity.label} (${player.faction})`}
                      data-testid={`tech-roster-player-${player.id}`}
                    >
                      <span
                        className="player-tech-marker__symbol-wrap"
                        style={{
                          backgroundColor: identity.color,
                          color: isDarkColor ? "#ffffff" : "#090d16",
                        }}
                      >
                        {identity.symbol}
                      </span>
                      <span>{identity.label}</span>
                    </span>
                  );
                })}
              </div>
            )}

            <Dialog.Close
              data-testid="technology-modal-close"
              className="button button--secondary"
              aria-label="Close technologies modal"
            >
              Close
            </Dialog.Close>
          </div>

          {/* Research Mode Controls Panel (Tech Skips + Available Resources + Cost) */}
          {activeResearchMode && (
            <div className="technology-modal__research-panel" data-testid="tech-research-controls">
              {/* Tech Skips Row */}
              <div className="technology-modal__skips-row">
                <div className="technology-modal__skips-header">
                  <span className="technology-modal__skips-title">🪐 Tech Skips:</span>
                  <span className="technology-modal__skips-hint">
                    Toggle a ready planet to ignore 1 prerequisite
                  </span>
                </div>
                <div className="technology-modal__skips-list" data-testid="tech-skips-list">
                  {maxSelectable === 2 &&
                    selectedTechs.length >= 1 &&
                    (() => {
                      const firstTrack = getTechnologyTrack(selectedTechs[0]);
                      if (!firstTrack) return null;
                      const firstTech = hydrateTech(selectedTechs[0]);
                      return (
                        <span
                          key="first-tech-bonus"
                          className="tech-skip-pill tech-skip-pill--toggled"
                          style={{ cursor: "default" }}
                          data-testid="first-tech-prereq-bonus"
                          title={`1st selected technology (${firstTech.meta.name}) grants +1 ${firstTrack} prerequisite for your 2nd technology`}
                        >
                          <span
                            className={`tech-skip-pill__badge tech-skip-pill__badge--${firstTrack.toLowerCase()}`}
                          >
                            {firstTrack.charAt(0)}
                          </span>
                          <span>{firstTech.meta.name}</span>
                          <span className="tech-skip-pill__status">Tech 1 (+1)</span>
                        </span>
                      );
                    })()}

                  {controlledSpecialties.length === 0 &&
                  (maxSelectable !== 2 || selectedTechs.length === 0) ? (
                    <span className="technology-modal__skips-empty">
                      No specialty planets controlled
                    </span>
                  ) : (
                    controlledSpecialties.map((planet) => {
                      const isToggled =
                        !planet.isExhausted && Boolean(toggledPlanets[planet.planetId]);
                      return (
                        <button
                          key={planet.planetId}
                          type="button"
                          data-testid={`tech-skip-${planet.planetId}`}
                          disabled={planet.isExhausted}
                          className={`tech-skip-pill ${
                            planet.isExhausted
                              ? "tech-skip-pill--exhausted"
                              : isToggled
                                ? "tech-skip-pill--toggled"
                                : "tech-skip-pill--ready"
                          }`}
                          onClick={() => togglePlanetSkip(planet.planetId)}
                          title={
                            planet.isExhausted
                              ? `${planet.name} is exhausted`
                              : isToggled
                                ? `Click to untoggle ${planet.name} skip`
                                : `Click to toggle ${planet.name} skip`
                          }
                        >
                          <span
                            className={`tech-skip-pill__badge tech-skip-pill__badge--${planet.specialty.toLowerCase()}`}
                          >
                            {planet.specialty.charAt(0)}
                          </span>
                          <span>{planet.name}</span>
                          <span className="tech-skip-pill__status">
                            {planet.isExhausted
                              ? "(Exhausted)"
                              : isToggled
                                ? "Active (+1)"
                                : "Ready"}
                          </span>
                        </button>
                      );
                    })
                  )}
                </div>
              </div>

              {/* Resources & Decision Actions */}
              <div className="technology-modal__research-footer">
                <div
                  className="technology-modal__resources-bar"
                  data-testid="tech-available-resources"
                >
                  <span>Available Resources:</span>
                  <span className="technology-modal__resources-tag">
                    {availableResources.total}
                  </span>
                  <span className="technology-modal__resources-detail">
                    ({availableResources.planet} planet + {availableResources.tradeGoods} TG)
                  </span>
                </div>

                <div className="technology-modal__selection-status">
                  {isPrimaryTechnology ? (
                    <span>
                      Selected: <strong>{selectedTechs.length} / 2</strong>
                      {selectedTechs.length === 2 ? (
                        <span className="tech-cost-badge tech-cost-badge--paid">
                          Cost: 6 Resources
                        </span>
                      ) : (
                        <span className="tech-cost-badge tech-cost-badge--free">Cost: Free</span>
                      )}
                    </span>
                  ) : (
                    <span>
                      Selected: <strong>{selectedTechs.length} / 1</strong>
                      {researchCost > 0 ? (
                        <span className="tech-cost-badge tech-cost-badge--paid">
                          Cost: {researchCost} Resources
                        </span>
                      ) : (
                        <span className="tech-cost-badge tech-cost-badge--free">Cost: Free</span>
                      )}
                    </span>
                  )}
                </div>

                <div className="technology-modal__actions">
                  {choice?.options?.some((o) => o.id === "decline") && (
                    <button
                      type="button"
                      className="button button--secondary"
                      onClick={handleDecline}
                      disabled={isSubmitting}
                      data-testid="decline-research-btn"
                    >
                      Decline
                    </button>
                  )}
                  <button
                    type="button"
                    className="button button--primary"
                    onClick={handleConfirm}
                    disabled={!canConfirm || isSubmitting}
                    data-testid="confirm-research-btn"
                  >
                    {isSubmitting
                      ? "Confirming..."
                      : selectedTechs.length === 0
                        ? "Select Technology"
                        : isPrimaryTechnology
                          ? selectedTechs.length === 2
                            ? "Confirm Research (2 Techs - 6 Resources)"
                            : "Confirm Research (1 Tech - Free)"
                          : researchCost > 0
                            ? `Confirm Research (${researchCost} Resources)`
                            : "Confirm Research (Free)"}
                  </button>
                </div>
              </div>
            </div>
          )}

          {lastError && (
            <div className="session-error" role="alert" style={{ margin: "10px 24px 0" }}>
              {lastError}
            </div>
          )}

          {/* Scrollable Body */}
          <div className="technology-modal__scroll-body">
            <div className="technology-modal__content-container">
              {/* Faction Technologies - ALWAYS FIRST AT THE TOP! */}
              {factionTechs.length > 0 && (
                <section
                  className="technology-modal__faction-section"
                  aria-label="Faction Technologies"
                  data-testid="faction-technologies-section"
                >
                  <h3 className="technology-modal__section-heading">
                    Faction Technologies
                    {viewerPlayer?.faction && (
                      <small style={{ color: "#94a3b8" }}>
                        ({humanizeId(viewerPlayer.faction)})
                      </small>
                    )}
                  </h3>
                  <div className="technology-modal__faction-grid">
                    {factionTechs.map((tech) => (
                      <TechCard
                        key={tech.id}
                        tech={tech}
                        accentColor="var(--color-accent, #38bdf8)"
                        players={playerList}
                        isResearchMode={activeResearchMode}
                        isResearchable={isTechResearchable(tech)}
                        isSelected={selectedTechs.includes(tech.id)}
                        onSelectTech={() => handleSelectCard(tech.id)}
                      />
                    ))}
                  </div>
                </section>
              )}

              {/* Color Tracks Section */}
              <section aria-label="Color Technologies">
                <h3 className="technology-modal__section-heading">Standard Technologies</h3>

                {/* Column Headers */}
                <div className="technology-track-headers">
                  {TECH_TRACKS.map((track) => (
                    <div
                      key={track.id}
                      className="technology-column__header"
                      data-testid={`tech-track-${track.id.toLowerCase()}`}
                      style={{
                        borderBottom: `2px solid ${track.accentColor}`,
                      }}
                    >
                      <span className="technology-column__title">{track.name}</span>
                      <span
                        className="technology-column__badge"
                        style={{
                          background: track.badgeBg,
                          color: track.accentColor,
                          borderColor: track.badgeBorder,
                        }}
                      >
                        {track.colorName}
                      </span>
                    </div>
                  ))}
                </div>

                {/* Tier Sections with Row-aligned Grids */}
                <div className="technology-tiers-container">
                  {TECH_TIERS.map((tierGroup) => (
                    <div key={tierGroup.tier} className="technology-tier-section">
                      <div className="technology-tier-divider">
                        <span className="technology-tier-badge">{tierGroup.label}</span>
                        <div className="technology-tier-line" />
                      </div>

                      <div className="technology-tier-rows">
                        {tierGroup.rows.map((rowTechIds, rowIdx) => (
                          <div key={rowIdx} className="technology-grid-row">
                            {rowTechIds.map((techId, colIdx) => {
                              const track = TECH_TRACKS[colIdx];
                              const tech = hydrateTech(techId);
                              return (
                                <TechCard
                                  key={tech.id}
                                  tech={tech}
                                  accentColor={track?.accentColor ?? TRACK_ACCENTS[track?.id]}
                                  players={playerList}
                                  isResearchMode={activeResearchMode}
                                  isResearchable={isTechResearchable(tech)}
                                  isSelected={selectedTechs.includes(tech.id)}
                                  onSelectTech={() => handleSelectCard(tech.id)}
                                />
                              );
                            })}
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </section>

              {/* Unit Upgrades Section */}
              <section aria-label="Unit Upgrade Technologies">
                <h3 className="technology-modal__section-heading">
                  Unit Upgrades{" "}
                  <small style={{ color: "#94a3b8" }}>({unitUpgradeTechs.length})</small>
                </h3>
                <div className="technology-modal__unit-grid" data-testid="tech-track-unitupgrade">
                  {unitUpgradeTechs.map((tech) => (
                    <TechCard
                      key={tech.id}
                      tech={tech}
                      accentColor="#c084fc"
                      players={playerList}
                      isResearchMode={activeResearchMode}
                      isResearchable={isTechResearchable(tech)}
                      isSelected={selectedTechs.includes(tech.id)}
                      onSelectTech={() => handleSelectCard(tech.id)}
                    />
                  ))}
                </div>
              </section>
            </div>
          </div>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};

interface TechCardProps {
  tech: HydratedTech;
  accentColor?: string;
  players: PlayerView[];
  isResearchMode?: boolean;
  isResearchable?: boolean;
  isSelected?: boolean;
  onSelectTech?: () => void;
}

const TechCard: React.FC<TechCardProps> = ({
  tech,
  accentColor,
  players,
  isResearchMode = false,
  isResearchable = true,
  isSelected = false,
  onSelectTech,
}) => {
  const display = usePlayerIdentity();

  const isSelectable = isResearchMode ? isResearchable : false;

  const researchedPlayers = useMemo(() => {
    return players
      .filter((p) => hasResearchedTech(p, tech.id))
      .map((p) => ({
        player: p,
        identity: display(p.id),
        exhausted: isTechExhausted(p, tech.id),
      }));
  }, [players, tech.id, display]);

  const handleClick = () => {
    if (isSelectable && onSelectTech) {
      onSelectTech();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.key === "Enter" || e.key === " ") && isSelectable && onSelectTech) {
      e.preventDefault();
      onSelectTech();
    }
  };

  return (
    <div
      className="technology-card"
      data-testid={`tech-card-${tech.id}`}
      data-selectable={isSelectable}
      data-selected={isSelected}
      data-researchable={isResearchMode ? isResearchable : undefined}
      tabIndex={isSelectable ? 0 : undefined}
      role={isSelectable ? "button" : undefined}
      aria-selected={isSelectable ? isSelected : undefined}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      style={{
        borderLeft: accentColor ? `3px solid ${accentColor}` : undefined,
      }}
    >
      <div className="technology-card__top">
        <span className="technology-card__name" data-testid={`tech-name-${tech.id}`}>
          {tech.meta.name}
        </span>
        <div className="technology-card__prereqs" data-testid={`tech-prereqs-${tech.id}`}>
          {tech.prereqs.length > 0 ? (
            tech.prereqs.map((prereq, index) => (
              <span
                key={`${prereq.type}-${index}`}
                className="prereq-badge"
                style={{ color: prereq.color, backgroundColor: prereq.bgColor }}
                title={`${prereq.type} prerequisite`}
              >
                {prereq.label}
              </span>
            ))
          ) : (
            <span className="prereq-badge prereq-badge--none" title="No prerequisites">
              —
            </span>
          )}
        </div>
      </div>

      <div className="technology-card__effect" data-testid={`tech-effect-${tech.id}`}>
        {tech.meta.description}
      </div>

      <div className="technology-card__researched" data-testid={`tech-researched-${tech.id}`}>
        <span className="technology-card__researched-label">Researched:</span>
        {researchedPlayers.length > 0 ? (
          researchedPlayers.map(({ player, identity, exhausted }) => {
            const isDarkColor = identity.position === 8;
            return (
              <span
                key={player.id}
                className="player-tech-marker player-tech-marker--compact"
                style={{
                  backgroundColor: identity.color,
                  color: isDarkColor ? "#ffffff" : "#090d16",
                  borderColor: isDarkColor ? "#64748b" : "transparent",
                }}
                data-exhausted={exhausted}
                data-testid={`player-tech-marker-${player.id}-${tech.id}`}
                title={`${identity.label} (${player.faction})${exhausted ? " - Exhausted" : ""}`}
              >
                {identity.symbol}
              </span>
            );
          })
        ) : (
          <span style={{ fontSize: 11, color: "#475569" }}>None</span>
        )}
      </div>
    </div>
  );
};
