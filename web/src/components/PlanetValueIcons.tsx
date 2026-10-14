import React from "react";
import "./PlanetValueIcons.css";

/**
 * One icon for resources and one for influence, used everywhere a planet or economy value is
 * shown. Visible text is the icon plus the number; screen readers and tooltips get the full word
 * ("3 resources", "1 influence", "2 of 5 resources").
 *
 * Colours come from the `--value-resource` / `--value-influence` tokens in PlanetValueIcons.css (dark by
 * default, light under `[data-theme="light"]`; the app has no light theme yet).
 */
export type PlanetValueKind = "resources" | "influence";
export type PlanetValueSize = "inline" | "bar" | "tooltip";
/** ready: normal; spent: exhausted/used up (dimmed); muted: zero or unavailable. */
export type PlanetValueState = "ready" | "spent" | "muted";

export const RESOURCE_PATH = "M8 1 L14 7 L8 15 L2 7 Z";
export const INFLUENCE_PATH = "M2 13h12v1.5H2zm1.5-2l2-6 2.5 3 2.5-3 2 6H3.5z";

const KIND_PATH: Record<PlanetValueKind, string> = {
  resources: RESOURCE_PATH,
  influence: INFLUENCE_PATH,
};
/** Each path is cropped to its own square so both icons fill the same visual box. */
const KIND_VIEWBOX: Record<PlanetValueKind, string> = {
  resources: "1 0.5 14 15",
  influence: "1.5 3.5 13 13",
};

/** "Resources" / "Influence" (a payment currency), "R" / "I" or the kind itself, normalised. */
export function valueKind(currency: string | null | undefined): PlanetValueKind {
  const c = String(currency ?? "").toLowerCase();
  return c.startsWith("inf") || c === "i" ? "influence" : "resources";
}

/** The word for a count: "1 resource", "0 resources", "3 influence", "2 of 5 resources". */
export function valueLabel(kind: PlanetValueKind, value: number, total?: number): string {
  const word = kind === "influence" ? "influence" : value === 1 && total === undefined ? "resource" : "resources";
  return total !== undefined && total !== value ? `${value} of ${total} ${word}` : `${value} ${word}`;
}

interface IconProps {
  style?: React.CSSProperties;
  className?: string;
}

const Icon: React.FC<IconProps & { kind: PlanetValueKind }> = ({ kind, style, className }) => (
  <svg
    viewBox={KIND_VIEWBOX[kind]}
    fill="currentColor"
    aria-hidden="true"
    focusable="false"
    className={`planet-value__icon planet-value__icon--${kind} ${className ?? ""}`.trim()}
    style={style}
  >
    <path d={KIND_PATH[kind]} />
  </svg>
);

/** The bare resource icon (decorative; pair it with text or use `PlanetValue`). */
export const ResourceIcon: React.FC<IconProps> = (props) => <Icon kind="resources" {...props} />;
/** The bare influence icon (decorative; pair it with text or use `PlanetValue`). */
export const InfluenceIcon: React.FC<IconProps> = (props) => <Icon kind="influence" {...props} />;

export interface PlanetValueProps {
  kind: PlanetValueKind;
  value: number;
  /** When given and different from `value`, shows `value/total` (ready of total). */
  total?: number;
  /** Show `value/total` even when they are equal (the player sheet always does). */
  alwaysTotal?: boolean;
  size?: PlanetValueSize;
  state?: PlanetValueState;
  /** Prefix such as "+" or "-" shown before the number. */
  sign?: "+" | "-";
  /** Overrides the accessible text, e.g. "Ready: 3 of 5 resources". */
  label?: string;
  className?: string;
  testId?: string;
}

/** Icon + number, with the full word as aria-label and tooltip. */
export const PlanetValue: React.FC<PlanetValueProps> = ({
  kind,
  value,
  total,
  alwaysTotal,
  size = "inline",
  state,
  sign,
  label,
  className,
  testId,
}) => {
  const text = label ?? `${sign === "-" ? "minus " : sign === "+" ? "plus " : ""}${valueLabel(kind, value, total)}`;
  const tone = state ?? (value === 0 ? "muted" : "ready");
  return (
    <span
      role="img"
      aria-label={text}
      title={text}
      data-testid={testId}
      data-kind={kind}
      data-state={tone}
      className={`planet-value planet-value--${kind} planet-value--${size} planet-value--${tone} ${className ?? ""}`.trim()}
    >
      <Icon kind={kind} />
      <span className="planet-value__num" aria-hidden="true">
        {sign}
        {value}
        {total !== undefined && (alwaysTotal || total !== value) ? (
          <span className="planet-value__total">/{total}</span>
        ) : null}
      </span>
    </span>
  );
};

