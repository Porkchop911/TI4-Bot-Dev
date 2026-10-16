export type ClassValue = string | false | null | undefined;

export const cx = (...parts: ClassValue[]) => parts.filter(Boolean).join(" ");
