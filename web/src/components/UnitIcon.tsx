import React from "react";

export type UnitBaseType =
  | "flagship"
  | "warsun"
  | "dreadnought"
  | "carrier"
  | "cruiser"
  | "destroyer"
  | "fighter"
  | "infantry"
  | "mech"
  | "pds"
  | "spacedock";

/**
 * Normalizes any unit ID (e.g. "sol_carrier2", "letani_warrior", "nowarsun") into its base unit type.
 */
export function getUnitBaseType(unitType: string): UnitBaseType {
  const norm = unitType.toLowerCase().replace(/[-_\s]/g, "");

  if (norm.includes("warsun") || norm === "nowarsun" || norm === "ws") return "warsun";
  if (
    norm.includes("spacedock") ||
    norm.includes("dock") ||
    norm.includes("floatingfactory") ||
    norm.includes("dimensionaltear")
  )
    return "spacedock";
  if (
    norm.includes("dreadnought") ||
    norm.includes("exotrireme") ||
    norm.includes("superdreadnought")
  )
    return "dreadnought";
  if (norm.includes("flagship") || norm.includes("cavalry") || norm === "genesis")
    return "flagship";
  if (norm.includes("carrier") || norm.includes("combattran")) return "carrier";
  if (norm.includes("cruiser") || norm.includes("saturnengine")) return "cruiser";
  if (norm.includes("destroyer") || norm.includes("strikewingalpha")) return "destroyer";
  if (norm.includes("fighter")) return "fighter";
  if (
    norm.includes("infantry") ||
    norm.includes("specops") ||
    norm.includes("letaniwarrior") ||
    norm.includes("crimsonlegionnaire")
  )
    return "infantry";
  if (
    norm.includes("mech") ||
    norm.includes("eidolon") ||
    norm.includes("reanimator") ||
    norm.includes("behemoth")
  )
    return "mech";
  if (norm.includes("pds") || norm.includes("heltitan")) return "pds";

  return "fighter";
}

const UNIT_LABELS: Record<UnitBaseType, { singular: string; plural: string }> = {
  flagship: { singular: "Flagship", plural: "Flagships" },
  warsun: { singular: "War Sun", plural: "War Suns" },
  dreadnought: { singular: "Dreadnought", plural: "Dreadnoughts" },
  carrier: { singular: "Carrier", plural: "Carriers" },
  cruiser: { singular: "Cruiser", plural: "Cruisers" },
  destroyer: { singular: "Destroyer", plural: "Destroyers" },
  fighter: { singular: "Fighter", plural: "Fighters" },
  infantry: { singular: "Infantry", plural: "Infantry" },
  mech: { singular: "Mech", plural: "Mechs" },
  pds: { singular: "PDS", plural: "PDS" },
  spacedock: { singular: "Space Dock", plural: "Space Docks" },
};

/**
 * Returns human-readable label for a unit type with singular / plural handling.
 */
export function getUnitDisplayName(type: string | UnitBaseType, count = 1): string {
  const baseType = getUnitBaseType(type);
  const labels = UNIT_LABELS[baseType];
  if (!labels) {
    return type.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }
  return count === 1 ? labels.singular : labels.plural;
}

/** Standard tactical priority order (large capital ships down to ground/structures). */
export const UNIT_PRIORITY: Record<UnitBaseType, number> = {
  warsun: 1,
  flagship: 2,
  dreadnought: 3,
  carrier: 4,
  cruiser: 5,
  destroyer: 6,
  fighter: 7,
  mech: 8,
  infantry: 9,
  pds: 10,
  spacedock: 11,
};

export interface UnitIconProps extends React.SVGProps<SVGSVGElement> {
  type: string | UnitBaseType;
  size?: number | string;
  color?: string;
  className?: string;
  title?: string;
}

export const UnitIcon: React.FC<UnitIconProps> = ({
  type,
  size = 18,
  color,
  className,
  title,
  style,
  ...rest
}) => {
  const baseType = getUnitBaseType(type);
  const label = title || getUnitDisplayName(baseType);

  const iconStyle: React.CSSProperties = {
    display: "inline-block",
    verticalAlign: "middle",
    flexShrink: 0,
    color: color || "currentColor",
    ...style,
  };

  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="currentColor"
      aria-label={rest["aria-hidden"] ? undefined : label}
      role={rest["aria-hidden"] ? undefined : "img"}
      className={className}
      style={iconStyle}
      {...rest}
    >
      {title && <title>{title}</title>}
      {renderIconPath(baseType)}
    </svg>
  );
};

