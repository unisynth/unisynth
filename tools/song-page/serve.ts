// Serves the listening page locally with byte ranges (seeking needs them) and audio content types.
//   bun tools/song-page/serve.ts [dir] [port]     # default tools/song-page/out, 6480
import { existsSync, statSync } from "node:fs";
import { join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = process.argv[2] ?? fileURLToPath(new URL("./out", import.meta.url));
const PORT = Number(process.argv[3] ?? 6480);
const TYPES: Record<string, string> = { ".html": "text/html; charset=utf-8", ".json": "application/json", ".m4a": "audio/mp4" };

Bun.serve({
  port: PORT,
  fetch(req) {
    const p = normalize(decodeURIComponent(new URL(req.url).pathname)).replace(/^(\.\.[/\\])+/, "");
    const f = join(DIR, p.endsWith("/") ? p + "index.html" : p);
    if (!f.startsWith(DIR) || !existsSync(f) || !statSync(f).isFile()) return new Response("not found", { status: 404 });
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
