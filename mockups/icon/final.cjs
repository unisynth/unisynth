// Final unisynth icon (chosen: b3-uwave-gradient). Run: node final.cjs → writes site/icon.svg, prints the nav mark + data URI.
const fs = require("fs");
const C = { bg: "#0e1013", bg2: "#1b1e23", line: "#2c3139", osc: "#f0a04b", filt: "#3fc1c9", env: "#6fd08c" };
const f = n => +n.toFixed(1);
const uwave = x => 30 + (13 * Math.exp(-(((x - 32) / 13) ** 2)) + 3) * Math.cos(2 * Math.PI * (x - 32) / 24);
let d = "";
for (let i = 0; i <= 64; i++) { const x = 8 + 48 * i / 64; d += (i ? "L" : "M") + f(x) + " " + f(uwave(x)); }
const grad = id => `<linearGradient id="${id}" x1="8" y1="0" x2="56" y2="0" gradientUnits="userSpaceOnUse"><stop offset=".12" stop-color="${C.osc}"/><stop offset=".36" stop-color="${C.filt}"/><stop offset=".64" stop-color="${C.filt}"/><stop offset=".88" stop-color="${C.env}"/></linearGradient>`;
const wave = id => `<path d="${d}" fill="none" stroke="url(#${id})" stroke-width="5.5" stroke-linecap="round" stroke-linejoin="round"/>`;
const icon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><title>unisynth</title><defs><linearGradient id="t" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${C.bg2}"/><stop offset="1" stop-color="${C.bg}"/></linearGradient>${grad("w")}</defs><rect x="1" y="1" width="62" height="62" rx="14" fill="url(#t)" stroke="${C.line}" stroke-width="1.5"/>${wave("w")}</svg>\n`;
fs.writeFileSync(process.argv[2] || "icon.svg", icon);
// Nav mark: the wave alone, cropped to its bounds.
const mark = `<svg class="mark" viewBox="4 10 56 40" width="22" height="16" aria-hidden="true"><defs>${grad("nm")}</defs>${wave("nm")}</svg>`;
fs.writeFileSync("mark.html", mark);
fs.writeFileSync("datauri.txt", "data:image/svg+xml," + encodeURIComponent(icon.trim()).replace(/%20/g, " ").replace(/%3D/g, "=").replace(/%3A/g, ":").replace(/%2F/g, "/").replace(/%22/g, "'").replace(/%2C/g, ","));
console.log("icon bytes", icon.length, "mark bytes", mark.length);
