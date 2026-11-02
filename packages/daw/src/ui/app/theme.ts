// Colour tokens of style.css (change daw-solid-ts 9.1, step 5). Static: no theme switching, so each is read once —
// getComputedStyle per knob/lane draw cost seconds per undo on large songs (bench/ui-profile.ts).
const cache = new Map<string, string>();
export function css(v: string): string {
  let x = cache.get(v);
  if (x === undefined) cache.set(v, x = getComputedStyle(document.documentElement).getPropertyValue(v).trim());
  return x;
}
