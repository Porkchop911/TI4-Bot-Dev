import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { LobbyStatus } from "./Lobby.tsx";
import { clearMapPreviewCache } from "../hooks/useMapCatalog.ts";
import { fixtureLobby, fixturePreview, fixtureTemplates } from "../dev/mapPickerFixtures.ts";

const callbacks = {
  onReady: vi.fn(),
  onStart: vi.fn(),
  onLeave: vi.fn(),
  onJoin: vi.fn(),
  onWatch: vi.fn(),
  onTakeover: vi.fn(),
  onReorder: vi.fn(),
};

function stubServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.startsWith("/api/maps?")
        ? fixtureTemplates(6)
        : fixturePreview(6, "6pStandard");
      return Promise.resolve({ ok: true, status: 200, json: async () => body });
    }),
  );
}

beforeEach(() => {
  clearMapPreviewCache();
  sessionStorage.clear();
  stubServer();
});
afterEach(() => vi.unstubAllGlobals());

describe("lobby map row", () => {
  it("opens the picker by itself for a host who has not chosen, once per tab", async () => {
    const onChooseMap = vi.fn().mockResolvedValue(true);
    const fresh = fixtureLobby(6, "6pStandard", 0);
    const { unmount } = render(
      <LobbyStatus lobby={fresh} playerId="host" onChooseMap={onChooseMap} {...callbacks} />,
    );
    expect(screen.getByRole("dialog", { name: "Choose your map" })).toBeInTheDocument();
    fireEvent.click(await screen.findByTestId("map-card-6pHyperlanes"));
    expect(onChooseMap).toHaveBeenCalledWith(
      { kind: "template", alias: "6pHyperlanes" },
      undefined,
    );
    fireEvent.click(screen.getByTestId("map-picker-done"));
    expect(screen.queryByRole("dialog")).toBeNull();
    // Still editable from the lobby until Start.
    expect(screen.getByTestId("lobby-map-button")).toHaveTextContent("Choose map");
    unmount();
    render(<LobbyStatus lobby={fresh} playerId="host" onChooseMap={onChooseMap} {...callbacks} />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("does not open by itself once the host has chosen, and lets the host reopen it", () => {
    render(
      <LobbyStatus
        lobby={fixtureLobby(6, "6pStandard", 2)}
        playerId="host"
        onChooseMap={vi.fn()}
        {...callbacks}
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("lobby-map-row")).toHaveTextContent("Map: 6pStandard (recommended)");
    fireEvent.click(screen.getByTestId("lobby-map-button"));
    expect(screen.getByRole("dialog", { name: "Choose your map" })).toBeInTheDocument();
  });

  it("gives players a read-only View map, never the host's controls", async () => {
    render(
      <LobbyStatus lobby={fixtureLobby(6, "6pStandard", 1)} playerId="guest_1" {...callbacks} />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    const button = screen.getByTestId("lobby-map-button");
    expect(button).toHaveTextContent("View map");
    fireEvent.click(button);
    const dialog = screen.getByRole("dialog", { name: "Map" });
    expect(within(dialog).queryByTestId("map-reroll")).toBeNull();
    expect(within(dialog).queryByTestId("map-picker-choices")).toBeNull();
    await screen.findByTestId("map-preview-board");
    expect(screen.getByLabelText("Seats")).toHaveTextContent("Seat 2");
    expect(screen.getByLabelText("Seats")).not.toHaveTextContent("(you)");
  });

  it("lets spectators watch too, and a host-only edit is not offered once the game runs", () => {
    const running = {
      ...fixtureLobby(6, "6pStandard", 3),
      phase: "running" as const,
    };
    render(<LobbyStatus lobby={running} playerId="host" onChooseMap={vi.fn()} {...callbacks} />);
    expect(screen.getByTestId("lobby-map-button")).toHaveTextContent("View map");
  });

  it("tells a player when the host changed the map", () => {
    const { rerender } = render(
      <LobbyStatus lobby={fixtureLobby(6, "6pStandard", 1)} playerId="guest_1" {...callbacks} />,
    );
    expect(screen.queryByTestId("map-change-notice")).toBeNull();
    rerender(
      <LobbyStatus lobby={fixtureLobby(6, "6pHyperlanes", 2)} playerId="guest_1" {...callbacks} />,
    );
    expect(screen.getByTestId("map-change-notice")).toHaveTextContent("The host changed the map.");
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByTestId("map-change-notice")).toBeNull();
  });

  it("shows no map row for a server that does not send one", () => {
    const { map: _map, map_revision: _revision, ...legacy } = fixtureLobby(6);
    void _map;
    void _revision;
    render(<LobbyStatus lobby={legacy} playerId="host" onChooseMap={vi.fn()} {...callbacks} />);
    expect(screen.queryByTestId("lobby-map-row")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
