import React from "react";
import { ANOMALY_NAMES, anomalyKinds, type AnomalyKind } from "./anomalyKind.ts";

const INK = "var(--anomaly-ink, #e2e8f0)";

/**
 * Flat glyphs in a 24x24 box (centre 12,12), one silhouette per anomaly so they differ by shape,
 * not by colour: angular rocks, stacked cloud lobes, a ray star, a swirl, a jagged crack.
 */
function Glyph({ kind }: { kind: AnomalyKind }) {
  switch (kind) {
    case "asteroid field":
      return (
        <g fill={INK}>
          <polygon points="3,9 7,4 12,5 13,10 8,13" />
          <polygon points="14,12 19,10 22,15 18,20 13,18" />
          <polygon points="4,17 8,15 10,19 7,22 3,21" />
        </g>
      );
    case "nebula":
      return (
        <g fill={INK}>
          <circle cx="8" cy="14" r="5" />
          <circle cx="15" cy="11" r="6" />
          <circle cx="17" cy="17" r="4" />
          <rect x="4" y="15" width="16" height="5" rx="2.5" />
        </g>
      );
    case "supernova":
      return (
        <g fill={INK} stroke={INK} strokeWidth="1.6" strokeLinecap="round">
          <circle cx="12" cy="12" r="3.6" stroke="none" />
          <path d="M12 1.5V6M12 18v4.5M1.5 12H6M18 12h4.5M4.6 4.6l3.1 3.1M16.3 16.3l3.1 3.1M19.4 4.6l-3.1 3.1M7.7 16.3l-3.1 3.1" />
        </g>
      );
    case "gravity rift":
      return (
        <g fill="none" stroke={INK} strokeWidth="2" strokeLinecap="round">
          <path d="M12 12m0 0a1.5 1.5 0 1 1 1.5 1.5a3 3 0 1 1-3-3a4.5 4.5 0 0 1 4.5 4.5" />
          <path d="M12.5 5a7 7 0 0 1 6.5 6M11.5 19a7 7 0 0 1-6.5-6" />
          <path d="M20 14.5a9 9 0 0 1-3 4M4 9.5a9 9 0 0 1 3-4" />
        </g>
      );
    case "entropic scar":
      return (
        <g fill="none" stroke={INK} strokeWidth="2.2" strokeLinejoin="miter" strokeLinecap="round">
          <polyline points="3,5 10,10 7,13 14,16 12,21" />
          <polyline points="14,16 21,14" />
        </g>
      );
  }
}

/** A small icon for badges, legends and the inspector. Decorative unless `title` is given. */
export const AnomalyIcon: React.FC<{ kind: AnomalyKind; size?: number; x?: number; y?: number }> = ({
  kind,
  size = 14,
  x = 0,
  y = 0,
}) => (
  <svg
    x={x}
    y={y}
    width={size}
    height={size}
    viewBox="0 0 24 24"
    data-testid={`anomaly-icon-${kind.replace(" ", "-")}`}
    aria-hidden="true"
    focusable="false"
  >
    <Glyph kind={kind} />
  </svg>
);

/** Stand-alone HTML icon with an accessible name. */
export const AnomalyIconInline: React.FC<{ kind: AnomalyKind; size?: number }> = ({ kind, size = 16 }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    role="img"
    aria-label={ANOMALY_NAMES[kind]}
    style={{ verticalAlign: "middle", flex: "none" }}
  >
    <title>{ANOMALY_NAMES[kind]}</title>
    <Glyph kind={kind} />
  </svg>
);

const SHORT: Record<AnomalyKind, string> = {
  "asteroid field": "Asteroid",
  nebula: "Nebula",
  supernova: "Supernova",
  "gravity rift": "Rift",
  "entropic scar": "Scar",
};

const rot = (deg: number) => `rotate(${deg})`;

