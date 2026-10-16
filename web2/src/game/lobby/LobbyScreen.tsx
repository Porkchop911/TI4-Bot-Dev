import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import type {
  FactionSheetView,
  Intent,
  LobbyButtonView,
  LobbyFactionEditorView,
  LobbyFactionOptionView,
  LobbyIntent,
  LobbyRowView,
  LobbySetupRowView,
  LobbySignView,
  LobbyStepView,
  LobbyView,
} from "../../model";
import { Button, Drawer, Eyebrow, Hint, Kbd, Peek, Select, Toast, cx, usePhone } from "../../ui";
import { Board } from "../board/Board";
import { GameProvider, SeatSymbol } from "../context";
import { LinkProvider } from "../link";
import { ShellFrame } from "../shell/ShellFrame";
import { keyIsFree } from "../shell/shortcuts";
import { FactionSheet } from "./FactionSheet";
import type { OtherGame } from "../shell/Toolbar";

/** The characters of a name in an input. The server takes 64 bytes at most: `validName` says. */
export const NICKNAME_MAX = 32;

/** Whether the server takes a name (trimmed). A name that it refuses is not sent. */
export type ValidName = (name: string) => boolean;

const SIGNS: Record<LobbySignView["kind"], string> = {
  host: "★",
  rule: "⚙",
  random: "⚄",
  model: "✦",
  away: "◇",
  open: "",
};

/** The one sign of a row, in a column of fixed width. The word is on hover. */
function Sign({ sign }: { sign: LobbySignView | null }) {
  if (!sign) {
    return <span className="w-5 flex-none" />;
  }
  return (
    <span
      role="img"
      aria-label={sign.word}
      title={sign.word}
      className={cx(
        "grid w-5 flex-none place-items-center text-base leading-none",
        sign.kind === "away" ? "text-muted" : "text-accent",
      )}
    >
      {sign.kind === "open" ? (
        <i className="block size-3 rounded-full border border-dashed border-muted" />
      ) : (
        SIGNS[sign.kind]
      )}
    </span>
  );
}

function RowButton({
  view,
  dispatch,
}: {
  view: LobbyButtonView;
  dispatch: (intent: LobbyIntent) => void;
}) {
  return (
    <Button tone="quiet" size="sm" title={view.title} onClick={() => dispatch(view.intent)}>
      {view.label}
    </Button>
  );
}

/**
 * The name of the viewer, open in its row. Enter takes the name, Esc and a click elsewhere leave
 * it. A name that the server refuses is marked, and Enter keeps the editor open.
 */
function NameEditor({
  name,
  onName,
  validName,
  onDone,
}: {
  name: string;
  onName: (name: string) => void;
  validName: ValidName;
  /** The name that was entered, or null when the editor was left. */
  onDone: (name: string | null) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  const wanted = name.trim();
  const refused = wanted !== "" && !validName(wanted);
  return (
    <form
      className="flex w-[300px] flex-none phone:w-auto phone:min-w-0 phone:flex-1"
      onSubmit={(event) => {
        event.preventDefault();
        if (!refused) {
          onDone(wanted);
        }
      }}
    >
      <input
        ref={input}
        aria-label="Your name"
        aria-invalid={refused}
        value={name}
        maxLength={NICKNAME_MAX}
        onChange={(event) => onName(event.target.value)}
        onBlur={() => onDone(null)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            onDone(null);
          }
        }}
        className="min-h-8 w-full min-w-0 rounded border border-line bg-well px-2 text-base text-text aria-[invalid=true]:border-red touch:min-h-11"
      />
    </form>
  );
}

/** The width of the sheet of a faction: it has the printed text of nine cards or more. */
const SHEET_WIDTH = 460;

