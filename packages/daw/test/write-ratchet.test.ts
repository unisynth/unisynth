// Write ratchet (change daw-solid-ts 2.7, design D2): app.js still writes the song model directly through the
// createMutable proxies. Ported panels move those writes into actions; the store becomes `createStore` (where a
// direct write is silently ignored) only when none are left. This counts the direct writes still in app.js and
// fails when the count changes: higher = a new direct write (put it in an action), lower = lower BASELINE.
// Task 9.1 splits app.js into modules: the moved code is still counted (SPLIT), so the baseline drops only when a
// write is really replaced, not when it changes file.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const BASELINE = 97;
const BASELINE = 96;
/** Modules split out of app.js (daw-solid-ts 9.1), counted with it. */
const SPLIT = ["ui/model/params.ts", "ui/model/track.ts", "ui/model/bytes.ts", "ui/app/session.ts", "ui/app/core.ts", "ui/app/transport.ts", "ui/app/engine.ts", "ui/app/automation.ts", "ui/app/arrange.ts", "ui/app/theme.ts", "ui/app/clips.ts", "ui/app/roll.ts", "ui/app/device.ts", "ui/app/screens.ts", "ui/app/samples.ts", "ui/app/files.ts", "ui/app/tracks.ts"];

/** Assignment targets that are DOM, canvas, engine, view or local state, not the song model (`store[…]` in the knob
 * is `d.values`: counted; `ip.map` is a cache on the function). */
const NOT_MODEL = /^(ip|dg|el|e|b|k|h|g|box|btn|lane|li|s|ph|ph2|cursorEl|shade|keys|grid|gridc|velc|ah|al|nb|m|i|o|wrap|row|dlg|d\.close|document|window|location|navigator|liveCost|status|ctx|x\.style|style|v|LOOP|E|av|peaks|fre|fim|taps|out|db|mono|labels|old|drag|aDrag|rDrag|ccDrag|clipboard|laneVals|previewSrc|vf|pr)\b|\.(style|textContent|innerHTML|innerText|hidden|value|className|title|tabIndex|width|height|scroll\w*|dataset|on\w+|disabled|checked|selectedIndex|href|download|src|_\w+)\b/;

export function countModelWrites(src: string): { n: number; lines: string[] } {
  const lines: string[] = [];
  src.split("\n").forEach((line, i) => {
    const code = line.replace(/\/\/.*$/, "").replace(/(["'`])(?:\\.|(?!\1).)*\1/g, '""');
    let hit = false;
    for (const m of code.matchAll(/([A-Za-z_$][\w$]*(?:\??\.[\w$]+|\[[^\]]+\])+)\s*(?:=|\+=|-=|\*=|\/=|\?\?=|\|\|=)(?!=)/g))
      if (!NOT_MODEL.test(m[1])) hit = true;
    if (/\.(clips|notes|devices|sends|lanes|pts|tracks|samples|pats)\.(push|splice|sort|unshift|pop|shift|reverse)\(/.test(code)) hit = true;
    if (/\b(tracks|master)\.(push|splice|sort|unshift|pop|shift|reverse)\(|Object\.assign\((master|t|d|c|n|tr)\b|delete (master|t|d|c|n)\b/.test(code)) hit = true;
    if (hit) lines.push(`${i + 1}: ${line.trim().slice(0, 120)}`);
  });
  return { n: lines.length, lines };
}

test("direct model writes in app.js (and the modules split from it) only go down", () => {
  let n = 0; const lines: string[] = [];
  for (const f of ["app.js", ...SPLIT]) { const r = countModelWrites(readFileSync(new URL(`../src/${f}`, import.meta.url), "utf8")); n += r.n; lines.push(...r.lines.map((l) => `${f}:${l}`)); }
  if (n > BASELINE) console.log(`app.js has ${n} lines with direct model writes (baseline ${BASELINE}). Move the new ones into an action.\n` + lines.join("\n"));
  if (n < BASELINE) console.log(`app.js is down to ${n} lines with direct model writes: lower BASELINE in ${import.meta.path} to ${n}.`);
  expect(n).toBe(BASELINE);
});

test("the counter sees a direct write and ignores DOM writes", () => {
  expect(countModelWrites("t.vol = 3;\nel.style.left = 1;\nnote.sel = true;\nh.textContent = 'x';").n).toBe(2);
  expect(countModelWrites("tracks.splice(1, 0, c);\nt.clips.push(c);\nObject.assign(master, d);").n).toBe(3);
  expect(countModelWrites("if (a.b === c) x();\nconst y = z;").n).toBe(0);
});
