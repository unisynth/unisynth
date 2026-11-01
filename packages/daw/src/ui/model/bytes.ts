// Byte and sample-index helpers (change daw-solid-ts 9.1, step 1): base64 for GSM/WAV blobs, the GM.DLS index.
export const b64ToBytes = (s: string): Uint8Array => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
export const bytesToB64 = (b: Uint8Array): string => { let s = ""; for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000)); return btoa(s); };
/** GM.DLS instrument of a 1-based sampler index (0 = none): drums from 16385, else bank × 128 + program. */
export const gmInstrument = (idx: number): { drums: boolean; bank: number; prog: number } | null => {
  if (idx < 1) return null; const n = idx - 1, drums = n >= 16384, r = n % 16384; return { drums, bank: Math.floor(r / 128), prog: r % 128 };
};
