import {
  findPlanetMeta,
  findPublicObjectiveMeta,
  findSecretObjectiveMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";
import { agendaNameFromId } from "./agendaStrip.ts";

/** One entry of the engine's `agenda_log`: a finished agenda vote (public, written at resolution). */
export interface AgendaRecordDto {
  round?: number;
  agenda: string;
  name?: string;
  /** `for`, `against`, a seat, a planet, an objective ...; absent when nothing was voted on. */
  outcome?: string | null;
  votes?: Record<string, number>;
  speaker_decided?: boolean;
  law?: boolean;
  enacted?: boolean;
  discarded?: boolean;
  redirected_to?: string | null;
}

export interface AgendaToast {
  id: string;
  /** One line per fact. */
  text: string;
}

export function readAgendaLog(state: Record<string, unknown> | null | undefined): AgendaRecordDto[] {
  const raw = state?.agenda_log;
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (r): r is AgendaRecordDto =>
      !!r && typeof r === "object" && typeof (r as AgendaRecordDto).agenda === "string",
  );
}

/** A seat's display name, or `null` when `id` is not a seat. */
export type SeatName = (id: string) => string | null;

/** "For", "Against", a player's name, a planet's name, an objective's name. */
export function agendaOutcomeLabel(outcome: string, seatName: SeatName): string {
  const lower = outcome.toLowerCase();
  if (lower === "for") return "For";
  if (lower === "against") return "Against";
  return (
    seatName(outcome) ??
    findPlanetMeta(outcome)?.name ??
    findSecretObjectiveMeta(outcome)?.name ??
    findPublicObjectiveMeta(outcome)?.name ??
    humanizeId(outcome)
  );
}

const isForAgainst = (outcome: string) => /^(for|against)$/i.test(outcome);

/** The toast text for one finished agenda: what won, the votes, and what happens to the card. */
export function agendaResultText(record: AgendaRecordDto, seatName: SeatName): string {
  const name = record.name?.trim() || agendaNameFromId(record.agenda);
  const outcome = record.outcome ?? null;
  if (!outcome) return `${name}\nResolved with no outcome.`;
  const label = agendaOutcomeLabel(outcome, seatName);
  const lines = [`${name}: ${isForAgainst(outcome) ? `${label} wins` : `${label} elected`}`];

  const tally = Object.entries(record.votes ?? {})
    .filter(([, n]) => n > 0)
    .sort((a, b) => b[1] - a[1]);
  const total = tally.reduce((sum, [, n]) => sum + n, 0);
  const counts = tally.map(([o, n]) => `${agendaOutcomeLabel(o, seatName)} ${n}`).join(" · ");
  if (record.speaker_decided) {
    lines.push(total === 0 ? "No votes cast: the speaker decided." : `Tied (${counts}): the speaker decided.`);
  } else if (counts) {
    lines.push(`Votes: ${counts}`);
  }

  if (record.discarded) lines.push("Discarded by Deadly Plot: no effect.");
  else if (record.law) lines.push(record.enacted ? "The law enters play." : "The law is discarded.");
  else lines.push("Directive resolved.");
  if (record.redirected_to && !record.discarded)
    lines.push(`Its effect goes to ${agendaOutcomeLabel(record.redirected_to, seatName)} instead.`);
  return lines.join("\n");
}

/**
 * Toasts for agenda records the viewer has not been told about; `from` is how many were already
 * known. Every seat gets them, the voters included: the outcome is news to the whole table.
 */
export function agendaToasts(
  log: readonly AgendaRecordDto[],
  from: number,
  seatName: SeatName,
): { toasts: AgendaToast[]; next: number } {
  const toasts: AgendaToast[] = [];
  for (let i = Math.min(from, log.length); i < log.length; i++) {
    const record = log[i];
    toasts.push({ id: `agenda:${i}:${record.round ?? 0}:${record.agenda}`, text: agendaResultText(record, seatName) });
  }
  return { toasts, next: log.length };
}
