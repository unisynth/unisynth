// Transport state of the running session (change daw-solid-ts 9.1, steps 3–4): song length, playhead, loop, play
// and follow-hold as live bindings (read anywhere, assigned through the setters, which also publish to the
// components' stores, ui/state/transport.ts and ui/state/live.ts), plus position maths in the song's signature.
import * as tr from "../state/transport.ts";
import * as liveState from "../state/live.ts";
import { E } from "./session.ts";
import { say } from "./core.ts";

/** Bars the arrangement shows past the song's end. */
export const PAD_BARS = 8;
/** Song length in bars; grows to cover the last clip (never shrinks by itself). */
export let BARS = 32;
/** Playhead in bars, 1-based. */
export let SONG_POS = 1;
/** Loop range in bars [start, end): edited in place. */
export const LOOP: [number, number] = [1, 5];
/** Loop off by default: playback runs through the song (L or ⟲ turns it on). */
export let loopOn = false;
export let playing = false;
/** Follow paused by a hand scroll or zoom (Ableton's rule); F, Play or a ruler click resume it. */
export let followHold = false;

export function setBars(v: number): void { BARS = v; }
export function setPos(b: number): void { SONG_POS = b; }
export function setLoopOn(v: boolean): void { loopOn = v; }
export function setPlaying(v: boolean): void { playing = v; liveState.setPlaying(v); }

// Time signature: beats per bar in the signature's unit (7/8 → 7 eighths); one quarter = 960 ticks.
export const sig = (): [number, number] => tr.sig().split("/").map(Number) as [number, number];
export const beatsPerBar = (): number => sig()[0];
export const quartersPerBar = (): number => tr.quartersPerBar(tr.sig());
export const fmtPos = (b: number): string => tr.fmtPos(b, tr.sig());
/** Bars as typed (two decimals at most). */
export const fmtBars = (b: number): number => +b.toFixed(2);
export const barsPerSec = (): number => (tr.tempo() || 128) / 60 / quartersPerBar();
/** Engine frame of a song position, and back. */
export const frameOf = (b: number): number => Math.max(0, Math.round((b - 1) / barsPerSec() * E.rate));
export const barOf = (f: number): number => 1 + f / E.rate * barsPerSec();

export function holdFollow(): void {
  if (followHold || !playing || !tr.follow()) return;
  followHold = true; tr.setFollowHeld(true); say("follow paused while you look around · F resumes");
}
export function resumeFollow(): void { followHold = false; tr.setFollowHeld(false); }
