// Static, server-less DAW build for a private preview (e.g. https://preview.judo.technology/dsp/): the page from
// packages/daw/dist plus a snapshot of what the server's /api routes return — the song list, every file under
// project/music (incl. the git-ignored released/ songs), the library catalog and the GM bank — as plain files.
// The page runs read-only (api.ts static mode: Save downloads the file).
//
// Usage: GM_DLS=<gm.dls> bun packages/daw/build-static.ts [outDir]   (default packages/daw/build/dsp)
// The bank and the released songs are third-party: the output is for private testing, never a public release.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { startServer } from "../cli/src/server.ts";
import { buildDaw, DIST } from "./build.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));

export async function buildStatic(out = join(ROOT, "packages/daw/build/dsp"), opts: { wasm?: boolean } = {}) {
  await buildDaw(opts);
  rmSync(out, { recursive: true, force: true });
  cpSync(DIST, out, { recursive: true });
  const html = join(out, "index.html");
  writeFileSync(html, readFileSync(html, "utf8").replace("<head>", '<head><meta name="daw-static" content="1">'));

  const s = await startServer({ port: 0, root: ROOT, build: false, dawDist: DIST, headless: false });
  try {
    const get = async (p: string) => {
      const r = await fetch(s.url + p.replace(/^\//, ""));
      if (!r.ok) throw new Error(`${p}: ${r.status}`);
      return new Uint8Array(await r.arrayBuffer());
    };
    const put = (rel: string, b: Uint8Array) => {
      const f = join(out, "data", rel);
      mkdirSync(dirname(f), { recursive: true });
      writeFileSync(f, b);
    };
    const list = await get("/api/music");
    put("music.json", list);
    for (const e of JSON.parse(new TextDecoder().decode(list))) put("files/" + e.path, await get("/api/music?path=" + encodeURIComponent(e.path)));
    put("library.json", await get("/api/library"));
    const bank = await fetch(s.url + "api/bank");
    if (bank.ok) put("bank.dls", new Uint8Array(await bank.arrayBuffer()));
    return { out, files: JSON.parse(new TextDecoder().decode(list)).length, bank: bank.ok };
  } finally {
    await s.stop();
  }
}

if (import.meta.main) {
  const r = await buildStatic(process.argv[2]);
  console.log(`static DAW in ${r.out}: ${r.files} files, bank ${r.bank ? "included" : "none"}`);
}
