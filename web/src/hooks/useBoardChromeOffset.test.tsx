import { describe, expect, it, vi, afterEach } from "vitest";
import { render } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { BOARD_CHROME_BOTTOM_VAR } from "./useBoardChromeOffset.ts";
import { Board } from "../components/Board.tsx";

const prepCss = readFileSync(resolve(__dirname, "../components/SecondaryPrep.css"), "utf8");
const offset = () => document.documentElement.style.getPropertyValue(BOARD_CHROME_BOTTOM_VAR);

describe("useBoardChromeOffset", () => {
  afterEach(() => vi.restoreAllMocks());

  it("publishes the bottom edge of the board toolbar row while the board is mounted", () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ bottom: 140.2 } as DOMRect);
    const { unmount } = render(<Board board={{ systems: {}, map: [] } as never} seatingOrder={["a"]} />);
    expect(offset()).toBe("141px");
    unmount();
    expect(offset()).toBe("");
  });

  // Run 01-2322 (d254): the "Prepare your secondary" chip sat at a fixed 56px, over the Standard/zoom row.
  it("is what the secondary-prep chip sits below on desktop", () => {
    const rule = prepCss.match(/^\.secondary-prep \{[^}]*\}/m)?.[0] ?? "";
    expect(rule).toContain(`top: calc(var(${BOARD_CHROME_BOTTOM_VAR}, 56px) + 8px)`);
  });
});