/** The faction of a row that is no control: its sheet is behind the name. */
function FactionName({ name, sheet }: { name: string; sheet: FactionSheetView | null }) {
  if (!sheet) {
    return <span className="truncate">{name}</span>;
  }
  return (
    <Peek
      label={`About ${name}`}
      width={SHEET_WIDTH}
      flush
      tall
      trigger={name}
      buttonClassName={(pinned) =>
        cx(
          "min-w-0 truncate underline decoration-dotted decoration-muted/60 underline-offset-4 hover:text-text",
          pinned && "text-text",
        )
      }
    >
      <FactionSheet sheet={sheet} />
    </Peek>
  );
}

/**
 * One faction of the choice. The row is the control, and a choice closes the list; the sheet of
 * the faction is behind "?".
 */
function FactionOption({
  option,
  onChoose,
}: {
  option: LobbyFactionOptionView;
  onChoose: (intent: LobbyIntent) => void;
}) {
  const { intent } = option;
  return (
    <li className="flex min-w-0 items-center gap-1.5">
      <button
        type="button"
        aria-pressed={option.chosen}
        aria-disabled={intent === null && !option.chosen}
        title={option.taken ? `${option.name}: ${option.taken} has it` : option.name}
        className={cx(
          "flex min-h-7 min-w-0 flex-1 items-center gap-1.5 rounded px-1 text-left text-sm touch:min-h-10",
          option.chosen ? "font-mid text-accent" : "hover:bg-raised",
          option.taken && "opacity-55 hover:bg-transparent",
        )}
        onClick={() => intent && onChoose(intent)}
      >
        <span className="w-3 flex-none" aria-hidden="true">
          {option.chosen && "▸"}
        </span>
        <span className="min-w-0 flex-none truncate">{option.name}</span>
        {option.taken && (
          <span className="ml-auto min-w-0 truncate text-xs text-muted">{option.taken}</span>
        )}
      </button>
      {option.sheet && (
        <Hint label={`About ${option.name}`} width={SHEET_WIDTH} tall>
          <FactionSheet sheet={option.sheet} />
        </Hint>
      )}
    </li>
  );
}

/**
 * The choice of the faction of a seat. A choice applies at once and closes the list; Esc leaves
 * it as it is. On desktop it is open under its row, and a click outside the row leaves it too; a
 * phone shows it as a sheet over the pane, with its own "×".
 */
