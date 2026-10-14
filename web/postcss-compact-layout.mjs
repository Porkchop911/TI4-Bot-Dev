/**
 * PostCSS plugin: the phone layout is decided in JS (src/hooks/useCompactLayout.ts) and mirrored on
 * `<html data-layout="compact">`, because a phone in "desktop site" mode lays the page out at ~980 px
 * and no media query can see that it is a phone. Every top-level `@media` block that holds on a
 * portrait phone (width/height queries only) is therefore repeated right after itself with its rules
 * scoped to `:where(:root[data-layout-forced])`, so the same rules also apply when the layout is forced
 * is set. The media-query path stays untouched (it is the fallback when scripts fail).
 */
const PHONE = { width: 390, height: 844 };

function evalQuery(query) {
  const cleaned = query
    .trim()
    .replace(/^(only\s+)?screen\s+and\s+/i, "")
    .replace(/^(only\s+)?screen$/i, "");
  if (cleaned === "") return true;
  for (const part of cleaned.split(/\s+and\s+/i)) {
    const m = /^\(\s*(max|min)-(width|height)\s*:\s*(\d+(?:\.\d+)?)px\s*\)$/i.exec(part.trim());
    if (!m) return null;
    const value = Number(m[3]);
    const size = PHONE[m[2].toLowerCase()];
    const ok = m[1].toLowerCase() === "max" ? size <= value : size >= value;
    if (!ok) return false;
  }
  return true;
}

/** True when the media list is made of width/height queries only and one of them holds on a portrait phone. */
export function holdsOnPhone(params) {
  let known = false;
  for (const query of params.split(",")) {
    const result = evalQuery(query);
    if (result === null) return false;
    if (result) known = true;
  }
  return known;
}

const PREFIX = ':where(:root[data-layout-forced])';

export function scopeSelector(selector) {
  const s = selector.trim();
  if (s.startsWith(":root")) return ':root[data-layout-forced]' + s.slice(":root".length);
  if (/^html(?![\w-])/.test(s)) return 'html[data-layout-forced]' + s.slice("html".length);
  return `${PREFIX} ${s}`;
}

const plugin = () => ({
  postcssPlugin: "compact-layout-attribute",
  Once(root) {
    const found = [];
    root.walkAtRules("media", (atRule) => {
      if (atRule.parent?.type === "root" && holdsOnPhone(atRule.params)) found.push(atRule);
    });
    for (const original of found) {
      let anchor = original;
      for (const node of original.nodes ?? []) {
        const moved = node.clone();
        if (moved.type === "rule") moved.selectors = moved.selectors.map(scopeSelector);
        else if (moved.type !== "comment") continue;
        original.parent.insertAfter(anchor, moved);
        anchor = moved;
      }
    }
  },
});
plugin.postcss = true;
export default plugin;
