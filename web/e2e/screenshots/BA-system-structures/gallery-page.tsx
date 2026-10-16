import React from "react";
import { createRoot } from "react-dom/client";
import { StructureIcon } from "../../../src/components/StructureIcon.tsx";
import { CANDIDATES } from "./candidates.tsx";

// Three owner colours from the app's colour-blind-safe palette; each candidate at 16, 20 and 28 px
// on a dark and a light background. Static markup, no app needed.
const COLOURS = ["#E69F00", "#56B4E9", "#CC79A7"];
const SIZES = [16, 20, 28];

const Cell: React.FC<{ bg: string; fg: string; glyph: React.FC; kind: "pds" | "spacedock" }> = ({ bg, fg, glyph, kind }) => (
  <div style={{ background: bg, color: fg, padding: "8px 10px", borderRadius: 6, display: "flex", alignItems: "center", gap: 12 }}>
    {COLOURS.map((c) => (
      <span key={c} style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
        {SIZES.map((s) => (
          <StructureIcon key={s} kind={kind} glyph={glyph} size={s} color={c} />
        ))}
      </span>
    ))}
  </div>
);

const Gallery: React.FC = () => (
  <div style={{ fontFamily: "system-ui, sans-serif", background: "#0b1220", color: "#e2e8f0", padding: 20, width: 880 }}>
    {CANDIDATES.map((c) => (
      <div key={c.id} style={{ marginBottom: 14 }}>
        <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 6 }}>
          {c.name}
          {c.chosen && (
            <span style={{ marginLeft: 10, fontSize: 12, background: "#16a34a", color: "#fff", padding: "2px 8px", borderRadius: 10 }}>
              chosen
            </span>
          )}
        </div>
        <div style={{ display: "flex", gap: 12 }}>
          <Cell bg="#0f172a" fg="#e2e8f0" glyph={c.glyph} kind={c.kind} />
          <Cell bg="#f1f5f9" fg="#0f172a" glyph={c.glyph} kind={c.kind} />
        </div>
      </div>
    ))}
    <div style={{ fontSize: 12, color: "#94a3b8" }}>Each cell: three owner colours, each at 16, 20 and 28 px.</div>
  </div>
);

createRoot(document.getElementById("root")!).render(<Gallery />);
