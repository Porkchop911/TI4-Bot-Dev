#!/usr/bin/env node
// Builds <folder>/index.html from <folder>/manifest.json: the PNGs listed there are inlined as
// data URIs, so the page is one self-contained file that can be published as an artifact.
//   node build-artifact.mjs <artifact folder>
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const folder = resolve(process.argv[2] ?? ".");
const manifestPath = join(folder, "manifest.json");
if (!existsSync(manifestPath)) throw new Error(`no manifest.json in ${folder}`);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const esc = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const inline = (name) => {
  const file = join(folder, name);
  if (!existsSync(file)) throw new Error(`missing screenshot ${name}; run the captures first`);
  return `data:image/png;base64,${readFileSync(file).toString("base64")}`;
};
// A shot is either one image (`file`) or a labelled set (`images: [{ label, file }]`, e.g. before and
// after, desktop and phone), optionally under a group heading (`group`, printed when it changes).
let lastGroup;
// Optional `groups`: [{ title, text, cards: [{ name, change, pairs: [{ label, before, after, note? }] }] }].
// Every pair is a before and an after image side by side (stacked on a narrow screen).
const image = (file, alt) => `<img alt="${esc(alt)}" loading="lazy" src="${inline(file)}">`;
const groups = (manifest.groups ?? [])
  .map(
    (group) =>
      `<section class="cardgroup"><h2>${esc(group.title)}</h2>${group.text ? `<p class="lede">${esc(group.text)}</p>` : ""}${group.cards
        .map(
          (card) =>
            `<article class="card"><h3>${esc(card.name)}</h3><p class="change">${esc(card.change)}</p>${card.pairs
              .map(
                (pair) =>
                  `<div class="pair"><p class="pair__label">${esc(pair.label)}</p><div class="pair__row"><figure><figcaption>Before</figcaption>${image(pair.before, `${card.name}, ${pair.label}, before`)}</figure><figure><figcaption>After</figcaption>${image(pair.after, `${card.name}, ${pair.label}, after`)}</figure></div>${pair.note ? `<p class="pair__note">${esc(pair.note)}</p>` : ""}</div>`,
              )
              .join("")}</article>`,
        )
        .join("")}</section>`,
  )
  .join("\n  ");

const figures = (manifest.shots ?? [])
  .map((shot) => {
    const group =
      shot.group && shot.group !== lastGroup ? `<h2 class="group">${esc(shot.group)}</h2>` : "";
    lastGroup = shot.group ?? lastGroup;
    if (shot.images) {
      const cells = shot.images
        .map(
          (image) =>
            `<figure class="cell"><figcaption class="label">${esc(image.label)}</figcaption><img alt="${esc(image.label)}" loading="lazy" src="${inline(image.file)}"></figure>`,
        )
        .join("");
      return `${group}<section>${shot.heading ? `<h3>${esc(shot.heading)}</h3>` : ""}${shot.caption ? `<p class="shot-caption">${esc(shot.caption)}</p>` : ""}<div class="grid">${cells}</div></section>`;
    }
    return `${group}<section>${shot.heading ? `<h2>${esc(shot.heading)}</h2>` : ""}<figure><img alt="${esc(shot.alt ?? shot.caption)}" src="${inline(shot.file)}"><figcaption>${esc(shot.caption)}</figcaption></figure></section>`;
  })
  .join("\n  ");
// Trusted HTML written by the folder's own manifest, printed above the shots (a table, a legend).
const intro = manifest.intro ? `<section class="intro">${manifest.intro}</section>` : "";
const notes = manifest.notes?.length
  ? `<section><h2>${esc(manifest.notesHeading ?? "Notes")}</h2><ul class="notes">${manifest.notes
      .map((n) => `<li><b>${esc(n.title)}</b>${esc(n.text)}</li>`)
      .join("")}</ul></section>`
  : "";

const html = readFileSync(join(here, "artifact.tpl.html"), "utf8")
  .replaceAll("{{TITLE}}", esc(manifest.title))
  .replaceAll("{{DESCRIPTION}}", esc(manifest.description))
  .replaceAll("{{TAG}}", esc(manifest.tag ?? "captured by web/e2e/screenshots"))
  .replace("{{SHOTS}}", () => intro + groups + figures)
  .replace("{{NOTES}}", () => notes);
writeFileSync(join(folder, "index.html"), html);
console.log(`built ${join(folder, "index.html")} (${Math.round(html.length / 1024)} KB, ${(manifest.shots ?? []).length + (manifest.groups ?? []).reduce((n, g) => n + g.cards.reduce((m, c) => m + c.pairs.length * 2, 0), 0)} shots)`);
