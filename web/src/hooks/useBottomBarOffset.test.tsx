import { describe, expect, it, vi, afterEach } from "vitest";
import { render } from "@testing-library/react";
import { useRef } from "react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { BOTTOM_BAR_OFFSET_VAR, useBottomBarOffset } from "./useBottomBarOffset.ts";

function Bar() {
  const ref = useRef<HTMLDivElement | null>(null);
  useBottomBarOffset(ref);
  return <div ref={ref} data-testid="bar" />;
}

const indexCss = readFileSync(resolve(__dirname, "../index.css"), "utf8");
const offset = () => document.documentElement.style.getPropertyValue(BOTTOM_BAR_OFFSET_VAR);

describe("useBottomBarOffset", () => {
  afterEach(() => vi.restoreAllMocks());

  it("publishes the bar's height while mounted and clears it afterwards", () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ height: 96.4 } as DOMRect);
    const { unmount } = render(<Bar />);
    expect(offset()).toBe("97px");
    unmount();
    expect(offset()).toBe("");
  });

  it("follows the bar when it changes size", () => {
    let notify: () => void = () => {};
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: () => void) {
          notify = cb;
        }
        observe() {}
        disconnect() {}
      },
    );
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
    rect.mockReturnValue({ height: 90 } as DOMRect);
    render(<Bar />);
    rect.mockReturnValue({ height: 140 } as DOMRect);
    notify();
    expect(offset()).toBe("140px");
    vi.unstubAllGlobals();
  });

  it("publishes the slim height when a bar is minimized and the full height when shown again", () => {
    let notify: () => void = () => {};
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: () => void) {
          notify = cb;
        }
        observe() {}
        disconnect() {}
      },
    );
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
    rect.mockReturnValue({ height: 190 } as DOMRect);
    render(<Bar />);
    expect(offset()).toBe("190px");
    rect.mockReturnValue({ height: 48 } as DOMRect);
    notify();
    expect(offset()).toBe("48px");
    rect.mockReturnValue({ height: 190 } as DOMRect);
    notify();
    expect(offset()).toBe("190px");
    vi.unstubAllGlobals();
  });

  it("is what the desktop event log docks above, and the phone drawer ignores", () => {
    const desktop = indexCss.match(/\.app-shell__event-log \{[^}]*\}/)?.[0] ?? "";
    expect(desktop).toContain(`bottom: var(${BOTTOM_BAR_OFFSET_VAR}, 0px)`);
    const phone = indexCss.slice(indexCss.indexOf("@media (max-width"));
    const phoneRule = phone.match(/\.app-shell__event-log \{[^}]*\}/)?.[0] ?? "";
    expect(phoneRule).toContain("bottom: 0;");
  });
});
