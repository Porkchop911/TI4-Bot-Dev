import React from "react";

/**
 * Icons for the two planetary structures. Monochrome silhouettes that take the owner colour from
 * `currentColor` (cut-outs use fill-rule evenodd, so they work on any background). The two shapes
 * differ in outline (a tall barrel on a base versus a ring with four arms), so they never rely on
 * colour alone. The glyph set is chosen in STRUCTURE_GLYPHS below: swap a glyph there to change it
 * everywhere. The unchosen candidates live in web/e2e/screenshots/BA-system-structures/candidates.tsx.
 */
export type StructureKind = "pds" | "spacedock";

/** PDS: a cannon turret on a bunker base. */
export const PdsTurretGlyph: React.FC = () => (
  <g>
    <path d="M2 22h20v-4.5l-3-2.5H5l-3 2.5z" />
    <path d="M6.5 15a5.5 5.5 0 0 1 11 0z" />
    <path d="M10.6 11.6 15.2 2.4l3.6 1.8-4.6 9.2z" />
  </g>
);

/** Space dock: a ring station with four docking arms and a hub. */
export const DockRingGlyph: React.FC = () => (
  <g>
    <path
      fillRule="evenodd"
      d="M12 4a8 8 0 1 0 0 16 8 8 0 1 0 0-16zm0 2.8a5.2 5.2 0 1 1 0 10.4 5.2 5.2 0 1 1 0-10.4z"
    />
    <circle cx="12" cy="12" r="2.2" />
    <rect x="10.8" y="1.5" width="2.4" height="4" />
    <rect x="10.8" y="18.5" width="2.4" height="4" />
    <rect x="1.5" y="10.8" width="4" height="2.4" />
    <rect x="18.5" y="10.8" width="4" height="2.4" />
  </g>
);

/** The one place that decides which glyph stands for which structure. */
export const STRUCTURE_GLYPHS: Record<StructureKind, React.FC> = {
  pds: PdsTurretGlyph,
  spacedock: DockRingGlyph,
};

export interface StructureIconProps {
  kind: StructureKind;
  /** Full accessible name, e.g. "PDS II, Sol". Without it the icon is decorative (aria-hidden). */
  label?: string;
  size?: number;
  color?: string;
  /** Glyph override (the screenshot gallery draws every candidate through the same frame). */
  glyph?: React.FC;
  className?: string;
}

export const StructureIcon: React.FC<StructureIconProps> = ({
  kind,
  label,
  size = 20,
  color,
  glyph,
  className,
}) => {
  const Glyph = glyph ?? STRUCTURE_GLYPHS[kind];
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="currentColor"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      className={className}
      data-structure-kind={kind}
      style={{ display: "inline-block", verticalAlign: "middle", flexShrink: 0, color: color || "currentColor" }}
    >
      {label && <title>{label}</title>}
      <Glyph />
    </svg>
  );
};
