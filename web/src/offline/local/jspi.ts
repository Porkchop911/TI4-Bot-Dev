export const JSPI_MISSING =
  "This browser cannot run the offline engine (it needs WebAssembly JSPI). Use Chrome or Edge 137 or newer.";

/** WebAssembly JSPI (Suspending / promising): Chrome and Edge 137 and later, Node 24. */
export function jspiSupported(): boolean {
  const wa = WebAssembly as unknown as Record<string, unknown>;
  return typeof wa.Suspending === "function" && typeof wa.promising === "function";
}
