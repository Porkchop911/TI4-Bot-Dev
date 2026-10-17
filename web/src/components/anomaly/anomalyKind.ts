export type AnomalyKind = "asteroid field" | "nebula" | "supernova" | "gravity rift" | "entropic scar";

export const ANOMALY_NAMES: Record<AnomalyKind, string> = {
  "asteroid field": "Asteroid field",
  nebula: "Nebula",
  supernova: "Supernova",
  "gravity rift": "Gravity rift",
  "entropic scar": "Entropic scar",
};

/** Maps the content's anomaly string ("gravity rift", "scar", "Asteroid_Field") to a known kind. */
export function anomalyKind(raw: string): AnomalyKind | null {
  const k = raw.trim().toLowerCase().replace(/_/g, " ");
  if (k.includes("asteroid")) return "asteroid field";
  if (k.includes("nebula")) return "nebula";
  if (k.includes("supernova")) return "supernova";
  if (k.includes("rift")) return "gravity rift";
  if (k.includes("scar")) return "entropic scar";
  return null;
}

/** Distinct known kinds of a system, in the order given, without duplicates. */
export function anomalyKinds(raw: readonly string[] | undefined): AnomalyKind[] {
  const out: AnomalyKind[] = [];
  for (const r of raw ?? []) {
    const k = anomalyKind(r);
    if (k && !out.includes(k)) out.push(k);
  }
  return out;
}
