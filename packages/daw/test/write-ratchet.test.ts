// Write ratchet (change daw-solid-ts 2.7, design D2): the session modules (formerly app.js, split in task 9.1) still
// write the song model directly through the createMutable proxies. Ported panels move those writes into actions; the
// store becomes `createStore` (where a direct write is silently ignored) only when none are left. This counts the
// direct writes in SESSION and fails when the count changes: higher = a new direct write (put it in an action),
// lower = lower BASELINE. Moving code between these files does not change the count.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// 98 since 9.1 step 9: the library GM load's `Object.assign(d.values, …)` / `delete d.values[…]` were missed while the
// local was named `inst` (the Object.assign / delete patterns know only master|t|d|c|n|tr); same writes, now seen.
// 97 since unify-v2-algorithms: a library load no longer moves lanes to `filt1.cutoff` (it refuses instead).
// 96 since 9.3: a song load restores the new master through the store instead of deleting and assigning its keys.
// 95 since add-fudge-softsynth: the panel's parameter edit writes through `setDeviceValue` (state/song.ts).
const BASELINE = 95;
/** The session code that still writes the model (app.js until daw-solid-ts 9.1 split it into these). */
const SESSION = ["ui/App.tsx", "ui/app/audition.ts", "ui/app/mixer.ts", "ui/app/tabs.ts", "ui/model/params.ts", "ui/model/track.ts", "ui/model/bytes.ts", "ui/app/session.ts", "ui/app/core.ts", "ui/app/transport.ts", "ui/app/engine.ts", "ui/app/automation.ts", "ui/app/arrange.ts", "ui/app/theme.ts", "ui/app/clips.ts", "ui/app/roll.ts", "ui/app/device.ts", "ui/app/screens.ts", "ui/app/samples.ts", "ui/app/files.ts", "ui/app/tracks.ts"];

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

test("direct model writes in the session modules only go down", () => {
  let n = 0; const lines: string[] = [];
  for (const f of SESSION) { const r = countModelWrites(readFileSync(new URL(`../src/${f}`, import.meta.url), "utf8")); n += r.n; lines.push(...r.lines.map((l) => `${f}:${l}`)); }
  if (n > BASELINE) console.log(`the session modules have ${n} lines with direct model writes (baseline ${BASELINE}). Move the new ones into an action.\n` + lines.join("\n"));
  if (n < BASELINE) console.log(`the session modules are down to ${n} lines with direct model writes: lower BASELINE in ${import.meta.path} to ${n}.`);
  expect(n).toBe(BASELINE);
});

test("the counter sees a direct write and ignores DOM writes", () => {
  expect(countModelWrites("t.vol = 3;\nel.style.left = 1;\nnote.sel = true;\nh.textContent = 'x';").n).toBe(2);
  expect(countModelWrites("tracks.splice(1, 0, c);\nt.clips.push(c);\nObject.assign(master, d);").n).toBe(3);
  expect(countModelWrites("if (a.b === c) x();\nconst y = z;").n).toBe(0);
});
