// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ErrorKind } from "./ErrorKind.ts";

/**
 * Protocol error notification.
 */
export type ProtocolErrorMsg = { protocol_version: number, kind: ErrorKind, message: string, };