function FactionEditor({
  editor,
  row,
  onClose,
  dispatch,
}: {
  editor: LobbyFactionEditorView;
  /** The row of the choice: a click outside it closes the choice. None: the choice is a sheet. */
  row?: RefObject<HTMLElement | null>;
  onClose: () => void;
  dispatch: (intent: LobbyIntent) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  // The list opens under its row: the panel shows as much of it as it has room for.
  useEffect(() => {
    if (row) {
      root.current?.scrollIntoView?.({ block: "nearest" });
    }
  }, [row]);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    // Esc and a click outside the row go one level back: they close a text that is open (the
    // sheet of a faction) first, then the choice.
    const onEvent = (event: Event) => {
      if (document.querySelector('[role="note"]')) {
        return;
      }
      if (
        event instanceof KeyboardEvent
          ? event.key === "Escape"
          : !!row && !row.current?.contains(event.target as Node)
      ) {
        close.current();
      }
    };
    document.addEventListener("keydown", onEvent);
    document.addEventListener("pointerdown", onEvent);
    return () => {
      document.removeEventListener("keydown", onEvent);
      document.removeEventListener("pointerdown", onEvent);
    };
  }, [row]);
  const choose = (intent: LobbyIntent) => {
    dispatch(intent);
    onClose();
  };
  return (
    <div
      ref={root}
      role="group"
      aria-label={editor.label}
      className={cx("text-base", row && "px-5 pt-0.5 pb-2")}
    >
      <ul>
        <FactionOption option={editor.random} onChoose={choose} />
      </ul>
      {editor.groups.map((group) => (
        <section key={group.name} aria-label={group.name || "Factions"}>
          {group.name && (
            <h4 className="mt-1.5 px-1 text-muted">
              <Eyebrow>{group.name}</Eyebrow>
            </h4>
          )}
          <ul className="grid grid-cols-3 gap-x-4 phone:grid-cols-2">
            {group.options.map((option) => (
              <FactionOption key={option.id} option={option} onChoose={choose} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function PlayerRow({
  row,
  open,
  inline,
  onOpen,
  dispatch,
  validName,
}: {
  row: LobbyRowView;
  /** The choice of the faction is open in this row. */
  open: boolean;
  /** The open choice is shown under the row (desktop); else the screen shows it as a sheet. */
  inline: boolean;
  onOpen: (open: boolean) => void;
  dispatch: (intent: LobbyIntent) => void;
  validName: ValidName;
}) {
  // The name of the viewer is edited in its row. Null: the row is closed.
  const [name, setName] = useState<string | null>(null);
  const item = useRef<HTMLLIElement>(null);
  const who = (
    <>
      <Sign sign={row.sign} />
      {/* A name is cut on desktop, with the full name on hover; a phone gives it a second line. */}
      {name !== null ? (
        <NameEditor
          name={name}
          onName={setName}
          validName={validName}
          onDone={(wanted) => {
            if (wanted && wanted !== row.name) {
              dispatch({ type: "rename", name: wanted });
            }
            setName(null);
          }}
        />
      ) : (
        <span
          title={row.name}
          className={cx(
            "flex-none truncate",
            // An open seat has its buttons in the row: the word takes no more room than it needs.
            row.sign?.kind === "open"
              ? "text-muted"
              : "w-[300px] font-mid phone:w-auto phone:min-w-0 phone:flex-1 phone:whitespace-normal phone:[overflow-wrap:anywhere]",
          )}
        >
          {row.name}
          {row.you && <span className="font-normal text-muted"> (you)</span>}
          {row.become && <span className="font-normal text-accent"> (yours)</span>}
        </span>
      )}
    </>
  );
  return (
    <li
      ref={item}
      aria-current={row.you ? "true" : undefined}
      className={cx("[li+&]:border-t [li+&]:border-line/50", row.you && "bg-cyan/[.06]")}
    >
      <div className="flex min-h-9 items-center gap-2.5 px-5 py-0.5 text-base phone:flex-wrap phone:px-3 phone:py-1.5">
        {row.become ? (
          <button
            type="button"
            title="Play this seat in this tab"
            className="flex min-w-0 items-center gap-2.5 text-left hover:text-accent phone:flex-1"
            onClick={() => dispatch(row.become!)}
          >
            {who}
          </button>
        ) : (
          who
        )}
        {/* A row that is no control has the sheet of its faction behind the name. */}
        {!row.editor && (
          <span className="flex w-[180px] flex-none text-muted phone:w-auto phone:max-w-[40%]">
            <FactionName name={row.faction} sheet={row.sheet} />
          </span>
        )}
        {/* The faction of the row is the control that opens its choice, in the same column. */}
        {row.editor && (
          <span className="flex w-[180px] flex-none phone:w-auto phone:max-w-[40%]">
            <button
              type="button"
              aria-expanded={open}
              aria-label={`${row.editor.label}: ${row.faction}`}
              title="Choose the faction"
              className={cx(
                "flex min-h-7 min-w-0 items-center gap-1.5 rounded border bg-well px-2 text-left font-mid hover:border-line-strong touch:min-h-10",
                open ? "border-cyan text-accent" : "border-line",
              )}
              onClick={() => onOpen(!open)}
            >
              <span className="min-w-0 truncate">{row.faction}</span>
              <span className="flex-none text-xs text-muted" aria-hidden="true">
                ▾
              </span>
            </button>
          </span>
        )}
        {/* The kinds of an open seat wrap on a phone: up to three models follow "Random". While the
            choice of the faction is open, the row is that choice: its other actions are hidden. */}
        {!open && (
          <span
            className={cx(
              "ml-auto flex flex-none items-center gap-1.5",
              row.buttons.length > 0 &&
                "phone:min-w-0 phone:flex-1 phone:flex-wrap phone:justify-end",
            )}
          >
            {row.rename && name === null && (
              <Button
                tone="quiet"
                size="icon"
                aria-label="Change your name"
                title="Change your name"
                onClick={() => setName(row.name)}
              >
                ✎
              </Button>
            )}
            {row.buttons.map((button) => (
              <RowButton key={button.label} view={button} dispatch={dispatch} />
            ))}
            {row.remove && (
              <Button
                tone="quiet"
                size="icon"
                aria-label={row.remove.label}
                title={row.remove.label}
                onClick={() => dispatch(row.remove!.intent)}
              >
                ×
              </Button>
            )}
          </span>
        )}
      </div>
      {open && inline && row.editor && (
        <FactionEditor
          editor={row.editor}
          row={item}
          onClose={() => onOpen(false)}
          dispatch={dispatch}
        />
      )}
    </li>
  );
}

function Heading({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex min-h-10 items-center gap-2 border-b border-line bg-surface py-1 pr-3 pl-5 phone:pl-3">
      <strong className="whitespace-nowrap">{children}</strong>
      {aside}
    </div>
  );
}

/** One step of a counter. A step that cannot be taken stays in its place and says why. */
function Step({
  step,
  sign,
  dispatch,
}: {
  step: LobbyStepView;
  sign: string;
  dispatch: (intent: LobbyIntent) => void;
}) {
  return (
    <button
      type="button"
      className="size-7 text-lg text-muted hover:bg-raised hover:text-text aria-disabled:opacity-30 aria-disabled:hover:bg-transparent aria-disabled:hover:text-muted touch:size-10"
      aria-label={step.reason ? `${step.label}: ${step.reason}` : step.label}
      title={step.reason || step.label}
      aria-disabled={step.intent === null}
      onClick={() => step.intent && dispatch(step.intent)}
    >
      {sign}
    </button>
  );
}

/** The value of a setup row, as its control when the viewer may change it. */
function SetupValue({
  row,
  dispatch,
}: {
  row: LobbySetupRowView;
  dispatch: (intent: LobbyIntent) => void;
}) {
  const { control } = row;
  if (control?.kind === "counter") {
    return (
      <div className="inline-flex items-center rounded-md border border-line bg-canvas">
        <Step step={control.less} sign="−" dispatch={dispatch} />
        <output className="min-w-12 px-1 text-center font-mid tabular-nums" aria-label={row.label}>
          {row.value}
        </output>
        <Step step={control.more} sign="+" dispatch={dispatch} />
      </div>
    );
  }
  if (control?.kind === "list") {
    return (
      <Select
        aria-label={row.label}
        className="min-h-8 font-mid touch:min-h-10"
        value={control.chosen}
        onChange={(event) => {
          const chosen = control.options.find((option) => option.id === event.target.value);
          if (chosen) {
            dispatch(chosen.intent);
          }
        }}
      >
        {/* A value that the list does not have (the cards of a scenario) is shown as it is. */}
        {control.options.every((option) => option.id !== control.chosen) && (
          <option value={control.chosen}>{row.value}</option>
        )}
        {control.options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
      </Select>
    );
  }
  return (
    <span className="min-w-0 truncate font-mid" title={row.value}>
      {row.value}
    </span>
  );
}

/** The values of the setup, one row each. `dense`: in a sheet, which has its own margins. */
function SetupRows({
  view,
  dispatch,
  dense = false,
}: {
  view: LobbyView;
  dispatch: (intent: LobbyIntent) => void;
  dense?: boolean;
}) {
  return (
    <dl className={cx(dense && "mt-1 rounded border border-line")}>
      {view.setup.map((row) => (
        <div
          key={row.id}
          className={cx(
            "flex min-h-9 items-center justify-between gap-3 py-0.5 text-base [div+&]:border-t [div+&]:border-line/50",
            dense ? "px-2.5" : "px-5 phone:px-3",
          )}
        >
          <dt className="flex flex-none items-center gap-2 text-muted">
            {row.label}
            {row.help.length > 0 && (
              <Hint label={`About ${row.label}`}>
                {row.help.map((item) => (
                  <span key={item.title} className="block">
                    <strong>{item.title}.</strong> {item.text}
                  </span>
                ))}
              </Hint>
            )}
          </dt>
          <dd className="flex min-w-0 items-center">
            <SetupValue row={row} dispatch={dispatch} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

function Setup({ view, dispatch }: { view: LobbyView; dispatch: (intent: LobbyIntent) => void }) {
  return (
    <section aria-label="Setup" className="flex-none">
      <Heading>Setup</Heading>
      <SetupRows view={view} dispatch={dispatch} />
    </section>
  );
}

/**
 * The table of a running game, as a reference sheet: who sits in each seat and how the game was
 * set up. It is the lobby with nothing open (pattern 10): the rows of the players in the order
 * of their seats, with the symbol of the seat, and the rows of the setup. Only the wait has a
 * control, for the host.
 */
export function TableSheet({
  view,
  dispatch,
}: {
  view: LobbyView;
  dispatch: (intent: LobbyIntent) => void;
}) {
  return (
    <>
      {view.status && (
        <p role="status" className="mt-2 text-muted">
          {view.status}
        </p>
      )}
      <section aria-label="Players">
        <h3 className="mt-3.5 text-muted">
          <Eyebrow>Players</Eyebrow>
        </h3>
        <ul className="mt-1 rounded border border-line">
          {view.rows.map((row) => (
            <li
              key={row.id}
              aria-current={row.you ? "true" : undefined}
              className={cx(
                "flex min-h-9 flex-wrap items-center gap-x-2 px-2.5 py-0.5 text-base [li+&]:border-t [li+&]:border-line/50",
                row.you && "bg-cyan/[.06]",
              )}
            >
              {row.player !== null && <SeatSymbol seat={row.player} />}
              <Sign sign={row.sign} />
              {/* A name is cut on desktop, with the full name on hover; a phone gives it a second line. */}
              <span
                title={row.name}
                className="min-w-0 flex-1 truncate font-mid phone:whitespace-normal phone:[overflow-wrap:anywhere]"
              >
                {row.name}
                {row.you && <span className="font-normal text-muted"> (you)</span>}
              </span>
              <span className="flex max-w-[45%] flex-none text-muted">
                <FactionName name={row.faction} sheet={row.sheet} />
              </span>
              {row.buttons.map((button) => (
                <RowButton key={button.label} view={button} dispatch={dispatch} />
              ))}
            </li>
          ))}
        </ul>
      </section>
      <section aria-label="Setup">
        <h3 className="mt-3.5 text-muted">
          <Eyebrow>Setup</Eyebrow>
        </h3>
        <SetupRows view={view} dispatch={dispatch} dense />
      </section>
    </>
  );
}

function Players({
  view,
  open,
  onOpen,
  inline,
  dispatch,
  validName,
}: {
  view: LobbyView;
  /** The row whose choice of the faction is open, by its id. One at a time. */
  open: string | null;
  onOpen: (row: string | null) => void;
  /** The open choice is shown under its row (desktop), not as a sheet (phone). */
  inline: boolean;
  dispatch: (intent: LobbyIntent) => void;
  validName: ValidName;
}) {
  return (
    <section aria-label="Players" className="flex min-h-0 flex-1 flex-col">
      <Heading
        aside={
          <>
            <span className="text-sm text-muted tabular-nums">{view.filled}</span>
            {view.help.length > 0 && (
              <Hint label="About the kinds of a seat">
                {view.help.map((item) => (
                  <span key={item.title} className="block">
                    <strong>{item.title}.</strong> {item.text}
                  </span>
                ))}
              </Hint>
            )}
            <span className="ml-auto" />
            {view.shuffle && <RowButton view={view.shuffle} dispatch={dispatch} />}
            {view.fill && <RowButton view={view.fill} dispatch={dispatch} />}
          </>
        }
      >
        Players
      </Heading>
      <ul className="min-h-0 flex-1 overflow-y-auto">
        {view.rows.map((row) => (
          <PlayerRow
            key={row.id}
            row={row}
            open={open === row.id && row.editor !== null}
            inline={inline}
            onOpen={(wanted) => onOpen(wanted ? row.id : null)}
            validName={validName}
            dispatch={dispatch}
          />
        ))}
      </ul>
    </section>
  );
}

/** What the main button does, and the main button. A phone shows it under every pane. */
function Footer({
  view,
  dispatch,
  nickname,
  onNickname,
  validName,
  phone,
}: {
  view: LobbyView;
  dispatch: (intent: LobbyIntent) => void;
  nickname: string;
  onNickname: (name: string) => void;
  validName: ValidName;
  phone: boolean;
}) {
  const { footer } = view;
  if (!footer.note && !footer.main) {
    return null;
  }
  // A visitor joins with a name that the server takes: an empty one is not marked, only waited for.
  const nameless = footer.nickname && !validName(nickname.trim());
  const refused = nameless && nickname.trim() !== "";
  return (
    <form
      className="flex items-center justify-between gap-[15px] border-t border-line bg-surface px-5 py-2.5 phone:flex-col phone:items-stretch phone:gap-1.5 phone:px-3 phone:py-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (footer.main && !footer.main.disabled && !nameless) {
          dispatch(footer.main.intent);
        }
      }}
    >
      <span id="lobby-note" role="status" className="min-w-0 text-sm text-muted">
        {footer.note}
      </span>
      <div className="flex shrink-0 items-center gap-2 phone:[&>button]:flex-auto">
        {footer.nickname && (
          <label className="flex min-w-0 items-center gap-2 text-sm text-muted phone:flex-1">
            Name
            <input
              value={nickname}
              maxLength={NICKNAME_MAX}
              aria-invalid={refused}
              onChange={(event) => onNickname(event.target.value)}
              className="min-h-9 w-[220px] min-w-0 rounded border border-line bg-well px-2.5 text-base text-text aria-[invalid=true]:border-red phone:w-auto phone:flex-1 touch:min-h-11"
            />
          </label>
        )}
        {footer.main && (
          <Button
            type="submit"
            tone="primary"
            disabled={footer.main.disabled || nameless}
            aria-describedby="lobby-note"
            aria-keyshortcuts="Enter"
            iconAfter="arrow"
          >
            {footer.main.label}
            {!phone && <Kbd>Enter</Kbd>}
          </Button>
        )}
      </div>
    </form>
  );
}

type Pane = "map" | "players" | "setup";

/**
 * The lobby of a table, in the frame of the game: the map that the table will play at the left,
 * the setup and the players in the panel, and the main button in the footer. It is one screen
 * with no dialog, and the same screen for the host, a player and a visitor: what a viewer may
 * change is in the view. A change applies at once.
 */
export function LobbyScreen({
  view,
  dispatch,
  nickname,
  onNickname,
  validName,
  link,
  toast,
  other,
}: {
  view: LobbyView;
  dispatch: (intent: LobbyIntent) => void;
  /** The name that a visitor joins with. */
  nickname: string;
  onNickname: (name: string) => void;
  /** Whether the server takes a name: the join and a new name wait for one that it takes. */
  validName: ValidName;
  /** The address of the table, for the other players. Without it nobody else can come. */
  link?: string;
  toast: { id: number; text: string } | null;
  /** The way out: the start screen. A page load. */
  other?: OtherGame;
}) {
  const phone = usePhone();
  const [pane, setPane] = useState<Pane>("players");
  // The row whose choice of the faction is open, by its id. One at a time.
  const [faction, setFaction] = useState<string | null>(null);
  const [copied, setCopied] = useState<{ id: number; text: string } | null>(null);

  // Enter is the main button, as in the game.
  const now = useRef({ view, dispatch, nickname, validName });
  now.current = { view, dispatch, nickname, validName };
  useEffect(() => {
    let focusByKeyboard = false;
    const onPointer = () => (focusByKeyboard = false);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Tab") {
        focusByKeyboard = true;
      }
      const { main, nickname: named } = now.current.view.footer;
      if (event.key !== "Enter" || !main || main.disabled || !keyIsFree(event, focusByKeyboard)) {
        return;
      }
      if (named && !now.current.validName(now.current.nickname.trim())) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      now.current.dispatch(main.intent);
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPointer, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPointer, true);
    };
  }, []);

  // The board is the board of the game: of what it can do, a lobby has the look at a system.
  const onBoard = (intent: Intent) => {
    if (intent.type === "inspectSystem" || intent.type === "closeInspector") {
      dispatch(intent);
    }
  };
  const copy = () => {
    const say = (text: string) => setCopied({ id: performance.now(), text });
    if (link === undefined) {
      return;
    }
    navigator.clipboard.writeText(link).then(
      () => say("Link copied"),
      () => say(link),
    );
  };

  const footer = (
    <Footer
      view={view}
      dispatch={dispatch}
      nickname={nickname}
      onNickname={onNickname}
      validName={validName}
      phone={phone}
    />
  );
  // Until the server said the board there is none to frame.
  const board = view.board.tiles.length ? (
    <Board view={view.board} />
  ) : (
    <section aria-label="Galaxy board" className="min-h-0 min-w-0 bg-board" />
  );
  const players = (
    <Players
      view={view}
      open={faction}
      onOpen={setFaction}
      inline={!phone}
      dispatch={dispatch}
      validName={validName}
    />
  );
  // A phone shows the open choice of a faction as a sheet over the pane, with the room of the
  // footer and of the tab bar.
  const choosing = phone ? view.rows.find((row) => row.id === faction)?.editor : null;
  const sheet = choosing && (
    <Drawer title={choosing.label} onClose={() => setFaction(null)}>
      <FactionEditor editor={choosing} onClose={() => setFaction(null)} dispatch={dispatch} />
    </Drawer>
  );
  const setup = <Setup view={view} dispatch={dispatch} />;
  const latest = (copied?.id ?? 0) > (toast?.id ?? 0) ? copied : toast;

  return (
    <GameProvider seats={view.seats} dispatch={onBoard}>
      <LinkProvider>
        <ShellFrame
          accent="var(--color-cyan)"
          header={
            <header className="flex min-h-12 items-center gap-3.5 border-b border-line bg-surface px-4 py-1.5 phone:gap-x-2 phone:px-3 phone:pt-[max(6px,env(safe-area-inset-top))]">
              <div className="mr-auto flex min-w-0 flex-col text-xs leading-[1.3] text-muted">
                <strong className="truncate text-sm font-semibold text-text">{view.title}</strong>
                {view.status && (
                  <span role="status">
                    <i className="mr-1.5 mb-0.5 inline-block size-1.5 rounded-full bg-accent" />
                    {view.status}
                  </span>
                )}
              </div>
              {link !== undefined && (
                <Button tone="quiet" size="sm" onClick={copy}>
                  Copy link
                </Button>
              )}
              {other && (
                <Button tone="quiet" size="sm" onClick={() => location.assign(other.href)}>
                  {other.label}
                </Button>
              )}
            </header>
          }
          board={board}
          column={
            <>
              {setup}
              {players}
              {footer}
            </>
          }
          panes={[
            { id: "map", label: "Map", content: board },
            { id: "players", label: "Players", content: players, scrolls: true },
            { id: "setup", label: "Setup", content: setup, scrolls: true },
          ]}
          pane={pane}
          onPane={(id) => setPane(id as Pane)}
          footer={footer}
          sheet={sheet || undefined}
        >
          <Toast message={latest} />
        </ShellFrame>
      </LinkProvider>
    </GameProvider>
  );
}
