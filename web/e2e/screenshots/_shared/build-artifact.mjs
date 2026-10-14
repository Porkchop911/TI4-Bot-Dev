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

const figures = manifest.shots
  .map((shot) => {
    const file = join(folder, shot.file);
    if (!existsSync(file)) throw new Error(`missing screenshot ${shot.file}; run the captures first`);
    const data = readFileSync(file).toString("base64");
    return `<section>${shot.heading ? `<h2>${esc(shot.heading)}</h2>` : ""}<figure><img alt="${esc(shot.alt ?? shot.caption)}" src="data:image/png;base64,${data}"><figcaption>${esc(shot.caption)}</figcaption></figure></section>`;
  })
  .join("\n  ");
const notes = manifest.notes?.length
  ? `<section><h2>${esc(manifest.notesHeading ?? "Notes")}</h2><ul class="notes">${manifest.notes
      .map((n) => `<li><b>${esc(n.title)}</b>${esc(n.text)}</li>`)
      .join("")}</ul></section>`
  : "";

const html = readFileSync(join(here, "artifact.tpl.html"), "utf8")
  .replaceAll("{{TITLE}}", esc(manifest.title))
  .replaceAll("{{DESCRIPTION}}", esc(manifest.description))
  .replaceAll("{{TAG}}", esc(manifest.tag ?? "captured by web/e2e/screenshots"))
  .replace("{{SHOTS}}", () => figures)
  .replace("{{NOTES}}", () => notes);
writeFileSync(join(folder, "index.html"), html);
console.log(`built ${join(folder, "index.html")} (${Math.round(html.length / 1024)} KB, ${manifest.shots.length} shots)`);
