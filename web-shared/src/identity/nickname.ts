// The name of a player at a table of the server: what the server takes
// (`ti4-server` `storage::validate_nickname`), and where a browser keeps the last one.

/** The localStorage key of the name that the player gave last, for both apps. */
export const NICKNAME_KEY = "ti4.nickname";

/** The server counts the UTF-8 bytes of a name, not its characters. */
export const NICKNAME_MAX_BYTES = 64;

// The formatting code points that the server refuses, bidirectional overrides among them.
const formatCharacters =
  /[­؀-؅؜۝܏࢐-࢑࣢᠎​-‏‪-‮⁠-⁯﻿￹-￻\u{110bd}\u{110cd}\u{13430}-\u{1343f}\u{1bca0}-\u{1bca3}\u{1d173}-\u{1d17a}\u{e0001}\u{e0020}-\u{e007f}]/u;

/**
 * Whether the server takes `value` as a nickname: trimmed, not only whitespace, at most 64
 * UTF-8 bytes, with no control or formatting code point (and no lone surrogate, which a Rust
 * string cannot hold). `fixtures/nickname/cases.json` is checked against both.
 */
export function validNickname(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    new TextEncoder().encode(value).length <= NICKNAME_MAX_BYTES &&
    value.trim() === value &&
    !/^\s*$/u.test(value) &&
    !/[\p{Cc}\p{Cs}]/u.test(value) &&
    !formatCharacters.test(value)
  );
}
