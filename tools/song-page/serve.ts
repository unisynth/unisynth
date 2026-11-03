// Serves the listening page locally: byte ranges, the wasm content type (streaming compile needs it) and the GM bank at
// /daw/data/bank.dls (the page's default BANK_URL ../daw/data/bank.dls, as on unisynth.exceed.hu).
//   bun tools/song-page/serve.ts [dir] [port]     # default tools/song-page/out, 6480
//   GM_DLS=<gm.dls>  the bank to serve (default ~/Downloads/64k-tools/synth-lab/ws/gm.dls)
import { existsSync, statSync } from "node:fs";
import { join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = process.argv[2] ?? fileURLToPath(new URL("./out", import.meta.url));
const PORT = Number(process.argv[3] ?? 6480);
const BANK = process.env.GM_DLS ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab/ws/gm.dls");
const TYPES: Record<string, string> = { ".html": "text/html; charset=utf-8", ".json": "application/json", ".wasm": "application/wasm" };

Bun.serve({
  port: PORT,
  fetch(req) {
    const p = normalize(decodeURIComponent(new URL(req.url).pathname)).replace(/^(\.\.[/\\])+/, "");
    const f = p === "/daw/data/bank.dls" ? BANK : join(DIR, p.endsWith("/") ? p + "index.html" : p);
    if ((f !== BANK && !f.startsWith(DIR)) || !existsSync(f) || !statSync(f).isFile()) return new Response("not found", { status: 404 });
    const type = TYPES[f.slice(f.lastIndexOf("."))] ?? "application/octet-stream";
    const size = statSync(f).size;
    const m = /^bytes=(\d*)-(\d*)$/.exec(req.headers.get("range") ?? "");
    const cache = type.startsWith("text/html") ? "no-cache" : "public, max-age=300";
    if (!m) return new Response(Bun.file(f), { headers: { "content-type": type, "accept-ranges": "bytes", "cache-control": cache } });
    const start = m[1] ? +m[1] : size - +m[2];
    const end = m[1] && m[2] ? Math.min(+m[2], size - 1) : size - 1;
    if (start > end || start >= size) return new Response(null, { status: 416, headers: { "content-range": `bytes */${size}` } });
    return new Response(Bun.file(f).slice(start, end + 1), {
      status: 206,
      headers: { "content-type": type, "accept-ranges": "bytes", "content-range": `bytes ${start}-${end}/${size}`, "content-length": String(end - start + 1) },
    });
  },
});
console.log(`http://localhost:${PORT}/  (${DIR})`);
