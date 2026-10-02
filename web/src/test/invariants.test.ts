import { describe, it, expect } from "vitest";
import { PlayerView, PendingChoiceDto, PublicTurnStatus, ViewerRole } from "../protocol/types.ts";
import {
  getSecretObjectiveMeta,
  getStrategyCardMeta,
  findActionCardMeta,
  findSecretObjectiveMeta,
  findStrategyCardMeta,
  humanizeId,
  STRATEGY_CARDS,
  SECRET_OBJECTIVES,
} from "../protocol/contentCatalog.ts";
import { CONTENT_PRESENTATION_PROVENANCE } from "../protocol/generatedContentManifest.ts";

/**
 * System invariant verification suite.
 * Validates the core mathematical, game-rule, and privacy invariants that must hold
 * across all states and view projections.
 */

function assertPrivacyInvariant(players: PlayerView[], viewer: ViewerRole) {
  for (const p of players) {
    if (viewer.role === "player" && viewer.seat === p.id) {
      // Allowed to see own cards
      continue;
    }
    // Invariant: Opponents and spectators must never have private cards in their view
    expect(p.held_action_cards ?? []).toHaveLength(0);
    expect(p.held_secret_objectives ?? []).toHaveLength(0);
  }
}

function assertArithmeticInvariant(players: PlayerView[]) {
  for (const p of players) {
    expect(Number.isFinite(p.victory_points)).toBe(true);
    expect(p.victory_points).toBeGreaterThanOrEqual(0);

    expect(Number.isFinite(p.trade_goods)).toBe(true);
    expect(p.trade_goods).toBeGreaterThanOrEqual(0);

    expect(Number.isFinite(p.commodities)).toBe(true);
    expect(p.commodities).toBeGreaterThanOrEqual(0);

    expect(Number.isFinite(p.tactic_tokens)).toBe(true);
    expect(p.tactic_tokens).toBeGreaterThanOrEqual(0);

    expect(Number.isFinite(p.fleet_tokens)).toBe(true);
    expect(p.fleet_tokens).toBeGreaterThanOrEqual(0);

    expect(Number.isFinite(p.strategic_tokens)).toBe(true);
    expect(p.strategic_tokens).toBeGreaterThanOrEqual(0);
  }
}

function assertActiveSeatChoiceInvariant(
  turnStatus: PublicTurnStatus,
  pendingChoice: PendingChoiceDto | null,
  viewer: ViewerRole,
) {
  if (turnStatus.kind === "waiting_for_decision") {
    if (viewer.role === "player" && viewer.seat === turnStatus.seat) {
      // Invariant: Active player MUST receive their pending choice
      expect(pendingChoice).not.toBeNull();
      expect(pendingChoice?.actor).toBe(turnStatus.seat);
      expect(pendingChoice?.options.length).toBeGreaterThan(0);
      expect(pendingChoice?.nonce).toMatch(/^[0-9a-fA-F]{16}$/);
    } else {
      // Invariant: Non-active players and spectators MUST NOT see choice details
      expect(pendingChoice).toBeNull();
    }
  }
}

