import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { shortcuts, type ExplorationDeck, type GameSession, type Intent } from "../../model";
import { Badge, Button, Dialog, Drawer, InlineNote, Toast, usePhoneScreen } from "../../ui";
import { ActionFooter, footerOf } from "../action/ActionPanel";
import { StepSheet } from "../action/StepSheet";
import { Board } from "../board/Board";
import { GameProvider } from "../context";
import { LinkProvider } from "../link";
import { LogPanel } from "../log/LogPanel";
import { PlayerTable } from "../players/PlayerTable";
import { ReferenceDrawer } from "./Reference";
import { ShellFrame } from "./ShellFrame";
import { OVERLAY, keyIsFree } from "./shortcuts";
import { type OtherGame, Toolbar } from "./Toolbar";

const ACCENT = {
  draft: "var(--color-gold)",
  live: "var(--color-cyan)",
  done: "var(--color-green)",
};

type Pane = "map" | "players" | "log";

/**
 * The intents that ask the game what follows from a choice of the viewer: an answer is sent, a
 * draft starts or changes, a step is taken back. What is only staged on the screen (a count, a
 * toggle, an option before its button) does not ask the game.
 */
const asksTheGame = (intent: Intent): boolean => {
  switch (intent.type) {
    case "confirmApply":
    case "commitEdit":
    case "endTurn":
    case "reaction":
    case "undo":
    case "redo":
    case "startOver":
    case "removeEntry":
      return true;
    case "flow":
      return intent.action === "resolve" || intent.action === "play";
    case "workspace":
      return intent.mode === "draft";
    case "pickAction":
      return intent.action === "tactical";
    default:
      return false;
  }
};

/**
 * The game screen: one toolbar, then the board with the open step as a sheet over it, and the right
 * column (player table, log). The right column has one fixed width. The design target is 1920×1080
 * or larger; see AGENTS.md. A portrait phone shows the map, the players and the log one at a time,
 * with the footer of the action under each of them.
 * It needs a `GameSession` and nothing else. State that only changes what is in view lives here.
 */
