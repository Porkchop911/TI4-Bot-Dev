import { activeOfflineGame, getOfflineLauncher } from "./registry.ts";
import type { FeedbackCategory, QueuedFeedbackReport } from "./types.ts";

export type RoutedFeedbackResult =
  | { ok: true; id: string; queued?: boolean }
  | { ok: false; message: string };

/** A fresh report id (8 to 64 characters of A-Za-z0-9_-): what makes a retried send idempotent. */
export function newReportId(): string {
  const bytes = new Uint8Array(12);
  crypto.getRandomValues(bytes);
  return `r_${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

/**
 * The JSON body the queue posts to `/api/feedback` for a queued report: the local replay data
 * goes into `context.local`, and `report_id` makes a retry store the report only once (the
 * answer has `duplicate: true` then). Same shape the server validates.
 */
export function feedbackRequestBody(report: QueuedFeedbackReport): Record<string, unknown> {
  const { category, text, context, local, reportId } = report;
  return {
    category,
    text,
    context: local ? { ...context, local } : context,
    ...(reportId ? { report_id: reportId } : {}),
  };
}

/**
 * Sends a report like the online sheet does, except that during an offline game it is put into
 * the launcher's queue (resolves at once, `queued: true`) and goes out in the background. The
 * game never waits for the network.
 */
export async function routeFeedback(
  body: { category: FeedbackCategory; text: string; context: Record<string, unknown> },
  online: (body: {
    category: FeedbackCategory;
    text: string;
    context: Record<string, unknown>;
  }) => Promise<RoutedFeedbackResult>,
): Promise<RoutedFeedbackResult> {
  const game = activeOfflineGame();
  const launcher = getOfflineLauncher();
  if (!game || !launcher) return online(body);
  try {
    const item = await launcher.feedback.enqueue({ ...body, local: game.local(), reportId: newReportId() });
    return { ok: true, id: item.localId, queued: true };
  } catch {
    return { ok: false, message: "Could not save the report on this device. Try again." };
  }
}
