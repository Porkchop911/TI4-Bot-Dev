// Which browser errors the smoke playthrough may survive. Chromium refuses requests with
// net::ERR_INSUFFICIENT_RESOURCES when the host runs short (parallel runs, a full tmpfs).

const EXHAUSTED = "net::ERR_INSUFFICIENT_RESOURCES";
// Polled every few seconds; the client ignores a failed one and the next tick replaces it.
const PERIODIC = ["/lobby/heartbeat", "/lobby/join"];

/** A console error the playthrough does not count: a refused periodic lobby request. */
export function tolerableConsoleError(text: string, url: string): boolean {
  return text.includes(EXHAUSTED) && url.includes("/api/") && PERIODIC.some((p) => url.includes(p));
}

/** A seat that never loaded only because the browser refused its page modules may be opened again. */
export function startupRetryable(seatErrors: string[]): boolean {
  return (
    seatErrors.length > 0 &&
    seatErrors.every((e) => e.includes(EXHAUSTED) && !e.includes("/api/"))
  );
}
