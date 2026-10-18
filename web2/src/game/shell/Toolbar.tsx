import type { ToolbarView, Workspace } from "../../model";
import { Button, Icon, Menu } from "../../ui";
import { useDispatch } from "../context";

export const REFERENCES = [
  ["objectives", "Objectives"],
  ["technology", "Technology"],
  ["cards", "Cards"],
  ["secrets", "Secrets"],
  ["agendas", "Agendas"],
  ["exploration", "Exploration"],
  ["log", "Log"],
] as const;

/** The way out of the game ("Start", to the start screen). A page load, not a game choice. */
export interface OtherGame {
  label: string;
  href: string;
}

/** One header row: status, workspace, reference sheets, and the controls of the draft. */
export function Toolbar({
  view,
  drawer,
  onDrawer,
  phone = false,
  other,
  settings = false,
  table = false,
  history = null,
}: {
  view: ToolbarView;
  drawer: string | null;
  onDrawer: (id: string) => void;
  /** A phone has the reference sheets in the menu: the row has no room for four buttons. */
  phone?: boolean;
  other?: OtherGame;
  /** The local game has a Dev sheet, opened like the reference sheets. */
  settings?: boolean;
  /** The game has a table: the sheet "Table" is the last reference sheet. */
  table?: boolean;
  /**
   * The history mode: which point of the game the screen shows ("Round 1 · Turn 5 · Mahact"), with
   * "‹" and "›" to the rows of the Log around it that open. The status of the game and the
   * workspace give way to it and to "Back to now"; no control of the live game or of the draft is
   * shown. Null: the screen shows now.
   */
  history?: { title: string; previous: string | null; next: string | null } | null;
}) {
  const dispatch = useDispatch();
  const references: (readonly [string, string])[] = table
    ? [...REFERENCES, ["table", "Table"]]
    : [...REFERENCES];
  const tab = (mode: Workspace, label: string, disabled = false, title?: string) => (
    <button
      type="button"
      aria-pressed={view.workspace === mode}
      disabled={disabled}
      title={title}
      onClick={() => dispatch({ type: "workspace", mode })}
      className="rounded-[5px] border border-transparent px-3.5 py-1 text-sm font-strong text-muted disabled:opacity-40 aria-pressed:border-line aria-pressed:bg-raised aria-pressed:text-accent"
    >
      {label}
    </button>
  );
  // "‹" and "›" of the history mode: as small as the line of text they stand in.
  const step = (label: string, sign: string, turn: string | null) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={!turn}
      onClick={() => turn && dispatch({ type: "inspectTurn", turn })}
      className="-my-1 grid h-6 w-5 shrink-0 place-items-center rounded text-base leading-none text-muted hover:text-text disabled:opacity-30 touch:h-8 touch:w-7"
    >
      <span aria-hidden="true">{sign}</span>
    </button>
  );
  const draftItems = [
    { label: "Start over", onSelect: () => dispatch({ type: "startOver" }) },
    { label: "Discard draft", onSelect: () => dispatch({ type: "discardDraft" }) },
  ];
  return (
    <header className="flex min-h-12 items-center gap-3.5 border-b border-line bg-surface px-4 py-1.5 phone:flex-wrap phone:gap-x-2 phone:gap-y-1.5 phone:px-3 phone:pt-[max(6px,env(safe-area-inset-top))]">
      {history ? (
        <>
          <div className="flex min-w-0 flex-col text-xs leading-[1.3] text-muted phone:w-0 phone:flex-1">
            <span className="flex items-center gap-1">
              <Icon name="history" className="size-3.5" />
              History
            </span>
            <nav aria-label="Turns of the Log" className="-ml-1 flex min-w-0 items-center">
              {step("Older turn", "‹", history.previous)}
              <strong
                className="min-w-0 truncate text-sm font-semibold text-text"
                title={history.title}
              >
                {history.title}
              </strong>
              {step("Newer turn", "›", history.next)}
            </nav>
          </div>
          <Button tone="quiet" size="sm" onClick={() => dispatch({ type: "closeTurn" })}>
            Back to now
          </Button>
        </>
      ) : (
        <>
          {/* A status can name a player: on a phone it has the room that is left, whatever its length. */}
          <div className="flex min-w-0 flex-col text-xs leading-[1.3] text-muted phone:w-0 phone:flex-1">
            <span className="truncate">
              Round {view.round} · {view.phase}
              {view.turn > 0 && ` · Turn ${view.turn}`}
            </span>
            <strong className="truncate text-sm font-semibold text-text">
              <i className="mr-1.5 mb-0.5 inline-block size-1.5 rounded-full bg-accent" />
              {view.status}
            </strong>
          </div>
          <nav
            aria-label="Workspace"
            className="flex gap-0.5 rounded-lg border border-line bg-well p-[3px]"
          >
            {tab("live", "Live")}
            {tab("draft", "Draft", !!view.draftLocked, view.draftLocked ?? undefined)}
          </nav>
        </>
      )}
      {phone ? null : (
        <nav aria-label="Reference" className="ml-auto flex gap-1">
          {references.map(([id, label]) => (
            <Button
              key={id}
              tone="quiet"
              size="sm"
              active={drawer === id}
              aria-expanded={drawer === id}
              onClick={() => onDrawer(id)}
            >
              {label}
            </Button>
          ))}
        </nav>
      )}
      {other && !phone && (
        <Button tone="quiet" size="sm" onClick={() => location.assign(other.href)}>
          {other.label}
        </Button>
      )}
      {settings && !phone && (
        <Button
          tone="quiet"
          size="sm"
          active={drawer === "settings"}
          aria-expanded={drawer === "settings"}
          onClick={() => onDrawer("settings")}
        >
          Dev
        </Button>
      )}
      {view.draft && !history && (
        <div className="flex items-center gap-1.5 phone:order-last phone:w-full phone:[&>:last-child]:flex-1">
          <Button
            tone="quiet"
            size="icon"
            icon="undo"
            aria-label="Undo"
            title="Undo"
            disabled={!view.draft.canUndo}
            onClick={() => dispatch({ type: "undo" })}
          />
          <Button
            tone="quiet"
            size="icon"
            icon="redo"
            aria-label="Redo"
            title="Redo"
            disabled={!view.draft.canRedo}
            onClick={() => dispatch({ type: "redo" })}
          />
          <Button
            tone="primary"
            iconAfter="arrow"
            title={view.draft.applyHint}
            disabled={!view.draft.canApply}
            onClick={() => dispatch({ type: "openApply" })}
          >
            Apply to Live
          </Button>
          {!phone && <Menu label="More draft actions" items={draftItems} />}
        </div>
      )}
      {phone && (
        <Menu
          label="Reference and more"
          items={[
            ...references.map(([id, label]) => ({ label, onSelect: () => onDrawer(id) })),
            ...(view.draft && !history ? draftItems : []),
            ...(other ? [{ label: other.label, onSelect: () => location.assign(other.href) }] : []),
            ...(settings ? [{ label: "Dev", onSelect: () => onDrawer("settings") }] : []),
          ]}
        />
      )}
    </header>
  );
}
