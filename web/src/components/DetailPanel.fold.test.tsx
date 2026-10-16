import { fireEvent, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { DetailPanel } from "./DetailPanel.tsx";

const css = readFileSync(resolve(__dirname, "../index.css"), "utf8");
const panel = () => screen.getByTestId("detail-panel");

describe("DetailPanel fold (phone)", () => {
  it("starts open, folds to the title bar and opens again, keeping the content mounted", () => {
    render(
      <DetailPanel title="Politics" onClose={() => undefined}>
        <p>Primary ability text</p>
      </DetailPanel>,
    );
    const fold = screen.getByTestId("detail-panel-fold");
    expect(panel().className).not.toContain("detail-panel--folded");
    expect(fold.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(fold);
    expect(panel().className).toContain("detail-panel--folded");
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(fold.getAttribute("aria-label")).toBe("Show Politics details");
    expect(screen.getByText("Primary ability text")).toBeInTheDocument();
    fireEvent.click(fold);
    expect(panel().className).not.toContain("detail-panel--folded");
  });

  it("stays folded when the title changes (another system tapped) and still closes", () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <DetailPanel title="System A #1" onClose={onClose} testId="system-inspector">
        <p>a</p>
      </DetailPanel>,
    );
    fireEvent.click(screen.getByTestId("system-inspector-fold"));
    rerender(
      <DetailPanel title="System B #2" onClose={onClose} testId="system-inspector">
        <p>b</p>
      </DetailPanel>,
    );
    expect(screen.getByTestId("system-inspector").className).toContain("detail-panel--folded");
    expect(screen.getByRole("heading", { name: "System B #2" })).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Close System B #2 details"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("only offers the fold on phone layouts, and the folded bar clears the Players / Events buttons", () => {
    expect(css).toMatch(/\.detail-panel__fold \{\s*display: none;/);
    expect(css).toMatch(/@media \(max-width: 720px\), \(max-height: 500px\) \{\s*\.detail-panel__fold \{\s*display: inline-flex;/);
    expect(css).toMatch(/\.detail-panel\.detail-panel--folded \{[^}]*bottom: 12px !important;[^}]*left: 12px !important;/);
  });
});

describe("collapsed decision pills (phone)", () => {
  it("span the width above the Players / Events buttons and clear a docked prepare banner", () => {
    const bottom = "bottom: max\\(60px, calc\\(var\\(--prep-dock, 0px\\) \\+ 8px\\)\\);";
    expect(css).toMatch(new RegExp(`\\.choice-banner \\{\\s*${bottom}`));
    expect(css).toMatch(new RegExp(`\\.choice-minimized-pill,\\s*\\.combat-arena-dock \\{[^}]*${bottom}[^}]*transform: none;`));
    expect(css).toMatch(/@media \(max-height: 500px\) and \(min-width: 721px\) \{\s*:root:has\(\.secondary-prep--preparing\) \.choice-banner,\s*\.choice-minimized-pill,\s*\.combat-arena-dock \{[^}]*left: 12px;/);
  });
});
