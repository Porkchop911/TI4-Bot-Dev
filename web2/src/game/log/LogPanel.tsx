/**
 * The log: the turns of the other players since the viewer's last turn. It is read-only. Desktop
 * has it under the player table, where it fills the column and scrolls inside itself; a phone has
 * it as the pane Log.
 */
export function LogPanel() {
  return (
    <section aria-label="Log" className="min-h-0 flex-1 overflow-y-auto px-3 py-2.5">
      <p className="text-sm text-muted">No turns since your last turn</p>
    </section>
  );
}
