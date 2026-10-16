import { useEffect, useMemo, useState } from "react";
import { TableSheet } from "../game/lobby/LobbyScreen";
import type { LobbyIntent } from "../model";
import { selectLobby } from "../session/select/lobby";
import type { Table, TableState } from "../session/table";
import type { Catalog } from "../session/wire";
import { Toast } from "../ui";

/**
 * The content of the sheet "Table" of a running game: the table of the game, read while the
 * sheet is in view. `open` makes the table (the socket of a table on the server, or the table of
 * the page); it is closed when the sheet closes. The host sets the wait here, and a change that
 * the table refuses is a toast.
 */
export function GameTable({
  open,
  catalog,
  onTakeOver,
}: {
  /** Makes the table. A new function is a new table. */
  open: () => Table;
  /** The printed names. Without it a faction is shown by its id. */
  catalog: Catalog | null;
  /** A visitor took the seat of a person who is away. Without it the sheet offers no seat. */
  onTakeOver?: (table: Table, player: string, name: string) => void;
}) {
  const [table, setTable] = useState<Table | null>(null);
  const [state, setState] = useState<TableState | null>(null);
  const [offline, setOffline] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [toast, setToast] = useState<{ id: number; text: string } | null>(null);

  useEffect(() => {
    const made = open();
    const stop = made.subscribe((event) => {
      switch (event.kind) {
        case "table":
          setOffline(false);
          return setState(event);
        case "offline":
          return setOffline(true);
        case "rejected":
          return setToast({ id: performance.now(), text: event.reason });
        case "error":
          return setFailure(event.message);
      }
    });
    setTable(made);
    return () => {
      stop();
      made.close();
      setTable(null);
    };
  }, [open]);

  const view = useMemo(() => {
    const shown = state && selectLobby(state, { mine: [], inspected: null, offline }, catalog);
    // A seat is offered only where the page can take it.
    return shown && !onTakeOver
      ? { ...shown, rows: shown.rows.map((row) => ({ ...row, buttons: [] })) }
      : shown;
  }, [state, offline, catalog, onTakeOver]);

  if (failure) {
    return (
      <p role="alert" className="mt-3 text-red">
        {failure}
      </p>
    );
  }
  if (!view) {
    return <p className="mt-3 text-muted">Reading the table…</p>;
  }
  const dispatch = (intent: LobbyIntent) => {
    if (intent.type === "set") {
      table?.setSettings(intent.change);
    } else if (intent.type === "takeOver" && table) {
      onTakeOver?.(table, intent.player, intent.name);
    }
  };
  return (
    <>
      <TableSheet view={view} dispatch={dispatch} />
      <Toast message={toast} />
    </>
  );
}
