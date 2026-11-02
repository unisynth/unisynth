// Arrangement view state of the running session (change daw-solid-ts 9.1, step 5): horizontal zoom in px per bar
// (live binding; the components read `arrange.bw()`, ui/state/arrange.ts).
/** Arrangement px per bar. */
export let BW = 44;
export function setBW(v: number): void { BW = v; }
