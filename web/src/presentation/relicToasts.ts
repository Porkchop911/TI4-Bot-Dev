import { findRelicMeta, humanizeId } from "../protocol/contentCatalog.ts";

/** One entry of the engine's `relic_log`: a relic a player gained, by any route (public). */
export interface RelicGainDto {
  round?: number;
  player: string;
  /** The relic's alias. */
  relic: string;
  /** Victory points the gain itself awarded (the Shard of the Throne: 1); absent for none. */
  victory_points?: number;
}

export interface RelicToast {
  id: string;
  /** The seat that gained it. */
  actor: string;
  /** True for the viewer's own gain: the toast is highlighted. */
  own: boolean;
  /** Lines: what was gained, a short effect summary, and (own gains) the victory point note. */
  text: string;
}

export function readRelicLog(state: Record<string, unknown> | null | undefined): RelicGainDto[] {
  const raw = state?.relic_log;
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (r): r is RelicGainDto =>
      !!r &&
      typeof r === "object" &&
      typeof (r as RelicGainDto).player === "string" &&
      typeof (r as RelicGainDto).relic === "string",
  );
}

/** The first sentence of a relic's card text, without the "ACTION:" marker or quote bars. */
export function relicEffectSummary(text: string | undefined): string {
  const flat = (text ?? "")
    .split("\n")
    .map((line) => line.replace(/^\s*>\s*/, "").replace(/^ACTION:\s*/, "").trim())
    .filter(Boolean)
    .join(" ");
  const first = flat.match(/^.*?[.!?](?=\s|$)/)?.[0] ?? flat;
  return first.trim();
}

/** "gained Nano-Forge", its effect, and whether the gain itself scored. */
export function relicGainText(record: RelicGainDto, own: boolean): string {
  const meta = findRelicMeta(record.relic);
  const name = meta?.name ?? humanizeId(record.relic);
  const lines = [`gained ${name}`];
  const effect = relicEffectSummary(meta?.text);
  if (effect) lines.push(effect);
  const points = record.victory_points ?? 0;
  if (points > 0) lines.push(`+${points} victory point${points === 1 ? "" : "s"} on gain`);
  else if (own) lines.push("no victory point on gain");
  return lines.join("\n");
}

/** Toasts for relic gains the viewer has not been told about; `from` is how many were known. */
export function relicToasts(
  log: readonly RelicGainDto[],
  from: number,
  viewerSeat: string | null | undefined,
): { toasts: RelicToast[]; next: number } {
  const toasts: RelicToast[] = [];
  for (let i = Math.min(from, log.length); i < log.length; i++) {
    const record = log[i];
    const own = !!viewerSeat && record.player === viewerSeat;
    toasts.push({
      id: `relic:${i}:${record.round ?? 0}:${record.player}:${record.relic}`,
      actor: record.player,
      own,
      text: relicGainText(record, own),
    });
  }
  return { toasts, next: log.length };
}
