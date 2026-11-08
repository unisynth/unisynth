// Generates unisynth icon candidates (SVG) + preview.html. Run: node gen.js
const fs = require("fs");
const C = { bg: "#0e1013", bg2: "#1b1e23", line: "#2c3139", fg: "#e4e8ee", osc: "#f0a04b", filt: "#3fc1c9", env: "#6fd08c" };
const f = n => +n.toFixed(2);
const path = (fn, x0, x1, n = 120) => {
  let d = "";
  for (let i = 0; i <= n; i++) { const x = x0 + (x1 - x0) * i / n; d += (i ? "L" : "M") + f(x) + " " + f(fn(x)); }
  return d;
};
const sm = (a, b, t) => { t = Math.min(1, Math.max(0, (t - a) / (b - a))); return t * t * (3 - 2 * t); };
const stroke = (d, c, w) => `<path d="${d}" fill="none" stroke="${c}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>`;
const tile = (id, inner) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
<defs>
<linearGradient id="${id}-bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${C.bg2}"/><stop offset="1" stop-color="${C.bg}"/></linearGradient>
<linearGradient id="${id}-g" x1="8" y1="0" x2="56" y2="0" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="${C.osc}"/><stop offset=".5" stop-color="${C.filt}"/><stop offset="1" stop-color="${C.env}"/></linearGradient>
</defs>
<rect x="1" y="1" width="62" height="62" rx="14" fill="url(#${id}-bg)" stroke="${C.line}" stroke-width="1.5"/>
${inner}
</svg>
`;

// A "merge": three voices (osc / filt / env colours) converge into one wave.
const A = (() => {
  const lanes = [[-12, C.osc, 1.5, 0], [0, C.filt, 2, 1.2], [12, C.env, 2.5, 2.4]];
  const fn = (o, fr, ph) => x => {
    const t = (x - 9) / 46, m = sm(.3, .62, t);
    return 32 + (1 - m) * (o + 3.2 * Math.sin(2 * Math.PI * fr * t + ph)) + m * (-7 * Math.sin(2 * Math.PI * 2.2 * (t - .5)));
  };
  return tile("a", lanes.map(([o, c, fr, ph]) => stroke(path(fn(o, fr, ph), 9, 40), c, 3.6)).join("\n")
    + "\n" + stroke(path(fn(0, 2, 1.2), 37, 55, 60), C.fg, 4.2));
})();

// B "u-wave": one wave, its deep central trough is the "u"; gradient osc → filt → env.
const uwave = x => 30 + (13 * Math.exp(-(((x - 32) / 13) ** 2)) + 3) * Math.cos(2 * Math.PI * (x - 32) / 24);
const B = tile("b", stroke(path(uwave, 8, 56), "url(#b-g)", 5.5));

// C "three waveforms": saw, sine, square stacked (the oscillator families unisynth merges).
const C3 = (() => {
  const saw = x => { const p = ((x - 12) / 13) % 1; return 20 + 5 - 10 * p; };
  const sawD = (() => { let d = "M12 25"; for (let k = 0; k < 3; k++) { const x = 12 + 13 * k; d += `L${x + 13} 15L${x + 13} 25`; } return d.replace(/L51 25$/, ""); })();
  const sine = x => 32 + 5 * Math.sin(2 * Math.PI * (x - 12) / 13);
  let sq = "M12 49"; for (let k = 0; k < 3; k++) { const x = 12 + 13 * k; sq += `L${x} 39L${x + 6.5} 39L${x + 6.5} 49L${x + 13} 49`; }
  return tile("c", stroke(sawD, C.osc, 3.4) + "\n" + stroke(path(sine, 12, 51), C.filt, 3.4) + "\n" + stroke(sq, C.env, 3.4));
})();

// D "bars + wave": the site's three-bar mark with one white wave running through it.
const D = (() => {
  const bars = [[14, 44, C.osc], [28, 32, C.filt], [42, 20, C.env]]
    .map(([x, h, c]) => `<rect x="${x}" y="${52 - h}" width="9" height="${h}" rx="2.5" fill="${c}"/>`).join("\n");
  const w = x => 30 + 9 * Math.sin(2 * Math.PI * (x - 8) / 24);
  return tile("d", bars + "\n" + stroke(path(w, 8, 56), C.bg, 7) + "\n" + stroke(path(w, 8, 56), C.fg, 3.2));
})();

const A2 = (() => {
  const lanes = [[-11, C.osc, 0], [0, C.filt, 2.1], [11, C.env, 4.2]];
  const fn = (o, ph) => x => {
    const t = (x - 9) / 46, m = sm(.18, .5, t), g = sm(.42, .66, t);
    return 32 + (1 - m) * (o + 2.6 * Math.sin(2 * Math.PI * 1.6 * t + ph)) - g * 9 * Math.sin(2 * Math.PI * 1.7 * (t - .5));
  };
  return tile("a2", lanes.map(([o, c, ph]) => stroke(path(fn(o, ph), 9, 33), c, 3.8)).join("\n")
    + "\n" + stroke(path(fn(0, 2.1), 30, 55, 80), C.fg, 4.4));
})();
// B2: the u-wave in three solid segments (osc | filt u | env)
const B2 = tile("b2", stroke(path(uwave, 8, 21.5, 40), C.osc, 5.5) + "\n" + stroke(path(uwave, 42.5, 56, 40), C.env, 5.5) + "\n" + stroke(path(uwave, 20, 44, 80), C.filt, 5.5));
// B3: u-wave gradient with clean stops (no brown mid-blend)
const B3 = tile("b3", stroke(path(uwave, 8, 56), "url(#b3-g3)", 5.5)).replace("</defs>", `<linearGradient id="b3-g3" x1="8" y1="0" x2="56" y2="0" gradientUnits="userSpaceOnUse"><stop offset=".12" stop-color="${C.osc}"/><stop offset=".36" stop-color="${C.filt}"/><stop offset=".64" stop-color="${C.filt}"/><stop offset=".88" stop-color="${C.env}"/></linearGradient></defs>`);

const icons = { "a-merge": A, "a2-merge": A2, "b-uwave": B, "b2-uwave-segments": B2, "b3-uwave-gradient": B3 };
for (const [k, v] of Object.entries(icons)) fs.writeFileSync(`${k}.svg`, v);

const sizes = [16, 32, 64, 128, 256];
const rows = Object.keys(icons).map(k => `<section><h2>${k}</h2>
<div class="row dark">${sizes.map(s => `<img src="${k}.svg" width="${s}" height="${s}">`).join("")}</div>
<div class="row light">${sizes.map(s => `<img src="${k}.svg" width="${s}" height="${s}">`).join("")}</div>
<div class="tab"><span class="fav"><img src="${k}.svg" width="16" height="16">unisynth · by robson / Exceed</span></div>
</section>`).join("\n");
fs.writeFileSync("preview.html", `<!doctype html><meta charset="utf-8"><title>unisynth icon candidates</title>
<style>
body{margin:0;padding:24px;background:#0e1013;color:#e4e8ee;font:14px system-ui;display:grid;grid-template-columns:repeat(2,1fr);gap:20px}
h2{font:600 13px ui-monospace,monospace;margin:0 0 10px;color:#8a94a3}
section{background:#1b1e23;border:1px solid #2c3139;border-radius:10px;padding:16px}
.row{display:flex;align-items:end;gap:18px;padding:14px;border-radius:8px;margin-bottom:8px}
.dark{background:#121417}.light{background:#f2f3f5}
.tab{background:#2a2d33;border-radius:8px 8px 0 0;padding:8px 12px;width:max-content}
.fav{display:flex;gap:8px;align-items:center;font-size:12px;color:#d6dae0}
</style>
${rows}`);
console.log("wrote", Object.keys(icons).join(", "));
