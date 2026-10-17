// The wire types of the server and the engine, generated from Rust (`generated/`, written by
// `cargo wire-web`). Import from here.
export * from "./generated/index.ts";
// The one value, by name too: Node (the e2e of web) loads this dir as CommonJS, where a value that
// only `export *` passes on is not seen by an ES module that imports it.
export { PROTOCOL_VERSION } from "./generated/index.ts";
