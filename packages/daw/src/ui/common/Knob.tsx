// Rotary knob (change daw-solid-ts 6.2): a slider whose value comes from an accessor (the song store, or the
// automation value at the playhead), so a write anywhere updates just this knob. Gestures: ↑/→ ↓/← (Shift finer),
// Home/End; drag up/down (160 px = full range, Shift 800 px); double-click resets to `def`. Each gesture is one
// undo step: `begin()` before, `end(before, message)` after (keys commit in a microtask, as before the port).
import type { JSX } from "solid-js";

export interface KnobProps {
  label: string;
  /** Accessible name when it differs from the visible label. */
  aria?: string;
  value: () => number;
  text: (v: number) => string;
  def: number;
  /** Writes the new value (model + engine); called for every step of a gesture. */
  input(v: number): void;
  begin(): unknown;
  end(before: unknown, msg: string): void;
  onGrab?(): void;
  onRelease?(): void;
  mod?: boolean;
  auto?: () => boolean;
  ovr?: () => boolean;
  title?: () => string;
  param?: string;
}

const R = 12, C = 15, A0 = -135;
const pt = (deg: number) => [C + R * Math.sin((deg * Math.PI) / 180), C - R * Math.cos((deg * Math.PI) / 180)] as const;
const [X0, Y0] = pt(A0), [X2, Y2] = pt(135);
const TRACK = `M${X0} ${Y0} A${R} ${R} 0 1 1 ${X2} ${Y2}`;

export function Knob(p: KnobProps): JSX.Element {
  const a = () => A0 + 270 * p.value(), end = () => pt(a());
  const set = (v: number) => p.input(Math.min(1, Math.max(0, v)));
  const key = (e: KeyboardEvent) => {
    const st = e.shiftKey ? 0.002 : 0.01, before = p.begin(), v = p.value();
    queueMicrotask(() => p.end(before, `${p.label} ${p.text(p.value())}`));
    if (e.key === "ArrowUp" || e.key === "ArrowRight") set(v + st);
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") set(v - st);
    else if (e.key === "Home") set(0);
    else if (e.key === "End") set(1);
    else return;
    e.preventDefault();
  };
  const down = (e: PointerEvent) => {
    const el = e.currentTarget as HTMLElement, y0 = e.clientY, v0 = p.value(), before = p.begin();
    el.setPointerCapture(e.pointerId); p.onGrab?.();
    const mv = (ev: PointerEvent) => set(v0 + (y0 - ev.clientY) / (ev.shiftKey ? 800 : 160));
    el.addEventListener("pointermove", mv);
    el.addEventListener("pointerup", () => { el.removeEventListener("pointermove", mv); p.onRelease?.(); p.end(before, `${p.label} ${p.text(p.value())}`); }, { once: true });
  };
  const reset = () => { const before = p.begin(); set(p.def); p.end(before, `${p.label} reset to ${p.text(p.value())}`); };
  return <div class="knob" classList={{ mod: !!p.mod, auto: !!p.auto?.(), ovr: !!p.ovr?.() }} tabIndex={0} role="slider" aria-label={p.aria ?? p.label}
    aria-valuemin="0" aria-valuemax="1" aria-valuenow={p.value().toFixed(3)} aria-valuetext={p.text(p.value())}
    title={p.title?.()} data-param={p.param} onKeyDown={key} onPointerDown={down} onDblClick={reset}>
    <svg viewBox="0 0 30 30" aria-hidden="true">
      <path class="trk" d={TRACK} fill="none" stroke-width="3" stroke-linecap="round" />
      <path class="arc" d={`M${X0} ${Y0} A${R} ${R} 0 ${a() - A0 > 180 ? 1 : 0} 1 ${end()[0]} ${end()[1]}`} fill="none" stroke-width="3" stroke-linecap="round" />
      <line class="ptr" x1={C} y1={C} x2={end()[0]} y2={end()[1]} stroke-width="1.5" />
    </svg>
    <span class="v num">{p.text(p.value())}</span><span class="n">{p.label}</span>
  </div>;
}
