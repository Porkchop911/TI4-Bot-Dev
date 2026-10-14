// The Objectives sheet: every source of a victory point against every seat. The engine says what
// each seat has from each source (`score` of the update); this only puts it in rows and words.
import type { ScoreCellView, ScoreRowView, ScoreView } from "../../model";
import type { Catalog, PointSource, SessionUpdate } from "../wire";
import { plain } from "./components";
import { titleCase } from "./names";
import { seated } from "./players";

/** The name of a source that the game has once. */
const FIXED: Partial<Record<PointSource["kind"], string>> = {
  secrets: "Secret objectives",
  custodians: "Custodians of Mecatol Rex",
  imperial: "Imperial · Mecatol Rex",
  support: "Support for the Throne",
};
/** What a source with a printed name is. */
const KIND: Partial<Record<PointSource["kind"], string>> = {
  agenda: "Agenda",
  relic: "Relic",
  action_card: "Action card",
  planet: "Planet",
};

export function selectScore(update: SessionUpdate, catalog: Catalog | null): ScoreView | null {
  const facts = update.score;
  if (!facts) {
    return null;
  }
  const viewer = update.viewer;
  const me = viewer.role === "player" ? viewer.seat : null;
  // The viewer is first, then the seats by their points: the sheet cuts the last ones off.
  const players = seated(update).sort(
    (a, b) => Number(b.id === me) - Number(a.id === me) || b.victory_points - a.victory_points,
  );
  const seats = players.map((player) => player.id);
  const revealed = new Set(facts.public.map((item) => item.id));
  // The game reveals the objectives of stage II after those of stage I: with one of stage II
  // on the table, no objective of stage I comes any more.
  const second = facts.public.some((item) => item.points >= 2);
  const unrevealed = (catalog?.components.public_objectives ?? []).filter(
    (card) => !revealed.has(card.id) && !(second && card.points < 2),
  );
  const objective = (item: (typeof facts.public)[number]): ScoreRowView => ({
    name: item.name,
    text: plain(item.text),
    worth: item.points,
    open: item.scored.length < seats.length,
    cells: seats.map((seat) => ({ kind: item.scored.includes(seat) ? "scored" : "none" })),
  });
  const points = (
    name: string,
    open: boolean,
    of: Record<string, number>,
    text = "",
    from: Record<string, string[]> = {},
    detail: Record<string, { name: string; text: string }[]> = {},
  ): ScoreRowView => ({
    name,
    text,
    worth: null,
    open,
    cells: seats.map((seat): ScoreCellView => {
      const value = of[seat] ?? 0;
      return value === 0
        ? { kind: "none" }
        : { kind: "points", value, from: from[seat] ?? [], detail: detail[seat] ?? [] };
    }),
  });
  // The scored secret objectives of each seat by name, for the row that counts them.
  const printed = new Map(catalog?.components.secret_objectives.map((card) => [card.id, card]));
  const secrets = Object.fromEntries(
    Object.entries(update.secret_objectives?.scored ?? {}).map(([seat, ids]) => [
      seat,
      ids.map((id) => {
        const card = printed.get(id);
        return { name: card?.name ?? titleCase(id), text: card ? plain(card.text) : "" };
      }),
    ]),
  );
  const other = facts.sources.map((source) =>
    points(
      FIXED[source.kind] ??
        (KIND[source.kind] ? `${KIND[source.kind]} · ${source.name}` : source.name),
      // What is still to get or to lose: a note changes hands, a law can be repealed, and the
      // custodians are there once.
      source.kind === "support" ||
        source.kind === "agenda" ||
        (source.kind === "custodians" && Object.keys(source.points).length === 0),
      source.points,
      plain(source.text ?? ""),
      source.from,
      source.kind === "secrets" ? secrets : {},
    ),
  );
  if (facts.unexplained && Object.keys(facts.unexplained).length > 0) {
    other.push(points("Other", false, facts.unexplained));
  }
  return {
    target: facts.target,
    seats: players.map((player) => ({
      seat: player.id,
      points: player.victory_points,
      isMe: player.id === me,
    })),
    groups: [
      {
        title: "Public objectives · stage I",
        rows: facts.public.filter((item) => item.points < 2).map(objective),
      },
      {
        title: "Public objectives · stage II",
        rows: facts.public.filter((item) => item.points >= 2).map(objective),
      },
      { title: "Other points", rows: other },
    ],
    deck: (catalog?.components.public_objective_groups ?? []).flatMap((group) => {
      const cards = unrevealed.filter((card) => card.group === group);
      return cards.length === 0
        ? []
        : [
            {
              group,
              cards: cards.map((card) => ({
                name: card.name,
                text: plain(card.text),
                worth: card.points,
              })),
            },
          ];
    }),
  };
}
