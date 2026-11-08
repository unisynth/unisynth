// DAW dev server: serves the DAW (packages/daw/dist) at /daw on 127.0.0.1 plus its file API (/api/music, /api/library,
// /api/bank: see music.ts), confined to <root>/project/music. Split out of the 64k intro's control server, which also
// served the intro editor; only the DAW part moved here.
// Hardening: /api/* requests carrying a foreign Origin (another website in the user's browser) are refused.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { musicApi } from "./music.ts";
import { loadSynth, type Synth } from "./synth.ts";

/**
 * DAW responses (change add-wasm-threads, design D6): cross-origin isolation for the page (COOP + COEP), so the threaded
 * engine can share memory with its helper Workers, and CORP on every DAW file. Workers get the same headers on their
 * scripts. The DAW loads only same-origin resources.
 */
const ISOLATED = {
  "cross-origin-opener-policy": "same-origin",
  "cross-origin-embedder-policy": "require-corp",
  "cross-origin-resource-policy": "same-origin",
};

export const ROOT = new URL("../../../", import.meta.url).pathname;
export const DEFAULT_PORT = 6464;

export type Server = { url: string; port: number; stop(): Promise<void> };

/** `headless` is accepted for compatibility with the old control server's callers and ignored (no editor page here). */
export async function startServer(opts: { root?: string; port?: number; headless?: boolean; build?: boolean; dawDist?: string } = {}): Promise<Server> {
  const root = opts.root ?? ROOT;
  const dawDist = opts.dawDist ?? join(ROOT, "packages/daw/dist");
  if (opts.build && !opts.dawDist) {
    const r = Bun.spawnSync(["bun", "build.ts"], { cwd: join(ROOT, "packages/daw"), stdout: "inherit", stderr: "inherit" });
    if (r.exitCode !== 0) throw new Error("DAW build failed");
  }
  let synth: Synth | null | undefined;
  const engine = () => {
    if (synth === undefined) {
      try {
        synth = loadSynth();
      } catch (e) {
        console.error("[daw] engine module:", (e as Error).message);
        synth = null;
      }
    }
    return synth;
  };
  const token = randomUUID();
  const page = (file: string) => readFileSync(file, "utf8").replace("<head>", `<head><meta name="x-64k-token" content="${token}">`);

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: opts.port ?? DEFAULT_PORT,
    async fetch(req, srv) {
      const url = new URL(req.url);
      const origin = req.headers.get("origin");
      if (origin !== null && origin !== `http://127.0.0.1:${srv.port}` && url.pathname.startsWith("/api/")) {
        return new Response("foreign origin", { status: 403 });
      }
      const music = await musicApi(root, req, url, engine);
      if (music) return music;
      if (url.pathname === "/") return Response.redirect("/daw/", 302);
      if (url.pathname === "/daw") return Response.redirect(`/daw/${url.search}`, 302); // relative asset URLs need the slash
      if (url.pathname === "/daw/" || url.pathname === "/daw/index.html") {
        const f = join(dawDist, "index.html");
        if (!existsSync(f)) return new Response("DAW not built: run `bun packages/daw/build.ts`", { status: 404 });
        return new Response(page(f), { headers: { "content-type": "text/html", ...ISOLATED } });
      }
      if (url.pathname.startsWith("/daw/")) {
        const file = url.pathname.slice(5);
        const f = join(dawDist, file);
        if (file.includes("..") || !existsSync(f)) return new Response("not found", { status: 404 });
        const type = f.endsWith(".wasm") ? "application/wasm" : f.endsWith(".js") ? "text/javascript" : f.endsWith(".css") ? "text/css" : f.endsWith(".html") ? "text/html" : "application/octet-stream";
        return new Response(Bun.file(f), { headers: { "content-type": type, ...ISOLATED } });
      }
      return new Response("not found", { status: 404 });
    },
  });
  return {
    url: `http://127.0.0.1:${server.port}/`,
    port: server.port,
    async stop() {
      server.stop(true);
    },
  };
}
