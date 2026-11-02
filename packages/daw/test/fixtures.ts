// Shared browser-test fixtures (packages/daw/test): a generated 50-track song for the arrangement windowing and the
// spec budget tests.
export const BIG_TRACKS = 50;

/** The demo's bass track (instrument + delay, one automation lane) 50 times, every one routed to the master. */
export function bigSong(demo: any, n = BIG_TRACKS): any {
  const bass = demo.tracks.find((t: any) => t.id === "bass");
  const tracks = Array.from({ length: n }, (_, i) => ({ ...structuredClone(bass), id: `t${i + 1}`, name: `Track ${i + 1}` }));
  return { ...demo, tracks, clips: demo.clips.filter((c: any) => c.id === "bass-a"),
    master: { ...demo.master, receives: tracks.map((t) => ({ from: t.id, channel: "main", volume: 1 })) } };
}
