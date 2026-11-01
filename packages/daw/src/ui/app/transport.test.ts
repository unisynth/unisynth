// Transport state (change daw-solid-ts 9.1, steps 3–4): setters publish to the components' stores; follow hold
// only while playing with follow on; position maths in the signature.
import { expect, test } from "bun:test";
import * as tr from "../state/transport.ts";
import * as liveState from "../state/live.ts";
import * as T from "./transport.ts";

test("setters assign the live bindings; setPlaying publishes to the live store", () => {
  T.setBars(12); T.setPos(3.5); T.setLoopOn(true);
  expect([T.BARS, T.SONG_POS, T.loopOn]).toEqual([12, 3.5, true]);
  T.setPlaying(true); expect(T.playing).toBe(true); expect(liveState.playing()).toBe(true);
  T.setPlaying(false); expect(liveState.playing()).toBe(false);
});
test("follow hold: only while playing with follow on; resume clears it", () => {
  tr.setFollow(true);
  T.holdFollow(); expect(T.followHold).toBe(false);  // not playing
  T.setPlaying(true); T.holdFollow(); expect(T.followHold).toBe(true); expect(tr.followHeld()).toBe(true);
  T.resumeFollow(); expect(T.followHold).toBe(false); expect(tr.followHeld()).toBe(false);
  T.setPlaying(false);
});
test("signature maths: 7/8 has 7 beats, 3.5 quarters; bars per second at the tempo", () => {
  tr.ensureSig("7/8"); tr.setTempo(120);
  expect(T.sig()).toEqual([7, 8]); expect(T.beatsPerBar()).toBe(7); expect(T.quartersPerBar()).toBe(3.5);
  expect(T.barsPerSec()).toBeCloseTo(2 / 3.5, 12);
  expect(T.fmtPos(2)).toBe("2.1.0"); expect(T.fmtBars(1.23456)).toBe(1.23);
  tr.ensureSig("4/4");
});
