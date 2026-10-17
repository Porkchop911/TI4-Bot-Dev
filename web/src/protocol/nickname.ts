import { NICKNAME_KEY, validNickname } from "@ti4/shared/identity/nickname.ts";

export { NICKNAME_KEY, validNickname };

export function preferredNickname(): string {
  const value = localStorage.getItem(NICKNAME_KEY);
  return value && validNickname(value) ? value : "";
}

export function rememberNickname(value: string): void {
  localStorage.setItem(NICKNAME_KEY, value);
}
