import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { shortcuts, type ExplorationDeck, type GameSession, type Intent } from "../../model";
import { Badge, Button, Dialog, Drawer, InlineNote, LiveRegion, Toast, usePhone } from "../../ui";
import { ActionFooter, ActionPanel, footerOf } from "../action/ActionPanel";
import { Board } from "../board/Board";
import { GameProvider } from "../context";
import { LinkProvider } from "../link";
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

type Pane = "map" | "players" | "action";

/** Intents that only change what is in view: they are no choice of the viewer. */
const VIEW_ONLY = new Set<Intent["type"]>([
  "inspectSystem",
  "closeInspector",
  "findSystem",
  "inspectLogEntry",
  "openReference",
  "openApply",
]);

/**
 * The game screen: one toolbar, then the board and the right column (player table, action panel).
 * The right column has one fixed width. The design target is 1920×1080 or larger; see AGENTS.md.
 * A portrait phone shows the same three parts one at a time, with the footer of the action under
 * each of them.
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
  /** The viewer chose something while a decision was open, and its next step is not yet in view. */
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
  const phone = usePhone();
  // The pane where the open choice is made: the board, the player table, or the action.
  const choiceOn: Pane | null = view.players.pick
    ? "players"
    : view.board.task?.interactive || view.board.targeting
      ? "map"
      : view.asked
        ? "action"
        : null;
  // The game opens on the pane of its choice. After that the pane changes with a tap on a tab, and
  // when a choice of the viewer opens the next step of the viewer in another pane. Anything else
  // (another player, a bot, a window that opens) only marks the tab of the choice.
  const [pane, setPane] = useState<Pane>(() => choiceOn ?? "action");
  const lastChoice = useRef(choiceOn);
  // The game moved on: an entry of the log for each event.
  const events = view.reference.log.reduce((sum, round) => sum + round.entries.length, 0);
  const lastEvents = useRef(events);
  // Before the paint: the next step is never shown in the pane that the viewer leaves.
  useLayoutEffect(() => {
    const moved = events !== lastEvents.current;
    lastEvents.current = events;
    if (choiceOn === lastChoice.current) {
      // The next step is in the same pane: a later change does not follow from the choice.
      if (moved) {
        acted.current = false;
      }
      return;
    }
    lastChoice.current = choiceOn;
    // The first change of the pane after the viewer's choice is its next step. With no decision
    // open the viewer waits for the game, and what opens next does not follow from the choice.
    if (choiceOn && acted.current) {
      setPane(choiceOn);
    }
    acted.current = false;
  }, [choiceOn, events]);

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
      if (lastChoice.current && !VIEW_ONLY.has(intent.type)) {
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
  const board = <Board view={view.board} />;

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
          board={<Board view={view.board}>{sheet}</Board>}
          column={
            <>
              {players}
              <ActionPanel view={view.action} reveal={view.reveal} />
            </>
          }
          panes={[
            { id: "map", label: "Map", content: board },
            { id: "players", label: "Players", content: players, scrolls: true },
            {
              id: "action",
              label: "Action",
              content: <ActionPanel view={view.action} reveal={view.reveal} footer={false} />,
            },
          ]}
          pane={pane}
          onPane={(id) => setPane(id as Pane)}
          marked={choiceOn}
          sheet={sheet}
          footer={
            // An open fleet has the room of the footer, and so have the open landings and the
            // open build of a production. The check mark of the sheet gives the footer back.
            !(
              pane === "map" &&
              (view.board.origin || view.board.landing || view.board.production)
            ) && <ActionFooter view={footerOf(view.action)} />
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
          <LiveRegion text={view.announcement} />
        </ShellFrame>
      </LinkProvider>
    </GameProvider>
  );
}
