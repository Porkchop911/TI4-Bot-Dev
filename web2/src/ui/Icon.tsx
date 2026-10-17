import { cx } from "./cx";

const PATHS = {
  check: '<path d="m5 12 4 4L19 6"/>',
  lock: '<rect x="6" y="10" width="12" height="10" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>',
  arrow: '<path d="M5 12h14m-5-5 5 5-5 5"/>',
  alert: '<path d="m12 3 10 17H2L12 3Z"/><path d="M12 9v4m0 3v.1"/>',
  edit: '<path d="m15 4 5 5-10 10-6 1 1-6L15 4Z"/><path d="m12 7 5 5"/>',
  minus: '<path d="M6 12h12"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  undo: '<path d="M8 6 3 11l5 5"/><path d="M3 11h11a5 5 0 0 1 0 10h-3"/>',
  redo: '<path d="m16 6 5 5-5 5"/><path d="M21 11H10a5 5 0 0 0 0 10h3"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  carrier: '<path d="M4 9h16v6H4zM7 6h10v3M7 15v3m5-3v3m5-3v3M2 12h2m16 0h2"/>',
  flagship: '<path d="m12 2 6 7 2 11H4L6 9l6-7Zm0 5v9m-4-3h8M9 20v2m6-2v2"/>',
  warsun: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M4 12h5m6 0h5"/>',
  dreadnought: '<path d="m12 2 5 6 2 12H5L7 8l5-6Zm0 4v10M7 11h10"/>',
  cruiser: '<path d="m12 3 7 15-7-3-7 3L12 3Zm0 5v7"/>',
  destroyer: '<path d="m12 4 7 15-7-4-7 4L12 4ZM8 12h8"/>',
  fighter: '<path d="m12 4 2 7 7 6-9-3-9 3 7-6 2-7Zm0 10v6"/>',
  infantry: '<circle cx="12" cy="5" r="2"/><path d="M12 8v7m-5-4 5-2 5 2m-5 4-4 6m4-6 4 6"/>',
  mech: '<rect x="8" y="4" width="8" height="7" rx="1"/><path d="M6 11h12v5H6zM9 16v4m6-4v4M4 12v4m16-4v4"/>',
  pds: '<path d="M5 20h14M8 20v-5h8v5M12 15V9m0 0 6-5"/>',
  dock: '<circle cx="12" cy="12" r="3"/><path d="M12 3v6m0 6v6M3 12h6m6 0h6"/>',
  token: '<path d="M12 4 21 19H3Z"/>',
  card: '<rect x="6" y="3" width="12" height="18" rx="2"/><path d="M9 8h6m-6 4h6"/>',
  flag: '<path d="M6 21V3m0 1h12l-3 4.5 3 4.5H6"/>',
  planet: '<circle cx="12" cy="12" r="6"/><path d="M6 9c-6 1-5 7 6 5s13-8 6-6"/>',
  coin: '<circle cx="12" cy="12" r="8"/><path d="M14.5 9.5c-.6-1-1.5-1.5-2.7-1.5-1.5 0-2.5.8-2.5 2s1 1.7 2.7 2 2.7.8 2.7 2-1 2-2.7 2c-1.3 0-2.3-.5-2.9-1.5M12 6.5V8m0 8v1.5"/>',
  tech: '<path d="M9 3h6m-5 0v6l-5 9a2 2 0 0 0 1.8 3h10.4a2 2 0 0 0 1.8-3l-5-9V3M7.5 15h9"/>',
  target:
    '<circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="1"/><path d="M12 2v4m0 12v4M2 12h4m12 0h4"/>',
  pickup: '<path d="M5 20c0-7 4-11 13-11m-4-4 4 4-4 4"/>',
  // Planet types, after the symbols of the planet cards.
  cultural: '<path d="M3 9 12 4l9 5H3Zm3 3v6m4-6v6m4-6v6m4-6v6M4 20h16"/>',
  hazardous:
    '<path d="m12 3 2.2 5.2L20 7l-2.8 4.9L20 17l-5.8-1.2L12 21l-2.2-5.2L4 17l2.8-5.1L4 7l5.8 1.2L12 3Z"/>',
  industrial:
    '<circle cx="12" cy="12" r="3"/><path d="M12 3v3m0 12v3M3 12h3m12 0h3M5.6 5.6l2.2 2.2m8.4 8.4 2.2 2.2m0-12.8-2.2 2.2m-8.4 8.4-2.2 2.2"/><circle cx="12" cy="12" r="6.5"/>',
  frame: '<path d="M4 9V4h5m6 0h5v5m0 6v5h-5m-6 0H4v-5"/>',
  // The values of a seat in the player table.
  star: '<path d="m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9L12 3Z"/>',
  // Resources and influence, after the planet value icons of web/ (a gem and a crown).
  resources: '<path d="M12 2.5 20 10.5 12 21.5 4 10.5Z"/><path d="M4 10.5h16"/>',
  influence: '<path d="M3 20h18M5 16.5 7.5 7.5l4.5 4.5 4.5-4.5 2.5 9Z"/>',
  commodity: '<path d="M4 8 12 4l8 4v8l-8 4-8-4V8Z"/><path d="m4 8 8 4 8-4M12 12v8"/>',
  secret:
    '<rect x="5" y="3" width="14" height="18" rx="2"/><circle cx="12" cy="10.5" r="2"/><path d="M12 12.5V16"/>',
  speaker:
    '<path d="M9.7 7.2 13.2 3.7l7.1 7.1-3.5 3.5Z"/><path d="M13.2 10.8 4.5 19.5"/><path d="M13 21h8"/>',
  passed: '<circle cx="12" cy="12" r="8"/><path d="M10 9v6m4-6v6"/>',
  // A command token of the tactic or the strategy pool; `token` is the fleet pool.
  tokenDown: '<path d="M12 20 3 5h18Z"/>',
  // The history mode: the screen shows a point of the past.
  history:
    '<path d="M4 12a8 8 0 1 0 2.4-5.7L4 8.7"/><path d="M4 4v4.7h4.7"/><path d="M12 8v4l3 2"/>',
} as const;

export type IconName = keyof typeof PATHS;
export const ICON_NAMES = Object.keys(PATHS) as IconName[];

export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <svg
      className={cx("inline-block shrink-0 align-middle", className ?? "size-4")}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: PATHS[name] }}
    />
  );
}