/** Large background texture for the outer ring of a hex (radius ~44..64), centred on 0,0. */
function Texture({ kind }: { kind: AnomalyKind }) {
  switch (kind) {
    case "asteroid field": {
      const rocks: [number, number, number, number][] = [
        [-40, -34, 7, 10], [38, -36, 6, 70], [-52, 6, 8, 130], [50, 10, 7, 200],
        [-34, 42, 6, 20], [34, 44, 8, 300], [4, 56, 5, 90], [-14, -56, 5, 160], [18, -58, 4, 250],
      ];
      return (
        <g fill={INK}>
          {rocks.map(([x, y, s, a], i) => (
            <polygon
              key={i}
              transform={`translate(${x} ${y}) ${rot(a)}`}
              points={`${-s},${-s * 0.3} ${-s * 0.3},${-s} ${s * 0.8},${-s * 0.6} ${s},${s * 0.4} ${-s * 0.2},${s}`}
            />
          ))}
        </g>
      );
    }
    case "nebula":
      return (
        <g fill={INK}>
          {[[-44, 30, 17], [-30, 46, 14], [44, -26, 16], [32, -44, 13], [50, 22, 11], [-50, -22, 11]].map(
            ([x, y, r], i) => (
              <circle key={i} cx={x} cy={y} r={r} />
            ),
          )}
        </g>
      );
    case "supernova":
      return (
        <g stroke={INK} strokeWidth="3" strokeLinecap="round">
          {Array.from({ length: 12 }, (_, i) => {
            const a = (i * 30 + 15) * (Math.PI / 180);
            const r0 = 46;
            const r1 = i % 2 ? 56 : 64;
            return (
              <line
                key={i}
                x1={(r0 * Math.cos(a)).toFixed(1)}
                y1={(r0 * Math.sin(a)).toFixed(1)}
                x2={(r1 * Math.cos(a)).toFixed(1)}
                y2={(r1 * Math.sin(a)).toFixed(1)}
              />
            );
          })}
        </g>
      );
    case "gravity rift":
      return (
        <g fill="none" stroke={INK} strokeWidth="2.4" strokeLinecap="round">
          <path d="M-52,-20 A56,56 0 0 1 20,-52" />
          <path d="M52,20 A56,56 0 0 1 -20,52" />
          <path d="M-44,10 A46,46 0 0 0 -8,44" />
          <path d="M44,-10 A46,46 0 0 0 8,-44" />
        </g>
      );
    case "entropic scar":
      return (
        <g fill="none" stroke={INK} strokeWidth="3" strokeLinejoin="miter">
          <polyline points="-56,-26 -44,-14 -50,-6 -38,6 -44,16" />
          <polyline points="56,26 44,14 50,6 38,-6 44,-16" />
        </g>
      );
  }
}

/**
 * Background art for a hex: sits in the outer ring behind labels, planets and units, at low
 * opacity and never captures pointer events. With two anomalies each takes one half-plane.
 */
export const AnomalyArt: React.FC<{ anomalies: readonly string[] | undefined; cx: number; cy: number }> = ({
  anomalies,
  cx,
  cy,
}) => {
  const kinds = anomalyKinds(anomalies);
  if (kinds.length === 0) return null;
  const names = kinds.map((k) => ANOMALY_NAMES[k]).join(" and ");
  return (
    <g
      data-testid="anomaly-art"
      data-anomaly-kinds={kinds.join(",")}
      transform={`translate(${cx} ${cy})`}
      opacity={0.3}
      pointerEvents="none"
      role="img"
      aria-label={names}
    >
      <title>{names}</title>
      {kinds.map((k, i) => (
        <g key={k} clipPath={kinds.length > 1 ? `url(#anomaly-half-${i})` : undefined}>
          <Texture kind={k} />
        </g>
      ))}
    </g>
  );
};

/** Clip paths used when a system carries two anomalies; render once inside the board's <defs>. */
export const AnomalyClipDefs: React.FC = () => (
  <>
    <clipPath id="anomaly-half-0">
      <rect x="-80" y="-80" width="160" height="80" />
    </clipPath>
    <clipPath id="anomaly-half-1">
      <rect x="-80" y="0" width="160" height="80" />
    </clipPath>
  </>
);

/** The small badge row under the system number: icons plus names ("Nebula"). */
export const AnomalyBadge: React.FC<{ anomalies: readonly string[] | undefined; cx: number; y: number }> = ({
  anomalies,
  cx,
  y,
}) => {
  const kinds = anomalyKinds(anomalies);
  if (kinds.length === 0) return null;
  const text = kinds.length > 1 ? kinds.map((k) => SHORT[k]).join(" + ") : ANOMALY_NAMES[kinds[0]];
  const dual = kinds.length > 1;
  const fontSize = dual ? 7 : 8;
  const textW = text.length * (dual ? 4.9 : 6);
  const iconW = 12 * kinds.length + 2;
  const total = iconW + textW;
  const left = cx - total / 2;
  return (
    <g data-testid="anomaly-badge" pointerEvents="none">
      {kinds.map((k, i) => (
        <AnomalyIcon key={k} kind={k} size={11} x={left + i * 12} y={y - 9} />
      ))}
      <text
        x={left + iconW}
        y={y}
        fill="#fef08a"
        fontSize={fontSize}
        fontWeight="bold"
        letterSpacing={dual ? 0.3 : 0.6}
      >
        {text.toUpperCase()}
      </text>
    </g>
  );
};
