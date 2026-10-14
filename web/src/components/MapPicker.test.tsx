import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MapPicker } from "./MapPicker.tsx";
import { MapThumbnail } from "./MapThumbnail.tsx";
import { clearMapPreviewCache } from "../hooks/useMapCatalog.ts";
import { fixtureLobby, fixturePreview, fixtureTemplates } from "../dev/mapPickerFixtures.ts";
import { mainTiles } from "../presentation/mapPicker.ts";

type Reply = () => Promise<unknown>;
const ok =
  (body: unknown): Reply =>
  () =>
    Promise.resolve({ ok: true, status: 200, json: async () => body });

function stubServer(overrides: { list?: Reply; preview?: Reply } = {}) {
  const fetchMock = vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    if (url.startsWith("/api/maps?")) return (overrides.list ?? ok(fixtureTemplates(6)))();
    if (overrides.preview) return overrides.preview();
    const alias = /\/api\/maps\/([^/?]+)\/preview/.exec(url)?.[1];
    return ok(fixturePreview(6, alias === "random" ? null : (alias ?? "6pStandard")))();
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

const props = {
  lobby: fixtureLobby(6, "6pStandard", 1),
  saving: false,
  onChoose: vi.fn(),
  onClose: vi.fn(),
};

beforeEach(() => {
  clearMapPreviewCache();
  props.onChoose.mockClear();
  props.onClose.mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe("MapPicker (host)", () => {
  it("shows the server's maps plus Random, the current one selected, and the picture", async () => {
    stubServer();
    render(<MapPicker {...props} editable />);
    expect(screen.getByRole("dialog", { name: "Choose your map" })).toBeInTheDocument();
    expect(screen.getByTestId("map-picker-loading")).toBeInTheDocument();
    await screen.findByTestId("map-card-6pStandard");
    for (const alias of fixtureTemplates(6).map((t) => t.alias))
      expect(screen.getByTestId(`map-card-${alias}`)).toBeInTheDocument();
    expect(screen.getByTestId("map-card-random")).toBeInTheDocument();
    expect(screen.getByTestId("map-card-6pStandard")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("map-card-6pStandard")).toHaveTextContent("Recommended");
    expect(screen.getByTestId("map-card-6pStandardNucleus")).toHaveTextContent(
      "Nucleus (no special rules yet)",
    );
    await screen.findByTestId("map-preview-board");
    expect(screen.getByTestId("map-picker-summary")).toHaveTextContent(
      "6 players · Standard by Community · 37 systems · no hyperlanes",
    );
    expect(screen.getByLabelText("Seats")).toHaveTextContent("Seat 1");
    expect(screen.getByLabelText("Seats")).not.toHaveTextContent("(you)");
  });

  it("picks another card, re-rolls the current one, and closes with Done", async () => {
    stubServer();
    render(<MapPicker {...props} editable />);
    fireEvent.click(await screen.findByTestId("map-card-6pHyperlanes"));
    expect(props.onChoose).toHaveBeenLastCalledWith(
      { kind: "template", alias: "6pHyperlanes" },
      undefined,
    );
    fireEvent.click(screen.getByTestId("map-card-random"));
    expect(props.onChoose).toHaveBeenLastCalledWith({ kind: "random" }, undefined);
    // The selected card is not saved again by a click; Re-roll does that on purpose.
    props.onChoose.mockClear();
    fireEvent.click(screen.getByTestId("map-card-6pStandard"));
    expect(props.onChoose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("map-reroll"));
    expect(props.onChoose).toHaveBeenCalledWith(
      { kind: "template", alias: "6pStandard" },
      undefined,
    );
    fireEvent.click(screen.getByTestId("map-picker-done"));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("disables the choices while saving and marks the selected card", async () => {
    stubServer();
    render(<MapPicker {...props} editable saving />);
    const card = await screen.findByTestId("map-card-6pStandard");
    expect(card).toBeDisabled();
    expect(screen.getByTestId("map-card-random")).toBeDisabled();
    expect(within(card).getByRole("status", { name: "Saving" })).toBeInTheDocument();
    expect(screen.getByTestId("map-reroll")).toBeDisabled();
  });

  it("sends a dev start preset only once the host has picked one", async () => {
    stubServer();
    render(<MapPicker {...props} editable />);
    fireEvent.click(await screen.findByTestId("map-card-6pHyperlanes"));
    expect(props.onChoose).toHaveBeenLastCalledWith(
      { kind: "template", alias: "6pHyperlanes" },
      undefined,
    );
    fireEvent.change(screen.getByTestId("dev-start-preset"), { target: { value: "combat" } });
    expect(props.onChoose).toHaveBeenLastCalledWith(
      { kind: "template", alias: "6pStandard" },
      "combat",
    );
    fireEvent.click(screen.getByTestId("map-card-random"));
    expect(props.onChoose).toHaveBeenLastCalledWith({ kind: "random" }, "combat");
  });

  it("shows an error with Retry when the list fails", async () => {
    let fail = true;
    stubServer({
      list: () =>
        fail
          ? Promise.resolve({ ok: false, status: 503, json: async () => ({}) })
          : ok(fixtureTemplates(6))(),
    });
    render(<MapPicker {...props} editable />);
    const alert = await screen.findByTestId("map-picker-error");
    expect(alert).toHaveTextContent("Could not load the maps");
    fail = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await screen.findByTestId("map-card-6pStandard");
  });

  it("shows an error with Retry when the preview fails", async () => {
    let fail = true;
    stubServer({
      preview: () =>
        fail
          ? Promise.resolve({ ok: false, status: 500, json: async () => ({}) })
          : ok(fixturePreview(6))(),
    });
    render(<MapPicker {...props} editable />);
    const alert = await screen.findByTestId("map-preview-error");
    fail = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await screen.findByTestId("map-preview-board");
  });

  it("explains an empty list and still offers Random", async () => {
    stubServer({ list: ok([]) });
    render(<MapPicker {...props} editable />);
    expect(await screen.findByTestId("map-picker-empty")).toHaveTextContent("random map");
    expect(screen.getByTestId("map-card-random")).toBeInTheDocument();
  });

  it("refetches the picture when the map revision changes and keeps the old one meanwhile", async () => {
    const fetchMock = stubServer();
    const { rerender } = render(<MapPicker {...props} editable />);
    await screen.findByTestId("map-preview-board");
    const lobbyCalls = () =>
      fetchMock.mock.calls.filter(([url]) => String(url).includes("/lobby/map-preview")).length;
    expect(lobbyCalls()).toBe(1);
    rerender(<MapPicker {...props} lobby={fixtureLobby(6, null, 2)} editable />);
    expect(screen.getByTestId("map-preview-board")).toBeInTheDocument();
    await waitFor(() => expect(lobbyCalls()).toBe(2));
  });
});

describe("MapPicker (everyone else)", () => {
  it("shows only the picture, read-only, with the places of the seats and no faction", async () => {
    const fetchMock = stubServer();
    render(<MapPicker {...props} editable={false} />);
    expect(screen.getByRole("dialog", { name: "Map" })).toBeInTheDocument();
    await screen.findByTestId("map-preview-board");
    expect(screen.queryByTestId("map-picker-choices")).toBeNull();
    expect(screen.queryByTestId("map-reroll")).toBeNull();
    expect(screen.queryByTestId("map-picker-done")).toBeNull();
    // It never even asks for the list of choices.
    expect(fetchMock.mock.calls.some(([url]) => String(url).startsWith("/api/maps?"))).toBe(false);
    // Nobody knows before Start who sits where: a place has its number only.
    expect(screen.getByTestId("map-tile-seat_2")).toBeInTheDocument();
    expect(screen.getByLabelText("Seats")).toHaveTextContent("Seat 2");
    expect(screen.getByLabelText("Seats")).not.toHaveTextContent(/Hacan|\(you\)/);
    fireEvent.click(screen.getByTestId("map-picker-close"));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("closes on Escape", async () => {
    stubServer();
    render(<MapPicker {...props} editable={false} />);
    await screen.findByTestId("map-preview-board");
    await act(async () => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(props.onClose).toHaveBeenCalled();
  });
});

describe("MapThumbnail", () => {
  it("draws one hex per main-map tile", () => {
    const preview = fixturePreview(6);
    const withNexus = [
      ...preview.tiles,
      { system_id: "82a", label: "Nexus", q: 0, r: 0, special_area: "nexus" },
    ];
    const { container } = render(
      <MapThumbnail tiles={withNexus} seats={preview.seats} label="Standard layout" />,
    );
    expect(container.querySelectorAll("polygon")).toHaveLength(mainTiles(preview.tiles).length);
    expect(container.querySelectorAll("polygon")).toHaveLength(37);
    expect(screen.getByRole("img", { name: "Standard layout" })).toBeInTheDocument();
  });
});
