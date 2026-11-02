// Panel tabs of the running session (change daw-solid-ts 9.1, step 9): Piano roll / Device / Mixer, roving focus with
// the arrow keys. A panel's content exists only while its tab shows (design D5).
import { setRollShown } from "../state/view.ts";
import { setMixerShown } from "../mixer.tsx";
import * as devst from "../state/device.ts";
import { centreRoll, drawRoll } from "./roll.ts";
import { updateWatch } from "./device.ts";

let tabs: HTMLElement[] = [];
/** Shows the tab with id `id` ("tab-roll", "tab-dev", "tab-mix") and its panel. */
export function showTab(id: string): void {
  const t = tabs.find(x => x.id === id); if (!t) return;
  if (t.id !== "tab-roll") setRollShown(false);  // unmount while still laid out: detachRoll keeps the scroll position
  for (const x of tabs) { const on = x === t; x.setAttribute("aria-selected", String(on)); x.tabIndex = on ? 0 : -1; document.getElementById(x.getAttribute("aria-controls")!)!.hidden = !on; }
  setRollShown(t.id === "tab-roll");  // the roll panel is mounted only while its tab shows (attachRoll draws it, laid out)
  if (t.id === "tab-roll") { drawRoll(); centreRoll(); }
  setMixerShown(t.id === "tab-mix");  // the mixer is mounted only while its tab shows
  devst.setDeviceShown(t.id === "tab-dev");  // so is the device panel
  updateWatch();
}
/** Binds the tabs and shows `?tab=` (roll, dev, mix), else the Device tab. */
export function initTabs(): void {
  tabs = Array.from(document.querySelectorAll<HTMLElement>('[role="tab"]'));
  tabs.forEach((t, i) => { t.addEventListener("click", () => showTab(t.id)); t.addEventListener("keydown", e => {
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") { const n = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length]!; n.focus(); showTab(n.id); }
  }); });
  const initial = new URLSearchParams(location.search).get("tab");
  showTab((tabs.find(t => t.id === "tab-" + initial) || tabs[1]!).id);
}
