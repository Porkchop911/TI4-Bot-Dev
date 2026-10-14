/**
 * Network resilience shared by the lobby poll, the presence heartbeat and the game session:
 * which failures are transient, how long to wait between attempts, a retrying fetch for idempotent
 * requests, human wording for network errors, and one subscription for "the page came back"
 * (tab visible again, bfcache restore, network online, window focus).
 *
 * Only network-level failures (fetch rejects with a TypeError) and 502/503/504 from the proxy are
 * transient. A 4xx or an ordinary 500 is the server's real answer and is never retried here.
 */

/** Thrown after every attempt of an idempotent request failed at the network level. */
export class ServerUnreachableError extends Error {
  constructor(
    message = UNREACHABLE_MESSAGE,
    readonly cause?: unknown,
  ) {
    super(message);
    this.name = "ServerUnreachableError";
  }
}

export const UNREACHABLE_MESSAGE =
  "Cannot reach the server. Check your connection; your game is not lost.";
export const GONE_MESSAGE =
  "This game was not found on the server. It may have been removed, or the server was reset.";

export const TERMINATED_MESSAGE = "This game was ended by its host.";

/** `fetch` rejected before any HTTP answer: offline, tunnel down, DNS, connection reset, CORS. */
export function isNetworkError(error: unknown): boolean {
  if (error instanceof ServerUnreachableError) return true;
  if (error instanceof TypeError) return true; // "Failed to fetch", "Load failed", "NetworkError ..."
  if (error instanceof DOMException) return error.name === "NetworkError" || error.name === "TimeoutError";
  return false;
}

/** Statuses a reverse proxy answers with while the backend is down or restarting. */
export function isTransientStatus(status: number): boolean {
  return status === 502 || status === 503 || status === 504;
}

/**
 * The vite dev proxy answers an unreachable backend with a bare 500 (no body). A real server
 * error always carries a message, so an empty-bodied 500 counts as transient too.
 */
export async function isTransientResponse(response: Response): Promise<boolean> {
  if (isTransientStatus(response.status)) return true;
  if (response.status !== 500) return false;
  try {
    return (await response.clone().text()).trim() === "";
  } catch {
    return false;
  }
}

export interface BackoffOptions {
  /** Delay before the first retry (default 1000 ms). */
  base?: number;
  /** Upper bound of any delay (default 15000 ms). */
  cap?: number;
  /** Source of jitter in [0,1); injectable for tests. */
  random?: () => number;
}

/**
 * Exponential backoff with jitter: attempt 1 waits in [base/2, base], attempt 2 in [base, 2*base],
 * ... never more than `cap`. Jitter keeps a whole table of phones from reconnecting in lockstep.
 */
export function backoffDelay(attempt: number, options: BackoffOptions = {}): number {
  const { base = 1_000, cap = 15_000, random = Math.random } = options;
  const exponential = Math.min(cap, base * 2 ** Math.max(0, attempt - 1));
  return Math.round(exponential * (0.5 + 0.5 * random()));
}

export interface RetryOptions extends BackoffOptions {
  /** Total tries including the first (default 4). */
  attempts?: number;
  sleep?: (ms: number) => Promise<void>;
  /** Stop retrying (the owner went away). */
  cancelled?: () => boolean;
  onRetry?: (attempt: number, cause: unknown) => void;
}

const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Runs an IDEMPOTENT request, retrying with backoff while it fails transiently. Returns the first
 * response that is not transient (including 4xx and ordinary 5xx: the caller reports those). When
 * every attempt failed it throws {@link ServerUnreachableError}. Never use for a request whose
 * repetition could apply twice.
 */
export async function fetchWithRetry(
  send: () => Promise<Response>,
  options: RetryOptions = {},
): Promise<Response> {
  const { attempts = 4, sleep = defaultSleep, cancelled, onRetry } = options;
  let cause: unknown;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    try {
      const response = await send();
      if (!(await isTransientResponse(response))) return response;
      cause = new Error(`HTTP ${response.status}`);
    } catch (error) {
      if (!isNetworkError(error)) throw error;
      cause = error;
    }
    if (attempt === attempts || cancelled?.()) break;
    onRetry?.(attempt, cause);
    await sleep(backoffDelay(attempt, options));
    if (cancelled?.()) break;
  }
  throw new ServerUnreachableError(UNREACHABLE_MESSAGE, cause);
}

/** Plain wording for an error caught around a request; never "TypeError: Failed to fetch". */
export function describeError(error: unknown, whatFailed?: string): string {
  if (isNetworkError(error)) return UNREACHABLE_MESSAGE;
  const text = error instanceof Error ? error.message : String(error);
  return whatFailed ? `${whatFailed}: ${text}` : text;
}

export type ResumeReason = "visible" | "pageshow" | "online" | "focus";

export interface ResumeEvent {
  reason: ResumeReason;
  /** How long the page was hidden/offline before this event (ms; 0 when it never was). */
  awayMs: number;
}

/** Quiet period during which one comeback (focus + visibilitychange + pageshow together) counts once. */
const RESUME_DEBOUNCE_MS = 400;

/**
 * Calls `listener` when the page has plausibly been away and is back: `visibilitychange` to
 * visible, `pageshow` (bfcache restore), `online`, window `focus`. Events that arrive together
 * collapse into one call; the strongest reason (`online`, `pageshow`) wins over `focus`.
 * Returns the unsubscribe function.
 */
export function onResume(listener: (event: ResumeEvent) => void, now: () => number = Date.now): () => void {
  let awaySince: number | null = document.visibilityState === "hidden" ? now() : null;
  let last = -Infinity;
  let pendingReason: ResumeReason | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const away = () => {
    awaySince ??= now();
  };
  const fire = () => {
    timer = null;
    const reason = pendingReason ?? "focus";
    pendingReason = null;
    const stamp = now();
    const awayMs = awaySince === null ? 0 : Math.max(0, stamp - awaySince);
    awaySince = null;
    last = stamp;
    listener({ reason, awayMs });
  };
  const back = (reason: ResumeReason) => {
    if (reason === "focus" && now() - last < RESUME_DEBOUNCE_MS) return;
    const strong = reason === "online" || reason === "pageshow";
    if (!pendingReason || strong) pendingReason = reason;
    if (timer === null) timer = setTimeout(fire, 0);
  };
  const onVisibility = () => {
    if (document.visibilityState === "hidden") away();
    else back("visible");
  };
  const onPageShow = (event: Event) => {
    if ((event as PageTransitionEvent).persisted) back("pageshow");
  };
  const onOnline = () => back("online");
  const onOffline = () => away();
  const onFocus = () => back("focus");
  const onBlur = () => away();
  document.addEventListener("visibilitychange", onVisibility);
  window.addEventListener("pageshow", onPageShow);
  window.addEventListener("online", onOnline);
  window.addEventListener("offline", onOffline);
  window.addEventListener("focus", onFocus);
  window.addEventListener("blur", onBlur);
  window.addEventListener("pagehide", away);
  return () => {
    if (timer !== null) clearTimeout(timer);
    document.removeEventListener("visibilitychange", onVisibility);
    window.removeEventListener("pageshow", onPageShow);
    window.removeEventListener("online", onOnline);
    window.removeEventListener("offline", onOffline);
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("blur", onBlur);
    window.removeEventListener("pagehide", away);
  };
}
