import { createContext, useContext, type ReactNode } from "react";
import type { ActionButtonView, Intent, Rich, SeatId, SeatView, TechColor } from "../model";
import { Button, Kbd } from "../ui";

const SeatsContext = createContext<Record<SeatId, SeatView>>({});
const DispatchContext = createContext<(intent: Intent) => void>(() => {});

export function GameProvider({
  seats,
  dispatch,
  children,
}: {
  seats: Record<SeatId, SeatView>;
  dispatch: (intent: Intent) => void;
  children: ReactNode;
}) {
  return (
    <SeatsContext.Provider value={seats}>
      <DispatchContext.Provider value={dispatch}>{children}</DispatchContext.Provider>
    </SeatsContext.Provider>
  );
}

export const useSeats = () => useContext(SeatsContext);
export const useSeat = (id: SeatId) => useContext(SeatsContext)[id];
export const useDispatch = () => useContext(DispatchContext);

/** The colour of a technology colour, in the tree and on the board. The tokens are in theme.css. */
export const TECH_COLOR: Record<TechColor, string> = {
  B: "var(--tech-b)",
  G: "var(--tech-g)",
  Y: "var(--tech-y)",
  R: "var(--tech-r)",
};

// Seat shapes are drawn, not typed: font glyphs such as ● ◆ ▲ have different sizes and look small.
// Each shape fills the same box, from −5.6 to 5.6.
const SEAT_SHAPES: Record<string, { d: string; rotate?: number }> = {
  "●": { d: "M-5.2 0a5.2 5.2 0 1 0 10.4 0a5.2 5.2 0 1 0 -10.4 0Z" },
  "◆": { d: "M0-5.6 5.6 0 0 5.6-5.6 0Z" },
  "▲": { d: "M0-5.2 5.6 4.6H-5.6Z" },
  "■": { d: "M-4.6-4.6h9.2v9.2h-9.2Z" },
  "✚": { d: "M-1.7-5.2h3.4v3.5h3.5v3.4H1.7v3.5h-3.4V1.7h-3.5v-3.4h3.5Z" },
  "✖": { d: "M-1.7-5.2h3.4v3.5h3.5v3.4H1.7v3.5h-3.4V1.7h-3.5v-3.4h3.5Z", rotate: 45 },
  "♣": {
    d: "M-2.4 -2.5a2.4 2.4 0 1 0 4.8 0a2.4 2.4 0 1 0 -4.8 0ZM-5.1 1a2.4 2.4 0 1 0 4.8 0a2.4 2.4 0 1 0 -4.8 0ZM0.3 1a2.4 2.4 0 1 0 4.8 0a2.4 2.4 0 1 0 -4.8 0ZM0 0 1.7 5.2H-1.7Z",
  },
  "★": {
    d: "M0.00 -5.60 L1.41 -1.94 L5.33 -1.73 L2.28 0.74 L3.29 4.53 L0.00 2.40 L-3.29 4.53 L-2.28 0.74 L-5.33 -1.73 L-1.41 -1.94Z",
  },
};

/** The shape of a seat inside an SVG, centred on (x, y). `size` is its width and height. */
export function SeatShape({
  symbol,
  x,
  y,
  size,
  color,
}: {
  symbol: string;
  x: number;
  y: number;
  size: number;
  color?: string;
}) {
  const shape = SEAT_SHAPES[symbol];
  if (!shape) {
    return (
      <text x={x} y={y + size * 0.35} fontSize={size} textAnchor="middle" style={{ fill: color }}>
        {symbol}
      </text>
    );
  }
  return (
    <path
      d={shape.d}
      transform={`translate(${x} ${y}) scale(${size / 11.2}) rotate(${shape.rotate ?? 0})`}
      style={{ fill: color, stroke: "none" }}
    />
  );
}

/** The shape and colour of a seat. Shape alone identifies it. */
export function SeatSymbol({ seat }: { seat: SeatId }) {
  const view = useSeat(seat);
  if (!view) {
    return null;
  }
  return (
    <svg
      viewBox="-6 -6 12 12"
      className="inline-block size-[1em] shrink-0 align-[-0.12em]"
      role="img"
      aria-label={view.faction}
    >
      <SeatShape symbol={view.symbol} x={0} y={0} size={11.2} color={view.color} />
    </svg>
  );
}

/** Who controls something: seat symbol and faction, or nobody. */
export function OwnerLabel({ seat }: { seat: SeatId | null }) {
  const view = useSeats()[seat ?? ""];
  return (
    <span className="flex items-baseline gap-1.5 text-muted">
      {view ? (
        <SeatSymbol seat={view.id} />
      ) : (
        <i className="inline-block size-[7px] shrink-0 rounded-full bg-faint" />
      )}
      {view ? view.faction : "Nobody"}
    </span>
  );
}

export function RichText({ value }: { value: Rich }) {
  return (
    <>
      {value.map((part, index) =>
        typeof part === "string" ? (
          part
        ) : "seat" in part ? (
          <SeatSymbol key={index} seat={part.seat} />
        ) : (
          <span key={index} className="text-accent">
            {part.accent}
          </span>
        ),
      )}
    </>
  );
}

/** A button that the data source offers. A click sends its intent. */
export function ActionButton({
  view,
  size,
  describedBy,
}: {
  view: ActionButtonView;
  size?: "md" | "sm";
  describedBy?: string;
}) {
  const dispatch = useDispatch();
  return (
    <Button
      tone={view.tone}
      size={size}
      disabled={view.disabled}
      aria-pressed={view.pressed}
      aria-describedby={describedBy}
      aria-keyshortcuts={view.key}
      iconAfter={view.arrow ? "arrow" : undefined}
      onClick={() => dispatch(view.intent)}
    >
      {view.label}
      {view.key && <Kbd>{view.key}</Kbd>}
    </Button>
  );
}
