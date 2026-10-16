import React from "react";
import { PdsTurretGlyph, DockRingGlyph } from "../../../src/components/StructureIcon.tsx";

/** The candidate glyphs of the icon gallery. (a) of each is the chosen one and lives in production. */
export interface Candidate {
  id: string;
  name: string;
  kind: "pds" | "spacedock";
  chosen?: boolean;
  glyph: React.FC;
}

const PdsDome: React.FC = () => (
  <g>
    <path
      fillRule="evenodd"
      d="M3 20a9 9 0 0 1 18 0zm3.3 0a5.7 5.7 0 0 1 11.4 0z"
    />
    <rect x="10.9" y="5" width="2.2" height="10" />
    <rect x="9" y="15.5" width="6" height="4.5" />
    <rect x="2" y="20.5" width="20" height="2" />
  </g>
);

const PdsDish: React.FC = () => (
  <g>
    <path d="M5 3.5h14a7 7 0 0 1-14 0z" />
    <rect x="11" y="10.5" width="2" height="3.5" />
    <rect x="11" y="1.5" width="2" height="2" />
    <path
      fillRule="evenodd"
      d="M6 21l-3-4 3-4h12l3 4-3 4zm3.2-5.2v2.4h5.6v-2.4z"
    />
  </g>
);

const DockHangar: React.FC = () => (
  <g>
    <path
      fillRule="evenodd"
      d="M12 2l8.7 5v10L12 22l-8.7-5V7zm0 3.2L6.1 8.6v6.8L12 18.8l5.9-3.4V8.6z"
    />
    <path d="M12 7.6l3.2 6.6-3.2-1.5-3.2 1.5z" />
  </g>
);

const DockGantry: React.FC = () => (
  <g>
    <rect x="2" y="3" width="20" height="2.6" />
    <rect x="3.5" y="5.6" width="2" height="12" />
    <rect x="18.5" y="5.6" width="2" height="12" />
    <rect x="11" y="5.6" width="2" height="5" />
    <path d="M8.6 10.6h6.8l-1.2 4.4H9.8z" />
    <rect x="2" y="18" width="20" height="3" />
  </g>
);

export const CANDIDATES: Candidate[] = [
  { id: "pds-a", name: "PDS (a) Turret on a base", kind: "pds", chosen: true, glyph: PdsTurretGlyph },
  { id: "pds-b", name: "PDS (b) Shield dome with barrel", kind: "pds", glyph: PdsDome },
  { id: "pds-c", name: "PDS (c) Radar dish on a hex bunker", kind: "pds", glyph: PdsDish },
  { id: "dock-a", name: "Space dock (a) Ring station with docking arms", kind: "spacedock", chosen: true, glyph: DockRingGlyph },
  { id: "dock-b", name: "Space dock (b) Hex hangar with a ship", kind: "spacedock", glyph: DockHangar },
  { id: "dock-c", name: "Space dock (c) Gantry orbital platform", kind: "spacedock", glyph: DockGantry },
];
