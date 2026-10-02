export const NICKNAME_PREFERENCE_KEY = "ti4.nickname";

// Mirrors ti4-server storage::validate_nickname. Count bytes, not UTF-16 code units.
const formatCharacters =
  /[\u00ad\u0600-\u0605\u061c\u06dd\u070f\u0890-\u0891\u08e2\u180e\u200b-\u200f\u202a-\u202e\u2060-\u206f\ufeff\ufff9-\ufffb\u{110bd}\u{110cd}\u{13430}-\u{1343f}\u{1bca0}-\u{1bca3}\u{1d173}-\u{1d17a}\u{e0001}\u{e0020}-\u{e007f}]/u;

export function validNickname(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    new TextEncoder().encode(value).length <= 64 &&
    value.trim() === value &&
    !/^\s*$/u.test(value) &&
    !/[\p{Cc}\p{Cs}]/u.test(value) &&
    !formatCharacters.test(value)
  );
}

export function preferredNickname(): string {
  const value = localStorage.getItem(NICKNAME_PREFERENCE_KEY);
  return value && validNickname(value) ? value : "";
}

export function rememberNickname(value: string): void {
  localStorage.setItem(NICKNAME_PREFERENCE_KEY, value);
}
