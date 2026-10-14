import {
  findPublicObjectiveMeta,
  findSecretObjectiveMeta,
  getPublicObjectiveMeta,
  getSecretObjectiveMeta,
  type ObjectiveMeta,
} from "../protocol/contentCatalog.ts";

/** Prefix of a Plots Within Plots option: `plot|<secret>|<seat whose token goes on it>`. */
const PLOT_PREFIX = "plot|";

/** A scoring option that scores a secret objective, read from its id. */
export interface SecretScoringOption {
  /** The secret objective's alias. */
  secretId: string;
  meta: ObjectiveMeta;
  /** Plots Within Plots: the seat whose control token goes on the plot (scores no point). */
  plotToken: string | null;
}

/**
 * `null` for a public objective (or an unknown id); otherwise the secret it scores. A plain id is a
 * secret when only the secret objective catalog knows it.
 */
export function readSecretScoringOption(optionId: string): SecretScoringOption | null {
  if (optionId.startsWith(PLOT_PREFIX)) {
    const [secretId, token] = optionId.slice(PLOT_PREFIX.length).split("|");
    if (!secretId) return null;
    return { secretId, meta: getSecretObjectiveMeta(secretId), plotToken: token || null };
  }
  if (findPublicObjectiveMeta(optionId)) return null;
  const meta = findSecretObjectiveMeta(optionId);
  return meta ? { secretId: optionId, meta, plotToken: null } : null;
}

/**
 * Name, points and text of any objective a scoring window can name: a public objective, a secret
 * (also one made public by Classified Document Leaks) or a plot option.
 */
export function scoringObjectiveMeta(id: string): ObjectiveMeta {
  const secret = readSecretScoringOption(id);
  if (secret) return secret.meta;
  return findPublicObjectiveMeta(id) ?? findSecretObjectiveMeta(id) ?? getPublicObjectiveMeta(id);
}

/** "Status phase", "Action phase", ...: when a secret is scored, as printed. */
export function secretPhaseLabel(meta: ObjectiveMeta): string {
  const phase = meta.phase.trim();
  if (!phase || phase.toLowerCase() === "secret") return "Secret objective";
  return `${phase} phase`;
}
