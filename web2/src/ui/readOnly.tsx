import { createContext, useContext, type ButtonHTMLAttributes, type ReactNode } from "react";

const Scope = createContext(false);

/**
 * Content that is read, not used: the current action in the Log. Inside it a control shows its
 * value or its state and is no control: a `Button` is not there, a `Counter` is its count, a
 * `Press` is its content, and a shortcut key (`Kbd`) is not shown. A view toggle (a group that
 * opens, a rule text behind "?") stays.
 */
export function ReadOnlyScope({ children }: { children: ReactNode }) {
  return <Scope.Provider value={true}>{children}</Scope.Provider>;
}

/** True inside a `ReadOnlyScope`. */
export const useReadOnly = () => useContext(Scope);

/**
 * A bare control that is its content (a row of a list, a token, a cell of the technology tree):
 * a button, and in a read-only scope a plain element with the same look and its state
 * (`aria-pressed` styles), which does nothing.
 */
export function Press({
  onClick,
  disabled,
  ...rest
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type">) {
  const readOnly = useReadOnly();
  if (readOnly) {
    return <span data-read-only="" {...rest} />;
  }
  return <button type="button" disabled={disabled} onClick={onClick} {...rest} />;
}
