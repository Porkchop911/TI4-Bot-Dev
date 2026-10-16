/**
 * The log: the turns of the other players since the viewer's last turn. It is read-only. A phone
 * has it as the pane Log.
 */
export function LogPanel() {
  return (
    <section aria-label="Log" className="px-3 py-2.5">
      <p className="text-sm text-muted">No turns since your last turn</p>
    </section>
  );
}