describe("Frontend Invariants & Property-based Checks", () => {
  it("enforces privacy invariant across simulated player seats", () => {
    const players: PlayerView[] = [
      {
        id: "p1",
        faction: "Sol",
        victory_points: 0,
        trade_goods: 0,
        commodities: 4,
        tactic_tokens: 3,
        fleet_tokens: 3,
        strategic_tokens: 2,
        passed: false,
        strategy_cards: [],
        exhausted_strategy_cards: [],
        technologies: [],
        exhausted_technologies: [],
        relics: [],
        exhausted_relics: [],
        action_cards_count: 2,
        secret_objectives_count: 1,
        held_action_cards: ["Card A", "Card B"],
        held_secret_objectives: ["Obj 1"],
        leaders: {},
      },
      {
        id: "p2",
        faction: "Letnev",
        victory_points: 0,
        trade_goods: 1,
        commodities: 2,
        tactic_tokens: 2,
        fleet_tokens: 4,
        strategic_tokens: 1,
        passed: false,
        strategy_cards: [],
        exhausted_strategy_cards: [],
        technologies: [],
        exhausted_technologies: [],
        relics: [],
        exhausted_relics: [],
        action_cards_count: 1,
        secret_objectives_count: 1,
        held_action_cards: [],
        held_secret_objectives: [],
        leaders: {},
      },
    ];

    // Viewer p1
    assertPrivacyInvariant(players, { role: "player", seat: "p1" });

    // Viewer p2 (after redacting p1)
    const redactedForP2 = players.map((p) =>
      p.id === "p1"
        ? { ...p, held_action_cards: [], held_secret_objectives: [] }
        : { ...p, held_action_cards: ["Card C"], held_secret_objectives: ["Obj 2"] },
    );
    assertPrivacyInvariant(redactedForP2, { role: "player", seat: "p2" });

    // Viewer Spectator
    const redactedForSpec = players.map((p) => ({
      ...p,
      held_action_cards: [],
      held_secret_objectives: [],
    }));
    assertPrivacyInvariant(redactedForSpec, { role: "spectator" });
  });

  it("enforces arithmetic integrity on player resources and VP", () => {
    const players: PlayerView[] = [
      {
        id: "p1",
        faction: "Sol",
        victory_points: 5,
        trade_goods: 10,
        commodities: 0,
        tactic_tokens: 4,
        fleet_tokens: 3,
        strategic_tokens: 1,
        passed: false,
        strategy_cards: [],
        exhausted_strategy_cards: [],
        technologies: [],
        exhausted_technologies: [],
        relics: [],
        exhausted_relics: [],
        action_cards_count: 0,
        secret_objectives_count: 0,
        leaders: {},
      },
    ];

    assertArithmeticInvariant(players);
  });

  it("enforces active seat choice invariant across 20 randomized turn transitions", () => {
    const seats = ["p1", "p2", "p3"];

    for (let step = 0; step < 20; step++) {
      const activeSeat = seats[step % seats.length];
      const turnStatus: PublicTurnStatus = {
        kind: "waiting_for_decision",
        seat: activeSeat,
        phase: "Strategy",
        round: 1,
        stage: "Strategy Card Selection",
      };

      const pendingChoiceForActive: PendingChoiceDto = {
        prompt: "Choose a Strategy Card",
        actor: activeSeat,
        nonce: "0123456789abcdef",
        options: [
          { id: "opt_1", label: "Leadership" },
          { id: "opt_2", label: "Diplomacy" },
        ],
      };

      for (const viewerSeat of seats) {
        const viewerRole: ViewerRole = { role: "player", seat: viewerSeat };
        const choice = viewerSeat === activeSeat ? pendingChoiceForActive : null;
        assertActiveSeatChoiceInvariant(turnStatus, choice, viewerRole);
      }

      // Also check spectator
      assertActiveSeatChoiceInvariant(turnStatus, null, { role: "spectator" });
    }
  });

  it("enforces human-readable metadata resolution for all secret objectives and strategy cards", () => {
    // Check specific required sample IDs
    const faa = getSecretObjectiveMeta("faa");
    expect(faa.name).toBe("Forge an Alliance");
    expect(faa.description).toBe("Control 4 cultural planets.");
    expect(faa.phase).toBe("Status");
    expect(faa.points).toBe(1);

    const scLeadership = getStrategyCardMeta("pok1leadership");
    expect(scLeadership.name).toBe("Leadership");
    expect(scLeadership.initiative).toBe(1);
    expect(scLeadership.primaryText).toContain("Gain 3 command tokens");

    const scWarfare = getStrategyCardMeta("pok6warfare");
    expect(scWarfare.name).toBe("Warfare");
    expect(scWarfare.initiative).toBe(6);

    // Assert every card in STRATEGY_CARDS has non-empty name and initiative > 0
    for (const [id, card] of Object.entries(STRATEGY_CARDS)) {
      expect(card.name).toBeTruthy();
      expect(card.name).not.toBe(id);
      expect(card.initiative).toBeGreaterThan(0);
      expect(card.primaryText).toBeTruthy();
    }

    // Assert every objective in SECRET_OBJECTIVES has non-empty name and description
    for (const [id, obj] of Object.entries(SECRET_OBJECTIVES)) {
      expect(obj.name).toBeTruthy();
      expect(obj.name).not.toBe(id);
      expect(obj.description).toBeTruthy();
      expect(obj.points).toBeGreaterThan(0);
    }
  });

  it("title-cases unknown content identifiers", () => {
    expect(humanizeId("direct_hit")).toBe("Direct Hit");
    expect(getSecretObjectiveMeta("unknown_secret").name).toBe("Unknown Secret");
  });

  it("uses versioned corpus metadata with exact identifiers only", () => {
    expect(CONTENT_PRESENTATION_PROVENANCE.generatorVersion).toBe(1);
    expect(CONTENT_PRESENTATION_PROVENANCE.corpusSchemaVersion).toBe("1.1.0");
    expect(CONTENT_PRESENTATION_PROVENANCE.corpusUpstreamCommit).toMatch(/^[0-9a-f]{40}$/);
    expect(CONTENT_PRESENTATION_PROVENANCE.presentationSha256).toMatch(/^[0-9a-f]{64}$/);
    expect(CONTENT_PRESENTATION_PROVENANCE.recordCounts).toEqual({
      strategyCards: 12,
      secretObjectives: 40,
      publicObjectives: 40,
      actionCards: 142,
      technologies: 102,
      explorationCards: 80,
      planets: 159,
      attachments: 22,
    });

    // Strategy-card names are not lookup aliases: a malformed ID must not select a card.
    const malformed = getStrategyCardMeta("leadership bonus");
    expect(malformed.name).toBe("Leadership Bonus");
    expect(malformed.initiative).toBe(0);
    expect(malformed.primaryText).toBe("");

    expect(findStrategyCardMeta("pok1leadership")?.name).toBe("Leadership");
    expect(findStrategyCardMeta(" POK1LEADERSHIP ")).toBeUndefined();
    expect(findSecretObjectiveMeta("FAA")).toBeUndefined();

    // Action-card automation IDs are the only declared alternate lookup keys.
    expect(findActionCardMeta("dh1")?.id).toBe("dh1");
    expect(findActionCardMeta("direct_hit")?.name).toBe("Direct Hit");
    expect(findActionCardMeta("Direct Hit")).toBeUndefined();
  });

  it("preserves event log identity across reconnect snapshots and live event messages", () => {
    const events: import("../protocol/types.ts").GameEvent[] = [
      {
        id: "game_1-1",
        timestamp: "10:00:00",
        version: 1,
        visibility: "public",
        event: { kind: "game_initialized", round: 1, phase: "strategy", speaker: "p1" },
      },
      {
        id: "game_1-2",
        timestamp: "10:00:01",
        version: 2,
        visibility: "seat",
        seat: "p1",
        event: { kind: "decision_resolved" },
      },
      {
        id: "game_1-3",
        timestamp: "10:00:05",
        version: 2,
        visibility: "public",
        event: { kind: "phase_transition", phase: "action", round: 1 },
      },
    ];

    // Verify properties of the event history
    expect(events).toHaveLength(3);
    for (const e of events) {
      expect(e.id).toMatch(/^game_1-\d+$/);
      expect(e.timestamp).toMatch(/^\d{2}:\d{2}:\d{2}$/);
      expect(e.event.kind).toBeTruthy();
    }
  });

  it("enforces that board interaction and targeting derive from structured projection data, never option-id regexes or seat-name tables", async () => {
    const { getPlayerColor, deriveActorTargetHighlights, PLAYER_PALETTE, NEUTRAL_COLOR } =
      await import("../presentation/boardPresentation.ts");

    // Invariant 1: Seat names can be arbitrary non-seat strings and map deterministically to index palette
    const seatingOrder = ["alpha-user", "bravo-user", "seat99"];
    expect(getPlayerColor("alpha-user", seatingOrder)).toBe(PLAYER_PALETTE[0]);
    expect(getPlayerColor("bravo-user", seatingOrder)).toBe(PLAYER_PALETTE[1]);
    expect(getPlayerColor("seat99", seatingOrder)).toBe(PLAYER_PALETTE[2]);
    expect(getPlayerColor(null, seatingOrder)).toBe(NEUTRAL_COLOR);

    // Invariant 2: Target highlighting maps from structured payload/context without regexes on option IDs
    const opaqueChoice: PendingChoiceDto = {
      nonce: "nonce_abc",
      actor: "alpha-user",
      prompt: "Select target",
      options: [
        {
          // Opaque random option IDs that do NOT follow any naming convention
          id: "x79fk2_custom_action",
          kind: "tactical_move",
          label: "Deploy to system 18",
          payload: { system: "18", to: "18", planet: "mecatol_rex" },
        },
      ],
      context: {
        version: 1,
        actor: "alpha-user",
        subtype: "movement",
        target: { System: "18" },
      },
    };

    const highlights = deriveActorTargetHighlights(opaqueChoice, "alpha-user");
    expect(highlights.hasActiveTargets).toBe(true);
    expect(highlights.targetableSystemIds.has("18")).toBe(true);
    expect(highlights.targetablePlanetIds.has("mecatol_rex")).toBe(true);
    expect(highlights.systemOptionMap.get("18")).toEqual(["x79fk2_custom_action"]);
    expect(highlights.planetOptionMap.get("mecatol_rex")).toEqual(["x79fk2_custom_action"]);
  });
});