export function GameShell({
  session,
  other,
  settings,
  settingsOpen = false,
  table,
}: {
  session: GameSession;
  other?: OtherGame;
  /**
   * The content of the sheet "Table": who sits in each seat and how the game was set up. A
   * game with no table has none. It is there only while the sheet is open.
   */
  table?: ReactNode;
  /**
   * The content of the Dev sheet. `close` shuts the sheet, for an action that ends it. The
   * sheet is shown when the session has a capability for it.
   */
  settings?: (close: () => void) => ReactNode;
  /** The Dev sheet was open before the shell was there: it stays open. */
  settingsOpen?: boolean;
}) {
  const { view: latest } = session;
  /**
   * The viewer sent a choice that the game answers (`asksTheGame`), and the game has not answered
   * it yet: the next change of the pane of the decision follows from it.
   */
  const acted = useRef(false);
  // While the game has not yet answered what follows from the viewer's choice, the screen stays
  // as it was: the pane of the next step is not known, and no pane shows a step with nothing in it.
  const settled = useRef(latest);
  if (!latest.settling) {
    settled.current = latest;
  }
  const view = latest.settling && acted.current ? settled.current : latest;
  // The Dev sheet has the tools for testing: it is there when the game has one of them.
  const dev = Object.values(session.capabilities).some(Boolean) ? settings : undefined;
  const [drawer, setDrawer] = useState<string | null>(settingsOpen && dev ? "settings" : null);
  const [tableOpen, setTableOpen] = useState(true);
  const [applyOpen, setApplyOpen] = useState(false);
  /** The exploration deck that a planet type opened: the sheet shows it on top. */
  const [deck, setDeck] = useState<ExplorationDeck | null>(null);
  const toggleDrawer = (id: string) => {
    setDeck(null);
    setDrawer((now) => (now === id ? null : id));
  };
  const phone = usePhoneScreen();
  // The pane where the open choice is made: a seat is picked in the player table, every other
  // choice is made on the map, where the open step is a sheet over the board.
  const asked: Pane | null = view.players.pick
    ? "players"
    : view.board.task?.interactive || view.board.targeting || view.board.blocks || view.asked
      ? "map"
      : null;
  const lastChoice = useRef(asked);
  // A sent answer is not a step: the pane of the choice is the one before, until the game answers.
  const choiceOn = view.answered && acted.current ? lastChoice.current : asked;
  // The game opens on the pane of its choice. After that the pane changes with a tap on a tab, and
  // when a choice of the viewer opens the next step of the viewer in another pane. Anything else
  // (another player, a bot, a window that opens) only marks the tab of the choice.
  const [pane, setPane] = useState<Pane>(() => choiceOn ?? "map");
  const lastReply = useRef(view.reply);
  // Before the paint: the next step is never shown in the pane that the viewer leaves.
  useLayoutEffect(() => {
    const replied = view.reply !== lastReply.current;
    lastReply.current = view.reply;
    if (choiceOn !== lastChoice.current) {
      lastChoice.current = choiceOn;
      // The first change of the pane after the viewer's choice is its next step. With no
      // decision open the viewer waits for the game, and what opens next does not follow from
      // the choice.
      if (choiceOn && acted.current) {
        setPane(choiceOn);
      }
      acted.current = false;
    }
    // The game answered the choice: what changes later does not follow from it, also when the
    // next step is in the same pane.
    if (replied) {
      acted.current = false;
    }
  }, [choiceOn, view.reply]);

  const dispatch = useCallback(
    (intent: Intent) => {
      if (intent.type === "openApply") {
        return setApplyOpen(true);
      }
      if (intent.type === "openReference") {
        setDeck(intent.sheet === "exploration" ? intent.deck : null);
        return setDrawer(intent.sheet);
      }
      if (intent.type === "inspectLogEntry") {
        setDrawer(null);
      }
      // An undo takes the viewer back also while the viewer waits for the game.
      if ((lastChoice.current || intent.type === "undo") && asksTheGame(intent)) {
        acted.current = true;
      }
      session.dispatch(intent);
    },
    [session],
  );
  useEffect(() => {
    if (!view.apply) {
      setApplyOpen(false);
    }
  }, [view.apply]);

  // Shortcut keys of the open action. Esc closes an open reference sheet first.
  const keys = useRef({ view: view.action, drawer, dispatch });
  keys.current = { view: view.action, drawer, dispatch };
  useEffect(() => {
    // Focus that Tab moved belongs to the keyboard; focus that a click left behind does not.
    let focusByKeyboard = false;
    const onPointer = () => (focusByKeyboard = false);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Tab") {
        focusByKeyboard = true;
      }
      const now = keys.current;
      // Esc closes one thing: an open dialog, menu or rule text goes first.
      if (event.key === "Escape" && now.drawer && !document.querySelector(OVERLAY)) {
        return setDrawer(null);
      }
      if (!keyIsFree(event, focusByKeyboard)) {
        return;
      }
      const intent = shortcuts(now.view).get(event.key.toLowerCase());
      if (!intent) {
        return;
      }
      // The key is used: the control that has focus must not act on it too.
      event.preventDefault();
      event.stopPropagation();
      now.dispatch(intent);
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPointer, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPointer, true);
    };
  }, []);

  const reference = drawer && drawer !== "table" && (
    <ReferenceDrawer
      id={drawer}
      deck={deck}
      view={view.reference}
      players={view.players.rows}
      onClose={() => setDrawer(null)}
    />
  );
  const close = () => setDrawer(null);
  // Dev is a sheet like the reference sheets, in the same place.
  const sheet =
    drawer === "settings"
      ? dev && (
          <Drawer title="Dev" onClose={close}>
            {dev(close)}
          </Drawer>
        )
      : drawer === "table"
        ? table && (
            <Drawer title="Table" onClose={close}>
              {table}
            </Drawer>
          )
        : reference;
  const players = (
    <PlayerTable
      view={view.players}
      open={tableOpen || phone}
      onToggle={() => setTableOpen(!tableOpen)}
      onOpenSeat={(seat) => toggleDrawer(`player:${seat}`)}
    />
  );
  // The action that is played now, read-only, under the turns since the viewer's last turn.
  const log = <LogPanel action={view.action} status={view.toolbar.status} />;
  // The open step is a sheet over the board. An open fleet, the open landings and the open build
  // of a production have its room; their check mark gives it back.
  const boardSheet = view.board.origin || view.board.landing || view.board.production;
  const step = !boardSheet && (
    <StepSheet view={view.action} status={view.toolbar.status} asked={view.asked} />
  );

  return (
    <GameProvider seats={view.seats} dispatch={dispatch}>
      <LinkProvider>
        <ShellFrame
          accent={ACCENT[view.accent]}
          header={
            <Toolbar
              view={view.toolbar}
              drawer={drawer}
              onDrawer={toggleDrawer}
              phone={phone}
              other={other}
              settings={!!dev}
              table={!!table}
            />
          }
          board={
            <Board view={view.board}>
              {step}
              {sheet}
            </Board>
          }
          column={
            <>
              {players}
              {log}
            </>
          }
          panes={[
            { id: "map", label: "Map", content: <Board view={view.board}>{step}</Board> },
            { id: "players", label: "Players", content: players, scrolls: true },
            { id: "log", label: "Log", content: log, scrolls: true },
          ]}
          pane={pane}
          onPane={(id) => setPane(id as Pane)}
          marked={choiceOn}
          sheet={sheet}
          footer={
            // An open fleet has the room of the footer, and so have the open landings and the
            // open build of a production. The check mark of the sheet gives the footer back.
            !(pane === "map" && boardSheet) && <ActionFooter view={footerOf(view.action)} />
          }
        >
          <Dialog
            open={applyOpen && !!view.apply}
            onClose={() => setApplyOpen(false)}
            lead={<Badge tone="draft">Private draft</Badge>}
            title="Apply to Live?"
            description="These recorded choices are committed to the live game in order."
            actions={
              <>
                <Button tone="quiet" autoFocus onClick={() => setApplyOpen(false)}>
                  Cancel
                </Button>
                <Button
                  tone="primary"
                  iconAfter="arrow"
                  onClick={() => {
                    setApplyOpen(false);
                    session.dispatch({ type: "confirmApply" });
                  }}
                >
                  Apply to Live
                </Button>
              </>
            }
          >
            <ul className="my-3.5 rounded border border-line">
              {view.apply?.items.map((item) => (
                <li key={item} className="px-3 py-[9px] text-sm [li+&]:border-t [li+&]:border-line">
                  {item}
                </li>
              ))}
            </ul>
            <InlineNote strong={view.apply?.cost}>{view.apply?.note}</InlineNote>
          </Dialog>
          <Toast message={view.toast} />
        </ShellFrame>
      </LinkProvider>
    </GameProvider>
  );
}
