// Browser-remembered view settings (splitter sizes, block collapse, buffer, …), never in the song. localStorage
// throws in a sandboxed frame without allow-same-origin (the dashboard canvas): it falls back to memory then.
export interface Persist { get(k: string): string | null; set(k: string, v: string): void }

function make(): Persist {
  const mem = new Map<string, string>();
  let ls: Storage | null = null;
  try { ls = window.localStorage; ls.getItem("x"); } catch { ls = null; }
  return {
    get: (k) => { try { return ls ? ls.getItem(k) : mem.get(k) ?? null; } catch { return mem.get(k) ?? null; } },
    set: (k, v) => { try { if (ls) ls.setItem(k, v); else mem.set(k, v); } catch { mem.set(k, v); } },
  };
}
export const persist: Persist = typeof window === "undefined" ? { get: () => null, set: () => {} } : make();
