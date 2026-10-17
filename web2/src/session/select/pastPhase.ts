// A strategy, status or agenda phase of the Log, opened: what the seats did in it, read-only, as
// the seat lists of a strategy card show it. The strategy phase is the picks in their order with
// the trade goods that each card had; the status phase is the objectives scored by whom, the
// objectives revealed as printed, and the command tokens of each seat; the agenda phase is each
// agenda as printed, its outcome, and each seat's vote with its planets. Everything is of the
// public events: the facts of the phase, and the board at its start and end (`history.ts`).
import type {
  BlockView,
  FlowActionView,
  LogGroupView,
  LogLineView,
  SeatRowView,
} from "../../model";
import type { EventLog } from "../events";
import { boardAt, withBoard } from "../history";
import type { Catalog, SessionUpdate } from "../wire";
import { plain } from "./components";
import { phaseParts } from "./log";
import { seatLabel, strategyCard } from "./names";
import { boardEffects } from "./pastFlow";

/** A phase of the Log that is open. */
export interface PastPhase {
  /** The game with the board of the end of the phase (of now, while it is played). */
  update: SessionUpdate;
  /** No system is marked: a phase is of the whole board. */
  system: null;
  action: FlowActionView;
}

const NOTHING: FlowActionView["footer"] = { note: "", error: false, actions: [] };

type Status = SeatRowView["status"];
const DONE = (label: string): Status => ({ tone: "done", sign: "✓", label });

/**
 * Of the phase `id` of the Log ("status-3"): what the seats did in it, read-only. Null for an id
 * that is no strategy, status or agenda phase of the events.
 */
