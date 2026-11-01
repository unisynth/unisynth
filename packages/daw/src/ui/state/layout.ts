// Splitter sizes (change daw-solid-ts 8.2): one signal per CSS variable, applied to the root element and remembered
// in the browser under `daw<var>` (e.g. `daw--browser-w` = "260px"). Unset = the stylesheet's default.
import { createSignal, type Accessor } from "solid-js";
import { persist } from "./persist.ts";

const sizes = new Map<string, [Accessor<number | null>, (v: number) => void]>();

/** Size in px of a splitter variable (null until set or remembered), and its setter (clamped by the caller). */
export function sizeOf(cssVar: string): [Accessor<number | null>, (v: number) => void] {
  let s = sizes.get(cssVar);
  if (!s) {
    const saved = parseFloat(persist.get("daw" + cssVar) ?? "");
    const [get, set] = createSignal<number | null>(Number.isFinite(saved) ? saved : null);
    const apply = (v: number) => {
      set(v); persist.set("daw" + cssVar, v + "px");
      if (typeof document !== "undefined") document.documentElement.style.setProperty(cssVar, v + "px");
    };
    if (get() != null && typeof document !== "undefined") document.documentElement.style.setProperty(cssVar, get() + "px");
    s = [get, apply]; sizes.set(cssVar, s);
  }
  return s;
}
