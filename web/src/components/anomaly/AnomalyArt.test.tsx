import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AnomalyArt, AnomalyBadge } from "./AnomalyArt.tsx";
import { anomalyKind, anomalyKinds } from "./anomalyKind.ts";

describe("anomaly kinds", () => {
  it("maps content strings", () => {
    expect(anomalyKind("Gravity_Rift")).toBe("gravity rift");
    expect(anomalyKind("asteroid field")).toBe("asteroid field");
    expect(anomalyKind("scar")).toBe("entropic scar");
    expect(anomalyKind("weird")).toBeNull();
    expect(anomalyKinds(["nebula", "Nebula", "supernova"])).toEqual(["nebula", "supernova"]);
  });
});

describe("AnomalyArt", () => {
  const wrap = (el: React.ReactNode) => render(<svg>{el}</svg>);
  it.each(["asteroid field", "nebula", "supernova", "gravity rift", "entropic scar"])(
    "draws %s with an accessible name and no pointer capture",
    (kind) => {
      wrap(<AnomalyArt anomalies={[kind]} cx={0} cy={0} />);
      const art = screen.getByTestId("anomaly-art");
      expect(art).toHaveAttribute("aria-label");
      expect(art).toHaveAttribute("pointer-events", "none");
    },
  );
  it("draws nothing without anomalies", () => {
    wrap(<AnomalyArt anomalies={[]} cx={0} cy={0} />);
    expect(screen.queryByTestId("anomaly-art")).toBeNull();
  });
  it("names both anomalies of a dual system", () => {
    wrap(
      <>
        <AnomalyArt anomalies={["nebula", "gravity rift"]} cx={0} cy={0} />
        <AnomalyBadge anomalies={["nebula", "gravity rift"]} cx={0} y={0} />
      </>,
    );
    expect(screen.getByTestId("anomaly-art")).toHaveAttribute("aria-label", "Nebula and Gravity rift");
    expect(screen.getByTestId("anomaly-badge")).toHaveTextContent("NEBULA + RIFT");
    expect(screen.getByTestId("anomaly-icon-nebula")).toBeInTheDocument();
    expect(screen.getByTestId("anomaly-icon-gravity-rift")).toBeInTheDocument();
  });
});
