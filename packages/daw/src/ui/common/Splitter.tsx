// Splitter (change daw-solid-ts 8.2): a focusable separator that sizes a panel through a root CSS variable. Drag, or
// arrows (Shift = 40 px steps), clamped to [min, max]; the size is remembered in the browser (state/layout.ts).
// Mounted into a `display: contents` slot so it stays a direct grid/flex item of its container.
import { type JSX } from "solid-js";
import { render } from "solid-js/web";
import { sizeOf } from "../state/layout.ts";

export interface SplitterProps {
  id: string; label: string; cssVar: string; min: number; max: number;
  /** "vertical" resizes along x (a column), "horizontal" along y (a row). */
  orientation: "vertical" | "horizontal";
  /** Current size in px while none is set (the stylesheet's default, measured). */
  measure(): number;
  /** Size in px for a pointer position. */
  at(e: PointerEvent): number;
}

export function mountSplitter(slot: HTMLElement, p: SplitterProps): () => void {
  return render(() => <Splitter {...p} />, slot);
}

export function Splitter(p: SplitterProps): JSX.Element {
  const [size, setSize] = sizeOf(p.cssVar);
  const clamp = (v: number) => Math.round(Math.min(p.max, Math.max(p.min, v)));
  const cur = () => size() ?? p.measure();
  const fwd = p.orientation === "vertical" ? "ArrowRight" : "ArrowDown", back = p.orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
  const down = (e: PointerEvent) => {
    const el = e.currentTarget as HTMLElement; el.setPointerCapture(e.pointerId);
    const mv = (ev: PointerEvent) => setSize(clamp(p.at(ev)));
    el.addEventListener("pointermove", mv); el.addEventListener("pointerup", () => el.removeEventListener("pointermove", mv), { once: true });
  };
  const key = (e: KeyboardEvent) => {
    const d = (e.shiftKey ? 40 : 10) * (e.key === fwd ? 1 : e.key === back ? -1 : 0);
    if (d) { e.preventDefault(); setSize(clamp(cur() + d)); }
  };
  return <div class="splitter" id={p.id} role="separator" aria-orientation={p.orientation} aria-label={p.label} tabindex="0"
    aria-valuenow={Math.round(cur())} aria-valuemin={p.min} aria-valuemax={p.max} onPointerDown={down} onKeyDown={key} />;
}
