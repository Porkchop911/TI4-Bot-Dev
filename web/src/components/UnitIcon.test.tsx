import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { UnitIcon, getUnitBaseType, getUnitDisplayName, UnitBaseType } from "./UnitIcon.tsx";

describe("UnitIcon", () => {
  const allUnitTypes: UnitBaseType[] = [
    "flagship",
    "warsun",
    "dreadnought",
    "carrier",
    "cruiser",
    "destroyer",
    "fighter",
    "infantry",
    "mech",
    "pds",
    "spacedock",
  ];

  it.each(allUnitTypes)("renders icon for %s without error", (unitType) => {
    const { container } = render(<UnitIcon type={unitType} color="#E69F00" size={20} />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute("width", "20");
    expect(svg).toHaveAttribute("height", "20");
    expect(svg).toHaveStyle({ color: "#E69F00" });
  });

  it("normalizes diverse unit ids correctly", () => {
    expect(getUnitBaseType("sol_carrier2")).toBe("carrier");
    expect(getUnitBaseType("muaat_warsun")).toBe("warsun");
    expect(getUnitBaseType("nowarsun")).toBe("warsun");
    expect(getUnitBaseType("l1z1x_dreadnought")).toBe("dreadnought");
    expect(getUnitBaseType("argent_destroyer2")).toBe("destroyer");
    expect(getUnitBaseType("naalu_fighter")).toBe("fighter");
    expect(getUnitBaseType("letani_warrior")).toBe("infantry");
    expect(getUnitBaseType("letani_behemoth")).toBe("mech");
    expect(getUnitBaseType("hel_titan")).toBe("pds");
    expect(getUnitBaseType("floating_factory")).toBe("spacedock");
    expect(getUnitBaseType("genesis")).toBe("flagship");
  });

  it("formats singular and plural display names", () => {
    expect(getUnitDisplayName("carrier", 1)).toBe("Carrier");
    expect(getUnitDisplayName("carrier", 2)).toBe("Carriers");
    expect(getUnitDisplayName("destroyer", 1)).toBe("Destroyer");
    expect(getUnitDisplayName("destroyer", 3)).toBe("Destroyers");
    expect(getUnitDisplayName("infantry", 1)).toBe("Infantry");
    expect(getUnitDisplayName("infantry", 4)).toBe("Infantry");
    expect(getUnitDisplayName("pds", 1)).toBe("PDS");
    expect(getUnitDisplayName("pds", 2)).toBe("PDS");
    expect(getUnitDisplayName("warsun", 2)).toBe("War Suns");
  });
});
