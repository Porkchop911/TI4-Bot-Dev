// A text of the engine (`Phrase`: a template whose `{key}` stands for `refs[key]`, a `Subject`)
// in the names of an app. The app names each kind of subject (a seat by its player, a card by its
// printed name); this fills the template and has no words of its own.
import type { Phrase, Subject } from "./index.ts";

/** The subject of one `kind`. */
export type SubjectOf<K extends Subject["kind"]> = Extract<Subject, { kind: K }>;

/** How an app says each kind of subject. */
export type SubjectNames = { [K in Subject["kind"]]: (subject: SubjectOf<K>) => string };

/** `subject` as `names` says it. */
export function nameSubject(subject: Subject, names: SubjectNames): string {
  const name = names[subject.kind] as (subject: Subject) => string;
  return name(subject);
}

/**
 * The text of `phrase` with each `{key}` said by `name`. A `{key}` without a subject stays as it
 * is.
 */
export function fillPhrase(
  phrase: Phrase,
  name: (subject: Subject, key: string) => string,
): string {
  return phrase.text.replace(/\{(\w+)\}/g, (token, key: string) => {
    const subject = phrase.refs[key];
    return subject ? name(subject, key) : token;
  });
}

/** The text of `phrase` with each subject said by `names`. */
export function sayPhrase(phrase: Phrase, names: SubjectNames): string {
  return fillPhrase(phrase, (subject) => nameSubject(subject, names));
}
