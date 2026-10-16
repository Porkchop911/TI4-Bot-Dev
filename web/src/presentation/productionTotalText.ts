/** The wording of the production total, shared by the builder (first dialog) and the payment panel. */
export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;

/** "Total: 7 resources — 3 builds"; `partial` marks a total that does not cover every build yet. */
export function productionTotalText(total: number, builds: number, partial = false): string {
  return `Total${partial ? " so far" : ""}: ${plural(total, "resource")} — ${plural(builds, "build")}`;
}
