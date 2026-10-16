// Writes the app icons to public/: icon.svg and the PNG sizes that the web manifest names.
// Run: node scripts/make-icons.mjs. It uses the Chromium of Playwright; the output is committed.
import { writeFile } from "node:fs/promises";
import { chromium } from "@playwright/test";

const out = new URL("../public/", import.meta.url);
// The colours are the tokens of src/styles/theme.css: canvas, the hex of the board, gold, cyan.
const hex = (radius) =>
  Array.from({ length: 6 }, (_, corner) => {
    const angle = (Math.PI / 180) * (60 * corner - 90);
    return `${(256 + radius * Math.cos(angle)).toFixed(1)},${(256 + radius * Math.sin(angle)).toFixed(1)}`;
  }).join(" ");

/** One system of the board with a command token. `scale` keeps the drawing inside a launcher mask. */
const svg = (scale) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#0b1017"/>
  <g transform="translate(256 256) scale(${scale}) translate(-256 -256)" stroke-linejoin="round">
    <polygon points="${hex(214)}" fill="#131c28" stroke="#edbd7b" stroke-width="14"/>
    <polygon points="${hex(172)}" fill="none" stroke="#8cd5ef" stroke-width="7" stroke-opacity=".7"/>
    <path d="M176 196h160l-80 138Z" fill="#edbd7b" stroke="#edbd7b" stroke-width="12"/>
  </g>
</svg>
`;

const browser = await chromium.launch();
const page = await browser.newPage();
const png = async (name, size, scale) => {
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(
    `<style>*{margin:0}svg{display:block;width:${size}px;height:${size}px}</style>${svg(scale)}`,
  );
  await writeFile(new URL(name, out), await page.screenshot());
};
await writeFile(new URL("icon.svg", out), svg(1));
await png("icon-192.png", 192, 1);
await png("icon-512.png", 512, 1);
// A launcher cuts a maskable icon to a circle of 80 % of the width.
await png("icon-maskable-512.png", 512, 0.72);
await browser.close();
