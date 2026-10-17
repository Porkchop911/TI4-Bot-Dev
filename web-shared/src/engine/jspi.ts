/**
 * JSPI (WebAssembly.Suspending / promising): Chrome and Edge 137 and later, Node 24. Its own
 * module, so that a page can check for it without loading the host.
 */
export function hasJspi(): boolean {
  const wa = WebAssembly as unknown as Record<string, unknown>;
  return typeof wa.Suspending === "function" && typeof wa.promising === "function";
}
