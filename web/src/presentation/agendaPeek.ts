import type { AgendaPeekCardDto, PlayerView } from "../protocol/types.ts";

export interface AgendaPeekEntry {
  /** 1 is the next agenda to be revealed. */
  place: number;
  name: string;
  /** "Law", "Directive", with what it decides ("For/Against", "Elect Player"). */
  tags: string[];
  /** The printed text, one paragraph per entry. */
  text: string[];
}

export interface AgendaPeek {
  entries: AgendaPeekEntry[];
  /** Changes when the peek does, so a dismissed card stays dismissed only for the cards it showed. */
  key: string;
}

/** What the viewing seat was shown by Insider Information, or `null` when there is nothing to show now. */
export function describeAgendaPeek(players: readonly PlayerView[], viewerSeat: string | null | undefined): AgendaPeek | null {
  if (!viewerSeat) return null;
  const cards: readonly AgendaPeekCardDto[] = players.find((player) => player.id === viewerSeat)?.agenda_peek ?? [];
  if (cards.length === 0) return null;
  return {
    key: cards.map((card) => card.id).join(","),
    entries: cards.map((card, index) => ({
      place: index + 1,
      name: card.name,
      tags: [card.type, card.target].filter((tag) => tag.length > 0),
      text: [card.text1, card.text2].map((line) => line.trim()).filter((line) => line.length > 0),
    })),
  };
}