/**
 * Just the icon, standing in for the word after a number that is shown elsewhere ("3 / 5 [icon]").
 * Screen readers and the tooltip still get the word.
 */
export const ValueUnit: React.FC<{ kind: PlanetValueKind; size?: PlanetValueSize; className?: string }> = ({
  kind,
  size = "inline",
  className,
}) => (
  <span
    role="img"
    aria-label={kind}
    title={kind}
    data-kind={kind}
    className={`planet-value planet-value--${kind} planet-value--${size} ${className ?? ""}`.trim()}
  >
    <Icon kind={kind} />
  </span>
);

export interface PlanetValuePairProps {
  resources: number;
  influence: number;
  size?: PlanetValueSize;
  state?: PlanetValueState;
  className?: string;
  testId?: string;
}

/** Resources then influence, e.g. a planet's printed values. */
export const PlanetValuePair: React.FC<PlanetValuePairProps> = ({
  resources,
  influence,
  size = "inline",
  state,
  className,
  testId,
}) => (
  <span className={`planet-value-pair ${className ?? ""}`.trim()} data-testid={testId}>
    <PlanetValue kind="resources" value={resources} size={size} state={state} />
    <PlanetValue kind="influence" value={influence} size={size} state={state} />
  </span>
);

const VALUE_TEXT = /(\d+)R\/(\d+)I\b|\b(\d+) (resources?|influence)\b/gi;

/**
 * Text from our own presentation models ("Cost: 3 resources", "Lodor (3R/1I)") with every
 * "N resources", "N influence" and "NR/MI" swapped for icon + number. Other words stay as written;
 * free text from the engine or content should not go through this.
 */
export const ValueText: React.FC<{ text: string; size?: PlanetValueSize }> = ({ text, size = "inline" }) => {
  const parts: React.ReactNode[] = [];
  let last = 0;
  let key = 0;
  for (const m of text.matchAll(VALUE_TEXT)) {
    const at = m.index ?? 0;
    if (at > last) parts.push(text.slice(last, at));
    if (m[1] !== undefined) {
      parts.push(<PlanetValuePair key={key++} resources={Number(m[1])} influence={Number(m[2])} size={size} />);
    } else {
      parts.push(<PlanetValue key={key++} kind={valueKind(m[4])} value={Number(m[3])} size={size} />);
    }
    last = at + m[0].length;
  }
  if (last === 0) return <>{text}</>;
  if (last < text.length) parts.push(text.slice(last));
  return <>{parts}</>;
};

/**
 * A unit's build cost from `formatUnitCost` / `costText`: "3", "2 for 1" (two to a resource) or
 * "Free". A number becomes icon + number; "2 for 1" keeps the batch size as text and puts the
 * icon on the price. Anything else stays text.
 */
export const CostValue: React.FC<{ text: string; size?: PlanetValueSize }> = ({ text, size = "inline" }) => {
  const batch = /^(\d+) for (\d+)$/.exec(text);
  if (batch) {
    return (
      <>
        {batch[1]} for <PlanetValue kind="resources" value={Number(batch[2])} size={size} />
      </>
    );
  }
  if (/^\d+(\.\d+)?$/.test(text)) return <PlanetValue kind="resources" value={Number(text)} size={size} />;
  return <>{text}</>;
};

/** Plain-text equivalent of a pair, for aria-labels and titles of containers. */
export function pairLabel(resources: number, influence: number): string {
  return `${valueLabel("resources", resources)}, ${valueLabel("influence", influence)}`;
}

/**
 * The same glyph for use inside SVG (the map): draws at (x, y) top-left, `size` px square, with a
 * <title> carrying the full word. Fixed hexes because map cards have their own fills.
 */
export const PlanetValueGlyph: React.FC<{
  kind: PlanetValueKind;
  x: number;
  y: number;
  size?: number;
  fill?: string;
  title?: string;
}> = ({ kind, x, y, size = 12, fill, title }) => (
  <svg x={x} y={y} width={size} height={size} viewBox={KIND_VIEWBOX[kind]} aria-hidden={title ? undefined : true}>
    {title && <title>{title}</title>}
    <path d={KIND_PATH[kind]} fill={fill ?? (kind === "influence" ? "#38bdf8" : "#fbbf24")} />
  </svg>
);