export function selectPastPhase(
  update: SessionUpdate,
  events: EventLog,
  id: string,
  printed: Catalog | null = null,
): PastPhase | null {
  const parts = phaseParts(update, events, id, printed);
  if (!parts) {
    return null;
  }
  const { view, facts } = parts;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const name = (seat: string) => (seat === viewer ? "You" : seatLabel(update, seat, printed));
  /** A row of a seat in a list of the phase. */
  const row = (seat: string, order: string, status: Status, text?: string): SeatRowView => ({
    order,
    seat,
    name: name(seat),
    you: seat === viewer,
    status,
    ...(text ? { text } : {}),
  });
  const group = (title: string) => view.groups.find((item) => item.title === title);
  const shown = new Set<LogGroupView>();
  const blocks: BlockView[] = [];

  if (view.phase === "strategy") {
    const picks = group("Picks");
    // What each seat took from the board in the phase: the trade goods on its card.
    const end = (parts.at.at(-1) ?? events.length - 1) + 1;
    const goods = boardEffects(
      update,
      boardAt(update.view, events, parts.at[0] ?? end),
      boardAt(update.view, events, end),
      ["trade_goods"],
    );
    const picked = (picks?.lines ?? []).flatMap((line) => (line.seat ? [line.seat] : []));
    const rows = (picks?.lines ?? []).flatMap((line, at) =>
      line.seat
        ? [row(line.seat, `${at + 1}`, DONE(line.text), goods.get(line.seat)?.join(" · "))]
        : [],
    );
    if (!view.live && view.round === update.view.round) {
      // A card that the game gave with no choice (the last card of the phase) is no fact: the
      // seats hold the cards of the round in play.
      const told = new Set((picks?.lines ?? []).map((line) => line.text));
      for (const player of update.view.players) {
        for (const card of player.strategy_cards ?? []) {
          const label = strategyCard(card, printed).name;
          if (!told.has(label)) {
            rows.push(
              row(player.id, `${rows.length + 1}`, DONE(label), goods.get(player.id)?.join(" · ")),
            );
          }
        }
      }
    }
    if (view.live) {
      // The seats that have not picked yet: the seat that picks now, then the others.
      const active = update.view.active_player ?? (update.pending_choice ? viewer : null);
      for (const player of update.view.players) {
        if (!picked.includes(player.id)) {
          rows.push(
            row(
              player.id,
              `${rows.length + 1}`,
              player.id === active
                ? { tone: "live", sign: "▶", label: "Deciding" }
                : { tone: "quiet", sign: "·", label: "Waits" },
            ),
          );
        }
      }
    }
    if (picks) {
      shown.add(picks);
    }
    if (rows.length) {
      blocks.push({ kind: "seats", title: "Picks", rows });
    }
  }

  if (view.phase === "status") {
    const scored = group("Scored");
    if (scored) {
      shown.add(scored);
      blocks.push({
        kind: "seats",
        title: "Scored",
        rows: seatRows(scored.lines, row, DONE("Scored")),
      });
    }
    const revealed = group("Revealed");
    if (revealed) {
      shown.add(revealed);
      const cards = printed?.components.public_objectives ?? [];
      for (const fact of facts) {
        if (fact.fact.kind !== "objective_revealed") {
          continue;
        }
        const id = fact.fact.objective;
        const card = cards.find((item) => item.id === id);
        blocks.push(
          card
            ? {
                kind: "source",
                name: card.name,
                type: `Revealed · Stage ${card.points === 2 ? "II" : "I"} objective`,
                text: plain(card.text),
              }
            : {
                kind: "note",
                tone: "quiet",
                strong: "Revealed",
                text: revealedName(revealed, fact.fact.objective),
              },
        );
      }
    }
    const tokens = group("Command tokens");
    if (tokens) {
      shown.add(tokens);
      blocks.push({
        kind: "seats",
        title: "Command tokens",
        rows: seatRows(tokens.lines, row, DONE("Redistributed")),
      });
    }
  }

  if (view.phase === "agenda") {
    const resolved = facts.flatMap((item) =>
      item.fact.kind === "agenda_resolved" ? [item.fact] : [],
    );
    // The groups of the agendas are in their order, as their facts of the end are.
    const deck = printed?.components.agendas ?? [];
    let number = 0;
    for (const item of view.groups.filter((group) => group.agenda)) {
      const fact = resolved[number];
      number += 1;
      shown.add(item);
      const card = fact ? deck.find((agenda) => agenda.id === fact.agenda) : undefined;
      if (card) {
        blocks.push({
          kind: "source",
          name: card.name,
          type: ["Agenda", card.kind, card.elect].filter(Boolean).join(" · "),
          text: plain([card.text, card.against].filter(Boolean).join("\n")),
        });
      }
      // The outcome, and how the game ended the agenda: the lines of no seat.
      const end = item.lines.filter((line) => line.seat === null).map((line) => line.text);
      blocks.push({
        kind: "note",
        tone: fact ? "success" : "accent",
        strong: card ? outcomeOf(item.title, card.name) : item.title,
        text: fact ? end.join(" · ") : view.live ? "Voting" : end.join(" · "),
      });
      const votes = item.lines.flatMap((line, at) => {
        if (!line.seat) {
          return [];
        }
        if (line.vote) {
          const { outcome, votes: count, planets } = line.vote;
          return [
            row(
              line.seat,
              `${at + 1}`,
              outcome === null ? { tone: "quiet", sign: "–", label: "Abstained" } : DONE(outcome),
              outcome === null
                ? undefined
                : [count ? `${count} ${count === 1 ? "vote" : "votes"}` : "", planets.join(", ")]
                    .filter(Boolean)
                    .join(": "),
            ),
          ];
        }
        // A card played for the agenda, a tie that the speaker broke.
        return [
          row(
            line.seat,
            `${at + 1}`,
            DONE(line.text.startsWith("Played ") ? "Played" : "Speaker"),
            line.text,
          ),
        ];
      });
      if (votes.length) {
        blocks.push({ kind: "seats", title: "Votes", rows: votes });
      }
    }
  }

  // What else the seats did in the phase: an effect, a card played.
  for (const item of view.groups) {
    if (shown.has(item)) {
      continue;
    }
    const seated = item.lines.filter((line) => line.seat);
    if (seated.length) {
      blocks.push({ kind: "seats", title: item.title, rows: seatRows(seated, row, DONE("Done")) });
    }
    for (const line of item.lines.filter((line) => !line.seat)) {
      blocks.push({ kind: "note", tone: "quiet", strong: item.title, text: line.text });
    }
  }

  const [heading] = view.label.split(" · ");
  const end = (parts.at.at(-1) ?? events.length - 1) + 1;
  return {
    update: {
      ...update,
      view: withBoard(update.view, boardAt(update.view, events, end)),
      pending_choice: undefined,
    },
    system: null,
    action: {
      kind: "flow",
      context: `Round ${view.round}`,
      contentKey: `past:${id}`,
      heading,
      pill: view.live ? { tone: "live", label: "Played now" } : { tone: "done", label: "Resolved" },
      help: [],
      badge: null,
      tabs: [],
      selected: 0,
      current: null,
      interrupt: null,
      closing: [],
      trail: [],
      footer: NOTHING,
      blocks: blocks.length
        ? blocks
        : [{ kind: "note", tone: "quiet", text: view.live ? "Nothing yet" : "Nothing happened" }],
    },
  };
}

/** The lines of seats as rows of a list, each with its text. */
function seatRows(
  lines: LogLineView[],
  row: (seat: string, order: string, status: Status, text?: string) => SeatRowView,
  status: Status,
): SeatRowView[] {
  return lines.flatMap((line, at) =>
    line.seat ? [row(line.seat, `${at + 1}`, status, line.text)] : [],
  );
}

/** "For (For 7 · Against 3)" of the title of a resolved agenda ("Mutiny · For (For 7 · …)"). */
const outcomeOf = (title: string, name: string) =>
  title.startsWith(`${name} · `) ? title.slice(name.length + 3) : title;

/** The name of a revealed objective, as the Log says it, when the catalogue does not have it. */
const revealedName = (group: LogGroupView, id: string) =>
  group.lines.find((line) => line.text.toLowerCase().replaceAll(" ", "_") === id)?.text ??
  group.lines.map((line) => line.text).join(", ");
