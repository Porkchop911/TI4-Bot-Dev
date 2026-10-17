// The seats and the player table.
import { planetValuesNow } from "@ti4/shared/planets/values.ts";
import type { PlayerRowView, PlayerTableView, SeatId, SeatView } from "../../model";
import { MLP_PREFIX } from "../models";
import type { Catalog, GameView, PlayerView, ReactionModes, SessionUpdate } from "../wire";
import { componentCards } from "./components";
import { DECKS } from "./exploration";
import { commodityValue, factionName, seatMark, strategyCard } from "./names";

/** The players in seat order. */
export const seated = (update: SessionUpdate): PlayerView[] => {
  const order = update.view.seating_order;
  return [...update.view.players].sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
};

export function selectSeats(
  update: SessionUpdate,
  /** The printed names. Without it a faction is shown by its id. */
  catalog: Catalog | null = null,
): Record<SeatId, SeatView> {
  return Object.fromEntries(
    seated(update).map((player, index) => {
      // The table says who sits in the seat. A game with no table has no names: the letter of
      // the seat tells the seats apart.
      const fact = update.seats?.[player.id];
      const seat: SeatView = {
        id: player.id,
        name: fact?.nickname || `Seat ${player.id.toUpperCase()}`,
        faction: factionName(player.faction, catalog),
        ...seatMark(index),
      };
      if (fact?.kind.startsWith(MLP_PREFIX)) {
        // A host names the seat of a model by its label and a number ("Reviewer 473312 1").
        seat.played = {
          kind: "model",
          word: `${fact.nickname.replace(/ \d+$/, "")}: no person plays this seat`,
        };
      } else if (fact && fact.kind !== "human") {
        seat.played = {
          kind: fact.kind === "smart" ? "rule" : "random",
          word: "No person plays this seat",
        };
      }
      return [player.id, seat];
    }),
  );
}

/** Who the game waits for, or whose turn it is. */
export const actingSeat = (update: SessionUpdate): SeatId | null => {
  const status = update.turn_status;
  return status.kind === "waiting_for_decision"
    ? status.seat
    : status.kind === "active_turn"
      ? status.player
      : null;
};

export function selectPlayers(
  update: SessionUpdate,
  /** The printed cards. Without it a card is shown by its id. */
  catalog: Catalog | null = null,
  /** The modes that the viewer set for its reactions: the sheet of the viewer has them. */
  modes: ReactionModes = {},
): PlayerTableView {
  const { view, viewer } = update;
  const acting = actingSeat(update);
  // What each seat's planets give: ready and total, from the planets that the seat controls.
  const values = new Map<string, { resources: number; influence: number }>();
  for (const tile of view.board.map_tiles ?? []) {
    for (const planet of tile.planets ?? []) {
      values.set(planet.id, planet);
    }
  }
  const row = (player: PlayerView): PlayerRowView => {
    const resources: [number, number] = [0, 0];
    const influence: [number, number] = [0, 0];
    let planets = 0;
    for (const system of Object.values(view.board.systems)) {
      for (const planet of Object.values(system.planets)) {
        const value = values.get(planet.planet_id);
        if (planet.controlled_by !== player.id || !value) {
          continue;
        }
        planets++;
        // The values now, with the attachments and the laws on the planet, when they differ.
        const now = planetValuesNow(value, planet);
        resources[1] += now.resources;
        influence[1] += now.influence;
        if (!planet.exhausted) {
          resources[0] += now.resources;
          influence[0] += now.influence;
        }
      }
    }
    return {
      seat: player.id,
      isMe: viewer.role === "player" && viewer.seat === player.id,
      turn: acting === player.id ? "now" : null,
      speaker: view.speaker === player.id,
      passed: player.passed,
      victoryPoints: player.victory_points,
      strategyCards: player.strategy_cards
        .map((id) => ({
          ...strategyCard(id, catalog),
          used: player.exhausted_strategy_cards.includes(id),
        }))
        .sort((a, b) => a.number - b.number),
      resources,
      influence,
      tradeGoods: player.trade_goods,
      commodities: [
        player.commodities,
        commodityValue(player.faction, catalog) ?? player.commodities,
      ],
      tokens: [player.tactic_tokens, player.fleet_tokens, player.strategic_tokens],
      actionCards: player.action_cards_count,
      secrets: [
        // The facts of the host have every scored secret; the view of a player has the plots only.
        (update.secret_objectives?.scored[player.id] ?? player.scored_secret_objectives ?? [])
          .length,
        player.secret_objectives_count,
      ],
      planets,
      ...componentCards(
        update,
        player,
        catalog,
        viewer.role === "player" && viewer.seat === player.id ? modes : null,
      ),
      // The ids are the short names that players use: AMD, DET, NM.
      technologies: player.technologies.map((id) => id.toUpperCase()),
      fragments: DECKS.flatMap(([key, deck]) => {
        const count = player.relic_fragments?.[key] ?? 0;
        return count > 0 ? [{ deck, count }] : [];
      }),
    };
  };
  const rows = seated(update).map(row);
  return {
    pick: null,
    rows: ranked(view, rows),
    sort: view.phase === "action" ? "initiative" : "vp",
    initiative: view.initiative_order?.length
      ? view.initiative_order
      : rows
          .filter((row) => row.strategyCards.length > 0)
          .sort((a, b) => a.strategyCards[0]!.number - b.strategyCards[0]!.number)
          .map((row) => row.seat),
    turn: view.phase === "action" ? view.turn : 0,
    freeCards: view.table.unclaimed_strategy_cards
      .map((id) => ({
        ...strategyCard(id, catalog),
        tradeGoods: view.table.strategy_card_goods[id] ?? 0,
      }))
      .sort((a, b) => a.number - b.number),
  };
}

/**
 * The rows of the table: in the action phase in the order of the turns that the engine sends (an
 * initiative override such as the Naalu "0" token counted), a seat that is not in it last; in
 * every other phase by victory points, the most first. A tie keeps the seat order.
 */
function ranked(view: GameView, rows: PlayerRowView[]): PlayerRowView[] {
  const order = view.initiative_order ?? [];
  const key =
    view.phase === "action"
      ? (row: PlayerRowView) => {
          const index = order.indexOf(row.seat);
          return index < 0 ? order.length : index;
        }
      : (row: PlayerRowView) => -row.victoryPoints;
  return [...rows].sort((a, b) => key(a) - key(b));
}
