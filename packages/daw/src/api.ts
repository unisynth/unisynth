// /api/music client (change add-synth-daw): list, read and write songs, presets and blobs under project/music;
// library catalog and GM bank. Writes are validated by the server (422 with JSON-pointer errors).
//
// Static mode (`<meta name="daw-static">`, written by build-static.ts): no server. The same data is read from
// files next to the page — `data/music.json`, `data/library.json`, `data/bank.dls`, `data/files/<path>` — and a
// write downloads the file instead (a read-only preview cannot store anything).
export type FileEntry = { path: string; kind: "song" | "patch" | "blob"; size: number; mtime: number };

const STATIC = typeof document !== "undefined" && !!document.querySelector('meta[name="daw-static"]');

/** Saves `body` as a browser download named after the last path segment. */
function download(path: string, body: string | Uint8Array) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([body as BlobPart], { type: path.endsWith(".json") ? "application/json" : "application/octet-stream" }));
  a.download = path.split("/").pop()!;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

export const api = {
  static: STATIC,
  async list(): Promise<FileEntry[]> {
    const r = await fetch(STATIC ? "data/music.json" : "/api/music");
    return r.ok ? r.json() : [];
  },
  async read(path: string): Promise<string> {
    const r = await fetch(STATIC ? "data/files/" + path.split("/").map(encodeURIComponent).join("/") : "/api/music?path=" + encodeURIComponent(path));
    if (!r.ok) throw new Error(`${path}: ${r.status} ${await r.text()}`);
    return r.text();
  },
  async write(path: string, body: string | Uint8Array): Promise<{ ok: boolean; errors?: { path: string; message: string }[]; error?: string }> {
    if (STATIC) {
      download(path, body);
      return { ok: true };
    }
    const r = await fetch("/api/music?path=" + encodeURIComponent(path), { method: "PUT", body: body as BodyInit });
    if (r.status === 422) return r.json();
    return r.ok ? { ok: true } : { ok: false, error: `${r.status} ${await r.text()}` };
  },
  async library(): Promise<any> {
    const r = await fetch(STATIC ? "data/library.json" : "/api/library");
    return r.ok ? r.json() : null;
  },
  async bank(): Promise<Uint8Array | null> {
    const r = await fetch(STATIC ? "data/bank.dls" : "/api/bank");
    return r.ok ? new Uint8Array(await r.arrayBuffer()) : null;
  },
};