function renderIconPath(type: UnitBaseType): React.ReactNode {
  switch (type) {
    case "flagship":
      return (
        <g>
          {/* Main hull with sweeping command wings */}
          <path d="M12 1L14 6L22 14L19 18L16 16V22L12 20L8 22V16L5 18L2 14L10 6Z" />
          {/* Inner command bridge cutout */}
          <path
            d="M12 8L13.5 11L12 14L10.5 11Z"
            fill="#0f172a"
            stroke="currentColor"
            strokeWidth="0.5"
          />
        </g>
      );

    case "warsun":
      return (
        <g>
          {/* Outer armored sphere */}
          <circle cx="12" cy="12" r="9.5" />
          {/* Equatorial trench line */}
          <path d="M2.5 11.2H21.5V12.8H2.5Z" fill="#0f172a" />
          {/* Concave superlaser focus dish */}
          <circle cx="15.5" cy="7.5" r="3.2" fill="#0f172a" />
          <circle cx="15.5" cy="7.5" r="1.3" fill="currentColor" />
        </g>
      );

    case "dreadnought":
      return (
        <g>
          {/* Heavy forward twin prow cannons and armored hammerhead hull */}
          <path d="M6 3H9.5L10.5 7H13.5L14.5 3H18L19 9L20 15L22 21L16 19.5L12 21.5L8 19.5L2 21L4 15L5 9Z" />
          {/* Center bridge core cutout */}
          <rect x="11" y="9.5" width="2" height="6" rx="0.5" fill="#0f172a" />
        </g>
      );

    case "carrier":
      return (
        <g>
          {/* Broad dual flight-deck hull */}
          <path d="M4 3H10V7H14V3H20V21L16 20V18H8V20L4 21Z" />
          {/* Recessed central launch runway strip */}
          <rect x="11" y="8" width="2" height="9" fill="#0f172a" />
        </g>
      );

    case "cruiser":
      return (
        <g>
          {/* Sleek, sharp predatory arrowhead hull */}
          <path d="M12 2L15 8L16.5 14L20 19.5L16.5 20L13.5 17.5L12 18.5L10.5 17.5L7.5 20L4 19.5L7.5 14L9 8Z" />
        </g>
      );

    case "destroyer":
      return (
        <g>
          {/* Narrow high-speed escort dart */}
          <path d="M12 3L14 9.5L15 15.5L18 20.5L14.5 19.5L12 17.5L9.5 19.5L6 20.5L9 15.5L10 9.5Z" />
        </g>
      );

    case "fighter":
      return (
        <g>
          {/* Agile tri-wing chevron interceptor */}
          <path d="M12 3.5L21.5 18L17 17L12 14.5L7 17L2.5 18Z" />
          {/* Cockpit visor cutout */}
          <polygon points="12,8 13.5,12 10.5,12" fill="#0f172a" />
        </g>
      );

    case "infantry":
      return (
        <g>
          {/* Helmet */}
          <path d="M8 8C8 5.2 9.8 3.5 12 3.5C14.2 3.5 16 5.2 16 8V11H8Z" />
          {/* Visor slit */}
          <rect x="9.5" y="7" width="5" height="1.8" rx="0.5" fill="#0f172a" />
          {/* Armored shoulders */}
          <path d="M4 20.5C4 16 7.5 13.5 12 13.5C16.5 13.5 20 16 20 20.5H4Z" />
        </g>
      );

    case "mech":
      return (
        <g>
          {/* Heavy walker cockpit */}
          <rect x="8" y="4" width="8" height="7" rx="1.5" />
          {/* Visor */}
          <rect x="9.5" y="6.5" width="5" height="1.8" rx="0.5" fill="#0f172a" />
          {/* Shoulder weapon pods */}
          <rect x="4" y="5.5" width="3" height="5" rx="0.8" />
          <rect x="17" y="5.5" width="3" height="5" rx="0.8" />
          {/* Walker legs & feet */}
          <path d="M8 11L6 16.5L4 20H8L9 16L10 11Z" />
          <path d="M16 11L18 16.5L20 20H16L15 16L14 11Z" />
        </g>
      );

    case "pds":
      return (
        <g>
          {/* Planetary bunker foundation */}
          <path d="M3 21H21V18C21 16 17.5 15 12 15C6.5 15 3 16 3 18Z" />
          {/* Dual skyward anti-orbital cannon barrels */}
          <polygon points="8.5,15 7.5,4 10,4 11,15" />
          <polygon points="13,15 14,4 16.5,4 15.5,15" />
          {/* Turret center mount */}
          <circle cx="12" cy="15" r="2.5" fill="#0f172a" />
          <circle cx="12" cy="15" r="1.2" />
        </g>
      );

    case "spacedock":
      return (
        <g>
          {/* Central station spire */}
          <rect x="11" y="2" width="2" height="20" rx="0.5" />
          {/* Upper docking gantry */}
          <rect x="3" y="7" width="18" height="2.5" rx="0.5" />
          {/* Lower curved drydock construction arms */}
          <path d="M5 9.5V16C5 18 7.5 19.5 11 19.5V17C8.5 17 7 16 7 14.5V9.5Z" />
          <path d="M19 9.5V16C19 18 16.5 19.5 13 19.5V17C15.5 17 17 16 17 14.5V9.5Z" />
          {/* Core power hub */}
          <circle cx="12" cy="13" r="2" fill="#0f172a" stroke="currentColor" strokeWidth="1" />
        </g>
      );
  }
}
