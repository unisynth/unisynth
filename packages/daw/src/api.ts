// /api/music client (change add-synth-daw): list, read and write songs, presets and blobs under project/music;
// library catalog and GM bank. Writes are validated by the server (422 with JSON-pointer errors).
export type FileEntry = { path: string; kind: "song" | "patch" | "blob"; size: number; mtime: number };

export const api = {
  async list(): Promise<FileEntry[]> {
    const r = await fetch("/api/music");
    return r.ok ? r.json() : [];
  },
  async read(path: string): Promise<string> {
    const r = await fetch("/api/music?path=" + encodeURIComponent(path));
    if (!r.ok) throw new Error(`${path}: ${r.status} ${await r.text()}`);
    return r.text();
  },
  async write(path: string, body: string | Uint8Array): Promise<{ ok: boolean; errors?: { path: string; message: string }[]; error?: string }> {
    const r = await fetch("/api/music?path=" + encodeURIComponent(path), { method: "PUT", body: body as BodyInit });
    if (r.status === 422) return r.json();
    return r.ok ? { ok: true } : { ok: false, error: `${r.status} ${await r.text()}` };
  },
  async library(): Promise<any> {
    const r = await fetch("/api/library");
    return r.ok ? r.json() : null;
  },
  async bank(): Promise<Uint8Array | null> {
    const r = await fetch("/api/bank");
    return r.ok ? new Uint8Array(await r.arrayBuffer()) : null;
  },
};
